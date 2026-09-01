use spargebra::algebra::Expression;

use crate::ir::expr::{BinaryOp, IrExpr, Lit};

use super::terms::{binding, literal};

pub(crate) fn lower(expression: &Expression) -> IrExpr {
    match expression {
        Expression::NamedNode(value) => IrExpr::Lit(Lit::String(value.as_str().into())),
        Expression::Literal(value) => match literal(value) {
            crate::ir::plan::RdfTerm::Literal(value) => IrExpr::Lit(value),
            other => IrExpr::Lit(Lit::String(format!("{other:?}"))),
        },
        Expression::Variable(value) => IrExpr::Binding(binding(value)),
        Expression::Or(a, b) => binary(BinaryOp::Or, a, b),
        Expression::And(a, b) => binary(BinaryOp::And, a, b),
        Expression::Equal(a, b) => binary(BinaryOp::Eq, a, b),
        Expression::Greater(a, b) => binary(BinaryOp::Gt, a, b),
        Expression::GreaterOrEqual(a, b) => binary(BinaryOp::Gte, a, b),
        Expression::Less(a, b) => binary(BinaryOp::Lt, a, b),
        Expression::LessOrEqual(a, b) => binary(BinaryOp::Lte, a, b),
        Expression::Add(a, b) => binary(BinaryOp::Add, a, b),
        Expression::Subtract(a, b) => binary(BinaryOp::Sub, a, b),
        Expression::Multiply(a, b) => binary(BinaryOp::Mul, a, b),
        Expression::Divide(a, b) => binary(BinaryOp::Div, a, b),
        Expression::SameTerm(a, b) => call("sparql_same_term", vec![lower(a), lower(b)]),
        Expression::In(value, choices) => {
            let mut args = vec![lower(value)];
            args.extend(choices.iter().map(lower));
            call("sparql_in", args)
        }
        Expression::UnaryPlus(value) => call("sparql_unary_plus", vec![lower(value)]),
        Expression::UnaryMinus(value) => call("sparql_unary_minus", vec![lower(value)]),
        Expression::Not(value) => IrExpr::Not(Box::new(lower(value))),
        Expression::Exists(pattern) => {
            call("sparql_exists", vec![IrExpr::lit_str(pattern.to_string())])
        }
        Expression::Bound(variable) => IrExpr::IsBound(binding(variable)),
        Expression::If(condition, yes, no) => IrExpr::Case {
            arms: vec![(lower(condition), lower(yes))],
            otherwise: Some(Box::new(lower(no))),
        },
        Expression::Coalesce(values) => call("sparql_coalesce", values.iter().map(lower).collect()),
        Expression::FunctionCall(function, args) => call(
            &function.to_string().to_ascii_lowercase(),
            args.iter().map(lower).collect(),
        ),
    }
}

fn binary(op: BinaryOp, lhs: &Expression, rhs: &Expression) -> IrExpr {
    IrExpr::Binary {
        op,
        lhs: Box::new(lower(lhs)),
        rhs: Box::new(lower(rhs)),
    }
}

fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        name: name.into(),
        args,
    }
}
