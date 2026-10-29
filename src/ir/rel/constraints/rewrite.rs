//! SQL lowering audit
//! SQL audit [L26](../../../../tools/diagnostics/sql-lowering-audit.md#l26-constraint-backed-simplification): Remove relational work only from usable integrity facts and lineage.
//! Cardinality estimates and matching column names cannot establish these proofs.
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
// Find a payload column belonging to this particular key occurrence. A
// physical origin alone cannot distinguish two aliases of the same table.
fn occurrence_column(plan: &LogicalPlan, key: usize, origin: &super::properties::Origin) -> Option<usize> {
    match plan {
        LogicalPlan::TableScan(_) => {
            let facts = analyze(plan);
            if facts.origins.get(key)?.as_ref()?.table != origin.table { return None; }
            facts.origins.iter().position(|candidate| candidate.as_ref() == Some(origin))
        }
        LogicalPlan::Projection(p) => {
            let key = input_column(p.expr.get(key)?, p.input.schema())?;
            let value = occurrence_column(&p.input, key, origin)?;
            p.expr.iter().position(|expr| input_column(expr, p.input.schema()) == Some(value))
        }
        LogicalPlan::Distinct(datafusion::logical_expr::Distinct::On(p)) => {
            let key = input_column(p.select_expr.get(key)?, p.input.schema())?;
            let value = occurrence_column(&p.input, key, origin)?;
            p.select_expr.iter().position(|expr| input_column(expr, p.input.schema()) == Some(value))
        }
        LogicalPlan::SubqueryAlias(p) => occurrence_column(&p.input, key, origin),
        LogicalPlan::Filter(p) => occurrence_column(&p.input, key, origin),
        LogicalPlan::Sort(p) => occurrence_column(&p.input, key, origin),
        LogicalPlan::Limit(p) => occurrence_column(&p.input, key, origin),
        LogicalPlan::Window(p) if key < p.input.schema().fields().len() => occurrence_column(&p.input, key, origin),
        LogicalPlan::Join(j) => {
            let width = j.left.schema().fields().len();
            if key < width { occurrence_column(&j.left, key, origin) }
            else { occurrence_column(&j.right, key - width, origin).map(|index| index + width) }
        }
        _ => None,
    }
}

// Restore a pruned payload on the specific row occurrence identified by key.
// Existing columns retain their qualified names. Never cross aggregates,
// unions or other operators that merge distinct physical row occurrences.
fn expose_occurrence_column(plan: &LogicalPlan, key: usize, origin: &super::properties::Origin) -> Result<Option<(LogicalPlan, usize)>> {
    if let Some(index) = occurrence_column(plan, key, origin) {
        return Ok(Some((plan.clone(), index)));
    }
    let mut name = "__constraint_payload".to_string();
    while plan.schema().fields().iter().any(|f| f.name() == &name) { name.push('_'); }
    expose_named_occurrence_column(plan, key, origin, &name)
}
fn expose_named_occurrence_column(plan: &LogicalPlan, key: usize, origin: &super::properties::Origin, name: &str) -> Result<Option<(LogicalPlan, usize)>> {
    if let Some(index) = occurrence_column(plan, key, origin) { return Ok(Some((plan.clone(), index))); }
    let width = plan.schema().fields().len();
    let (expanded, value) = match plan {
        LogicalPlan::TableScan(scan) => {
            let facts = analyze(plan);
            if !facts.origins.get(key).and_then(|o| o.as_ref()).is_some_and(|o| o.table == origin.table) { return Ok(None); }
            let Ok(index) = scan.source.schema().index_of(&origin.column) else { return Ok(None); };
            let mut projection = scan.projection.clone().unwrap_or_else(|| (0..scan.source.schema().fields().len()).collect());
            projection.push(index);
            let scan = datafusion::logical_expr::TableScan::try_new(scan.table_name.clone(), scan.source.clone(), Some(projection), scan.filters.clone(), scan.fetch)?;
            let expanded = LogicalPlan::TableScan(scan);
            let value = Expr::Column(expanded.schema().columns()[width].clone());
            (expanded, value)
        }
        LogicalPlan::Projection(p) => {
            let Some(key) = input_column(&p.expr[key], p.input.schema()) else { return Ok(None); };
            let Some((input, index)) = expose_named_occurrence_column(&p.input, key, origin, name)? else { return Ok(None); };
            let mut expressions = p.expr.clone();
            expressions.push(Expr::Column(input.schema().columns()[index].clone()).alias(name));
            return Ok(Some((LogicalPlanBuilder::from(input).project(expressions)?.build()?, width)));
        }
        LogicalPlan::SubqueryAlias(_) | LogicalPlan::Filter(_) | LogicalPlan::Sort(_) | LogicalPlan::Limit(_) => {
            let input = plan.inputs()[0];
            let Some((input, index)) = expose_named_occurrence_column(input, key, origin, name)? else { return Ok(None); };
            let expanded = plan.with_new_exprs(plan.expressions(), vec![input])?;
            // Keep the alias as the outer boundary of this input. Wrapping it
            // in an unaliased projection would hide qualified columns in SQL.
            return Ok(Some((expanded, index)));
        }
        LogicalPlan::Distinct(datafusion::logical_expr::Distinct::On(selected)) => {
            let Some(key) = input_column(&selected.select_expr[key], selected.input.schema()) else { return Ok(None); };
            let Some((input, index)) = expose_named_occurrence_column(&selected.input, key, origin, name)? else { return Ok(None); };
            // ON and ORDER BY still select the same input row. Extending its
            // payload cannot affect selection (unlike DISTINCT over all fields).
            let mut expressions = selected.select_expr.clone();
            expressions.push(Expr::Column(input.schema().columns()[index].clone()).alias(name));
            let expanded = LogicalPlanBuilder::from(input)
                .distinct_on(selected.on_expr.clone(), expressions, selected.sort_expr.clone())?.build()?;
            return Ok(Some((expanded, width)));
        }
        LogicalPlan::Window(window) if key < window.input.schema().fields().len() => {
            let Some((input, index)) = expose_named_occurrence_column(&window.input, key, origin, name)? else { return Ok(None); };
            let expanded = plan.with_new_exprs(plan.expressions(), vec![input])?;
            return Ok(Some((expanded, index)));
        }
        LogicalPlan::Join(j) => {
            let lw = j.left.schema().fields().len();
            // Semi/anti/mark joins do not expose both input schemas.
            if !matches!(j.join_type, JoinType::Inner | JoinType::Left | JoinType::Right | JoinType::Full) { return Ok(None); }
            let (input, local_key, offset) = if key < lw { (&j.left, key, 0) } else { (&j.right, key-lw, lw) };
            let Some((input, index)) = expose_named_occurrence_column(input, local_key, origin, name)? else { return Ok(None); };
            let children = if key < lw { vec![input, j.right.as_ref().clone()] } else { vec![j.left.as_ref().clone(), input] };
            let expanded = plan.with_new_exprs(plan.expressions(), children)?;
            // Do not wrap a join in a payload-only projection: that hides
            // qualified duplicate field names from dialect SQL emission.
            return Ok(Some((expanded, offset+index)));
        }
        _ => return Ok(None),
    };
    let mut expressions = plan.schema().columns().into_iter().map(Expr::Column).collect::<Vec<_>>();
    expressions.push(value.alias(name));
    Ok(Some((LogicalPlanBuilder::from(expanded).project(expressions)?.build()?, width)))
}

fn join_pairs(j: &datafusion::logical_expr::Join) -> Option<Vec<(usize, usize)>> {
    let l = analyze(&j.left);
    let r = analyze(&j.right);
    let mut pairs = super::properties::equijoin_pairs(j, false)?;
    pairs.retain(|(a, b)| {
        !(l.non_null.contains(a) && r.non_null.contains(b)
            && l.constants.get(a).is_some_and(|v| r.constants.get(b) == Some(v)))
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
            // Expansion places the source endpoint on the left of the edge.
            // The same FK/uniqueness proof used for the target endpoint must
            // work in this orientation too; a mapping alone is not an FK proof.
            if j.join_type == JoinType::Inner && l.removable {
                let reversed = pairs.iter().map(|(a, b)| (*b, *a)).collect::<Vec<_>>();
                if l.unique_on(&pairs.iter().map(|(a, _)| *a).collect::<Vec<_>>(),
                    j.null_equality == datafusion::common::NullEquality::NullEqualsNull)
                    && guaranteed_match(&r, &l, &reversed) {
                    // A surviving FK/key value also supplies an observed left
                    // key. Keep the row-owning relation rather than scanning
                    // its node identity again just to project that same key.
                    let replacements: Option<Vec<_>> = projection.expr.iter()
                        .flat_map(|e| referenced(e, projection.input.schema()))
                        .filter(|index| *index < lw)
                        .map(|index| {
                            let origin = l.origins.get(index)?.as_ref()?;
                            let (_, right) = pairs.iter().find(|(left, _)|
                                l.origins.get(*left).and_then(|o| o.as_ref()) == Some(origin))?;
                            // Substitution must preserve the exposed type as
                            // well as the equality used to prove the join.
                            (j.left.schema().field(index).data_type() == j.right.schema().field(*right).data_type())
                                .then_some((index, *right))
                        }).collect();
                    if let Some(replacements) = replacements.filter(|replacements| replacements.is_empty() || r.removable) {
                        let self_join = !replacements.is_empty() && same_rows(&r, &l, &reversed);
                        let expressions = projection.expr.iter().zip(projection.schema.iter())
                            .map(|(expression, (qualifier, field))| {
                                expression.clone().transform_up(|expr| {
                                    if let Expr::Column(column) = &expr {
                                        if let Ok(index) = projection.input.schema().index_of_column(column) {
                                            if let Some((_, right)) = replacements.iter().find(|(left, _)| *left == index) {
                                                return Ok(Transformed::yes(Expr::Column(j.right.schema().columns()[*right].clone())));
                                            }
                                        }
                                    }
                                    Ok(Transformed::no(expr))
                                }).map(|result| result.data.alias_qualified(qualifier.cloned(), field.name()))
                            }).collect::<Result<Vec<_>>>()?;
                        let result = LogicalPlanBuilder::from(j.right.as_ref().clone())
                            .project(expressions)?.build()?;
                        let mut evidence = r;
                        evidence.evidence.extend(l.evidence);
                        proofs.push(proof(if self_join { "eliminate_self_join" } else { "eliminate_source_endpoint" }, "left endpoint has exactly one matching row; observed keys come from the surviving join keys", &evidence));
                        return Ok(Transformed::yes(result));
                    }
                }
            }
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
            let mut surviving = j.left.as_ref().clone();
            let replacements: Option<Vec<_>> = used
                .iter()
                .filter(|i| **i >= lw)
                .map(|i| {
                    if !self_join {
                        return None;
                    }
                    let origin = r.origins.get(i - lw)?.as_ref()?;
                    // A table/column origin does not identify a row occurrence.
                    // A traversal can carry both source.id and target.id from
                    // the same table. Anchor replacement to this join's key,
                    // including aliases of that key on the right side.
                    if let Some((left, _)) = pairs.iter().find(|(_, right)| {
                        r.origins.get(*right).and_then(|o| o.as_ref()) == Some(origin)
                    }) {
                        return Some((*i, j.left.schema().columns()[*left].clone()));
                    }
                    // A single joined key identifies the same physical row.
                    // Follow its occurrence through the left tree instead of
                    // borrowing the first payload with the same table origin.
                    if let [(key, _)] = pairs.as_slice() {
                        let key = surviving.schema().index_of_column(&j.left.schema().columns()[*key]).ok()?;
                        if let Ok(Some((expanded, left))) = expose_occurrence_column(&surviving, key, origin) {
                            if expanded.schema().field(left).data_type() == j.right.schema().field(i - lw).data_type() {
                                let column = expanded.schema().columns()[left].clone();
                                surviving = expanded;
                                return Some((*i, column));
                            }
                        }
                    }
                    // Non-key payload is interchangeable only when the left
                    // input is a single-row-source lens, not a join carrying
                    // multiple occurrences of that physical table.
                    domain(&j.left)?;
                    let left = l.origins.iter().position(|o| o.as_ref() == Some(origin))?;
                    Some((*i, j.left.schema().columns()[left].clone()))
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
                                                left.clone(),
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
                let result = LogicalPlanBuilder::from(surviving)
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
    // Removing an endpoint join can expose another one directly beneath the
    // surviving projection. Revisit those newly adjacent operators. Every
    // rewrite removes a join/set operator or at least one grouping key, so
    // their initial count is a structural bound on the number of passes.
    let mut budget = 1;
    plan.apply(|node| {
        budget += match node {
            LogicalPlan::Join(_) | LogicalPlan::Distinct(_) => 1,
            LogicalPlan::Aggregate(a) => a.group_expr.len(),
            _ => 0,
        };
        Ok(datafusion::common::tree_node::TreeNodeRecursion::Continue)
    })?;
    let mut proofs = Vec::new();
    let mut result = plan;
    for _ in 0..budget {
        let before = proofs.len();
        result = result.transform_up(|p| rewrite_node(p, &mut proofs))?.data;
        if proofs.len() == before { break; }
    }
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
