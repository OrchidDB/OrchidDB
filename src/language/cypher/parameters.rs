//! Typed Cypher parameter binding.
//!
//! `bind_parameters` replaces every `Expr::Parameter` in a parsed `Query`
//! with a typed `Expr` literal built from the corresponding [`Value`]. The
//! substitution is purely structural — a `Value::String` becomes a
//! `Literal::String`, never spliced query text — so parameters cannot be
//! used as a vector for Cypher injection.

use std::collections::BTreeMap;

use crate::ir::value::Value;
use crate::language::cypher::ast::*;

/// Replace every `Expr::Parameter` in `query` with a typed literal, list, or
/// map expression derived from `parameters`.
///
/// # Errors
///
/// Returns an error if a referenced parameter has no value in `parameters`,
/// or if the value's type cannot be represented as a Cypher literal (for
/// example a node or edge identifier).
pub fn bind_parameters(
    query: &mut Query,
    parameters: &BTreeMap<String, Value>,
) -> Result<(), String> {
    // Preserve the caller's parsed query when any parameter fails binding.
    *query = bind_query(query.clone(), parameters)?;
    Ok(())
}

fn bind_query(query: Query, parameters: &BTreeMap<String, Value>) -> Result<Query, String> {
    Ok(Query {
        clauses: query
            .clauses
            .into_iter()
            .map(|clause| bind_clause(clause, parameters))
            .collect::<Result<Vec<_>, _>>()?,
        unions: query
            .unions
            .into_iter()
            .map(|branch| bind_union_branch(branch, parameters))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn bind_union_branch(
    branch: UnionBranch,
    parameters: &BTreeMap<String, Value>,
) -> Result<UnionBranch, String> {
    Ok(UnionBranch {
        all: branch.all,
        query: Box::new(bind_query(*branch.query, parameters)?),
    })
}

fn bind_clause(clause: Clause, parameters: &BTreeMap<String, Value>) -> Result<Clause, String> {
    Ok(match clause {
        Clause::Match(clause) => Clause::Match(MatchClause {
            optional: clause.optional,
            patterns: bind_pattern_parts(clause.patterns, parameters)?,
            predicate: clause
                .predicate
                .map(|expr| bind_expr(expr, parameters))
                .transpose()?,
        }),
        Clause::Unwind(clause) => Clause::Unwind(UnwindClause {
            expr: bind_expr(clause.expr, parameters)?,
            alias: clause.alias,
        }),
        Clause::Call(clause) => Clause::Call(ProcedureCallClause {
            name: clause.name,
            args: clause
                .args
                .into_iter()
                .map(|arg| bind_expr(arg, parameters))
                .collect::<Result<Vec<_>, _>>()?,
            yields: clause.yields,
            yield_all: clause.yield_all,
            predicate: clause
                .predicate
                .map(|expr| bind_expr(expr, parameters))
                .transpose()?,
            standalone: clause.standalone,
        }),
        Clause::Create(clause) => Clause::Create(CreateClause {
            patterns: bind_pattern_parts(clause.patterns, parameters)?,
        }),
        Clause::Merge(clause) => Clause::Merge(MergeClause {
            pattern: bind_pattern_part(clause.pattern, parameters)?,
            on_create: clause
                .on_create
                .into_iter()
                .map(|item| bind_set_item(item, parameters))
                .collect::<Result<Vec<_>, _>>()?,
            on_match: clause
                .on_match
                .into_iter()
                .map(|item| bind_set_item(item, parameters))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        Clause::Set(clause) => Clause::Set(SetClause {
            items: clause
                .items
                .into_iter()
                .map(|item| bind_set_item(item, parameters))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        Clause::Delete(clause) => Clause::Delete(DeleteClause {
            detach: clause.detach,
            expressions: clause
                .expressions
                .into_iter()
                .map(|expr| bind_expr(expr, parameters))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        Clause::With(clause) => Clause::With(WithClause {
            projection: bind_projection(clause.projection, parameters)?,
            predicate: clause
                .predicate
                .map(|expr| bind_expr(expr, parameters))
                .transpose()?,
        }),
        Clause::Return(clause) => Clause::Return(ReturnClause {
            projection: bind_projection(clause.projection, parameters)?,
        }),
    })
}

fn bind_set_item(item: SetItem, parameters: &BTreeMap<String, Value>) -> Result<SetItem, String> {
    Ok(match item {
        SetItem::Property { target, key, value } => SetItem::Property {
            target: bind_expr(target, parameters)?,
            key,
            value: bind_expr(value, parameters)?,
        },
        SetItem::Replace { variable, value } => SetItem::Replace {
            variable,
            value: bind_expr(value, parameters)?,
        },
        SetItem::Merge { variable, value } => SetItem::Merge {
            variable,
            value: bind_expr(value, parameters)?,
        },
        SetItem::Labels { variable, labels, remove } => SetItem::Labels { variable, labels, remove },
    })
}

fn bind_projection(
    projection: ProjectionBody,
    parameters: &BTreeMap<String, Value>,
) -> Result<ProjectionBody, String> {
    Ok(ProjectionBody {
        distinct: projection.distinct,
        include_existing: projection.include_existing,
        items: projection
            .items
            .into_iter()
            .map(|item| bind_projection_item(item, parameters))
            .collect::<Result<Vec<_>, _>>()?,
        order_by: projection
            .order_by
            .into_iter()
            .map(|item| bind_sort_item(item, parameters))
            .collect::<Result<Vec<_>, _>>()?,
        skip: projection
            .skip
            .map(|expr| bind_expr(expr, parameters))
            .transpose()?,
        limit: projection
            .limit
            .map(|expr| bind_expr(expr, parameters))
            .transpose()?,
    })
}

fn bind_projection_item(
    item: ProjectionItem,
    parameters: &BTreeMap<String, Value>,
) -> Result<ProjectionItem, String> {
    Ok(ProjectionItem {
        expr: bind_expr(item.expr, parameters)?,
        alias: item.alias,
        explicit_alias: item.explicit_alias,
    })
}

fn bind_sort_item(
    item: SortItem,
    parameters: &BTreeMap<String, Value>,
) -> Result<SortItem, String> {
    Ok(SortItem {
        expr: bind_expr(item.expr, parameters)?,
        direction: item.direction,
    })
}

fn bind_expr(expr: Expr, parameters: &BTreeMap<String, Value>) -> Result<Expr, String> {
    Ok(match expr {
        Expr::Star => Expr::Star,
        Expr::Variable(name) => Expr::Variable(name),
        Expr::Property { target, key } => Expr::Property {
            target: bind_expr_box(target, parameters)?,
            key,
        },
        Expr::LabelPredicate { target, labels } => Expr::LabelPredicate {
            target: bind_expr_box(target, parameters)?,
            labels,
        },
        Expr::Parameter(name) => {
            let value = parameters
                .get(&name)
                .ok_or_else(|| format!("missing value for parameter `${name}`"))?;
            value_to_expr(value)?
        }
        Expr::Literal(literal) => Expr::Literal(literal),
        Expr::List(items) => Expr::List(
            items
                .into_iter()
                .map(|item| bind_expr(item, parameters))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Expr::Map(entries) => Expr::Map(
            entries
                .into_iter()
                .map(|(key, value)| Ok((key, bind_expr(value, parameters)?)))
                .collect::<Result<Vec<_>, String>>()?,
        ),
        Expr::Unary { op, expr } => Expr::Unary {
            op,
            expr: bind_expr_box(expr, parameters)?,
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op,
            lhs: bind_expr_box(lhs, parameters)?,
            rhs: bind_expr_box(rhs, parameters)?,
        },
        Expr::IsNull(inner) => Expr::IsNull(bind_expr_box(inner, parameters)?),
        Expr::IsNotNull(inner) => Expr::IsNotNull(bind_expr_box(inner, parameters)?),
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => Expr::StringPredicate {
            op,
            target: bind_expr_box(target, parameters)?,
            pattern: bind_expr_box(pattern, parameters)?,
        },
        Expr::Function {
            name,
            distinct,
            args,
        } => Expr::Function {
            name,
            distinct,
            args: args
                .into_iter()
                .map(|arg| bind_expr(arg, parameters))
                .collect::<Result<Vec<_>, _>>()?,
        },
        Expr::Case {
            case,
            arms,
            otherwise,
        } => Expr::Case {
            case: case
                .map(|expr| bind_expr_box(expr, parameters))
                .transpose()?,
            arms: arms
                .into_iter()
                .map(|(when, then)| {
                    Ok((bind_expr(when, parameters)?, bind_expr(then, parameters)?))
                })
                .collect::<Result<Vec<_>, String>>()?,
            otherwise: otherwise
                .map(|expr| bind_expr_box(expr, parameters))
                .transpose()?,
        },
        Expr::Exists(exists) => Expr::Exists(bind_exists_subquery(exists, parameters)?),
        Expr::PatternPredicate(parts) => {
            Expr::PatternPredicate(bind_pattern_parts(parts, parameters)?)
        }
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => Expr::ListComprehension {
            variable,
            collection: bind_expr_box(collection, parameters)?,
            predicate: predicate
                .map(|expr| bind_expr_box(expr, parameters))
                .transpose()?,
            map: bind_expr_box(map, parameters)?,
        },
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => Expr::ListReduce {
            accumulator,
            variable,
            collection: bind_expr_box(collection, parameters)?,
            map: bind_expr_box(map, parameters)?,
        },
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => Expr::ListTransform {
            variable,
            collection: bind_expr_box(collection, parameters)?,
            map: bind_expr_box(map, parameters)?,
        },
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => Expr::ListFilter {
            variable,
            collection: bind_expr_box(collection, parameters)?,
            predicate: bind_expr_box(predicate, parameters)?,
        },
        Expr::PatternComprehension {
            variable,
            pattern,
            predicate,
            map,
        } => Expr::PatternComprehension {
            variable,
            pattern: bind_pattern_part_box(pattern, parameters)?,
            predicate: predicate
                .map(|expr| bind_expr_box(expr, parameters))
                .transpose()?,
            map: bind_expr_box(map, parameters)?,
        },
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => Expr::Quantifier {
            kind,
            variable,
            collection: bind_expr_box(collection, parameters)?,
            predicate: bind_expr_box(predicate, parameters)?,
        },
        Expr::CountStar => Expr::CountStar,
    })
}

fn bind_expr_box(
    expr: Box<Expr>,
    parameters: &BTreeMap<String, Value>,
) -> Result<Box<Expr>, String> {
    Ok(Box::new(bind_expr(*expr, parameters)?))
}

fn bind_exists_subquery(
    exists: ExistsSubquery,
    parameters: &BTreeMap<String, Value>,
) -> Result<ExistsSubquery, String> {
    Ok(ExistsSubquery {
        query: match exists.query {
            Some(mut query) => {
                bind_parameters(&mut query, parameters)?;
                Some(query)
            }
            None => None,
        },
        patterns: bind_pattern_parts(exists.patterns, parameters)?,
        predicate: exists
            .predicate
            .map(|expr| bind_expr_box(expr, parameters))
            .transpose()?,
    })
}

fn bind_pattern_parts(
    parts: Vec<PatternPart>,
    parameters: &BTreeMap<String, Value>,
) -> Result<Vec<PatternPart>, String> {
    parts
        .into_iter()
        .map(|part| bind_pattern_part(part, parameters))
        .collect()
}

fn bind_pattern_part(
    part: PatternPart,
    parameters: &BTreeMap<String, Value>,
) -> Result<PatternPart, String> {
    Ok(PatternPart {
        variable: part.variable,
        element: bind_pattern_element(part.element, parameters)?,
    })
}

fn bind_pattern_part_box(
    part: Box<PatternPart>,
    parameters: &BTreeMap<String, Value>,
) -> Result<Box<PatternPart>, String> {
    Ok(Box::new(bind_pattern_part(*part, parameters)?))
}

fn bind_pattern_element(
    element: PatternElement,
    parameters: &BTreeMap<String, Value>,
) -> Result<PatternElement, String> {
    Ok(PatternElement {
        start: bind_node_pattern(element.start, parameters)?,
        chains: element
            .chains
            .into_iter()
            .map(|chain| bind_pattern_element_chain(chain, parameters))
            .collect::<Result<Vec<_>, _>>()?,
    })
}

fn bind_pattern_element_chain(
    chain: PatternElementChain,
    parameters: &BTreeMap<String, Value>,
) -> Result<PatternElementChain, String> {
    Ok(PatternElementChain {
        relationship: bind_relationship_pattern(chain.relationship, parameters)?,
        node: bind_node_pattern(chain.node, parameters)?,
    })
}

fn bind_node_pattern(
    node: NodePattern,
    parameters: &BTreeMap<String, Value>,
) -> Result<NodePattern, String> {
    Ok(NodePattern {
        variable: node.variable,
        labels: node.labels,
        properties: node
            .properties
            .map(|expr| bind_pattern_properties(expr, parameters))
            .transpose()?,
    })
}

fn bind_relationship_pattern(
    relationship: RelationshipPattern,
    parameters: &BTreeMap<String, Value>,
) -> Result<RelationshipPattern, String> {
    Ok(RelationshipPattern {
        variable: relationship.variable,
        types: relationship.types,
        range: relationship.range,
        direction: relationship.direction,
        properties: relationship
            .properties
            .map(|expr| bind_pattern_properties(expr, parameters))
            .transpose()?,
        recursive: relationship
            .recursive
            .map(|recursive| bind_recursive(recursive, parameters))
            .transpose()?,
    })
}

fn bind_pattern_properties(
    expr: Expr,
    parameters: &BTreeMap<String, Value>,
) -> Result<Expr, String> {
    if let Expr::Parameter(name) = &expr {
        match parameters.get(name) {
            Some(Value::Map(_)) => {}
            Some(value) => {
                return Err(format!(
                    "pattern parameter ${name} must be a map, got {}",
                    value.type_name()
                ));
            }
            None => return Err(format!("missing query parameter ${name}")),
        }
    }
    bind_expr(expr, parameters)
}

fn bind_recursive(
    recursive: RecursiveRelationshipPattern,
    parameters: &BTreeMap<String, Value>,
) -> Result<RecursiveRelationshipPattern, String> {
    Ok(RecursiveRelationshipPattern {
        rel_variable: recursive.rel_variable,
        node_variable: recursive.node_variable,
        predicate: recursive
            .predicate
            .map(|expr| bind_expr(expr, parameters))
            .transpose()?,
        rel_projection_keys: recursive.rel_projection_keys,
        node_projection_keys: recursive.node_projection_keys,
    })
}

fn value_to_expr(value: &Value) -> Result<Expr, String> {
    Ok(match value {
        Value::Null => Expr::Literal(Literal::Null),
        Value::Bool(value) => Expr::Literal(Literal::Bool(*value)),
        Value::Byte(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::UInt8(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::Short(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::UInt16(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::Int(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::UInt32(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::Long(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::UInt64(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::Float32(value) if value.is_finite() => Expr::Literal(Literal::Float(*value as f64)),
        Value::Float(value) if value.is_finite() => Expr::Literal(Literal::Float(*value)),
        Value::BigInt(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::UInt128(value) => Expr::Literal(Literal::Integer(value.to_string())),
        Value::String(value) => Expr::Literal(Literal::String(value.clone())),
        Value::List(items) => Expr::List(
            items
                .iter()
                .map(value_to_expr)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Value::Map(entries) => Expr::Map(
            entries
                .iter()
                .map(|(key, value)| Ok((key.clone(), value_to_expr(value)?)))
                .collect::<Result<Vec<_>, String>>()?,
        ),
        value => {
            return Err(format!(
                "unsupported parameter value of type `{}`",
                value.type_name()
            ));
        }
    })
}
