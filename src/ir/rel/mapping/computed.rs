//! Declarative edges lower to the same relational operators as physical edges.
use super::*;
use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
use datafusion::logical_expr::{ExprFunctionExt, Volatility};
use datafusion::sql::{
    planner::PlannerContext,
    sqlparser::{ast, dialect::GenericDialect, parser::Parser, tokenizer::Token},
};
use serde::{Deserialize, Serialize};
use std::ops::ControlFlow;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComputedRelationship {
    pub name: String,
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub predicate: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    #[serde(default)]
    pub order_by: Vec<RelationshipOrder>,
    #[serde(default)]
    pub limit_per_source: Option<u64>,
    #[serde(default)]
    pub retrieval: RetrievalMode,
    /// Optional explicit candidate stage before final scoring/ranking.
    #[serde(default)]
    pub candidates: Option<RelationshipStage>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipStage {
    #[serde(default)]
    pub predicate: Option<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
    #[serde(default)]
    pub order_by: Vec<RelationshipOrder>,
    pub limit_per_source: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipOrder {
    pub expression: String,
    #[serde(default)]
    pub direction: SortDirection,
    #[serde(default)]
    pub nulls: NullOrder,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NullOrder {
    First,
    #[default]
    Last,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalMode {
    Exact,
    #[default]
    ApproximateAllowed,
}

fn error(message: impl Into<String>) -> RelError {
    RelError::Unsupported(message.into())
}
fn key(side: &str, i: usize) -> String {
    format!("__relationship_{side}_key_{i}")
}
fn prop(side: &str, name: &str) -> String {
    format!(
        "__relationship_{side}_{}",
        name.as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
impl GraphMapping {
    /// Validate every computed source with the existing relational planner.
    /// Hosts use this at definition time without scanning or executing sources.
    pub fn validate_computed_relationships(&self) -> RelResult<()> {
        for edge in self.edges.values() {
            if let MappedSource::Computed(rule) = &edge.source {
                plan(self, rule)?;
            }
        }
        Ok(())
    }

    /// Custom functions carry native execution and optional SQL implementations.
    pub fn register_logical_function(
        &mut self,
        function: crate::ir::functions::logical::LogicalFunction,
    ) -> RelResult<&mut Self> {
        let name = function.logical_name().to_owned();
        if self.logical_functions.contains_key(&name)
            || crate::ir::functions::search::function(&name).is_some()
        {
            return Err(error(format!("logical function `{name}` already exists")));
        }
        let udf = function.into_udf();
        if udf.signature().volatility != Volatility::Immutable {
            return Err(error("relationship functions must be immutable"));
        }
        // The immutable contract makes recomputation safe across SQL islands.
        // Retain the definition on the UDF, so compilation is request-local.
        self.logical_functions.insert(name, udf);
        Ok(self)
    }
    pub fn map_computed_relationship(
        &mut self,
        rule: ComputedRelationship,
    ) -> RelResult<&mut Self> {
        if rule.name.is_empty() || self.edges.contains_key(&rule.name) {
            return Err(error("relationship name must be nonempty and unique"));
        }
        let source = self
            .nodes
            .get(&rule.source)
            .ok_or_else(|| error(format!("unknown source label `{}`", rule.source)))?;
        let target = self
            .nodes
            .get(&rule.target)
            .ok_or_else(|| error(format!("unknown target label `{}`", rule.target)))?;
        if matches!(source.source, MappedSource::Computed(_))
            || matches!(target.source, MappedSource::Computed(_))
        {
            return Err(error("computed relationships require node sources"));
        }
        if let Some(candidate) = &rule.candidates {
            if candidate
                .properties
                .keys()
                .any(|p| rule.properties.contains_key(p))
            {
                return Err(error("candidate and final property names must be distinct"));
            }
            validate_order(&candidate.order_by, Some(candidate.limit_per_source))?;
        }
        validate_order(&rule.order_by, rule.limit_per_source)?;
        for expression in all_expressions(&rule) {
            parse(expression)?;
        }
        let src = (0..source.id_column.len())
            .map(|i| key("source", i))
            .collect::<Vec<_>>();
        let dst = (0..target.id_column.len())
            .map(|i| key("target", i))
            .collect::<Vec<_>>();
        let id = src.iter().chain(&dst).cloned().collect::<Vec<_>>();
        let mut edge = EdgeMapping::new(
            rule.name.clone(),
            MappedSource::Computed(Box::new(rule.clone())),
            src,
            dst,
            rule.source.clone(),
            rule.target.clone(),
        );
        edge.id_column = Some(id.into());
        for name in rule
            .properties
            .keys()
            .chain(rule.candidates.iter().flat_map(|c| c.properties.keys()))
        {
            if name.is_empty() {
                return Err(error("computed property name must be nonempty"));
            }
            edge.properties.insert(name.clone(), prop("edge", name));
        }
        self.map_edge(edge);
        Ok(self)
    }
}
fn validate_order(order: &[RelationshipOrder], limit: Option<u64>) -> RelResult<()> {
    if limit.is_some() && order.is_empty() {
        return Err(error("limit_per_source requires order_by"));
    }
    Ok(())
}
fn all_expressions(rule: &ComputedRelationship) -> Vec<&str> {
    rule.properties
        .values()
        .map(String::as_str)
        .chain(rule.predicate.iter().map(String::as_str))
        .chain(rule.order_by.iter().map(|o| o.expression.as_str()))
        .chain(rule.candidates.iter().flat_map(|c| {
            c.properties
                .values()
                .map(String::as_str)
                .chain(c.predicate.iter().map(String::as_str))
                .chain(c.order_by.iter().map(|o| o.expression.as_str()))
        }))
        .collect()
}
fn parse(text: &str) -> RelResult<ast::Expr> {
    let mut parser = Parser::new(&GenericDialect {})
        .try_with_sql(text)
        .map_err(|e| error(e.to_string()))?;
    let mut expr = parser.parse_expr().map_err(|e| error(e.to_string()))?;
    super::normalize_json_literals(&mut expr)?;
    if parser.peek_token().token != Token::EOF {
        return Err(error(
            "relationship expressions cannot contain statements or trailing SQL",
        ));
    }
    Ok(expr)
}
fn columns(plan: &LogicalPlan) -> Vec<Expr> {
    plan.schema()
        .columns()
        .into_iter()
        .map(Expr::Column)
        .collect()
}
fn endpoint(mapping: &GraphMapping, node: &NodeMapping, side: &str) -> RelResult<LogicalPlan> {
    let mut plan = LogicalPlanBuilder::from(mapping.source_plan(&node.source)?);
    for column in node.id_column.columns() {
        plan = plan.filter(col_exact(column).is_not_null())?;
    }
    let mut projection = node
        .id_column
        .columns()
        .iter()
        .enumerate()
        .map(|(i, c)| col_exact(c).alias(key(side, i)))
        .collect::<Vec<_>>();
    projection.extend(
        node.properties
            .iter()
            .map(|(p, c)| col_exact(c).alias(prop(side, p))),
    );
    Ok(plan.project(projection)?.build()?)
}
/// Expand only the relationship's property projections; keep endpoint columns
/// intact for native evaluation and backend access-path recognition.
fn inline_properties(
    mut expression: Expr,
    projected: &LogicalPlan,
    base: &LogicalPlan,
) -> RelResult<Expr> {
    let mut cursor = projected;
    while cursor != base {
        let LogicalPlan::Projection(p) = cursor else {
            return Err(error("invalid relationship property projection"));
        };
        expression = expression
            .transform_up(|expr| {
                if let Expr::Column(c) = &expr {
                    if let Ok(i) = p.schema.index_of_column(c) {
                        return Ok(datafusion::common::tree_node::Transformed::yes(
                            p.expr[i].clone().unalias(),
                        ));
                    }
                }
                Ok(datafusion::common::tree_node::Transformed::no(expr))
            })?
            .data;
        cursor = &p.input;
    }
    Ok(expression)
}
struct Expressions<'a> {
    mapping: &'a GraphMapping,
    // None denotes corpus statistics owned by a declared index.
    corpora: BTreeMap<String, Option<String>>,
}
impl Expressions<'_> {
    fn expression(&self, text: &str, plan: &LogicalPlan) -> RelResult<Expr> {
        let mut ast = parse(text)?;
        let flow = ast::visit_expressions_mut(&mut ast, |e| {
            match e {
                ast::Expr::CompoundIdentifier(parts)
                    if parts.len() == 2
                        && matches!(parts[0].value.as_str(), "source" | "target") =>
                {
                    *e = ast::Expr::Identifier(ast::Ident::with_quote(
                        '"',
                        prop(&parts[0].value, &parts[1].value),
                    ));
                }
                ast::Expr::Identifier(id)
                    if plan
                        .schema()
                        .has_column_with_unqualified_name(&prop("edge", &id.value)) =>
                {
                    *id = ast::Ident::with_quote('"', prop("edge", &id.value));
                }
                ast::Expr::Function(f) if f.name.to_string().eq_ignore_ascii_case("text.bm25") => {
                    if let ast::FunctionArguments::List(args) = &mut f.args {
                        if args.args.len() == 2 {
                            let ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(doc)) =
                                &args.args[1]
                            else {
                                return ControlFlow::Break(error(
                                    "BM25 requires a target property",
                                ));
                            };
                            let key = match doc {
                                ast::Expr::CompoundIdentifier(ids)
                                    if ids.len() == 2 && ids[0].value == "target" =>
                                {
                                    prop("target", &ids[1].value)
                                }
                                ast::Expr::Identifier(id) => id.value.clone(),
                                _ => {
                                    return ControlFlow::Break(error(
                                        "BM25 second argument must be a target property",
                                    ));
                                }
                            };
                            let Some(corpus) = self.corpora.get(&key) else {
                                return ControlFlow::Break(error(
                                    "BM25 second argument must be target.<text property>",
                                ));
                            };
                            let Some(corpus) = corpus else {
                                // Two arguments explicitly retain index-owned corpus semantics.
                                return ControlFlow::Continue(());
                            };
                            args.args
                                .push(ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(
                                    ast::Expr::Identifier(ast::Ident::with_quote('"', corpus)),
                                )));
                        }
                    }
                }
                _ => {}
            }
            ControlFlow::Continue(())
        });
        if let ControlFlow::Break(e) = flow {
            return Err(e);
        }
        let provider = MappingContextProvider::new(self.mapping);
        let expression =
            SqlToRel::new(&provider).sql_to_expr(ast, plan.schema(), &mut PlannerContext::new())?;
        expression.apply(|e| {
            if matches!(e,Expr::AggregateFunction(_)|Expr::WindowFunction(_)|Expr::ScalarSubquery(_)|Expr::Exists(_)|Expr::InSubquery(_)) {
                return Err(DataFusionError::Plan("relationship expressions must be scalar; subqueries, aggregates, and windows are composed by OrchidDB".into()));
            }
            if let Expr::ScalarFunction(f)=e { if !crate::ir::functions::immutable_call(&f.func,&f.args,plan.schema())? { return Err(DataFusionError::Plan("relationship expressions must be immutable".into())); } }
            Ok(TreeNodeRecursion::Continue)
        })?;
        Ok(expression)
    }
    fn search(
        &self,
        source: &LogicalPlan,
        target: &LogicalPlan,
        plan: &LogicalPlan,
        properties: &BTreeMap<String, String>,
        predicate: Option<&str>,
        order: &[RelationshipOrder],
        limit: Option<u64>,
        source_keys: &[Expr],
        target_keys: &[Expr],
        rule: &ComputedRelationship,
    ) -> RelResult<
        Option<(
            LogicalPlan,
            BTreeMap<String, String>,
            Vec<RelationshipOrder>,
        )>,
    > {
        use crate::ir::rel::search::{RankedJoin, SCORE_COLUMN, SearchMetric};
        use datafusion::logical_expr::ExprSchemable;
        let Some(limit) = limit else { return Ok(None) };
        if order.len() != 1 {
            return Ok(None);
        }
        let text = properties
            .get(&order[0].expression)
            .unwrap_or(&order[0].expression);
        let projected = self.stage(
            plan.clone(),
            properties,
            None,
            &[],
            None,
            source_keys,
            target_keys,
            0,
        )?;
        let score =
            inline_properties(self.expression(text, &projected)?, &projected, plan)?.unalias();
        let Expr::ScalarFunction(call) = &score else {
            return Ok(None);
        };
        let Some(logical) = crate::ir::functions::logical::definition(&call.func) else {
            return Ok(None);
        };
        let Some(metric) = logical.search_metric.clone() else {
            return Ok(None);
        };
        if call.args.len() < 2 {
            return Err(error(
                "indexed functions require query and document arguments",
            ));
        }
        if (order[0].direction == SortDirection::Asc) != (metric == SearchMetric::L2)
            || order[0].nulls != NullOrder::Last
        {
            return Ok(None);
        }
        let Expr::Column(document) = &call.args[1] else {
            return Ok(None);
        };
        if !target.schema().has_column(document)
            || call.args[0]
                .column_refs()
                .iter()
                .any(|c| !source.schema().has_column(c))
        {
            return Ok(None);
        }
        let target_node = self.mapping.node(&rule.target).unwrap();
        let source_metadata = if let MappedSource::Table(table) = &target_node.source {
            self.mapping.source_metadata.get(table).cloned()
        } else {
            None
        };
        let index = source_metadata.as_ref().and_then(|metadata| {
            target_node
                .properties
                .iter()
                .find(|(name, _)| prop("target", name) == document.name)
                .and_then(|(_, column)| {
                    metadata
                        .indexes
                        .iter()
                        .find(|i| i.column == *column && i.metric == metric)
                })
                .cloned()
        });
        if metric == SearchMetric::Bm25 && index.is_none() {
            return Ok(None);
        }
        let predicate = predicate
            .map(|text| {
                self.expression(text, &projected)
                    .and_then(|e| inline_properties(e, &projected, plan))
            })
            .transpose()?;
        let mut fields = plan
            .schema()
            .as_arrow()
            .fields()
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        fields.push(Arc::new(arrow::datatypes::Field::new(
            SCORE_COLUMN,
            score.get_type(plan.schema())?,
            true,
        )));
        let schema = Arc::new(DFSchema::try_from(arrow::datatypes::Schema::new(fields))?);
        let mut source_plan = source.clone();
        if let Some(predicate) = &predicate {
            let filters = datafusion::logical_expr::utils::split_conjunction(predicate)
                .into_iter()
                .filter(|e| {
                    e.column_refs()
                        .iter()
                        .all(|c| source.schema().has_column(c))
                })
                .cloned()
                .collect::<Vec<_>>();
            if let Some(filter) = datafusion::logical_expr::utils::conjunction(filters) {
                source_plan = LogicalPlanBuilder::from(source_plan)
                    .filter(filter)?
                    .build()?;
            }
        }
        let search = RankedJoin {
            source: Arc::new(source_plan),
            target: Arc::new(target.clone()),
            score,
            predicate,
            source_keys: source_keys.to_vec(),
            target_keys: target_keys.to_vec(),
            ascending: order[0].direction == SortDirection::Asc,
            nulls_first: false,
            limit,
            exact: rule.retrieval == RetrievalMode::Exact,
            index,
            source_metadata,
            schema,
        };
        let mut properties = properties.clone();
        if properties.contains_key(&order[0].expression) {
            properties.insert(order[0].expression.clone(), format!("\"{SCORE_COLUMN}\""));
        }
        let mut order = order.to_vec();
        order[0].expression = format!("\"{SCORE_COLUMN}\"");
        Ok(Some((search.into_plan(), properties, order)))
    }

    fn stage(
        &self,
        mut plan: LogicalPlan,
        properties: &BTreeMap<String, String>,
        predicate: Option<&str>,
        order: &[RelationshipOrder],
        limit: Option<u64>,
        source_keys: &[Expr],
        target_keys: &[Expr],
        number: usize,
    ) -> RelResult<LogicalPlan> {
        // Resolve dependencies topologically, independent of declaration/map order.
        let mut pending = properties.clone();
        while !pending.is_empty() {
            let mut progress = false;
            let mut last = None;
            for (name, text) in pending.clone() {
                match self.expression(&text, &plan) {
                    Ok(expr) => {
                        let mut projection = columns(&plan);
                        projection.push(expr.alias(prop("edge", &name)));
                        plan = LogicalPlanBuilder::from(plan)
                            .project(projection)?
                            .build()?;
                        pending.remove(&name);
                        progress = true;
                    }
                    Err(e) => last = Some(e),
                }
            }
            if !progress {
                return Err(error(format!(
                    "unresolved or cyclic relationship properties: {}",
                    last.unwrap()
                )));
            }
        }
        if let Some(predicate) = predicate {
            let expr = self.expression(predicate, &plan)?;
            plan = LogicalPlanBuilder::from(plan).filter(expr)?.build()?;
        }
        if let Some(limit) = limit {
            let mut sorts = order
                .iter()
                .map(|o| {
                    Ok(self
                        .expression(
                            properties
                                .get(&o.expression)
                                .map(String::as_str)
                                .unwrap_or(&o.expression),
                            &plan,
                        )?
                        .sort(
                            o.direction == SortDirection::Asc,
                            o.nulls == NullOrder::First,
                        ))
                })
                .collect::<RelResult<Vec<_>>>()?;
            sorts.extend(target_keys.iter().cloned().map(|e| e.sort(true, false)));
            let rank = format!("__relationship_rank_{number}");
            let row = datafusion::functions_window::expr_fn::row_number()
                .partition_by(source_keys.to_vec())
                .order_by(sorts)
                .build()?
                .alias(&rank);
            plan = LogicalPlanBuilder::from(plan)
                .window(vec![row])?
                .filter(col_exact(&rank).lt_eq(lit(limit)))?
                .build()?;
        } else {
            for o in order {
                self.expression(&o.expression, &plan)?;
            }
        }
        Ok(plan)
    }
}
pub(super) fn plan(mapping: &GraphMapping, rule: &ComputedRelationship) -> RelResult<LogicalPlan> {
    let src = mapping
        .nodes
        .get(&rule.source)
        .ok_or_else(|| error("missing source node"))?;
    let dst = mapping
        .nodes
        .get(&rule.target)
        .ok_or_else(|| error("missing target node"))?;
    let target = endpoint(mapping, dst, "target")?;
    let mut source = endpoint(mapping, src, "source")?;
    let mut plan = LogicalPlanBuilder::from(source.clone())
        .cross_join(target.clone())?
        .build()?;
    let mut expressions = Expressions {
        mapping,
        corpora: BTreeMap::new(),
    };
    let mut docs = BTreeSet::new();
    for text in all_expressions(rule) {
        let expr = parse(text)?;
        let flow = ast::visit_expressions(&expr, |e| {
            if let ast::Expr::Function(f) = e {
                if f.name.to_string().eq_ignore_ascii_case("text.bm25") {
                    if let ast::FunctionArguments::List(args) = &f.args {
                        if args.args.len() == 2 {
                            if let ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(
                                ast::Expr::CompoundIdentifier(ids),
                            )) = &args.args[1]
                            {
                                if ids.len() == 2 && ids[0].value == "target" {
                                    docs.insert(ids[1].value.clone());
                                    return ControlFlow::Continue(());
                                }
                            }
                            return ControlFlow::Break(error(
                                "BM25 second argument must be target.<text property>",
                            ));
                        }
                    }
                }
            }
            ControlFlow::Continue(())
        });
        if let ControlFlow::Break(e) = flow {
            return Err(e);
        }
    }
    for (i, doc) in docs.into_iter().enumerate() {
        let name = format!("__relationship_corpus_{i}");
        let column = prop("target", &doc);
        let indexed = if let MappedSource::Table(table) = &dst.source {
            dst.properties.get(&doc).is_some_and(|column| {
                mapping.source_metadata.get(table).is_some_and(|metadata| {
                    metadata.indexes.iter().any(|i| {
                        i.column == *column
                            && i.metric == crate::ir::rel::search::SearchMetric::Bm25
                    })
                })
            })
        } else {
            false
        };
        if indexed {
            let (properties, order, limit) = match &rule.candidates {
                Some(c) => (&c.properties, &c.order_by, Some(c.limit_per_source)),
                None => (&rule.properties, &rule.order_by, rule.limit_per_source),
            };
            if limit.is_none()
                || order.len() != 1
                || order[0].direction != SortDirection::Desc
                || order[0].nulls != NullOrder::Last
                || !properties
                    .get(&order[0].expression)
                    .unwrap_or(&order[0].expression)
                    .trim_start()
                    .to_ascii_lowercase()
                    .starts_with("text.bm25(")
                || all_expressions(rule)
                    .iter()
                    .filter(|e| e.to_ascii_lowercase().contains("text.bm25("))
                    .count()
                    != 1
            {
                return Err(error(
                    "index-owned BM25 must be the primary descending top-k score of a relationship or its candidate stage",
                ));
            }
            // No corpus scan or placeholder value: the two-argument score
            // explicitly requires an index-owned corpus and engine lowering.
            expressions.corpora.insert(column, None);
            continue;
        }
        let aggregate = datafusion::functions_aggregate::array_agg::array_agg_udaf()
            .call(vec![col_exact(&column)])
            .alias(&name);
        let corpus = LogicalPlanBuilder::from(target.clone())
            .aggregate(Vec::<Expr>::new(), vec![aggregate])?
            .project(vec![col_exact(&name)])?
            .alias(format!(
                "__w_sql_cte_{}",
                prop("corpus", &format!("{}:{doc}", rule.target))
            ))?
            .build()?;
        source = LogicalPlanBuilder::from(source)
            .cross_join(corpus)?
            .build()?;
        plan = LogicalPlanBuilder::from(source.clone())
            .cross_join(target.clone())?
            .build()?;
        expressions.corpora.insert(column, Some(name));
    }
    let source_keys = (0..src.id_column.len())
        .map(|i| col_exact(&key("source", i)))
        .collect::<Vec<_>>();
    let target_keys = (0..dst.id_column.len())
        .map(|i| col_exact(&key("target", i)))
        .collect::<Vec<_>>();
    let mut final_properties = rule.properties.clone();
    let mut final_order = rule.order_by.clone();
    if let Some(c) = &rule.candidates {
        let mut properties = c.properties.clone();
        let mut order = c.order_by.clone();
        if let Some((search, replaced, replaced_order)) = expressions.search(
            &source,
            &target,
            &plan,
            &properties,
            c.predicate.as_deref(),
            &c.order_by,
            Some(c.limit_per_source),
            &source_keys,
            &target_keys,
            rule,
        )? {
            plan = search;
            properties = replaced;
            order = replaced_order;
        }
        plan = expressions.stage(
            plan,
            &properties,
            c.predicate.as_deref(),
            &order,
            Some(c.limit_per_source),
            &source_keys,
            &target_keys,
            0,
        )?;
    } else if let Some((search, replaced, replaced_order)) = expressions.search(
        &source,
        &target,
        &plan,
        &final_properties,
        rule.predicate.as_deref(),
        &rule.order_by,
        rule.limit_per_source,
        &source_keys,
        &target_keys,
        rule,
    )? {
        plan = search;
        final_properties = replaced;
        final_order = replaced_order;
    }
    plan = expressions.stage(
        plan,
        &final_properties,
        rule.predicate.as_deref(),
        &final_order,
        rule.limit_per_source,
        &source_keys,
        &target_keys,
        1,
    )?;
    let mut projection = source_keys;
    projection.extend(target_keys);
    projection.extend(
        rule.properties
            .keys()
            .chain(rule.candidates.iter().flat_map(|c| c.properties.keys()))
            .map(|name| col_exact(&prop("edge", name))),
    );
    Ok(LogicalPlanBuilder::from(plan)
        .project(projection)?
        .build()?)
}
