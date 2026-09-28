//! Connected relational access choices shared by all graph frontends.
use super::{OptimizerDecision, estimate};
use datafusion::{
    common::{
        Result,
        tree_node::{Transformed, TreeNode, TreeNodeRecursion},
    },
    logical_expr::{Expr, ExprSchemable, JoinType, LogicalPlan, LogicalPlanBuilder, Operator},
};
use std::collections::BTreeMap;

// Restrict movement to total relational expressions. Extension operators, paths,
// limits, optional/anti joins and effectful expressions are region boundaries.
fn pure_expr(e: &Expr) -> bool {
    match e {
        Expr::Column(_) | Expr::Literal(_, _) => true,
        Expr::Alias(a) => pure_expr(&a.expr),
        Expr::IsNull(e) | Expr::IsNotNull(e) | Expr::IsTrue(e) | Expr::Not(e) => pure_expr(e),
        Expr::TryCast(c) => pure_expr(&c.expr),
        Expr::Cast(c) => {
            matches!(
                c.data_type,
                arrow::datatypes::DataType::Utf8 | arrow::datatypes::DataType::LargeUtf8
            ) && pure_expr(&c.expr)
        }
        Expr::BinaryExpr(b) => {
            matches!(
                b.op,
                Operator::Eq
                    | Operator::NotEq
                    | Operator::Lt
                    | Operator::LtEq
                    | Operator::Gt
                    | Operator::GtEq
                    | Operator::And
                    | Operator::Or
                    | Operator::IsNotDistinctFrom
            ) && pure_expr(&b.left)
                && pure_expr(&b.right)
        }
        Expr::InList(l) => pure_expr(&l.expr) && l.list.iter().all(pure_expr),
        Expr::Case(c) => {
            c.expr.as_deref().is_none_or(pure_expr)
                && c.when_then_expr
                    .iter()
                    .all(|(a, b)| pure_expr(a) && pure_expr(b))
                && c.else_expr.as_deref().is_none_or(pure_expr)
        }
        Expr::ScalarFunction(f) if f.name() == "encode" => {
            f.args.len() == 2
                && pure_expr(&f.args[0])
                && matches!(&f.args[1],Expr::Literal(datafusion::common::ScalarValue::Utf8(Some(format)),_) if format=="hex" || format=="base64")
        }
        Expr::ScalarFunction(f) => {
            [
                "get_field",
                "concat",
                "named_struct",
                "make_array",
                "array_length",
            ]
            .contains(&f.name())
                && f.args.iter().all(pure_expr)
        }
        _ => false,
    }
}
fn pure_in_schema(e: &Expr, schema: &datafusion::common::DFSchema) -> bool {
    let normalized = e.clone().transform_up(|e| {
        if let Expr::Cast(c) = &e {
            if c.expr.get_type(schema).ok().as_ref() == Some(&c.data_type) {
                return Ok(Transformed::yes(*c.expr.clone()));
            }
        }
        Ok(Transformed::no(e))
    });
    normalized.is_ok_and(|e| pure_expr(&e.data))
}
fn pure_plan(p: &LogicalPlan) -> bool {
    match p {
        LogicalPlan::TableScan(s) => s.fetch.is_none() && s.filters.iter().all(pure_expr),
        LogicalPlan::Projection(p) => {
            p.expr.iter().all(|e| pure_in_schema(e, p.input.schema())) && pure_plan(&p.input)
        }
        LogicalPlan::SubqueryAlias(a) => pure_plan(&a.input),
        LogicalPlan::Filter(f) => pure_expr(&f.predicate) && pure_plan(&f.input),
        LogicalPlan::Unnest(u) => pure_plan(&u.input),
        LogicalPlan::Union(u) => u.inputs.iter().all(|p| pure_plan(p)),
        LogicalPlan::EmptyRelation(_) => true,
        // RDF set construction remains an opaque leaf; never move the DISTINCT
        // itself across a join, but allow joins of its completed output.
        LogicalPlan::Aggregate(a) => {
            a.aggr_expr.is_empty() && a.group_expr.iter().all(pure_expr) && pure_plan(&a.input)
        }
        LogicalPlan::Join(j) => eligible(j) && pure_plan(&j.left) && pure_plan(&j.right),
        _ => false,
    }
}
fn terms(e: &Expr, out: &mut Vec<Expr>) {
    if let Expr::BinaryExpr(b) = e {
        if b.op == Operator::And {
            terms(&b.left, out);
            terms(&b.right, out);
            return;
        }
    }
    out.push(e.clone());
}
fn eligible(j: &datafusion::logical_expr::Join) -> bool {
    j.join_type == JoinType::Inner
        && !j.null_aware
        && j.null_equality == datafusion::common::NullEquality::NullEqualsNothing
        && j.on
            .iter()
            .all(|(a, b)| matches!(a, Expr::Column(_)) && matches!(b, Expr::Column(_)))
        && j.filter
            .as_ref()
            .is_none_or(|e| pure_in_schema(e, &j.schema))
}
type Bindings = BTreeMap<datafusion::common::Column, Expr>;
fn substitute(e: Expr, bindings: &Bindings) -> Result<Expr> {
    Ok(e.transform_up(|e| {
        if let Expr::Column(c) = &e {
            if let Some(replacement) = bindings.get(c) {
                return Ok(Transformed::yes(replacement.clone()));
            }
        }
        Ok(Transformed::no(e))
    })?
    .data)
}
fn contains_region(p: &LogicalPlan) -> bool {
    match p {
        LogicalPlan::Join(j) => eligible(j),
        LogicalPlan::Projection(p) => contains_region(&p.input),
        LogicalPlan::SubqueryAlias(a) => contains_region(&a.input),
        _ => false,
    }
}
fn flatten(
    p: &LogicalPlan,
    leaves: &mut Vec<LogicalPlan>,
    predicates: &mut Vec<Expr>,
) -> Result<Bindings> {
    match p {
        LogicalPlan::Join(j) if eligible(j) => {
            let mut bindings = flatten(&j.left, leaves, predicates)?;
            bindings.extend(flatten(&j.right, leaves, predicates)?);
            for (a, b) in &j.on {
                predicates.push(substitute(a.clone().eq(b.clone()), &bindings)?);
            }
            if let Some(e) = &j.filter {
                terms(&substitute(e.clone(), &bindings)?, predicates);
            }
            Ok(bindings)
        }
        LogicalPlan::Projection(proj) if contains_region(&proj.input) => {
            let bindings = flatten(&proj.input, leaves, predicates)?;
            proj.schema
                .columns()
                .into_iter()
                .zip(&proj.expr)
                .map(|(c, e)| Ok((c, substitute(e.clone().unalias(), &bindings)?)))
                .collect()
        }
        LogicalPlan::SubqueryAlias(a) if contains_region(&a.input) => {
            let bindings = flatten(&a.input, leaves, predicates)?;
            a.schema
                .columns()
                .into_iter()
                .zip(a.input.schema().columns())
                .map(|(out, input)| {
                    Ok((
                        out,
                        bindings.get(&input).cloned().unwrap_or(Expr::Column(input)),
                    ))
                })
                .collect()
        }
        _ => {
            leaves.push(p.clone());
            Ok(p.schema()
                .columns()
                .into_iter()
                .map(|c| (c.clone(), Expr::Column(c)))
                .collect())
        }
    }
}

fn join(
    left: &LogicalPlan,
    right: &LogicalPlan,
    predicates: &[Expr],
) -> Result<Option<LogicalPlan>> {
    let usable = predicates
        .iter()
        .filter(|e| {
            let cols = e.column_refs();
            !cols.is_empty()
                && cols.iter().all(|c| {
                    left.schema().index_of_column(c).is_ok()
                        || right.schema().index_of_column(c).is_ok()
                })
                && cols
                    .iter()
                    .any(|c| left.schema().index_of_column(c).is_ok())
                && cols
                    .iter()
                    .any(|c| right.schema().index_of_column(c).is_ok())
        })
        .cloned()
        .collect::<Vec<_>>();
    if usable.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        LogicalPlanBuilder::from(left.clone())
            .join_on(right.clone(), JoinType::Inner, usable)?
            .build()?,
    ))
}
fn cost(p: &LogicalPlan) -> Option<f64> {
    estimate(p).estimated_work.filter(|c| c.is_finite())
}
fn names(p: &LogicalPlan) -> Vec<String> {
    let mut names = Vec::new();
    let _ = p.apply_with_subqueries(|p| {
        if let LogicalPlan::TableScan(s) = p {
            names.push(s.table_name.to_string());
        }
        Ok::<_, datafusion::common::DataFusionError>(TreeNodeRecursion::Continue)
    });
    names
}
/// Enumerate connected left-deep orders for small regions; bounded beam for larger
/// ones. Leaf permutations compete, including both two-way build/probe orientations.
/// Keep the right input a leaf: SQL join chains cannot encode an unparenthesized
/// right-deep tree with the same ON-clause scope.
pub fn optimize(plan: LogicalPlan) -> Result<(LogicalPlan, Vec<OptimizerDecision>)> {
    let mut decisions = Vec::new();
    let plan=plan.transform_down_with_subqueries(|p| {
        let LogicalPlan::Join(j)=&p else{return Ok(Transformed::no(p));};
        if !eligible(j)||!pure_plan(&p){return Ok(Transformed::no(p));}
        let mut leaves=Vec::new();let mut predicates=Vec::new();let bindings=flatten(&p,&mut leaves,&mut predicates)?;
        if leaves.len()<2 || leaves.len()>32 {return Ok(Transformed::no(p));}
        // Every predicate must connect two leaves: don't drop local predicates or
        // ambiguous self-join names during enumeration.
        for e in &predicates {
            let cols=e.column_refs();let mut owners=std::collections::BTreeSet::new();
            for c in cols {let found=leaves.iter().enumerate().filter(|(_,l)|l.schema().index_of_column(c).is_ok()).map(|(i,_)|i).collect::<Vec<_>>();if found.len()!=1{return Ok(Transformed::no(p));}owners.insert(found[0]);}
            if owners.len()<2{return Ok(Transformed::no(p));}
        }
        let Some(before)=cost(&p) else{return Ok(Transformed::no(p));};
        let mut states:BTreeMap<u64,(f64,LogicalPlan)>=BTreeMap::new();
        for (i,l) in leaves.iter().enumerate(){let Some(c)=cost(l)else{return Ok(Transformed::no(p));};states.insert(1<<i,(c,l.clone()));}
        for _size in 2..=leaves.len(){
            let mut next:BTreeMap<u64,(f64,LogicalPlan)>=BTreeMap::new();
            for (mask,(_,prefix)) in &states {for (i,leaf) in leaves.iter().enumerate(){if mask&(1<<i)!=0 {continue;}
                for (a,b) in [(prefix,leaf)] {
                    let Some(candidate)=join(a,b,&predicates)? else{continue;};let Some(c)=cost(&candidate)else{continue;};let mask=mask|(1<<i);
                    if next.get(&mask).is_none_or(|(old,_)|c<*old){next.insert(mask,(c,candidate));}
                }
            }}
            if leaves.len()>8 && next.len()>16 {let mut ranked=next.into_iter().collect::<Vec<_>>();ranked.sort_by(|a,b|a.1.0.total_cmp(&b.1.0).then(a.0.cmp(&b.0)));ranked.truncate(16);next=ranked.into_iter().collect();}
            states=next;
        }
        let Some((after,candidate))=states.remove(&((1u64<<leaves.len())-1))else{return Ok(Transformed::no(p));};
        if after>=before {return Ok(Transformed::no(p));}
        let candidate=LogicalPlanBuilder::from(candidate).project(p.schema().columns().into_iter().map(|c|bindings.get(&c).cloned().unwrap_or_else(||Expr::Column(c.clone())).alias_qualified(c.relation,c.name)))?.build()?;
        decisions.push(OptimizerDecision{optimization:"connected_join_order".into(),before:names(&p),after:names(&candidate),estimated_work_before:before,estimated_work_after:after,reason:"connected equality region; collected cardinality/skew; right-side build cost; original output order restored".into()});
        Ok(Transformed::new(candidate,true,TreeNodeRecursion::Jump))
    })?.data;
    Ok((plan, decisions))
}

// Keep join bindings and multiplicity in the original join, but restrict a
// collection's parent relation using a neighboring binding before expansion.
fn parent_restriction(
    p: &LogicalPlan,
    keys: &[Expr],
    right: &LogicalPlan,
    right_keys: &[Expr],
    expanded: bool,
) -> Result<Option<LogicalPlan>> {
    let (input, translated, next_expanded) = match p {
        LogicalPlan::Projection(proj) => {
            let translated = keys
                .iter()
                .map(|k| {
                    let Expr::Column(c) = k else {
                        return None;
                    };
                    let i = proj.schema.index_of_column(c).ok()?;
                    let e = proj.expr[i].clone().unalias();
                    matches!(e, Expr::Column(_)).then_some(e)
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = translated else {
                return Ok(None);
            };
            (proj.input.as_ref(), keys, expanded)
        }
        LogicalPlan::SubqueryAlias(a) => {
            let translated = keys
                .iter()
                .map(|k| {
                    let Expr::Column(c) = k else {
                        return None;
                    };
                    let i = a.schema.index_of_column(c).ok()?;
                    Some(Expr::Column(a.input.schema().columns()[i].clone()))
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = translated else {
                return Ok(None);
            };
            (a.input.as_ref(), keys, expanded)
        }
        LogicalPlan::Filter(f) if pure_expr(&f.predicate) => {
            (f.input.as_ref(), keys.to_vec(), expanded)
        }
        LogicalPlan::Aggregate(a) if a.aggr_expr.is_empty() => {
            let translated = keys
                .iter()
                .map(|e| {
                    let Expr::Column(c) = e else {
                        return None;
                    };
                    let index = a.schema.index_of_column(c).ok()?;
                    let e = a.group_expr.get(index)?.clone();
                    matches!(e, Expr::Column(_)).then_some(e)
                })
                .collect::<Option<Vec<_>>>();
            let Some(keys) = translated else {
                return Ok(None);
            };
            (a.input.as_ref(), keys, expanded)
        }
        LogicalPlan::Unnest(u) => {
            if keys.iter().any(|e| {
                e.column_refs().iter().any(|c| {
                    u.input.schema().index_of_column(c).is_err() || u.exec_columns.contains(c)
                })
            }) {
                return Ok(None);
            }
            (u.input.as_ref(), keys.to_vec(), true)
        }
        LogicalPlan::TableScan(_) if expanded => {
            let predicates = keys
                .iter()
                .zip(right_keys)
                .map(|(a, b)| a.clone().eq(b.clone()))
                .collect::<Vec<_>>();
            return Ok(Some(
                LogicalPlanBuilder::from(p.clone())
                    .join_on(right.clone(), JoinType::LeftSemi, predicates)?
                    .build()?,
            ));
        }
        _ => return Ok(None),
    };
    let Some(restricted) =
        parent_restriction(input, &translated, right, right_keys, next_expanded)?
    else {
        return Ok(None);
    };
    if let LogicalPlan::Unnest(u) = p {
        let mut u = u.clone();
        u.input = std::sync::Arc::new(restricted);
        return Ok(Some(LogicalPlan::Unnest(u)));
    }
    Ok(Some(
        p.clone()
            .with_new_exprs(p.expressions(), vec![restricted])?,
    ))
}
pub fn restrict_collections(plan: LogicalPlan) -> Result<(LogicalPlan, Vec<OptimizerDecision>)> {
    let mut decisions = Vec::new();
    let plan=plan.transform_up_with_subqueries(|p|{
        let LogicalPlan::Join(j)=&p else{return Ok(Transformed::no(p));};
        if !eligible(j)||!pure_plan(&p){return Ok(Transformed::no(p));}
        let mut keys=j.on.clone();let mut predicates=Vec::new();if let Some(e)=&j.filter{terms(e,&mut predicates);}
        for e in predicates {if let Expr::BinaryExpr(b)=e {if b.op==Operator::Eq {
            if let (Expr::Column(a),Expr::Column(c))=(b.left.as_ref(),b.right.as_ref()) {
                if j.left.schema().index_of_column(a).is_ok()&&j.right.schema().index_of_column(c).is_ok(){keys.push((*b.left,*b.right));}
                else if j.right.schema().index_of_column(a).is_ok()&&j.left.schema().index_of_column(c).is_ok(){keys.push((*b.right,*b.left));}
            }
        }}}
        if keys.is_empty(){return Ok(Transformed::no(p));}
        let Some(before)=cost(&p)else{return Ok(Transformed::no(p));};let mut best=p.clone();let mut best_cost=before;
        for reverse in [false,true]{
            let (left,right,lk,rk)=if reverse {(&j.right,&j.left,keys.iter().map(|(_,b)|b.clone()).collect::<Vec<_>>(),keys.iter().map(|(a,_)|a.clone()).collect::<Vec<_>>())}else{(&j.left,&j.right,keys.iter().map(|(a,_)|a.clone()).collect::<Vec<_>>(),keys.iter().map(|(_,b)|b.clone()).collect::<Vec<_>>())};
            if let Some(restricted)=parent_restriction(left,&lk,right,&rk,false)?{
                let mut candidate=j.clone();if reverse{candidate.right=std::sync::Arc::new(restricted);}else{candidate.left=std::sync::Arc::new(restricted);}
                let candidate=LogicalPlan::Join(candidate);if let Some(c)=cost(&candidate){if c<best_cost {best_cost=c;best=candidate;}}
            }
        }
        if best_cost<before {decisions.push(OptimizerDecision{optimization:"restrict_collection_parents".into(),before:names(&p),after:names(&best),estimated_work_before:before,estimated_work_after:best_cost,reason:"neighbor key relation becomes a parent semijoin before UNNEST; original binding join and duplicates retained; keys remain runtime values".into()});Ok(Transformed::yes(best))}else{Ok(Transformed::no(p))}
    })?.data;
    Ok((plan, decisions))
}
