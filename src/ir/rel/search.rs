//! First-class ranked access. Native execution and backend search implement
//! the same relational boundary; SQL placement can keep surrounding joins.
use super::*;
use datafusion::common::DFSchemaRef;
use datafusion::logical_expr::{ExprFunctionExt, Extension, UserDefinedLogicalNodeCore};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchMetric {
    Cosine,
    Dot,
    L2,
    Bm25,
}
/// Compatibility input for the original search_indexes configuration. The
/// catalog immediately converts this into general source metadata.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SearchIndex {
    pub table: String,
    pub column: String,
    pub metric: SearchMetric,
    pub backend: LegacySearchOptions,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LegacySearchOptions {
    pub kind: String,
    #[serde(flatten)]
    pub options: std::collections::BTreeMap<String, serde_json::Value>,
}
impl From<SearchIndex> for super::source_metadata::SourceMetadata {
    fn from(index: SearchIndex) -> Self {
        let mut source_options = index.backend.options;
        let mut index_options = std::collections::BTreeMap::new();
        // Normalize the original per-index tuning fields at the input boundary.
        for name in ["nprobes", "refine_factor"] {
            if let Some(value) = source_options.remove(name) {
                index_options.insert(name.to_owned(), value);
            }
        }
        Self {
            table: index.table,
            format: Some(index.backend.kind),
            options: source_options,
            indexes: vec![super::source_metadata::IndexMetadata {
                column: index.column,
                metric: index.metric,
                options: index_options,
            }],
        }
    }
}
pub const SCORE_COLUMN: &str = "__relationship_search_score";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RankedJoin {
    pub source: Arc<LogicalPlan>,
    pub target: Arc<LogicalPlan>,
    pub score: Expr,
    pub predicate: Option<Expr>,
    pub source_keys: Vec<Expr>,
    pub target_keys: Vec<Expr>,
    pub ascending: bool,
    pub nulls_first: bool,
    pub limit: u64,
    pub exact: bool,
    pub index: Option<super::source_metadata::IndexMetadata>,
    pub source_metadata: Option<super::source_metadata::SourceMetadata>,
    pub schema: DFSchemaRef,
}
impl PartialOrd for RankedJoin {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(format!("{self:?}").cmp(&format!("{other:?}")))
    }
}
impl RankedJoin {
    pub fn metric(&self) -> Option<SearchMetric> {
        let Expr::ScalarFunction(f) = &self.score else {
            return None;
        };
        let function = crate::ir::functions::logical::definition(&f.func)?;
        function.search_metric.clone()
    }
    pub fn query(&self) -> &Expr {
        let Expr::ScalarFunction(f) = &self.score else {
            unreachable!()
        };
        &f.args[0]
    }
    pub fn document(&self) -> &Expr {
        let Expr::ScalarFunction(f) = &self.score else {
            unreachable!()
        };
        &f.args[1]
    }
    pub fn into_plan(self) -> LogicalPlan {
        LogicalPlan::Extension(Extension {
            node: Arc::new(self),
        })
    }
    pub fn native_plan(&self) -> RelResult<LogicalPlan> {
        if self.index.is_some() {
            return Err(RelError::Unsupported(
                "declared indexed retrieval requires lowering by the owning engine".into(),
            ));
        }
        let mut builder = LogicalPlanBuilder::from(self.source.as_ref().clone())
            .cross_join(self.target.as_ref().clone())?;
        if let Some(predicate) = &self.predicate {
            builder = builder.filter(predicate.clone())?;
        }
        let mut columns = builder
            .schema()
            .columns()
            .into_iter()
            .map(Expr::Column)
            .collect::<Vec<_>>();
        columns.push(self.score.clone().alias(SCORE_COLUMN));
        builder = builder.project(columns)?;
        let mut order = vec![col_exact(SCORE_COLUMN).sort(self.ascending, self.nulls_first)];
        order.extend(
            self.target_keys
                .iter()
                .cloned()
                .map(|e| e.sort(true, false)),
        );
        let rank = "__relationship_access_rank";
        let row = datafusion::functions_window::expr_fn::row_number()
            .partition_by(self.source_keys.clone())
            .order_by(order)
            .build()?
            .alias(rank);
        Ok(builder
            .window(vec![row])?
            .filter(col_exact(rank).lt_eq(lit(self.limit)))?
            .project(
                self.schema
                    .columns()
                    .into_iter()
                    .map(Expr::Column)
                    .collect::<Vec<_>>(),
            )?
            .build()?)
    }
}
impl UserDefinedLogicalNodeCore for RankedJoin {
    fn name(&self) -> &str {
        "RankedSearchJoin"
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        vec![&self.source, &self.target]
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<Expr> {
        std::iter::once(self.score.clone())
            .chain(self.predicate.clone())
            .collect()
    }
    fn fmt_for_explain(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "RankedSearchJoin: {:?}, k={}, exact={}, index={:?}",
            self.metric(),
            self.limit,
            self.exact,
            self.index
        )
    }
    fn with_exprs_and_inputs(
        &self,
        exprs: Vec<Expr>,
        inputs: Vec<LogicalPlan>,
    ) -> datafusion::common::Result<Self> {
        if inputs.len() != 2 || exprs.len() != 1 + usize::from(self.predicate.is_some()) {
            return Err(DataFusionError::Plan("ranked search inputs changed".into()));
        }
        let mut node = self.clone();
        node.source = Arc::new(inputs[0].clone());
        node.target = Arc::new(inputs[1].clone());
        node.score = exprs[0].clone().unalias();
        node.predicate = exprs.get(1).cloned().map(Expr::unalias);
        Ok(node)
    }
}
pub(crate) fn node(plan: &LogicalPlan) -> Option<&RankedJoin> {
    let LogicalPlan::Extension(e) = plan else {
        return None;
    };
    e.node.as_any().downcast_ref()
}
pub(crate) fn native(plan: LogicalPlan) -> RelResult<LogicalPlan> {
    Ok(plan
        .transform_up_with_subqueries(|p| match node(&p) {
            Some(n) => Ok(datafusion::common::tree_node::Transformed::yes(
                n.native_plan()
                    .map_err(|e| DataFusionError::Plan(e.to_string()))?,
            )),
            None => Ok(datafusion::common::tree_node::Transformed::no(p)),
        })?
        .data)
}

/// A source filter commutes with per-source top-k; target/score filters do not.
/// DataFusion's generic extension rule sends a predicate to every input, so
/// this rewrite deliberately targets only the source input.
pub(crate) fn push_source_filters(plan: LogicalPlan) -> RelResult<LogicalPlan> {
    use datafusion::common::tree_node::Transformed;
    Ok(plan
        .transform_up_with_subqueries(|plan| {
            let LogicalPlan::Filter(filter) = &plan else {
                return Ok(Transformed::no(plan));
            };
            let Some(search) = node(&filter.input) else {
                return Ok(Transformed::no(plan));
            };
            let (push, keep): (Vec<_>, Vec<_>) =
                datafusion::logical_expr::utils::split_conjunction(&filter.predicate)
                    .into_iter()
                    .cloned()
                    .partition(|e| {
                        e.column_refs()
                            .iter()
                            .all(|c| search.source.schema().has_column(c))
                    });
            if push.is_empty() {
                return Ok(Transformed::no(plan));
            }
            let mut search = search.clone();
            search.source = Arc::new(
                LogicalPlanBuilder::from(search.source.as_ref().clone())
                    .filter(datafusion::logical_expr::utils::conjunction(push).unwrap())?
                    .build()?,
            );
            let result = search.into_plan();
            let result = if let Some(predicate) = datafusion::logical_expr::utils::conjunction(keep)
            {
                LogicalPlanBuilder::from(result)
                    .filter(predicate)?
                    .build()?
            } else {
                result
            };
            Ok(Transformed::yes(result))
        })?
        .data)
}

/// Restrict a computed relationship to the source keys bound by its graph join.
/// This is a semijoin on source groups, so it cannot refill top-k after a target
/// filter or turn reverse traversal into a different nearest-neighbor query.
pub(crate) fn bind_seeds(plan: LogicalPlan) -> RelResult<LogicalPlan> {
    use datafusion::common::tree_node::{Transformed, TreeNode, TreeNodeRecursion};
    use datafusion::logical_expr::{JoinType, Operator};
    fn remap(
        expr: Expr,
        schema: &datafusion::common::DFSchema,
        values: &[Expr],
    ) -> datafusion::common::Result<Expr> {
        Ok(expr
            .transform_up(|e| {
                if let Expr::Column(c) = &e {
                    if let Ok(i) = schema.index_of_column(c) {
                        return Ok(Transformed::yes(values[i].clone().unalias()));
                    }
                }
                Ok(Transformed::no(e))
            })?
            .data)
    }
    fn restrict(
        right: &LogicalPlan,
        left: &LogicalPlan,
        keys: Vec<(Expr, Expr)>,
        next: &mut usize,
    ) -> RelResult<Option<LogicalPlan>> {
        if let Some(search) = node(right) {
            let keys = keys
                .into_iter()
                .filter(|(_, r)| {
                    !r.column_refs().is_empty()
                        && r.column_refs()
                            .iter()
                            .all(|c| search.source.schema().has_column(c))
                })
                .collect::<Vec<_>>();
            if keys.is_empty() {
                return Ok(None);
            }
            let mut source = search.clone();
            let expressions = keys
                .iter()
                .enumerate()
                .map(|(i, (l, _))| l.clone().alias(format!("__relationship_seed_{i}")))
                .collect::<Vec<_>>();
            let id = *next;
            *next += 1;
            let seeds = LogicalPlanBuilder::from(left.clone())
                .project(expressions)?
                .distinct()?
                .alias(format!("__w_sql_cte_search_seeds_{id}"))?
                .build()?;
            let predicates = keys
                .into_iter()
                .enumerate()
                .map(|(i, (_, r))| r.eq(col_exact(&format!("__relationship_seed_{i}"))))
                .collect::<Vec<_>>();
            source.source = Arc::new(
                LogicalPlanBuilder::from(source.source.as_ref().clone())
                    .alias(format!("__w_sql_cte_search_source_{id}"))?
                    .join_on(seeds, JoinType::LeftSemi, predicates)?
                    .build()?,
            );
            return Ok(Some(source.into_plan()));
        }
        let (input, keys) = match right {
            LogicalPlan::Projection(p) => (
                &p.input,
                keys.into_iter()
                    .map(|(l, r)| Ok((l, remap(r, &p.schema, &p.expr)?)))
                    .collect::<RelResult<Vec<_>>>()?,
            ),
            LogicalPlan::SubqueryAlias(a) => {
                let values = a
                    .input
                    .schema()
                    .columns()
                    .into_iter()
                    .map(Expr::Column)
                    .collect::<Vec<_>>();
                (
                    &a.input,
                    keys.into_iter()
                        .map(|(l, r)| Ok((l, remap(r, &a.schema, &values)?)))
                        .collect::<RelResult<Vec<_>>>()?,
                )
            }
            LogicalPlan::Filter(f) => (&f.input, keys),
            LogicalPlan::Window(w)
                if w.window_expr.iter().all(
                    |e| matches!(e,Expr::Alias(a) if a.name.starts_with("__relationship_rank_")),
                ) =>
            {
                (&w.input, keys)
            }
            _ => return Ok(None),
        };
        let Some(input) = restrict(input, left, keys, next)? else {
            return Ok(None);
        };
        Ok(Some(
            right.with_new_exprs(right.expressions(), vec![input])?,
        ))
    }
    let mut next = 0;
    Ok(plan
        .transform_up_with_subqueries(|plan| {
            let LogicalPlan::Join(join) = &plan else {
                return Ok(Transformed::no(plan));
            };
            if !matches!(join.join_type, JoinType::Inner | JoinType::Left) {
                return Ok(Transformed::no(plan));
            }
            let mut selective = false;
            join.left.apply_with_subqueries(|p| {
                selective |= match p {
                    LogicalPlan::Filter(f) => {
                        !matches!(f.predicate, Expr::IsNotNull(_) | Expr::Literal(..))
                    }
                    LogicalPlan::TableScan(s) => !s.filters.is_empty() || s.fetch.is_some(),
                    LogicalPlan::Limit(_) | LogicalPlan::Values(_) | LogicalPlan::Join(_) => true,
                    _ => false,
                };
                Ok(if selective {
                    TreeNodeRecursion::Stop
                } else {
                    TreeNodeRecursion::Continue
                })
            })?;
            if !selective {
                return Ok(Transformed::no(plan));
            }
            let mut keys = join.on.clone();
            if let Some(filter) = &join.filter {
                for e in datafusion::logical_expr::utils::split_conjunction(filter) {
                    if let Expr::BinaryExpr(b) = e {
                        if b.op == Operator::Eq {
                            let belongs = |e: &Expr, p: &LogicalPlan| {
                                !e.column_refs().is_empty()
                                    && e.column_refs().iter().all(|c| p.schema().has_column(c))
                            };
                            if belongs(&b.left, &join.left) && belongs(&b.right, &join.right) {
                                keys.push((*b.left.clone(), *b.right.clone()));
                            } else if belongs(&b.right, &join.left) && belongs(&b.left, &join.right)
                            {
                                keys.push((*b.right.clone(), *b.left.clone()));
                            }
                        }
                    }
                }
            }
            let Some(right) = restrict(&join.right, &join.left, keys, &mut next)
                .map_err(|e| DataFusionError::Plan(e.to_string()))?
            else {
                return Ok(Transformed::no(plan));
            };
            let mut result = join.clone();
            result.right = Arc::new(right);
            Ok(Transformed::yes(LogicalPlan::Join(result)))
        })?
        .data)
}
