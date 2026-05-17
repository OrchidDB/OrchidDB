//! `math(expr)` — arithmetic combinations of the current scalar and
//! optional named bindings. The IR lowering builds a binary expression
//! over the current scalar; named bindings are resolved at evaluation
//! time. Anything more elaborate than the documented `_` placeholder
//! shapes degenerates to identity (the AST-level `MathExpr::Identity`).

use std::iter::Peekable;

use super::context::{CURRENT, Lowerer, TraversalContext};
use super::helpers::{apply_by_spec, consume_by};
use super::literals::gvalue_to_expr;
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
            math_bin_expr(
                *op,
                IrExpr::property(lhs.clone(), key.clone(), PropertyMissing::DropUnproductive),
                IrExpr::property(rhs.clone(), key.clone(), PropertyMissing::DropUnproductive),
            ),
        ));
    }
    if let (MathExpr::BothNamed(op, _lhs, rhs), Some(first_by)) = (expr, by.as_ref()) {
        let second_by = consume_by(steps);
        let (input, lhs_expr) = apply_by_spec(input, first_by, lo, ctx)?;
        let rhs_expr = match second_by.as_ref().and_then(|spec| spec.key.as_ref()) {
            Some(key) if key == "id" => IrExpr::Call {
                name: "gremlin_id".into(),
                args: vec![IrExpr::Binding(rhs.clone())],
            },
            Some(key) if key == "label" => IrExpr::Label(rhs.clone()),
            Some(key) => {
                IrExpr::property(rhs.clone(), key.clone(), PropertyMissing::DropUnproductive)
            }
            _ => named_operand_expr(rhs, lo),
        };
        return Ok(project_expr(input, math_bin_expr(*op, lhs_expr, rhs_expr)));
    }
    let input = match by.as_ref() {
        Some(spec) if !matches!(expr, MathExpr::BothNamed(..)) => {
            let (input, by_expr) = apply_by_spec(input, spec, lo, ctx)?;
            return Ok(lower_math_expr(input, expr, by_expr, lo, true));
        }
        _ => input,
    };
    Ok(lower_math_expr(
        input,
        expr,
        IrExpr::Binding(CURRENT.into()),
        lo,
        false,
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

fn math_bin_expr(op: MathOp, lhs: IrExpr, rhs: IrExpr) -> IrExpr {
    IrExpr::Call {
        name: "gremlin_math_bin".into(),
        args: vec![
            IrExpr::lit_str(match op {
                MathOp::Add => "add",
                MathOp::Sub => "sub",
                MathOp::Mul => "mul",
                MathOp::Div => "div",
            }),
            lhs,
            rhs,
        ],
    }
}

fn lower_math_expr(
    input: Node,
    expr: &MathExpr,
    current: IrExpr,
    lo: &Lowerer,
    prefer_current_for_named: bool,
) -> Node {
    let projected = |body: IrExpr| -> Node {
        Node::GraphCurrentProject {
            expr: body,
            fields: vec![CURRENT.to_string()],
            input: input.clone().boxed(),
        }
    };
    let cur = || current.clone();
    let named = |name: &String| -> IrExpr {
        if prefer_current_for_named {
            return cur();
        }
        named_operand_expr(name, lo)
    };
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
        MathExpr::BinSelf(op) => projected(math_bin_expr(*op, cur(), cur())),
        MathExpr::SelfRhsName(op, name) => projected(math_bin_expr(*op, cur(), named(name))),
        MathExpr::SelfLhsName(op, name) => projected(math_bin_expr(*op, named(name), cur())),
        MathExpr::BothNamed(op, lhs, rhs) => projected(math_bin_expr(
            *op,
            named_operand_expr(lhs, lo),
            named_operand_expr(rhs, lo),
        )),
        MathExpr::NameRhsLit(op, name, value) => {
            projected(math_bin_expr(*op, named(name), lit(*value)))
        }
        MathExpr::LitRhsName(op, value, name) => {
            projected(math_bin_expr(*op, lit(*value), named(name)))
        }
        MathExpr::UnaryFn(name) => projected(IrExpr::Call {
            name: name.clone(),
            args: vec![cur()],
        }),
        MathExpr::UnaryCurrentOpLit { func, op, value } => projected(IrExpr::Call {
            name: func.clone(),
            args: vec![math_bin_expr(*op, cur(), lit(*value))],
        }),
        MathExpr::Var(name) => projected(named_operand_expr(name, lo)),
        MathExpr::Identity => input,
    }
}

fn named_operand_expr(name: &str, lo: &Lowerer) -> IrExpr {
    if let Some(seed) = lo.side_effect_seeds.get(name) {
        if let Ok(expr) = gvalue_to_expr(seed) {
            return expr;
        }
    }
    IrExpr::Call {
        name: "select_key_or_binding".into(),
        args: vec![
            IrExpr::Binding(CURRENT.into()),
            IrExpr::Binding(name.to_string()),
            IrExpr::lit_str(name.to_string()),
        ],
    }
}
