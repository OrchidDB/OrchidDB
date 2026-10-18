//! Remove OPTIONAL machinery when a nonnull match identity decides presence.
use super::*;
use datafusion::common::{DFSchema, tree_node::Transformed};
use datafusion::logical_expr::{JoinType, Projection, SubqueryAlias};
use datafusion::logical_expr::ExprSchemable;

fn total(expr: &Expr, schema: &DFSchema) -> bool {
    match expr {
        Expr::Column(_) | Expr::Literal(..) => true,
        Expr::Alias(a) => total(&a.expr,schema),
        Expr::Cast(c) => c.expr.get_type(schema).is_ok_and(|ty| ty == c.data_type
            || (c.data_type == DataType::Int64 && matches!(ty, DataType::Int8 | DataType::Int16 | DataType::Int32))) && total(&c.expr, schema),
        Expr::BinaryExpr(b) if matches!(b.op, datafusion::logical_expr::Operator::Eq |
            datafusion::logical_expr::Operator::NotEq | datafusion::logical_expr::Operator::And |
            datafusion::logical_expr::Operator::Or) => total(&b.left,schema) && total(&b.right,schema),
        Expr::IsNull(e) | Expr::IsNotNull(e) => total(e,schema),
        _ => false,
    }
}
fn safe(plan: &LogicalPlan) -> bool {
    match plan {
        LogicalPlan::TableScan(s) => s.filters.is_empty(),
        LogicalPlan::Projection(p) => p.expr.iter().all(|e|total(e,p.input.schema())) && safe(&p.input),
        LogicalPlan::SubqueryAlias(a) => safe(&a.input),
        LogicalPlan::Filter(f) => total(&f.predicate,f.input.schema()) && safe(&f.input),
        LogicalPlan::Union(u) => u.inputs.iter().all(|p|safe(p)),
        LogicalPlan::Join(j) => j.join_type==JoinType::Inner && safe(&j.left) && safe(&j.right)
            && j.on.iter().all(|(a,b)|total(a,j.left.schema()) && total(b,j.right.schema()))
            && j.filter.as_ref().is_none_or(|e|total(e,j.schema.as_ref())),
        _ => false,
    }
}
fn column(expr: &Expr) -> Option<&Column> {
    match expr { Expr::Column(c)=>Some(c), Expr::Alias(a)=>column(&a.expr), _=>None }
}
fn simplify(plan: &LogicalPlan, name: &Column, absent: bool) -> datafusion::common::Result<Option<LogicalPlan>> {
    match plan {
        LogicalPlan::Projection(p) if p.expr.iter().all(|e|matches!(e,Expr::Column(_) | Expr::Alias(_) | Expr::Literal(..))) => {
            // Only pass a presence test through a pure rename, preserving all
            // other expressions' evaluation (in particular no computed aliases).
            if !p.expr.iter().all(|e|total(e,p.input.schema())) {return Ok(None);}
            let index=p.schema.index_of_column(name)?;
            let Some(source)=column(&p.expr[index]) else {return Ok(None);};
            Ok(simplify(&p.input,source,absent)?.map(|input|
                Projection::try_new_with_schema(p.expr.clone(),Arc::new(input),p.schema.clone()).map(LogicalPlan::Projection)).transpose()?)
        }
        LogicalPlan::SubqueryAlias(a) => {
            let index=a.schema.index_of_column(name)?;
            let source=&a.input.schema().columns()[index];
            Ok(simplify(&a.input,source,absent)?.map(|input|
                SubqueryAlias::try_new(Arc::new(input),a.alias.clone()).map(LogicalPlan::SubqueryAlias)).transpose()?)
        }
        LogicalPlan::Join(j) if j.join_type==JoinType::Left && safe(&j.right) => {
            // A nullable property cannot prove that no relationship matched.
            if j.left.schema().index_of_column(name).is_ok() {return Ok(None);}
            let Ok(index)=j.right.schema().index_of_column(name) else {return Ok(None);};
            let nonnull = !j.right.schema().field(index).is_nullable()
                || crate::ir::rel::constraints::analyze(&j.right).non_null.contains(&index);
            if !absent {
                let mut joined=j.clone(); joined.join_type=JoinType::Inner;
                let joined=LogicalPlan::Join(joined);
                // Even a nullable right property rejects the null-extended
                // row. Keep its filter unless a non-null fact proves it redundant.
                return Ok(Some(if nonnull { joined } else {
                    LogicalPlan::Filter(datafusion::logical_expr::Filter::try_new(
                        Expr::Column(name.clone()).is_not_null(),Arc::new(joined))?)
                }));
            }
            if !nonnull {return Ok(None);}
            let mut joined=j.clone(); joined.join_type=JoinType::LeftAnti;
            joined.schema=j.left.schema().clone();
            let input=LogicalPlan::Join(joined);
            let exprs=j.schema.iter().map(|(qualifier,field)| {
                let c=Column::new(qualifier.cloned(),field.name());
                let value=if j.left.schema().index_of_column(&c).is_ok() {Expr::Column(c)}
                    else {lit(ScalarValue::try_from(field.data_type())?)};
                Ok(value.alias_qualified(qualifier.cloned(),field.name()))
            }).collect::<datafusion::common::Result<Vec<_>>>()?;
            Ok(Some(LogicalPlan::Projection(Projection::try_new_with_schema(exprs,Arc::new(input),j.schema.clone())?)))
        }
        _=>Ok(None),
    }
}
pub(super) fn rewrite(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        if let LogicalPlan::Filter(f)=&node {
            let (expr,absent)=match &f.predicate {Expr::IsNull(e)=>(e,true),Expr::IsNotNull(e)=>(e,false),_=>return Ok(Transformed::no(node))};
            if let Some(c)=column(expr) && let Some(plan)=simplify(&f.input,c,absent)? {
                return Ok(Transformed::yes(plan));
            }
        }
        Ok(Transformed::no(node))
    })?.data)
}
