//! Correlated subquery and comprehension materialization.

use super::aggregate::{aggregate_kind, contains_aggregate};
use super::expression::{lower_expr, lower_quantifier_kind};
use super::{
    AggCall, AggKind, ApplyKind, CypherPlanResult, CypherTraversalKind, ExistsSubquery, Expr,
    IrBinaryOp, IrExpr, Lit, Lowerer, Node, OptionalMissing, PatternPart, ProjectErrorPolicy,
    ProjectMode, ProjectionItem, QuantifierKind, pattern, predicate,
};
pub(crate) fn lower_expr_with_input(
    lowerer: &mut Lowerer,
    input: Node,
    expr: &Expr,
) -> CypherPlanResult<(Node, IrExpr)> {
    let (input, expr) = materialize_expr(lowerer, input, expr)?;
    Ok((input, lower_expr(lowerer, &expr)?))
}

pub(super) fn materialize_expr(
    lowerer: &mut Lowerer,
    input: Node,
    expr: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    match expr {
        Expr::Exists(exists) => materialize_exists(lowerer, input, exists),
        Expr::Function { name, args, .. }
            if name.eq_ignore_ascii_case("count_subquery")
                && matches!(args.as_slice(), [Expr::Exists(_)]) =>
        {
            let Some(Expr::Exists(exists)) = args.first() else {
                unreachable!("guard matched count_subquery(EXISTS ...)");
            };
            materialize_count_exists(lowerer, input, exists)
        }
        Expr::PatternPredicate(patterns) => materialize_pattern_predicate(lowerer, input, patterns),
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => materialize_list_comprehension(
            lowerer,
            input,
            variable,
            collection,
            predicate.as_deref(),
            map,
        ),
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => materialize_pattern_comprehension(
            lowerer,
            input,
            variable.as_deref(),
            pattern,
            predicate.as_deref(),
            map,
        ),
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => materialize_quantifier(lowerer, input, *kind, variable, collection, predicate),
        Expr::Function {
            name,
            distinct: false,
            args,
        } if name.eq_ignore_ascii_case("size")
            && matches!(args.as_slice(), [Expr::PatternPredicate(_)]) =>
        {
            let [Expr::PatternPredicate(patterns)] = args.as_slice() else {
                unreachable!("size(pattern) guard matched");
            };
            materialize_pattern_count(lowerer, input, patterns)
        }
        Expr::Function {
            name,
            distinct: false,
            args,
        } if name.eq_ignore_ascii_case("exists")
            && matches!(args.as_slice(), [Expr::PatternPredicate(_)]) =>
        {
            let [Expr::PatternPredicate(patterns)] = args.as_slice() else {
                unreachable!("exists(pattern) guard matched");
            };
            materialize_pattern_predicate(lowerer, input, patterns)
        }
        Expr::Unary { op, expr } => {
            let (input, expr) = materialize_expr(lowerer, input, expr)?;
            Ok((
                input,
                Expr::Unary {
                    op: *op,
                    expr: Box::new(expr),
                },
            ))
        }
        Expr::Binary { op, lhs, rhs } => {
            let (input, lhs) = materialize_expr(lowerer, input, lhs)?;
            let (input, rhs) = materialize_expr(lowerer, input, rhs)?;
            Ok((
                input,
                Expr::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            ))
        }
        Expr::Property { target, key } => {
            let (input, target) = materialize_expr(lowerer, input, target)?;
            Ok((
                input,
                Expr::Property {
                    target: Box::new(target),
                    key: key.clone(),
                },
            ))
        }
        Expr::LabelPredicate { target, labels } => {
            let (input, target) = materialize_expr(lowerer, input, target)?;
            Ok((
                input,
                Expr::LabelPredicate {
                    target: Box::new(target),
                    labels: labels.clone(),
                },
            ))
        }
        Expr::IsNull(expr) => {
            let (input, expr) = materialize_expr(lowerer, input, expr)?;
            Ok((input, Expr::IsNull(Box::new(expr))))
        }
        Expr::IsNotNull(expr) => {
            let (input, expr) = materialize_expr(lowerer, input, expr)?;
            Ok((input, Expr::IsNotNull(Box::new(expr))))
        }
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => {
            let (input, target) = materialize_expr(lowerer, input, target)?;
            let (input, pattern) = materialize_expr(lowerer, input, pattern)?;
            Ok((
                input,
                Expr::StringPredicate {
                    op: *op,
                    target: Box::new(target),
                    pattern: Box::new(pattern),
                },
            ))
        }
        Expr::Function {
            name,
            distinct,
            args,
        } => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(args.len());
            for arg in args {
                let (next, arg) = materialize_expr(lowerer, input, arg)?;
                input = next;
                lowered.push(arg);
            }
            Ok((
                input,
                Expr::Function {
                    name: name.clone(),
                    distinct: *distinct,
                    args: lowered,
                },
            ))
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let (input, collection) = materialize_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListReduce {
                    accumulator: accumulator.clone(),
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    map: map.clone(),
                },
            ))
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let (input, collection) = materialize_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListTransform {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    map: map.clone(),
                },
            ))
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let (input, collection) = materialize_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListFilter {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    predicate: predicate.clone(),
                },
            ))
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            let (mut input, case) = match case {
                Some(case) => {
                    let (next, case) = materialize_expr(lowerer, input, case)?;
                    (next, Some(Box::new(case)))
                }
                None => (input, None),
            };
            let mut lowered_arms = Vec::with_capacity(arms.len());
            for (when, then) in arms {
                let (next, when) = materialize_expr(lowerer, input, when)?;
                let (next, then) = materialize_expr(lowerer, next, then)?;
                input = next;
                lowered_arms.push((when, then));
            }
            let otherwise = match otherwise {
                Some(expr) => {
                    let (next, expr) = materialize_expr(lowerer, input, expr)?;
                    input = next;
                    Some(Box::new(expr))
                }
                None => None,
            };
            Ok((
                input,
                Expr::Case {
                    case,
                    arms: lowered_arms,
                    otherwise,
                },
            ))
        }
        Expr::List(items) => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(items.len());
            for item in items {
                let (next, item) = materialize_expr(lowerer, input, item)?;
                input = next;
                lowered.push(item);
            }
            Ok((input, Expr::List(lowered)))
        }
        Expr::Map(items) => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(items.len());
            for (key, value) in items {
                let (next, value) = materialize_expr(lowerer, input, value)?;
                input = next;
                lowered.push((key.clone(), value));
            }
            Ok((input, Expr::Map(lowered)))
        }
        _ => Ok((input, expr.clone())),
    }
}

pub(super) fn materialize_pre_aggregate_expr(
    lowerer: &mut Lowerer,
    input: Node,
    expr: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    if let Expr::Function {
        name,
        distinct,
        args,
    } = expr
    {
        if aggregate_kind(name).is_some() {
            let mut input = input;
            let mut lowered = Vec::with_capacity(args.len());
            for arg in args {
                let (next, arg) = materialize_pre_aggregate_expr(lowerer, input, arg)?;
                input = next;
                lowered.push(arg);
            }
            return Ok((
                input,
                Expr::Function {
                    name: name.clone(),
                    distinct: *distinct,
                    args: lowered,
                },
            ));
        }
    }
    if requires_scoped_materialization(expr) {
        if !contains_aggregate(expr) {
            return materialize_expr(lowerer, input, expr);
        }
        return Ok((input, expr.clone()));
    }
    match expr {
        Expr::Unary { op, expr } => {
            let (input, expr) = materialize_pre_aggregate_expr(lowerer, input, expr)?;
            Ok((
                input,
                Expr::Unary {
                    op: *op,
                    expr: Box::new(expr),
                },
            ))
        }
        Expr::Binary { op, lhs, rhs } => {
            let (input, lhs) = materialize_pre_aggregate_expr(lowerer, input, lhs)?;
            let (input, rhs) = materialize_pre_aggregate_expr(lowerer, input, rhs)?;
            Ok((
                input,
                Expr::Binary {
                    op: *op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
            ))
        }
        Expr::Property { target, key } => {
            let (input, target) = materialize_pre_aggregate_expr(lowerer, input, target)?;
            Ok((
                input,
                Expr::Property {
                    target: Box::new(target),
                    key: key.clone(),
                },
            ))
        }
        Expr::LabelPredicate { target, labels } => {
            let (input, target) = materialize_pre_aggregate_expr(lowerer, input, target)?;
            Ok((
                input,
                Expr::LabelPredicate {
                    target: Box::new(target),
                    labels: labels.clone(),
                },
            ))
        }
        Expr::IsNull(expr) => {
            let (input, expr) = materialize_pre_aggregate_expr(lowerer, input, expr)?;
            Ok((input, Expr::IsNull(Box::new(expr))))
        }
        Expr::IsNotNull(expr) => {
            let (input, expr) = materialize_pre_aggregate_expr(lowerer, input, expr)?;
            Ok((input, Expr::IsNotNull(Box::new(expr))))
        }
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => {
            let (input, target) = materialize_pre_aggregate_expr(lowerer, input, target)?;
            let (input, pattern) = materialize_pre_aggregate_expr(lowerer, input, pattern)?;
            Ok((
                input,
                Expr::StringPredicate {
                    op: *op,
                    target: Box::new(target),
                    pattern: Box::new(pattern),
                },
            ))
        }
        Expr::Function {
            name,
            distinct,
            args,
        } => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(args.len());
            for arg in args {
                let (next, arg) = materialize_pre_aggregate_expr(lowerer, input, arg)?;
                input = next;
                lowered.push(arg);
            }
            Ok((
                input,
                Expr::Function {
                    name: name.clone(),
                    distinct: *distinct,
                    args: lowered,
                },
            ))
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListReduce {
                    accumulator: accumulator.clone(),
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    map: map.clone(),
                },
            ))
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListTransform {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    map: map.clone(),
                },
            ))
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            Ok((
                input,
                Expr::ListFilter {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    predicate: predicate.clone(),
                },
            ))
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            let (mut input, case) = match case {
                Some(case) => {
                    let (next, case) = materialize_pre_aggregate_expr(lowerer, input, case)?;
                    (next, Some(Box::new(case)))
                }
                None => (input, None),
            };
            let mut lowered_arms = Vec::with_capacity(arms.len());
            for (when, then) in arms {
                let (next, when) = materialize_pre_aggregate_expr(lowerer, input, when)?;
                let (next, then) = materialize_pre_aggregate_expr(lowerer, next, then)?;
                input = next;
                lowered_arms.push((when, then));
            }
            let otherwise = match otherwise {
                Some(expr) => {
                    let (next, expr) = materialize_pre_aggregate_expr(lowerer, input, expr)?;
                    input = next;
                    Some(Box::new(expr))
                }
                None => None,
            };
            Ok((
                input,
                Expr::Case {
                    case,
                    arms: lowered_arms,
                    otherwise,
                },
            ))
        }
        Expr::List(items) => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(items.len());
            for item in items {
                let (next, item) = materialize_pre_aggregate_expr(lowerer, input, item)?;
                input = next;
                lowered.push(item);
            }
            Ok((input, Expr::List(lowered)))
        }
        Expr::Map(items) => {
            let mut input = input;
            let mut lowered = Vec::with_capacity(items.len());
            for (key, value) in items {
                let (next, value) = materialize_pre_aggregate_expr(lowerer, input, value)?;
                input = next;
                lowered.push((key.clone(), value));
            }
            Ok((input, Expr::Map(lowered)))
        }
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            let (input, predicate) = match predicate {
                Some(predicate) => {
                    let (next, predicate) =
                        materialize_pre_aggregate_expr(lowerer, input, predicate)?;
                    (next, Some(Box::new(predicate)))
                }
                None => (input, None),
            };
            let (input, map) = materialize_pre_aggregate_expr(lowerer, input, map)?;
            Ok((
                input,
                Expr::ListComprehension {
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    predicate,
                    map: Box::new(map),
                },
            ))
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let pattern = materialize_pre_aggregate_pattern(lowerer, pattern)?;
            let (input, predicate) = match predicate {
                Some(predicate) => {
                    let (next, predicate) =
                        materialize_pre_aggregate_expr(lowerer, input, predicate)?;
                    (next, Some(Box::new(predicate)))
                }
                None => (input, None),
            };
            let (input, map) = materialize_pre_aggregate_expr(lowerer, input, map)?;
            Ok((
                input,
                Expr::PatternComprehension {
                    variable: variable.clone(),
                    pattern: Box::new(pattern),
                    predicate,
                    map: Box::new(map),
                },
            ))
        }
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => {
            let (input, collection) = materialize_pre_aggregate_expr(lowerer, input, collection)?;
            let (input, predicate) = materialize_pre_aggregate_expr(lowerer, input, predicate)?;
            Ok((
                input,
                Expr::Quantifier {
                    kind: *kind,
                    variable: variable.clone(),
                    collection: Box::new(collection),
                    predicate: Box::new(predicate),
                },
            ))
        }
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => Ok((input, expr.clone())),
    }
}

pub(super) fn materialize_pre_aggregate_pattern(
    lowerer: &mut Lowerer,
    pattern: &PatternPart,
) -> CypherPlanResult<PatternPart> {
    let _ = lowerer;
    Ok(pattern.clone())
}

pub(super) fn materialize_exists(
    lowerer: &mut Lowerer,
    input: Node,
    exists: &ExistsSubquery,
) -> CypherPlanResult<(Node, Expr)> {
    let alias = lowerer.synthetic("exists");
    let count = lowerer.synthetic("exists_count");
    let right = lowerer.with_preserved_scope(|lowerer| {
        let right = lower_exists_right_plan(lowerer, exists)?;
        Ok(Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            items: vec![ProjectionItem {
                alias: alias.clone(),
                expr: IrExpr::Binary {
                    op: IrBinaryOp::Gt,
                    lhs: Box::new(IrExpr::Binding(count.clone())),
                    rhs: Box::new(IrExpr::Lit(Lit::Int(0))),
                },
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: Node::GraphAggregate {
                group: Vec::new(),
                aggs: vec![AggCall {
                    kind: AggKind::CountRows,
                    alias: count.clone(),
                    arg: None,
                    distinct: false,
                }],
                fields: vec![count],
                input: right.boxed(),
            }
            .boxed(),
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

/// `COUNT { MATCH ... }` — like `materialize_exists`, but projects the
/// subquery's row count instead of a boolean.
pub(super) fn materialize_count_exists(
    lowerer: &mut Lowerer,
    input: Node,
    exists: &ExistsSubquery,
) -> CypherPlanResult<(Node, Expr)> {
    let alias = lowerer.synthetic("count_subquery");
    let count = lowerer.synthetic("count_subquery_value");
    let right = lowerer.with_preserved_scope(|lowerer| {
        let right = lower_exists_right_plan(lowerer, exists)?;
        Ok(Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            items: vec![ProjectionItem {
                alias: alias.clone(),
                expr: IrExpr::Binding(count.clone()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: Node::GraphAggregate {
                group: Vec::new(),
                aggs: vec![AggCall {
                    kind: AggKind::CountRows,
                    alias: count.clone(),
                    arg: None,
                    distinct: false,
                }],
                fields: vec![count],
                input: right.boxed(),
            }
            .boxed(),
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

pub(super) fn materialize_pattern_predicate(
    lowerer: &mut Lowerer,
    input: Node,
    patterns: &[PatternPart],
) -> CypherPlanResult<(Node, Expr)> {
    predicate::validate_pattern_predicate_scope(lowerer, patterns)?;
    let exists = ExistsSubquery {
        query: None,
        patterns: patterns.to_vec(),
        predicate: None,
    };
    materialize_exists(lowerer, input, &exists)
}

pub(super) fn materialize_pattern_count(
    lowerer: &mut Lowerer,
    input: Node,
    patterns: &[PatternPart],
) -> CypherPlanResult<(Node, Expr)> {
    predicate::validate_pattern_predicate_scope(lowerer, patterns)?;
    let alias = lowerer.synthetic("pattern_count");
    let count = lowerer.synthetic("pattern_count_value");
    let exists = ExistsSubquery {
        query: None,
        patterns: patterns.to_vec(),
        predicate: None,
    };
    let right = lowerer.with_preserved_scope(|lowerer| {
        let right = lower_exists_right_plan(lowerer, &exists)?;
        Ok(Node::GraphProject {
            mode: ProjectMode::ReplaceScope,
            items: vec![ProjectionItem {
                alias: alias.clone(),
                expr: IrExpr::Binding(count.clone()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: Node::GraphAggregate {
                group: Vec::new(),
                aggs: vec![AggCall {
                    kind: AggKind::CountRows,
                    alias: count.clone(),
                    arg: None,
                    distinct: false,
                }],
                fields: vec![count],
                input: right.boxed(),
            }
            .boxed(),
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

pub(super) fn materialize_list_comprehension(
    lowerer: &mut Lowerer,
    input: Node,
    variable: &str,
    collection: &Expr,
    predicate_expr: Option<&Expr>,
    map: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    // Pure collection expressions preserve their values and local variable
    // scope directly. A correlated aggregate would otherwise discard a
    // preceding quantified expression's temporary binding.
    if !requires_scoped_materialization(collection)
        && !predicate_expr.is_some_and(requires_scoped_materialization)
        && !requires_scoped_materialization(map)
    {
        let collection = match predicate_expr {
            Some(predicate) => Expr::ListFilter {
                variable: variable.to_string(),
                collection: Box::new(collection.clone()),
                predicate: Box::new(predicate.clone()),
            },
            None => collection.clone(),
        };
        return Ok((
            input,
            Expr::ListTransform {
                variable: variable.to_string(),
                collection: Box::new(collection),
                map: Box::new(map.clone()),
            },
        ));
    }
    let alias = lowerer.synthetic("list");
    let collection_alias = lowerer.synthetic("list_collection");
    let collection_is_null = lowerer.synthetic("list_collection_null");
    let collected_alias = lowerer.synthetic("list_values");
    let right = lowerer.with_preserved_scope(|lowerer| {
        lowerer.with_child_traversal(CypherTraversalKind::ListComprehension, |lowerer| {
            let start = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let (right, collection) = lower_expr_with_input(lowerer, start, collection)?;
            let collection_project = Node::GraphProject {
                mode: ProjectMode::PreserveVisible,
                items: vec![
                    ProjectionItem {
                        alias: collection_alias.clone(),
                        expr: collection.clone(),
                    },
                    ProjectionItem {
                        alias: collection_is_null.clone(),
                        expr: IrExpr::IsNull(Box::new(collection)),
                    },
                ],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: right.boxed(),
            };
            lowerer.add_visible(collection_alias.clone());
            lowerer.add_visible(collection_is_null.clone());
            let collection_scope = lowerer.visible_fields();
            let start = Node::GraphCorrelate {
                bindings: collection_scope.clone(),
            };
            let mut right = Node::GraphUnwind {
                input_expr: IrExpr::Binding(collection_alias.clone()),
                bind: variable.to_string(),
                outer: false,
                input: start.boxed(),
            };
            lowerer.add_visible(variable.to_string());
            if let Some(predicate_expr) = predicate_expr {
                right = predicate::lower_where_predicate(lowerer, right, predicate_expr)?;
            }
            let (right, value) = lower_expr_with_input(lowerer, right, map)?;
            let collect = Node::GraphCollect {
                value,
                distinct: false,
                order: Vec::new(),
                alias: collected_alias.clone(),
                input: right.boxed(),
            };
            let right = Node::GraphApply {
                kind: ApplyKind::Scalar,
                correlation: collection_scope,
                outputs: vec![collected_alias.clone()],
                optional_missing: OptionalMissing::Null,
                left: collection_project.boxed(),
                right: collect.boxed(),
            };
            Ok(Node::GraphProject {
                mode: ProjectMode::ReplaceScope,
                items: vec![ProjectionItem {
                    alias: alias.clone(),
                    expr: IrExpr::Case {
                        arms: vec![(
                            IrExpr::Binding(collection_is_null.clone()),
                            IrExpr::Lit(Lit::Null),
                        )],
                        otherwise: Some(Box::new(IrExpr::Binding(collected_alias.clone()))),
                    },
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: right.boxed(),
            })
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

pub(super) fn materialize_quantifier(
    lowerer: &mut Lowerer,
    input: Node,
    kind: QuantifierKind,
    variable: &str,
    collection: &Expr,
    predicate_expr: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    let alias = lowerer.synthetic("quantifier");
    if !requires_scoped_materialization(collection)
        && !requires_scoped_materialization(predicate_expr)
    {
        return Ok((
            Node::GraphQuantifier {
                kind: lower_quantifier_kind(kind),
                item_binding: variable.to_string(),
                input_expr: lower_expr(lowerer, collection)?,
                predicate: lower_expr(lowerer, predicate_expr)?,
                output: alias.clone(),
                input: input.boxed(),
            },
            Expr::Variable(alias),
        ));
    }

    let total_count = lowerer.synthetic("quantifier_total");
    let known_count = lowerer.synthetic("quantifier_known");
    let true_count = lowerer.synthetic("quantifier_true");
    let collection_alias = lowerer.synthetic("quantifier_collection");
    let collection_is_null = lowerer.synthetic("quantifier_collection_null");
    let right = lowerer.with_preserved_scope(|lowerer| {
        lowerer.with_child_traversal(CypherTraversalKind::Quantifier, |lowerer| {
            let start = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let (right, collection) = lower_expr_with_input(lowerer, start, collection)?;
            let right = Node::GraphProject {
                mode: ProjectMode::PreserveVisible,
                items: vec![
                    ProjectionItem {
                        alias: collection_alias.clone(),
                        expr: collection.clone(),
                    },
                    ProjectionItem {
                        alias: collection_is_null.clone(),
                        expr: IrExpr::IsNull(Box::new(collection)),
                    },
                ],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: right.boxed(),
            };
            lowerer.add_visible(collection_alias.clone());
            lowerer.add_visible(collection_is_null.clone());
            let collection_scope = lowerer.visible_fields();

            let aggregate_start = Node::GraphCorrelate {
                bindings: collection_scope.clone(),
            };
            let rows = Node::GraphUnwind {
                input_expr: IrExpr::Binding(collection_alias.clone()),
                bind: variable.to_string(),
                outer: false,
                input: aggregate_start.boxed(),
            };
            lowerer.add_visible(variable.to_string());
            let (rows, predicate) = lower_expr_with_input(lowerer, rows, predicate_expr)?;
            let true_value = IrExpr::Case {
                arms: vec![(predicate.clone(), IrExpr::Lit(Lit::Int(1)))],
                otherwise: None,
            };
            let aggregate = Node::GraphAggregate {
                group: Vec::new(),
                aggs: vec![
                    AggCall {
                        kind: AggKind::CountRows,
                        alias: total_count.clone(),
                        arg: None,
                        distinct: false,
                    },
                    AggCall {
                        kind: AggKind::CountRows,
                        alias: known_count.clone(),
                        arg: Some(predicate),
                        distinct: false,
                    },
                    AggCall {
                        kind: AggKind::CountRows,
                        alias: true_count.clone(),
                        arg: Some(true_value),
                        distinct: false,
                    },
                ],
                fields: vec![total_count.clone(), known_count.clone(), true_count.clone()],
                input: rows.boxed(),
            };
            let right = Node::GraphApply {
                kind: ApplyKind::Scalar,
                correlation: collection_scope,
                outputs: vec![total_count.clone(), known_count.clone(), true_count.clone()],
                optional_missing: OptionalMissing::Null,
                left: right.boxed(),
                right: aggregate.boxed(),
            };
            Ok(Node::GraphProject {
                mode: ProjectMode::ReplaceScope,
                items: vec![ProjectionItem {
                    alias: alias.clone(),
                    expr: IrExpr::Case {
                        arms: vec![(
                            IrExpr::Binding(collection_is_null.clone()),
                            IrExpr::Lit(Lit::Null),
                        )],
                        otherwise: Some(Box::new(quantifier_result_expr(
                            kind,
                            &total_count,
                            &known_count,
                            &true_count,
                        ))),
                    },
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: right.boxed(),
            })
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

pub(super) fn materialize_pattern_comprehension(
    lowerer: &mut Lowerer,
    input: Node,
    _variable: Option<&str>,
    pattern_part: &PatternPart,
    predicate_expr: Option<&Expr>,
    map: &Expr,
) -> CypherPlanResult<(Node, Expr)> {
    let alias = lowerer.synthetic("pattern_list");
    let right = lowerer.with_preserved_scope(|lowerer| {
        lowerer.with_child_traversal(CypherTraversalKind::PatternComprehension, |lowerer| {
            let start = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let history = (!pattern_part.element.chains.is_empty())
                .then(|| lowerer.synthetic("pattern_history"));
            let mut right = pattern::lower_pattern_part(
                lowerer,
                start,
                pattern_part,
                false,
                history.as_deref(),
                false,
            )?;
            if let Some(predicate_expr) = predicate_expr {
                right = predicate::lower_where_predicate(lowerer, right, predicate_expr)?;
            }
            let (right, value) = lower_expr_with_input(lowerer, right, map)?;
            Ok(Node::GraphCollect {
                value,
                distinct: false,
                order: Vec::new(),
                alias: alias.clone(),
                input: right.boxed(),
            })
        })
    })?;
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Scalar,
            correlation: lowerer.visible_fields(),
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        Expr::Variable(alias),
    ))
}

pub(super) fn quantifier_result_expr(
    kind: QuantifierKind,
    total_count: &str,
    known_count: &str,
    true_count: &str,
) -> IrExpr {
    let total = IrExpr::Binding(total_count.to_string());
    let known = IrExpr::Binding(known_count.to_string());
    let true_values = IrExpr::Binding(true_count.to_string());
    let false_values = IrExpr::Binary {
        op: IrBinaryOp::Sub,
        lhs: Box::new(known.clone()),
        rhs: Box::new(true_values.clone()),
    };
    let null_values = IrExpr::Binary {
        op: IrBinaryOp::Sub,
        lhs: Box::new(total),
        rhs: Box::new(known.clone()),
    };
    let gt_zero = |expr: IrExpr| IrExpr::Binary {
        op: IrBinaryOp::Gt,
        lhs: Box::new(expr),
        rhs: Box::new(IrExpr::Lit(Lit::Int(0))),
    };
    let eq_zero = |expr: IrExpr| IrExpr::Binary {
        op: IrBinaryOp::Eq,
        lhs: Box::new(expr),
        rhs: Box::new(IrExpr::Lit(Lit::Int(0))),
    };
    let true_eq = |value: i64| IrExpr::Binary {
        op: IrBinaryOp::Eq,
        lhs: Box::new(true_values.clone()),
        rhs: Box::new(IrExpr::Lit(Lit::Int(value))),
    };
    let lit_bool = |value| IrExpr::Lit(Lit::Bool(value));
    let lit_null = IrExpr::Lit(Lit::Null);

    match kind {
        QuantifierKind::All => IrExpr::Case {
            arms: vec![
                (gt_zero(false_values), lit_bool(false)),
                (gt_zero(null_values), lit_null.clone()),
            ],
            otherwise: Some(Box::new(lit_bool(true))),
        },
        QuantifierKind::Any => IrExpr::Case {
            arms: vec![
                (gt_zero(true_values.clone()), lit_bool(true)),
                (gt_zero(null_values), lit_null.clone()),
            ],
            otherwise: Some(Box::new(lit_bool(false))),
        },
        QuantifierKind::None => IrExpr::Case {
            arms: vec![
                (gt_zero(true_values.clone()), lit_bool(false)),
                (gt_zero(null_values), lit_null.clone()),
            ],
            otherwise: Some(Box::new(lit_bool(true))),
        },
        QuantifierKind::Single => IrExpr::Case {
            arms: vec![
                (
                    IrExpr::Binary {
                        op: IrBinaryOp::Gt,
                        lhs: Box::new(true_values.clone()),
                        rhs: Box::new(IrExpr::Lit(Lit::Int(1))),
                    },
                    lit_bool(false),
                ),
                (
                    IrExpr::Binary {
                        op: IrBinaryOp::And,
                        lhs: Box::new(true_eq(1)),
                        rhs: Box::new(eq_zero(null_values.clone())),
                    },
                    lit_bool(true),
                ),
                (
                    IrExpr::Binary {
                        op: IrBinaryOp::And,
                        lhs: Box::new(true_eq(0)),
                        rhs: Box::new(eq_zero(null_values)),
                    },
                    lit_bool(false),
                ),
            ],
            otherwise: Some(Box::new(lit_null)),
        },
    }
}

pub(super) fn lower_exists_right_plan(
    lowerer: &mut Lowerer,
    exists: &ExistsSubquery,
) -> CypherPlanResult<Node> {
    lowerer.with_child_traversal(CypherTraversalKind::ExistsSubquery, |lowerer| {
        if let Some(query) = &exists.query {
            lowerer.lower_query_with_unions(query).map(|(node, _)| node)
        } else {
            let mut right = Node::GraphCorrelate {
                bindings: lowerer.visible_fields(),
            };
            let history = exists
                .patterns
                .iter()
                .any(|part| !part.element.chains.is_empty())
                .then(|| lowerer.synthetic("exists_history"));
            for part in &exists.patterns {
                right = pattern::lower_pattern_part(
                    lowerer,
                    right,
                    part,
                    false,
                    history.as_deref(),
                    false,
                )?;
            }
            if let Some(predicate_expr) = &exists.predicate {
                right = lowerer.with_child_traversal(
                    CypherTraversalKind::WherePredicate,
                    |lowerer| {
                        let right =
                            predicate::lower_where_predicate(lowerer, right, predicate_expr)?;
                        lowerer.record_current_imports(lowerer.visible_fields());
                        lowerer.record_current_correlation(lowerer.visible_fields());
                        Ok(right)
                    },
                )?;
            }
            Ok(right)
        }
    })
}

pub(super) fn requires_scoped_materialization(expr: &Expr) -> bool {
    match expr {
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::ListComprehension { .. }
        | Expr::PatternComprehension { .. }
        | Expr::Quantifier { .. } => true,
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            requires_scoped_materialization(expr)
        }
        Expr::Binary { lhs, rhs, .. }
        | Expr::StringPredicate {
            target: lhs,
            pattern: rhs,
            ..
        } => requires_scoped_materialization(lhs) || requires_scoped_materialization(rhs),
        Expr::Property { target, .. } | Expr::LabelPredicate { target, .. } => {
            requires_scoped_materialization(target)
        }
        Expr::Function { args, .. } | Expr::List(args) => {
            args.iter().any(requires_scoped_materialization)
        }
        Expr::ListReduce {
            collection, map, ..
        } => requires_scoped_materialization(collection) || requires_scoped_materialization(map),
        Expr::ListTransform {
            collection, map, ..
        } => requires_scoped_materialization(collection) || requires_scoped_materialization(map),
        Expr::ListFilter {
            collection,
            predicate,
            ..
        } => {
            requires_scoped_materialization(collection)
                || requires_scoped_materialization(predicate)
        }
        Expr::Map(items) => items
            .iter()
            .any(|(_, value)| requires_scoped_materialization(value)),
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            case.as_deref().is_some_and(requires_scoped_materialization)
                || arms.iter().any(|(when, then)| {
                    requires_scoped_materialization(when) || requires_scoped_materialization(then)
                })
                || otherwise
                    .as_deref()
                    .is_some_and(requires_scoped_materialization)
        }
        _ => false,
    }
}
