use spargebra::algebra::{Expression, Function};

use crate::ir::expr::{BinaryOp, IrExpr, Lit};

use super::terms::{binding, literal};

/// Typed RDF constants in SPARQL expressions. A plain `Lit::String` is always
/// a simple literal (`xsd:string`); IRIs and other literals keep their kind,
/// lexical form, datatype, and language through these calls.
pub(crate) const SPARQL_IRI: &str = "sparql_iri";
pub(crate) const SPARQL_LITERAL: &str = "sparql_literal";
pub(crate) const SPARQL_LANG_LITERAL: &str = "sparql_lang_literal";
/// Nested `EXISTS` that is not a top-level filter conjunct. It is not
/// executable yet; relational lowering declines it.
pub(crate) const SPARQL_NESTED_EXISTS: &str = "sparql_exists";

/// Legacy scalar lowering used with ontology mappings, where RDF terms are
/// represented by mapped property-graph values rather than RDF terms.
pub(crate) fn lower(expression: &Expression) -> IrExpr {
    lower_with(expression, false, None)
}

/// Lowering that preserves RDF term identity for dataset-backed queries.
pub(crate) fn lower_typed(expression: &Expression, base: Option<&str>) -> IrExpr {
    lower_with(expression, true, base)
}

fn lower_with(expression: &Expression, typed: bool, base: Option<&str>) -> IrExpr {
    let lower = |expression: &Expression| lower_with(expression, typed, base);
    let binary = |op: BinaryOp, lhs: &Expression, rhs: &Expression| IrExpr::Binary {
        op,
        lhs: Box::new(lower(lhs)),
        rhs: Box::new(lower(rhs)),
    };
    match expression {
        Expression::NamedNode(value) if typed => {
            call(SPARQL_IRI, vec![IrExpr::lit_str(value.as_str())])
        }
        Expression::NamedNode(value) => IrExpr::Lit(Lit::String(value.as_str().into())),
        Expression::Literal(value) if typed => typed_literal(value),
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
        Expression::Exists(pattern) => call(
            SPARQL_NESTED_EXISTS,
            vec![IrExpr::lit_str(pattern.to_string())],
        ),
        Expression::Bound(variable) => IrExpr::IsBound(binding(variable)),
        Expression::If(condition, yes, no) => IrExpr::Case {
            arms: vec![(lower(condition), lower(yes))],
            otherwise: Some(Box::new(lower(no))),
        },
        Expression::Coalesce(values) => call("sparql_coalesce", values.iter().map(lower).collect()),
        // Custom functions (including XSD constructor casts) are named by
        // IRI, which is case-sensitive.
        Expression::FunctionCall(Function::Custom(iri), args) if typed => {
            call(iri.as_str(), args.iter().map(lower).collect())
        }
        Expression::FunctionCall(Function::Iri, args) if typed => {
            let mut args: Vec<_> = args.iter().map(lower).collect();
            args.push(IrExpr::lit_str(base.unwrap_or("")));
            call("sparql_resolve_iri", args)
        }
        Expression::FunctionCall(function, args) => call(
            &function.to_string().to_ascii_lowercase(),
            args.iter().map(lower).collect(),
        ),
    }
}

fn typed_literal(value: &spargebra::term::Literal) -> IrExpr {
    if let Some(language) = value.language() {
        return call(
            SPARQL_LANG_LITERAL,
            vec![IrExpr::lit_str(value.value()), IrExpr::lit_str(language)],
        );
    }
    let datatype = value.datatype().as_str();
    if datatype == "http://www.w3.org/2001/XMLSchema#string" {
        return IrExpr::lit_str(value.value());
    }
    call(
        SPARQL_LITERAL,
        vec![IrExpr::lit_str(value.value()), IrExpr::lit_str(datatype)],
    )
}

fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        name: name.into(),
        args,
    }
}
