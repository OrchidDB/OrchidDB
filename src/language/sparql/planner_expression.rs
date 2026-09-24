//! Planner expression lowering, correlated EXISTS lifting, and aggregates.

use super::{
    AggCall, AggKind, AggregateExpression, AggregateFunction, ApplyKind, Expression, IrExpr,
    Lowered, Node, OptionalMissing, RdfGraphScope, SPARQL_GROUP_CONCAT, Slice, SparqlError,
    SparqlPlanner, binding, expression,
};
impl SparqlPlanner {
    /// Replace each `EXISTS` inside an expression by a count mark computed by
    /// a correlated scalar apply: `COUNT(*)` over the pattern limited to one
    /// row, so the mark is 1 when a solution exists and 0 otherwise.
    pub(super) fn lift_exists(
        &self,
        expr: &Expression,
        lowered: &mut Lowered,
        scope: &RdfGraphScope,
    ) -> Result<Expression, SparqlError> {
        use Expression as E;
        let mut lift = |expr: &Expression| self.lift_exists(expr, lowered, scope);
        let boxed = |expr: Expression| Box::new(expr);
        Ok(match expr {
            E::Exists(pattern) => {
                let id = self
                    .exists_marks
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let mark = spargebra::term::Variable::new_unchecked(format!("__exists_{id}"));
                let right = self.lower_in_scope(pattern, scope.clone())?;
                let alias = binding(&mark);
                let correlation = lowered
                    .variables
                    .iter()
                    .filter(|variable| right.variables.contains(*variable))
                    .cloned()
                    .collect();
                let node = std::mem::replace(&mut lowered.node, Node::GraphOneRow);
                lowered.node = Node::GraphApply {
                    kind: ApplyKind::Scalar,
                    correlation,
                    outputs: vec![alias.clone()],
                    optional_missing: OptionalMissing::Unbound,
                    left: Box::new(node),
                    right: Box::new(Node::GraphAggregate {
                        group: Vec::new(),
                        aggs: vec![AggCall {
                            kind: AggKind::CountRows,
                            alias: alias.clone(),
                            arg: None,
                            distinct: false,
                        }],
                        fields: vec![alias.clone()],
                        input: Box::new(Node::GraphSlice {
                            slice: Slice {
                                offset: 0,
                                fetch: Some(1),
                                tail: None,
                            },
                            input: Box::new(right.node),
                        }),
                    }),
                };
                lowered.variables.insert(alias);
                E::Greater(
                    boxed(E::Variable(mark)),
                    boxed(E::Literal(spargebra::term::Literal::from(0_i64))),
                )
            }
            E::NamedNode(_) | E::Literal(_) | E::Variable(_) | E::Bound(_) => expr.clone(),
            E::Or(a, b) => E::Or(boxed(lift(a)?), boxed(lift(b)?)),
            E::And(a, b) => E::And(boxed(lift(a)?), boxed(lift(b)?)),
            E::Equal(a, b) => E::Equal(boxed(lift(a)?), boxed(lift(b)?)),
            E::SameTerm(a, b) => E::SameTerm(boxed(lift(a)?), boxed(lift(b)?)),
            E::Greater(a, b) => E::Greater(boxed(lift(a)?), boxed(lift(b)?)),
            E::GreaterOrEqual(a, b) => E::GreaterOrEqual(boxed(lift(a)?), boxed(lift(b)?)),
            E::Less(a, b) => E::Less(boxed(lift(a)?), boxed(lift(b)?)),
            E::LessOrEqual(a, b) => E::LessOrEqual(boxed(lift(a)?), boxed(lift(b)?)),
            E::Add(a, b) => E::Add(boxed(lift(a)?), boxed(lift(b)?)),
            E::Subtract(a, b) => E::Subtract(boxed(lift(a)?), boxed(lift(b)?)),
            E::Multiply(a, b) => E::Multiply(boxed(lift(a)?), boxed(lift(b)?)),
            E::Divide(a, b) => E::Divide(boxed(lift(a)?), boxed(lift(b)?)),
            E::In(a, list) => E::In(
                boxed(lift(a)?),
                list.iter().map(&mut lift).collect::<Result<_, _>>()?,
            ),
            E::UnaryPlus(a) => E::UnaryPlus(boxed(lift(a)?)),
            E::UnaryMinus(a) => E::UnaryMinus(boxed(lift(a)?)),
            E::Not(a) => E::Not(boxed(lift(a)?)),
            // IF and COALESCE evaluate their arguments lazily; lifting an
            // EXISTS out of them is still correct because EXISTS has no
            // errors or side effects.
            E::If(a, b, c) => E::If(boxed(lift(a)?), boxed(lift(b)?), boxed(lift(c)?)),
            E::Coalesce(list) => E::Coalesce(list.iter().map(&mut lift).collect::<Result<_, _>>()?),
            E::FunctionCall(function, list) => E::FunctionCall(
                function.clone(),
                list.iter().map(&mut lift).collect::<Result<_, _>>()?,
            ),
        })
    }

    pub(super) fn lower_expression(&self, expr: &Expression) -> IrExpr {
        if self.typed() {
            expression::lower_typed(expr)
        } else {
            expression::lower(expr)
        }
    }

    pub(super) fn aggregate(
        &self,
        variable: &spargebra::term::Variable,
        aggregate: &AggregateExpression,
    ) -> Result<AggCall, SparqlError> {
        let alias = binding(variable);
        Ok(match aggregate {
            AggregateExpression::CountSolutions { distinct } => AggCall {
                kind: if *distinct {
                    AggKind::CountDistinct
                } else {
                    AggKind::CountRows
                },
                alias,
                arg: None,
                distinct: *distinct,
            },
            AggregateExpression::FunctionCall {
                name,
                expr,
                distinct,
            } => {
                let arg = self.lower_expression(expr);
                let (kind, arg) = match name {
                    AggregateFunction::Count if *distinct => (AggKind::CountDistinct, arg),
                    AggregateFunction::Count => (AggKind::CountRows, arg),
                    AggregateFunction::Sum => (AggKind::Sum, arg),
                    AggregateFunction::Avg => (AggKind::Avg, arg),
                    AggregateFunction::Min => (AggKind::Min, arg),
                    AggregateFunction::Max => (AggKind::Max, arg),
                    // SAMPLE may return any member; the SPARQL-ordered
                    // minimum is a conforming, deterministic choice.
                    AggregateFunction::Sample => (AggKind::Min, arg),
                    AggregateFunction::GroupConcat { separator } => (
                        AggKind::CollectRows,
                        IrExpr::Call {
                            name: SPARQL_GROUP_CONCAT.into(),
                            args: vec![
                                arg,
                                IrExpr::lit_str(separator.clone().unwrap_or_else(|| " ".into())),
                            ],
                        },
                    ),
                    AggregateFunction::Custom(iri) => {
                        return Err(SparqlError::Unsupported(format!(
                            "custom SPARQL aggregate <{}>",
                            iri.as_str()
                        )));
                    }
                };
                AggCall {
                    kind,
                    alias,
                    arg: Some(arg),
                    distinct: *distinct,
                }
            }
        })
    }
}
