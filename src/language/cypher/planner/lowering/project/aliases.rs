//! Projection alias substitution with local binding preservation.

use super::scope::pattern_binding_names;
use super::{BTreeMap, BTreeSet, Expr, PatternPart, ProjectionBody};
pub(super) fn shadowed_projection_fields(
    body: &ProjectionBody,
    source_fields: &[String],
) -> BTreeSet<String> {
    body.items
        .iter()
        .filter_map(|item| {
            let alias = item
                .alias
                .as_deref()
                .or_else(|| item.expr.variable_name())?;
            if source_fields.iter().any(|field| field == alias)
                && item.expr != Expr::Variable(alias.to_string())
            {
                Some(alias.to_string())
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn projection_aliases(body: &ProjectionBody) -> BTreeMap<String, Expr> {
    body.items
        .iter()
        .filter_map(|item| {
            let alias = item
                .alias
                .clone()
                .or_else(|| item.expr.variable_name().map(ToString::to_string))?;
            Some((alias, item.expr.clone()))
        })
        .collect()
}

pub(super) fn substitute_projection_aliases(expr: &Expr, aliases: &BTreeMap<String, Expr>) -> Expr {
    substitute_projection_aliases_with_bound(expr, aliases, &mut BTreeSet::new())
}

pub(super) fn substitute_projection_aliases_with_bound(
    expr: &Expr,
    aliases: &BTreeMap<String, Expr>,
    bound: &mut BTreeSet<String>,
) -> Expr {
    match expr {
        Expr::Variable(name) if !bound.contains(name) => {
            aliases.get(name).cloned().unwrap_or_else(|| expr.clone())
        }
        Expr::Property { target, key } => Expr::Property {
            target: Box::new(substitute_projection_aliases_with_bound(
                target, aliases, bound,
            )),
            key: key.clone(),
        },
        Expr::LabelPredicate { target, labels } => Expr::LabelPredicate {
            target: Box::new(substitute_projection_aliases_with_bound(
                target, aliases, bound,
            )),
            labels: labels.clone(),
        },
        Expr::Unary { op, expr } => Expr::Unary {
            op: *op,
            expr: Box::new(substitute_projection_aliases_with_bound(
                expr, aliases, bound,
            )),
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op: *op,
            lhs: Box::new(substitute_projection_aliases_with_bound(
                lhs, aliases, bound,
            )),
            rhs: Box::new(substitute_projection_aliases_with_bound(
                rhs, aliases, bound,
            )),
        },
        Expr::IsNull(expr) => Expr::IsNull(Box::new(substitute_projection_aliases_with_bound(
            expr, aliases, bound,
        ))),
        Expr::IsNotNull(expr) => Expr::IsNotNull(Box::new(
            substitute_projection_aliases_with_bound(expr, aliases, bound),
        )),
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => Expr::StringPredicate {
            op: *op,
            target: Box::new(substitute_projection_aliases_with_bound(
                target, aliases, bound,
            )),
            pattern: Box::new(substitute_projection_aliases_with_bound(
                pattern, aliases, bound,
            )),
        },
        Expr::Function {
            name,
            distinct,
            args,
        } => Expr::Function {
            name: name.clone(),
            distinct: *distinct,
            args: args
                .iter()
                .map(|arg| substitute_projection_aliases_with_bound(arg, aliases, bound))
                .collect(),
        },
        Expr::Case {
            case,
            arms,
            otherwise,
        } => Expr::Case {
            case: case
                .as_ref()
                .map(|expr| substitute_projection_aliases_with_bound(expr, aliases, bound))
                .map(Box::new),
            arms: arms
                .iter()
                .map(|(when, then)| {
                    (
                        substitute_projection_aliases_with_bound(when, aliases, bound),
                        substitute_projection_aliases_with_bound(then, aliases, bound),
                    )
                })
                .collect(),
            otherwise: otherwise
                .as_ref()
                .map(|expr| substitute_projection_aliases_with_bound(expr, aliases, bound))
                .map(Box::new),
        },
        Expr::List(items) => Expr::List(
            items
                .iter()
                .map(|item| substitute_projection_aliases_with_bound(item, aliases, bound))
                .collect(),
        ),
        Expr::Map(items) => Expr::Map(
            items
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        substitute_projection_aliases_with_bound(value, aliases, bound),
                    )
                })
                .collect(),
        ),
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate = predicate
                .as_ref()
                .map(|expr| substitute_projection_aliases_with_bound(expr, aliases, bound))
                .map(Box::new);
            let map = substitute_projection_aliases_with_bound(map, aliases, bound);
            if !was_bound {
                bound.remove(variable);
            }
            Expr::ListComprehension {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate,
                map: Box::new(map),
            }
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let acc_was_bound = bound.contains(accumulator);
            let variable_was_bound = bound.contains(variable);
            bound.insert(accumulator.clone());
            bound.insert(variable.clone());
            let map = substitute_projection_aliases_with_bound(map, aliases, bound);
            if !acc_was_bound {
                bound.remove(accumulator);
            }
            if !variable_was_bound {
                bound.remove(variable);
            }
            Expr::ListReduce {
                accumulator: accumulator.clone(),
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            }
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let map = substitute_projection_aliases_with_bound(map, aliases, bound);
            if !was_bound {
                bound.remove(variable);
            }
            Expr::ListTransform {
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            }
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate = substitute_projection_aliases_with_bound(predicate, aliases, bound);
            if !was_bound {
                bound.remove(variable);
            }
            Expr::ListFilter {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            }
        }
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => {
            let collection = substitute_projection_aliases_with_bound(collection, aliases, bound);
            let was_bound = bound.contains(variable);
            bound.insert(variable.clone());
            let predicate = substitute_projection_aliases_with_bound(predicate, aliases, bound);
            if !was_bound {
                bound.remove(variable);
            }
            Expr::Quantifier {
                kind: *kind,
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            }
        }
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => {
            let mut pattern = (**pattern).clone();
            let local_names = pattern_binding_names(&pattern);
            let previously_bound = local_names
                .iter()
                .filter(|name| bound.contains(*name))
                .cloned()
                .collect::<BTreeSet<_>>();
            for name in &local_names {
                bound.insert(name.clone());
            }
            substitute_pattern_property_aliases(&mut pattern, aliases, bound);
            let variable_was_bound = variable.as_ref().is_some_and(|name| bound.contains(name));
            if let Some(variable) = variable {
                bound.insert(variable.clone());
            }
            let predicate = predicate
                .as_ref()
                .map(|expr| substitute_projection_aliases_with_bound(expr, aliases, bound))
                .map(Box::new);
            let map = substitute_projection_aliases_with_bound(map, aliases, bound);
            for name in &local_names {
                if !previously_bound.contains(name) {
                    bound.remove(name);
                }
            }
            if let Some(variable) = variable {
                if !variable_was_bound {
                    bound.remove(variable);
                }
            }
            Expr::PatternComprehension {
                variable: variable.clone(),
                pattern: Box::new(pattern),
                predicate,
                map: Box::new(map),
            }
        }
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => expr.clone(),
    }
}

pub(super) fn substitute_pattern_property_aliases(
    pattern: &mut PatternPart,
    aliases: &BTreeMap<String, Expr>,
    bound: &mut BTreeSet<String>,
) {
    if let Some(properties) = pattern.element.start.properties.clone() {
        pattern.element.start.properties = Some(substitute_projection_aliases_with_bound(
            &properties,
            aliases,
            bound,
        ));
    }
    for chain in &mut pattern.element.chains {
        if let Some(properties) = chain.relationship.properties.clone() {
            chain.relationship.properties = Some(substitute_projection_aliases_with_bound(
                &properties,
                aliases,
                bound,
            ));
        }
        if let Some(properties) = chain.node.properties.clone() {
            chain.node.properties = Some(substitute_projection_aliases_with_bound(
                &properties,
                aliases,
                bound,
            ));
        }
    }
}
