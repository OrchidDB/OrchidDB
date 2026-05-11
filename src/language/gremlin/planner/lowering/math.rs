//! `math(expr)` — arithmetic combinations of the current scalar and
//! optional named bindings. The IR lowering builds a binary expression
//! over the current scalar; named bindings are resolved at evaluation
//! time. Anything more elaborate than the documented `_` placeholder
//! shapes degenerates to identity (the AST-level `MathExpr::Identity`).

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, consume_by};
use crate::ir::expr::{BinaryOp, IrExpr, Lit};
use crate::ir::plan::Node;
use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::{MathExpr, MathOp, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn lower_math<'a, I>(
    input: Node,
    expr: &MathExpr,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let by = consume_by(steps);
    if let (
        MathExpr::BothNamed(op, lhs, rhs),
        Some(crate::language::gremlin::ast::BySpec {
            key: Some(key),
            traversal: None,
            ..
        }),
    ) = (expr, by.as_ref())
    {
        return Ok(project_expr(
            input,
            IrExpr::Binary {
                op: bin_op(*op),
                lhs: Box::new(IrExpr::property(
                    lhs.clone(),
                    key.clone(),
                    PropertyMissing::DropUnproductive,
                )),
                rhs: Box::new(IrExpr::property(
                    rhs.clone(),
                    key.clone(),
                    PropertyMissing::DropUnproductive,
                )),
            },
        ));
    }
    let input = match by.as_ref() {
        Some(spec) if !matches!(expr, MathExpr::BothNamed(..)) => {
            let (input, by_expr) = apply_by_spec(input, spec, lo, ctx)?;
            return Ok(lower_math_expr(input, expr, by_expr));
        }
        _ => input,
    };
    Ok(lower_math_expr(
        input,
        expr,
        IrExpr::Binding(CURRENT.into()),
    ))
}

fn project_expr(input: Node, expr: IrExpr) -> Node {
    Node::GraphCurrentProject {
        expr,
        fields: vec![CURRENT.to_string()],
        input: input.boxed(),
    }
}

fn bin_op(op: MathOp) -> BinaryOp {
    match op {
        MathOp::Add => BinaryOp::Add,
        MathOp::Sub => BinaryOp::Sub,
        MathOp::Mul => BinaryOp::Mul,
        MathOp::Div => BinaryOp::Div,
    }
}

fn lower_math_expr(input: Node, expr: &MathExpr, current: IrExpr) -> Node {
    let projected = |body: IrExpr| -> Node {
        Node::GraphCurrentProject {
            expr: body,
            fields: vec![CURRENT.to_string()],
            input: input.clone().boxed(),
        }
    };
    let cur = || current.clone();
    let lit = |x: f64| IrExpr::Lit(Lit::Float(x));
    let bin = |op: BinaryOp, lhs: IrExpr, rhs: IrExpr| -> IrExpr {
        IrExpr::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    };
    match expr {
        MathExpr::Add(x) => projected(bin(BinaryOp::Add, cur(), lit(*x))),
        MathExpr::Sub(x) => projected(bin(BinaryOp::Sub, cur(), lit(*x))),
        MathExpr::Mul(x) => projected(bin(BinaryOp::Mul, cur(), lit(*x))),
        MathExpr::Div(x) => projected(bin(BinaryOp::Div, cur(), lit(*x))),
        MathExpr::SubFromLit(x) => projected(bin(BinaryOp::Sub, lit(*x), cur())),
        MathExpr::DivByLit(x) => projected(bin(BinaryOp::Div, lit(*x), cur())),
        MathExpr::BinSelf(op) => projected(bin(bin_op(*op), cur(), cur())),
        MathExpr::SelfRhsName(op, name) => {
            projected(bin(bin_op(*op), cur(), IrExpr::Binding(name.clone())))
        }
        MathExpr::SelfLhsName(op, name) => {
            projected(bin(bin_op(*op), IrExpr::Binding(name.clone()), cur()))
        }
        MathExpr::BothNamed(op, lhs, rhs) => projected(bin(
            bin_op(*op),
            IrExpr::Binding(lhs.clone()),
            IrExpr::Binding(rhs.clone()),
        )),
        MathExpr::Var(name) => projected(IrExpr::Binding(name.clone())),
        MathExpr::Identity => input,
    }
}
