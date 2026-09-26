use super::{
    PlanProperties, analyze,
    properties::{harmless, input_column, referenced},
};
use datafusion::{
    common::{
        Result,
        tree_node::{Transformed, TreeNode},
    },
    logical_expr::{Expr, JoinType, LogicalPlan, LogicalPlanBuilder},
    optimizer::{OptimizerConfig, OptimizerRule},
};
use serde::Serialize;
#[derive(Debug, Clone, Serialize)]
pub struct RewriteProof {
    pub rule: String,
    pub reason: String,
    pub evidence: Vec<String>,
}
fn proof(rule: &str, reason: &str, p: &PlanProperties) -> RewriteProof {
    RewriteProof {
        rule: rule.into(),
        reason: reason.into(),
        evidence: p.evidence.iter().cloned().collect(),
    }
}

/// An FK proves existence only against an unfiltered, complete target and with
/// non-null child values. Uniqueness is checked independently by the caller.
fn guaranteed_match(l: &PlanProperties, r: &PlanProperties, pairs: &[(usize, usize)]) -> bool {
    l.foreign_keys.iter().any(|f| {
        r.complete_sources.contains(&f.target)
            && f.columns.iter().all(|i| l.non_null.contains(i))
            && pairs.len() == f.columns.len()
            && f.columns.iter().zip(&f.references).all(|(a, b)| {
                pairs.iter().any(|(x, y)| {
                    x == a
                        && r.origins
                            .get(*y)
                            .and_then(|o| o.as_ref())
                            .is_some_and(|o| o.table == f.target && o.column == *b)
                })
            })
    }) || same_rows(l, r, pairs)
}
fn same_rows(l: &PlanProperties, r: &PlanProperties, pairs: &[(usize, usize)]) -> bool {
    !pairs.is_empty()
        && pairs.iter().all(|(a, b)| {
            l.non_null.contains(a)
                && l.origins.get(*a).and_then(|o| o.as_ref()).is_some_and(|o| {
                    r.complete_sources.contains(&o.table)
                        && r.origins.get(*b).and_then(|o| o.as_ref()) == Some(o)
                })
        })
}
// Bounded containment: selection/projection/alias lenses over one source,
// with identical output lineage and syntactically equal normalized predicates.
fn domain(plan: &LogicalPlan) -> Option<(String, std::collections::BTreeSet<String>)> {
    match plan {
        LogicalPlan::TableScan(s) if s.filters.is_empty() && s.fetch.is_none() => {
            let p = analyze(plan);
            Some((
                p.complete_sources.iter().next()?.clone(),
                Default::default(),
            ))
        }
        LogicalPlan::SubqueryAlias(n) => domain(&n.input),
        LogicalPlan::Projection(n) if n.expr.iter().all(|e| harmless(e, n.input.schema())) => {
            domain(&n.input)
        }
        LogicalPlan::Filter(n) if harmless(&n.predicate, n.input.schema()) => {
            let (table, mut predicates) = domain(&n.input)?;
            let p = analyze(&n.input);
            let normalized = n
                .predicate
                .clone()
                .transform_up(|e| {
                    if let Some(i) = input_column(&e, n.input.schema()) {
                        if let Some(Some(o)) = p.origins.get(i) {
                            return Ok(Transformed::yes(Expr::Column(
                                datafusion::common::Column::new_unqualified(format!(
                                    "{}.{}",
                                    o.table, o.column
                                )),
                            )));
                        }
                    }
                    Ok(Transformed::no(e))
                })
                .ok()?
                .data;
            predicates.insert(format!("{normalized:?}"));
            Some((table, predicates))
        }
        _ => None,
    }
}
fn contained(a: &LogicalPlan, b: &LogicalPlan) -> bool {
    let (Some((at, ap)), Some((bt, bp))) = (domain(a), domain(b)) else {
        return false;
    };
    let x = analyze(a);
    let y = analyze(b);
    at == bt
        && bp.is_subset(&ap)
        && x.origins.len() == y.origins.len()
        && x.origins.iter().all(Option::is_some)
        && x.origins == y.origins
        && a.schema()
            .fields()
            .iter()
            .zip(b.schema().fields())
            .all(|(a, b)| a.data_type() == b.data_type())
        && x.removable
        && y.removable
}
fn join_pairs(j: &datafusion::logical_expr::Join) -> Option<Vec<(usize, usize)>> {
    if j.null_aware {
        return None;
    }
    let l = analyze(&j.left);
    let r = analyze(&j.right);
    let mut pairs =
        j.on.iter()
            .map(|(a, b)| {
                Some((
                    input_column(a, j.left.schema())?,
                    input_column(b, j.right.schema())?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
    fn extract(
        e: &Expr,
        j: &datafusion::logical_expr::Join,
        pairs: &mut Vec<(usize, usize)>,
    ) -> Option<()> {
        if let Expr::BinaryExpr(b) = e {
            if b.op == datafusion::logical_expr::Operator::And {
                extract(&b.left, j, pairs)?;
                extract(&b.right, j, pairs)?;
                return Some(());
            }
            if b.op == datafusion::logical_expr::Operator::Eq {
                let pair = input_column(&b.left, j.left.schema())
                    .zip(input_column(&b.right, j.right.schema()))
                    .or_else(|| {
                        input_column(&b.right, j.left.schema())
                            .zip(input_column(&b.left, j.right.schema()))
                    })?;
                pairs.push(pair);
                return Some(());
            }
        }
        None
    }
    if let Some(f) = &j.filter {
        extract(f, j, &mut pairs)?;
    }
    pairs.retain(|(a, b)| {
        !l.constants
            .get(a)
            .is_some_and(|v| r.constants.get(b) == Some(v))
    });
    Some(pairs)
}
fn rewrite_node(
    plan: LogicalPlan,
    proofs: &mut Vec<RewriteProof>,
) -> Result<Transformed<LogicalPlan>> {
    if let LogicalPlan::Distinct(datafusion::logical_expr::Distinct::All(input)) = &plan {
        let p = analyze(input);
        if p.unique_on(
            &(0..input.schema().fields().len()).collect::<Vec<_>>(),
            true,
        ) {
            proofs.push(proof(
                "remove_distinct",
                "input has a NULL-equal unique key",
                &p,
            ));
            return Ok(Transformed::yes(input.as_ref().clone()));
        }
    }
    if let LogicalPlan::Distinct(datafusion::logical_expr::Distinct::All(input)) = &plan {
        if let LogicalPlan::Union(u) = input.as_ref() {
            if u.inputs.len() == 2 {
                let keep = if contained(&u.inputs[0], &u.inputs[1]) {
                    Some(&u.inputs[1])
                } else if contained(&u.inputs[1], &u.inputs[0]) {
                    Some(&u.inputs[0])
                } else {
                    None
                };
                if let Some(keep) = keep {
                    proofs.push(proof("contained_union","set union contains a proven selection/projection subset; outer DISTINCT retained",&analyze(keep)));
                    let exprs = keep
                        .schema()
                        .columns()
                        .into_iter()
                        .zip(u.schema.iter())
                        .map(|(c, (q, f))| Expr::Column(c).alias_qualified(q.cloned(), f.name()))
                        .collect::<Vec<_>>();
                    return Ok(Transformed::yes(
                        LogicalPlanBuilder::from(keep.as_ref().clone())
                            .project(exprs)?
                            .distinct()?
                            .build()?,
                    ));
                }
            }
        }
    }
    if let LogicalPlan::Aggregate(a) = &plan {
        if a.group_expr.len() > 1
            && a.group_expr
                .iter()
                .all(|e| input_column(e, a.input.schema()).is_some())
        {
            let p = analyze(&a.input);
            let indices = a
                .group_expr
                .iter()
                .map(|e| input_column(e, a.input.schema()).unwrap())
                .collect::<Vec<_>>();
            let mut kept = (0..indices.len()).collect::<Vec<_>>();
            let mut removed = Vec::new();
            for candidate in (0..indices.len()).rev() {
                let rest = kept
                    .iter()
                    .filter(|i| **i != candidate)
                    .map(|i| indices[*i])
                    .collect::<Vec<_>>();
                let ty = a.input.schema().field(indices[candidate]).data_type();
                if !rest.is_empty()
                    && p.closure(&rest).contains(&indices[candidate])
                    && matches!(
                        ty,
                        arrow::datatypes::DataType::Int64
                            | arrow::datatypes::DataType::Int32
                            | arrow::datatypes::DataType::Utf8
                    )
                {
                    kept.retain(|i| *i != candidate);
                    removed.push(candidate);
                }
            }
            if !removed.is_empty() {
                let groups = kept
                    .iter()
                    .map(|i| a.group_expr[*i].clone())
                    .collect::<Vec<_>>();
                let mut aggregates = a
                    .aggr_expr
                    .iter()
                    .enumerate()
                    .map(|(i, e)| {
                        e.clone()
                            .alias(a.schema.field(a.group_expr.len() + i).name())
                    })
                    .collect::<Vec<_>>();
                for i in &removed {
                    let mut name = format!("__constraint_group_{i}");
                    while a.schema.fields().iter().any(|f| f.name() == &name) {
                        name.push('_');
                    }
                    aggregates.push(
                        datafusion::functions_aggregate::expr_fn::min(a.group_expr[*i].clone())
                            .alias(name),
                    );
                }
                let reduced = LogicalPlanBuilder::from(a.input.as_ref().clone())
                    .aggregate(groups, aggregates)?
                    .build()?;
                let mut exprs = Vec::new();
                for i in 0..a.group_expr.len() {
                    let pos = kept.iter().position(|x| *x == i).unwrap_or_else(|| {
                        kept.len()
                            + a.aggr_expr.len()
                            + removed.iter().position(|x| *x == i).unwrap()
                    });
                    exprs.push(Expr::Column(reduced.schema().columns()[pos].clone()));
                }
                for i in 0..a.aggr_expr.len() {
                    exprs.push(Expr::Column(
                        reduced.schema().columns()[kept.len() + i].clone(),
                    ));
                }
                let exprs = exprs
                    .into_iter()
                    .zip(a.schema.iter())
                    .map(|(e, (q, f))| e.alias_qualified(q.cloned(), f.name()))
                    .collect::<Vec<_>>();
                proofs.push(proof(
                    "reduce_grouping",
                    "remaining grouping columns functionally determine removed scalar columns",
                    &p,
                ));
                return Ok(Transformed::yes(
                    LogicalPlanBuilder::from(reduced).project(exprs)?.build()?,
                ));
            }
        }
    }
    // Keeping the projection preserves column names, order, and expression evaluation.
    if let LogicalPlan::Projection(projection) = &plan {
        if let LogicalPlan::Join(j) = projection.input.as_ref() {
            if !matches!(j.join_type, JoinType::Inner | JoinType::Left) {
                return Ok(Transformed::no(plan));
            }
            let Some(pairs) = join_pairs(j) else {
                return Ok(Transformed::no(plan));
            };
            let l = analyze(&j.left);
            let r = analyze(&j.right);
            let lw = j.left.schema().fields().len();
            // Requiring NULL-equal uniqueness also handles null-safe join operators.
            if !r.removable
                || !r.unique_on(
                    &pairs.iter().map(|(_, b)| *b).collect::<Vec<_>>(),
                    j.null_equality == datafusion::common::NullEquality::NullEqualsNull,
                )
            {
                return Ok(Transformed::no(plan));
            }
            let exists = guaranteed_match(&l, &r, &pairs);
            if j.join_type == JoinType::Inner && !exists {
                return Ok(Transformed::no(plan));
            }
            let used = projection
                .expr
                .iter()
                .flat_map(|e| referenced(e, projection.input.schema()))
                .collect::<Vec<_>>();
            let self_join = exists && same_rows(&l, &r, &pairs);
            let replacements: Option<Vec<_>> = used
                .iter()
                .filter(|i| **i >= lw)
                .map(|i| {
                    if !self_join {
                        return None;
                    }
                    let origin = r.origins.get(i - lw)?.as_ref()?;
                    let left = l.origins.iter().position(|o| o.as_ref() == Some(origin))?;
                    Some((*i, left))
                })
                .collect();
            if let Some(replacements) = replacements {
                let expressions = projection
                    .expr
                    .iter()
                    .map(|e| {
                        e.clone()
                            .transform_up(|e| {
                                if let Expr::Column(c) = &e {
                                    if let Ok(i) = projection.input.schema().index_of_column(c) {
                                        if let Some((_, left)) =
                                            replacements.iter().find(|(right, _)| *right == i)
                                        {
                                            return Ok(Transformed::yes(Expr::Column(
                                                j.left.schema().columns()[*left].clone(),
                                            )));
                                        }
                                    }
                                }
                                Ok(Transformed::no(e))
                            })
                            .map(|t| t.data)
                    })
                    .collect::<Result<Vec<_>>>()?;
                // Alias every expression to its original output name, including right aliases.
                let expressions = expressions
                    .into_iter()
                    .zip(projection.schema.iter())
                    .map(|(e, (q, f))| e.alias_qualified(q.cloned(), f.name()))
                    .collect::<Vec<_>>();
                let result = LogicalPlanBuilder::from(j.left.as_ref().clone())
                    .project(expressions)?
                    .build()?;
                let mut combined = l;
                combined.evidence.extend(r.evidence);
                proofs.push(proof(
                    if self_join {
                        "eliminate_self_join"
                    } else {
                        "eliminate_join"
                    },
                    if j.join_type == JoinType::Left {
                        "at most one match; right output unused or same source identity"
                    } else {
                        "exactly one match from non-null FK or same source identity"
                    },
                    &combined,
                ));
                return Ok(Transformed::yes(result));
            }
        }
    }
    if let LogicalPlan::Join(j) = &plan {
        if matches!(j.join_type, JoinType::LeftSemi | JoinType::LeftAnti) {
            let pairs = join_pairs(j);
            let l = analyze(&j.left);
            let r = analyze(&j.right);
            if let Some(pairs) = pairs {
                if r.removable && guaranteed_match(&l, &r, &pairs) {
                    // An always-false filter keeps the left plan rather than silently
                    // discarding arbitrary expression evaluation here.
                    if j.join_type == JoinType::LeftSemi {
                        proofs.push(proof(
                            "contained_membership",
                            "all left rows have a target match; left occurrences preserved",
                            &l,
                        ));
                        return Ok(Transformed::yes(j.left.as_ref().clone()));
                    }
                }
            }
        }
    }
    Ok(Transformed::no(plan))
}
/// Public optimizer with a machine-readable proof trace for plan review.
pub fn optimize(plan: LogicalPlan) -> Result<(LogicalPlan, Vec<RewriteProof>)> {
    let mut supplied = false;
    plan.apply(|n| {
        if let LogicalPlan::TableScan(s) = n {
            if let Ok(p) = datafusion::datasource::source_as_provider(&s.source) {
                if p.as_any().is::<super::provider::ConstrainedProvider>() {
                    supplied = true;
                }
            }
        }
        Ok(datafusion::common::tree_node::TreeNodeRecursion::Continue)
    })?;
    if !supplied && analyze(&plan).evidence.is_empty() {
        return Ok((plan, Vec::new()));
    }
    let mut proofs = Vec::new();
    let result = plan.transform_up(|p| rewrite_node(p, &mut proofs))?.data;
    Ok((result, proofs))
}
#[derive(Debug)]
pub struct ConstraintOptimizer;
impl OptimizerRule for ConstraintOptimizer {
    fn name(&self) -> &str {
        "relational_constraints"
    }
    fn supports_rewrite(&self) -> bool {
        true
    }
    fn rewrite(
        &self,
        plan: LogicalPlan,
        _: &dyn OptimizerConfig,
    ) -> Result<Transformed<LogicalPlan>> {
        let (result, proofs) = optimize(plan)?;
        Ok(Transformed::new(
            result,
            !proofs.is_empty(),
            datafusion::common::tree_node::TreeNodeRecursion::Continue,
        ))
    }
}
