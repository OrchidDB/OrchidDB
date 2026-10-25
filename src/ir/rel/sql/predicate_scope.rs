//! Move total predicates through transparent scopes, before SQL boundaries
//! obscure source constants. Never move computations across LIMIT or windows.
use super::*;
use datafusion::logical_expr::{Filter, Projection, LogicalPlanBuilder, Operator, JoinType, ExprSchemable};

fn value(e: &Expr) -> bool {
    match e { Expr::Column(_) | Expr::Literal(..) => true, Expr::Alias(a) => value(&a.expr),
        Expr::Cast(c) => matches!(c.expr.as_ref(), Expr::Literal(v, _) if v.is_null()), _ => false }
}
fn total(e: &Expr, schema: &datafusion::common::DFSchema) -> bool {
    match e {
        Expr::Column(_) | Expr::Literal(..) => true,
        Expr::Alias(a) => total(&a.expr, schema),
        Expr::IsNull(e) | Expr::IsNotNull(e) | Expr::Not(e) => total(e, schema),
        Expr::BinaryExpr(b) if matches!(b.op, Operator::Eq | Operator::NotEq | Operator::Lt | Operator::LtEq | Operator::Gt | Operator::GtEq | Operator::And | Operator::Or) =>
            b.left.get_type(schema).ok().is_some_and(|t| Some(t) == b.right.get_type(schema).ok())
                && total(&b.left, schema) && total(&b.right, schema),
        _ => false,
    }
}
fn unalias(e: Expr) -> Expr {
    match e {
        Expr::Alias(a) => unalias(*a.expr),
        Expr::Cast(ref c) if matches!(c.expr.as_ref(), Expr::Literal(v, _) if v.is_null()) =>
            ScalarValue::try_from(&c.data_type).map(lit).unwrap_or(e),
        e => e,
    }
}
fn substitute(e: Expr, p: &Projection) -> datafusion::common::Result<Expr> {
    Ok(e.transform_up(|e| {
        if let Expr::Column(c) = &e {
            return Ok(Transformed::yes(unalias(p.expr[p.schema.index_of_column(c)?].clone())));
        }
        Ok(Transformed::no(e))
    })?.data)
}
fn split(e: Expr, out: &mut Vec<Expr>) {
    if let Expr::BinaryExpr(b) = &e && b.op == Operator::And {
        split(*b.left.clone(), out); split(*b.right.clone(), out);
    } else { out.push(e); }
}
fn filtered(input: LogicalPlan, predicate: Expr) -> datafusion::common::Result<LogicalPlan> {
    if matches!(&predicate,Expr::Literal(ScalarValue::Boolean(Some(true)),_)) { return Ok(input); }
    LogicalPlanBuilder::from(input).filter(predicate)?.build()
}
fn literals(e: Expr) -> datafusion::common::Result<Expr> {
    Ok(e.transform_up(|e| {
        match &e {
            Expr::IsNull(inner) | Expr::IsNotNull(inner) => {
                if let Expr::Literal(v, _) = inner.as_ref() {
                    return Ok(Transformed::yes(lit(if matches!(e, Expr::IsNull(_)) { v.is_null() } else { !v.is_null() })));
                }
            }
            Expr::Not(inner) => {
                if let Expr::Literal(ScalarValue::Boolean(v), _) = inner.as_ref() {
                    return Ok(Transformed::yes(lit(ScalarValue::Boolean(v.map(|v| !v)))));
                }
            }
            _ => {}
        }
        if let Expr::BinaryExpr(b) = &e {
            if let (Expr::Literal(a,_),Expr::Literal(c,_)) = (b.left.as_ref(),b.right.as_ref())
                && a.data_type() == c.data_type()
                && matches!(a.data_type(),DataType::Boolean | DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64
                    | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)
                && matches!(b.op,Operator::Eq|Operator::NotEq) {
                let v = if a.is_null() || c.is_null() { None } else { Some(if b.op==Operator::Eq {a==c} else {a!=c}) };
                return Ok(Transformed::yes(lit(ScalarValue::Boolean(v))));
            }
            if matches!(b.op,Operator::And|Operator::Or) {
                for (a,c) in [(&b.left,&b.right),(&b.right,&b.left)] {
                    if let Expr::Literal(ScalarValue::Boolean(Some(v)),_) = a.as_ref() {
                        let absorbing = (b.op==Operator::And && !v) || (b.op==Operator::Or && *v);
                        return Ok(Transformed::yes(if absorbing {lit(*v)} else {c.as_ref().clone()}));
                    }
                }
            }
        }
        Ok(Transformed::no(e))
    })?.data)
}

fn rewrite_node(node: LogicalPlan) -> datafusion::common::Result<Transformed<LogicalPlan>> {
    let LogicalPlan::Filter(f) = &node else { return Ok(Transformed::no(node)); };
    if !total(&f.predicate, f.input.schema()) { return Ok(Transformed::no(node)); }
    let bound = f.predicate.clone().transform_up(|e| {
        if let Expr::Column(c)=&e && let Some(v)=constant(&f.input,c) {
            return Ok(Transformed::yes(lit(v)));
        }
        Ok(Transformed::no(e))
    })?.data;
    let simplified = literals(bound)?;
    if simplified != f.predicate {
        return Ok(Transformed::yes(filtered(f.input.as_ref().clone(), simplified)?));
    }
    match f.input.as_ref() {
        LogicalPlan::Projection(p) if p.expr.iter().all(value) => {
            let predicate = substitute(f.predicate.clone(), p)?;
            let input = filtered(p.input.as_ref().clone(), predicate)?;
            return Ok(Transformed::yes(LogicalPlan::Projection(Projection::try_new_with_schema(p.expr.clone(), Arc::new(input), p.schema.clone())?)));
        }
        LogicalPlan::Filter(inner) if total(&inner.predicate, inner.input.schema()) => {
            return Ok(Transformed::yes(LogicalPlan::Filter(Filter::try_new(inner.predicate.clone().and(f.predicate.clone()),inner.input.clone())?)));
        }
        LogicalPlan::SubqueryAlias(a) => {
            let predicate = f.predicate.clone().transform_up(|e| {
                if let Expr::Column(c) = &e {
                    return Ok(Transformed::yes(Expr::Column(a.input.schema().columns()[a.schema.index_of_column(c)?].clone())));
                }
                Ok(Transformed::no(e))
            })?.data;
            let input = filtered(a.input.as_ref().clone(), predicate)?;
            return Ok(Transformed::yes(LogicalPlanBuilder::from(input).alias(a.alias.clone())?.build()?));
        }
        LogicalPlan::Union(u) => {
            let mut inputs = Vec::new();
            for input in &u.inputs {
                let predicate = f.predicate.clone().transform_up(|e| {
                    if let Expr::Column(c) = &e {
                        return Ok(Transformed::yes(Expr::Column(input.schema().columns()[u.schema.index_of_column(c)?].clone())));
                    }
                    Ok(Transformed::no(e))
                })?.data;
                inputs.push(Arc::new(filtered(input.as_ref().clone(),predicate)?));
            }
            let mut u = u.clone(); u.inputs = inputs;
            return Ok(Transformed::yes(LogicalPlan::Union(u)));
        }
        LogicalPlan::Join(j) if j.join_type == JoinType::Inner => {
            let mut parts = Vec::new(); split(f.predicate.clone(), &mut parts);
            let mut left = Vec::new(); let mut right = Vec::new(); let mut retained = Vec::new();
            for p in parts {
                let refs = p.column_refs();
                if !refs.is_empty() && refs.iter().all(|c| j.left.schema().index_of_column(c).is_ok() && j.right.schema().index_of_column(c).is_err()) { left.push(p); }
                else if !refs.is_empty() && refs.iter().all(|c| j.right.schema().index_of_column(c).is_ok() && j.left.schema().index_of_column(c).is_err()) { right.push(p); }
                else { retained.push(p); }
            }
            if !left.is_empty() || !right.is_empty() {
                // Do not suppress errors/volatile computation in the opposite
                // input by filtering an inner join earlier.
                if !crate::ir::rel::constraints::analyze(&j.left).removable || !crate::ir::rel::constraints::analyze(&j.right).removable { return Ok(Transformed::no(node)); }
                let mut j = j.clone();
                if let Some(p) = left.into_iter().reduce(|a,b| a.and(b)) { j.left = Arc::new(filtered(j.left.as_ref().clone(),p)?); }
                if let Some(p) = right.into_iter().reduce(|a,b| a.and(b)) { j.right = Arc::new(filtered(j.right.as_ref().clone(),p)?); }
                let mut result = LogicalPlan::Join(j);
                if let Some(p) = retained.into_iter().reduce(|a,b| a.and(b)) { result = filtered(result,p)?; }
                return Ok(Transformed::yes(result));
            }
        }
        _ => {},
    }
    Ok(Transformed::no(node))
}
fn constant(plan: &LogicalPlan, c: &Column) -> Option<ScalarValue> {
    let i=plan.schema().index_of_column(c).ok()?;
    match plan {
        LogicalPlan::Projection(p) => match unalias(p.expr[i].clone()) {
            Expr::Literal(v,_) => Some(v),
            Expr::Column(c) => constant(&p.input,&c),
            _ => None,
        },
        LogicalPlan::SubqueryAlias(a) => constant(&a.input,&a.input.schema().columns()[i]),
        LogicalPlan::Filter(f) => constant(&f.input,c),
        LogicalPlan::Join(j) if j.join_type==JoinType::Inner => {
            let left=j.left.schema().index_of_column(c).is_ok();
            let right=j.right.schema().index_of_column(c).is_ok();
            match (left,right) { (true,false)=>constant(&j.left,c), (false,true)=>constant(&j.right,c), _=>None }
        },
        LogicalPlan::Union(u) => {
            let first=u.inputs.first()?;
            let v=constant(first,&first.schema().columns()[i])?;
            u.inputs.iter().all(|p|constant(p,&p.schema().columns()[i]).as_ref()==Some(&v)).then_some(v)
        },
        _ => None,
    }
}
fn specialize(node: LogicalPlan) -> datafusion::common::Result<Transformed<LogicalPlan>> {
    let LogicalPlan::Join(j)=&node else {return Ok(Transformed::no(node));};
    if j.join_type!=JoinType::Inner
        || !crate::ir::rel::constraints::analyze(&j.left).removable
        || !crate::ir::rel::constraints::analyze(&j.right).removable {return Ok(Transformed::no(node));}
    let mut terms=Vec::new();
    for (a,b) in &j.on { terms.push(a.clone().eq(b.clone())); }
    if let Some(p)=&j.filter {split(p.clone(),&mut terms);}
    let mut l=Vec::new();let mut r=Vec::new();
    for term in terms {
        let Expr::BinaryExpr(b)=term else {continue;};
        if b.op!=Operator::Eq {continue;}
        let (Expr::Column(a),Expr::Column(b))=(*b.left,*b.right) else {continue;};
        for (a,b) in [(&a,&b),(&b,&a)] {
            if let Some(v)=constant(&j.left,a) && !v.is_null()
                && j.right.schema().index_of_column(b).is_ok()
                && constant(&j.right,b).is_none()
                && Expr::Column(b.clone()).get_type(j.right.schema())?==v.data_type() {
                r.push(Expr::Column(b.clone()).eq(lit(v)));
            }
            if let Some(v)=constant(&j.right,a) && !v.is_null()
                && j.left.schema().index_of_column(b).is_ok()
                && constant(&j.left,b).is_none()
                && Expr::Column(b.clone()).get_type(j.left.schema())?==v.data_type() {
                l.push(Expr::Column(b.clone()).eq(lit(v)));
            }
        }
    }
    if l.is_empty() && r.is_empty() {return Ok(Transformed::no(node));}
    let mut j=j.clone();
    if let Some(p)=l.into_iter().reduce(|a,b|a.and(b)) {j.left=Arc::new(filtered(j.left.as_ref().clone(),p)?);}
    if let Some(p)=r.into_iter().reduce(|a,b|a.and(b)) {j.right=Arc::new(filtered(j.right.as_ref().clone(),p)?);}
    Ok(Transformed::yes(LogicalPlan::Join(j)))
}

pub(super) fn rewrite(mut plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    plan = plan.transform_up_with_subqueries(specialize)?.data;
    // Each iteration moves filters strictly toward leaves. Bound work on very
    // deep user plans; remaining filters are still semantically identical.
    for _ in 0..16 {
        let result = plan.transform_up_with_subqueries(rewrite_node)?;
        plan = result.data;
        if !result.transformed { break; }
    }
    Ok(crate::ir::rel::empty_relations::simplify(plan)
        .map_err(|error| SqlError::Unsupported(error.to_string()))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_branch_specialization_keeps_null_semantics() {
        assert_eq!(literals(lit("A").eq(lit("B"))).unwrap(),lit(false));
        assert_eq!(literals(lit(ScalarValue::Utf8(None)).eq(lit("B"))).unwrap(),lit(ScalarValue::Boolean(None)));
    }
    #[test]
    fn filter_crosses_renames_but_not_limits() {
        let base=LogicalPlanBuilder::empty(true).project(vec![lit(1_i64).alias("x")]).unwrap().build().unwrap();
        let plan=LogicalPlanBuilder::from(base.clone()).project(vec![datafusion::prelude::col("x").alias("y")]).unwrap()
            .filter(datafusion::prelude::col("y").eq(lit(1_i64))).unwrap().build().unwrap();
        let optimized=rewrite(plan).unwrap();
        assert!(matches!(optimized,LogicalPlan::Projection(_)));
        let plan=LogicalPlanBuilder::from(base).limit(0,Some(1)).unwrap()
            .filter(datafusion::prelude::col("x").eq(lit(1_i64))).unwrap().build().unwrap();
        assert_eq!(rewrite(plan.clone()).unwrap(),plan);
    }
}
