use spargebra::algebra::PropertyPathExpression;

use crate::ir::plan::{RdfPathExpr, ZeroLengthPolicy};

pub(crate) fn lower(path: &PropertyPathExpression) -> RdfPathExpr {
    match path {
        PropertyPathExpression::NamedNode(value) => RdfPathExpr::Iri(value.as_str().into()),
        PropertyPathExpression::Reverse(value) => RdfPathExpr::Inverse(Box::new(lower(value))),
        PropertyPathExpression::Sequence(a, b) => RdfPathExpr::Sequence(vec![lower(a), lower(b)]),
        PropertyPathExpression::Alternative(a, b) => {
            RdfPathExpr::Alternative(vec![lower(a), lower(b)])
        }
        PropertyPathExpression::ZeroOrMore(value) => {
            RdfPathExpr::ZeroOrMore(Box::new(lower(value)))
        }
        PropertyPathExpression::OneOrMore(value) => RdfPathExpr::OneOrMore(Box::new(lower(value))),
        PropertyPathExpression::ZeroOrOne(value) => RdfPathExpr::ZeroOrOne(Box::new(lower(value))),
        PropertyPathExpression::NegatedPropertySet(values) => {
            let alternatives = values
                .iter()
                .map(|value| RdfPathExpr::Iri(value.as_str().into()))
                .collect();
            RdfPathExpr::Negated(Box::new(RdfPathExpr::Alternative(alternatives)))
        }
    }
}

pub(crate) fn zero_length(path: &PropertyPathExpression) -> ZeroLengthPolicy {
    match path {
        PropertyPathExpression::ZeroOrMore(_) | PropertyPathExpression::ZeroOrOne(_) => {
            ZeroLengthPolicy::Allowed
        }
        _ => ZeroLengthPolicy::Disallowed,
    }
}
