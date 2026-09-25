//! Scalar expression lowering and static type inference.

use super::aggregate::aggregate_kind;
use super::{
    BinaryOp, CypherPlanError, CypherPlanResult, Expr, IrBinaryOp, IrExpr, IrQuantifierKind, Lit,
    Literal, Lowerer, PropertyMissing, QuantifierKind, StringOp, UnaryOp,
};
use crate::language::cypher::planner::CypherSemanticError;
pub(super) fn lower_binary_op(op: BinaryOp) -> IrBinaryOp {
    match op {
        BinaryOp::Or => IrBinaryOp::Or,
        BinaryOp::And => IrBinaryOp::And,
        BinaryOp::Eq => IrBinaryOp::Eq,
        BinaryOp::Neq => IrBinaryOp::Neq,
        BinaryOp::Lt => IrBinaryOp::Lt,
        BinaryOp::Lte => IrBinaryOp::Lte,
        BinaryOp::Gt => IrBinaryOp::Gt,
        BinaryOp::Gte => IrBinaryOp::Gte,
        BinaryOp::Add => IrBinaryOp::Add,
        BinaryOp::Sub => IrBinaryOp::Sub,
        BinaryOp::Mul => IrBinaryOp::Mul,
        BinaryOp::Div => IrBinaryOp::Div,
    }
}

pub(super) fn lower_cypher_binary_expr(op: BinaryOp, lhs: IrExpr, rhs: IrExpr) -> IrExpr {
    let function_name = match op {
        BinaryOp::Eq => Some("cypher_eq"),
        BinaryOp::Neq => Some("cypher_neq"),
        BinaryOp::Lt => Some("cypher_lt"),
        BinaryOp::Lte => Some("cypher_lte"),
        BinaryOp::Gt => Some("cypher_gt"),
        BinaryOp::Gte => Some("cypher_gte"),
        _ => None,
    };
    if let Some(name) = function_name {
        return IrExpr::Call {
            name: name.to_string(),
            args: vec![lhs, rhs],
        };
    }
    IrExpr::Binary {
        op: lower_binary_op(op),
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

pub(super) fn lower_string_op(op: crate::language::cypher::ast::StringPredicateOp) -> StringOp {
    match op {
        crate::language::cypher::ast::StringPredicateOp::StartsWith => StringOp::StartsWith,
        crate::language::cypher::ast::StringPredicateOp::EndsWith => StringOp::EndsWith,
        crate::language::cypher::ast::StringPredicateOp::Contains => StringOp::Contains,
        crate::language::cypher::ast::StringPredicateOp::Regex => {
            unreachable!("regex string predicates lower through regex_match")
        }
    }
}

pub(super) fn lower_string_predicate_expr(
    op: crate::language::cypher::ast::StringPredicateOp,
    target: IrExpr,
    pattern: IrExpr,
) -> IrExpr {
    match op {
        crate::language::cypher::ast::StringPredicateOp::Regex => IrExpr::Call {
            name: "regex_match".to_string(),
            args: vec![target, pattern],
        },
        _ => IrExpr::StringPredicate {
            op: lower_string_op(op),
            target: Box::new(target),
            pattern: Box::new(pattern),
        },
    }
}

pub(super) fn lower_label_predicate_expr(target: IrExpr, labels: &[String]) -> IrExpr {
    let parts = labels
        .iter()
        .map(|label| match &target {
            IrExpr::Binding(binding) => IrExpr::Call { name: "cypher_has_label".into(), args: vec![IrExpr::Binding(binding.clone()), IrExpr::Lit(Lit::String(label.clone()))] },
            _ => IrExpr::Call {
                name: "in".to_string(),
                args: vec![
                    IrExpr::Lit(Lit::String(label.clone())),
                    IrExpr::Call {
                        name: "labels".to_string(),
                        args: vec![target.clone()],
                    },
                ],
            },
        })
        .collect();
    IrExpr::and(parts)
}

pub fn lower_expr(lowerer: &Lowerer, expr: &Expr) -> CypherPlanResult<IrExpr> {
    Ok(match expr {
        Expr::Star => IrExpr::Call {
            name: "cypher_star".to_string(),
            args: lowerer
                .visible_fields()
                .into_iter()
                .map(IrExpr::Binding)
                .collect(),
        },
        Expr::Variable(name) => IrExpr::Binding(name.clone()),
        Expr::Property { target, key } if key == "*" => IrExpr::Call {
            name: "cypher_property_star".to_string(),
            args: vec![lower_expr(lowerer, target)?],
        },
        Expr::Property { target, key } => match target.as_ref() {
            Expr::Variable(binding)
                if matches!(lowerer.binding_kind(binding), Some(
                    crate::language::cypher::semantics::BindingKind::Node
                    | crate::language::cypher::semantics::BindingKind::Relationship
                    | crate::language::cypher::semantics::BindingKind::RecursiveRelationship
                )) =>
            {
                IrExpr::Property {
                    binding: binding.clone(),
                    name: key.clone(),
                    policy: PropertyMissing::NullOnMissing,
                }
            }
            Expr::Variable(binding) => IrExpr::Call {
                name: "cypher_subscript".to_string(),
                args: vec![
                    IrExpr::Binding(binding.clone()),
                    IrExpr::Lit(Lit::String(key.clone())),
                ],
            },
            other => IrExpr::Call {
                name: "property".to_string(),
                args: vec![
                    lower_expr(lowerer, other)?,
                    IrExpr::Lit(Lit::String(key.clone())),
                ],
            },
        },
        Expr::LabelPredicate { target, labels } => match target.as_ref() {
            Expr::Variable(binding) => IrExpr::and(
                labels
                    .iter()
                    .map(|label| IrExpr::Call { name: "cypher_has_label".into(), args: vec![IrExpr::Binding(binding.clone()), IrExpr::Lit(Lit::String(label.clone()))] })
                    .collect(),
            ),
            other => lower_label_predicate_expr(lower_expr(lowerer, other)?, labels),
        },
        Expr::Parameter(name) => IrExpr::Call {
            name: "parameter".to_string(),
            args: vec![IrExpr::Lit(Lit::String(name.clone()))],
        },
        Expr::Literal(lit) => IrExpr::Lit(match lit {
            Literal::Null => Lit::Null,
            Literal::Bool(value) => Lit::Bool(*value),
            Literal::Integer(value) => {
                if let Ok(value) = value.parse::<i64>() {
                    Lit::Int(value)
                } else {
                    return Ok(IrExpr::Call {
                        name: "integer_literal".to_string(),
                        args: vec![IrExpr::Lit(Lit::String(value.clone()))],
                    });
                }
            }
            Literal::Float(value) => Lit::Float(*value),
            Literal::String(value) => Lit::String(value.clone()),
        }),
        Expr::List(items) => IrExpr::List(
            items
                .iter()
                .map(|item| lower_expr(lowerer, item))
                .collect::<CypherPlanResult<_>>()?,
        ),
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => IrExpr::ListReduce {
            collection: Box::new(lower_expr(lowerer, collection)?),
            accumulator: accumulator.clone(),
            item: variable.clone(),
            map: Box::new(lower_expr(lowerer, map)?),
        },
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => IrExpr::ListTransform {
            list: Box::new(lower_expr(lowerer, collection)?),
            item: variable.clone(),
            map: Box::new(lower_expr(lowerer, map)?),
        },
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => IrExpr::ListFilter {
            list: Box::new(lower_expr(lowerer, collection)?),
            item: variable.clone(),
            predicate: Box::new(lower_expr(lowerer, predicate)?),
        },
        Expr::Map(items) => IrExpr::Call {
            name: "map".to_string(),
            args: items
                .iter()
                .flat_map(|(key, value)| {
                    vec![
                        Ok(IrExpr::Lit(Lit::String(key.clone()))),
                        lower_expr(lowerer, value),
                    ]
                })
                .collect::<CypherPlanResult<_>>()?,
        },
        Expr::Unary { op, expr } => match op {
            UnaryOp::Not => IrExpr::Not(Box::new(lower_expr(lowerer, expr)?)),
            UnaryOp::Neg if is_typed_negate_operand(expr) => IrExpr::Call {
                name: "negate".to_string(),
                args: vec![lower_expr(lowerer, expr)?],
            },
            UnaryOp::Neg => IrExpr::Binary {
                op: IrBinaryOp::Sub,
                lhs: Box::new(IrExpr::Lit(Lit::Int(0))),
                rhs: Box::new(lower_expr(lowerer, expr)?),
            },
        },
        Expr::Binary { op, lhs, rhs } => {
            let lhs = lower_expr(lowerer, lhs)?;
            let rhs = lower_expr(lowerer, rhs)?;
            lower_cypher_binary_expr(*op, lhs, rhs)
        }
        Expr::IsNull(expr) => IrExpr::IsNull(Box::new(lower_expr(lowerer, expr)?)),
        Expr::IsNotNull(expr) => IrExpr::IsNotNull(Box::new(lower_expr(lowerer, expr)?)),
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => lower_string_predicate_expr(
            *op,
            lower_expr(lowerer, target)?,
            lower_expr(lowerer, pattern)?,
        ),
        Expr::Function {
            name,
            distinct,
            args,
        } => {
            if matches!(name.to_ascii_lowercase().as_str(), "date" | "localtime" | "time" | "localdatetime" | "datetime" | "duration")
                || ["date.", "localtime.", "time.", "localdatetime.", "datetime.", "duration."].iter().any(|prefix| name.to_ascii_lowercase().starts_with(prefix)) {
                return Ok(IrExpr::Call { name: format!("cypher_temporal.{}", name.to_ascii_lowercase()),
                    args: args.iter().map(|arg| lower_expr(lowerer, arg)).collect::<CypherPlanResult<_>>()? });
            }
            if name.eq_ignore_ascii_case("id") && args.len() == 1 {
                return Ok(IrExpr::Call { name: "cypher_id".into(), args: vec![lower_expr(lowerer, &args[0])?] });
            }
            if name.eq_ignore_ascii_case("typeof") && args.len() == 1 {
                if let Some(type_name) = static_typeof_expr(&args[0]) {
                    return Ok(IrExpr::Lit(Lit::String(type_name)));
                }
            }
            // Retain the Kuzu label() extension; openCypher labels() is a list.
            if name.eq_ignore_ascii_case("label")
                && args.len() == 1
            {
                return Ok(IrExpr::Call {
                    name: "cypher_label".to_string(),
                    args: vec![lower_expr(lowerer, &args[0])?],
                });
            }
            if aggregate_kind(name).is_some() {
                return Err(CypherPlanError::Unsupported(
                    "aggregate functions must be lowered through aggregate projection".to_string(),
                )
                .classified(CypherSemanticError::InvalidAggregation));
            }
            if *distinct {
                return Err(CypherPlanError::Invalid(format!(
                    "DISTINCT is only valid for aggregate function `{name}`"
                )));
            }
            IrExpr::Call {
                name: name.clone(),
                args: args
                    .iter()
                    .map(|arg| lower_expr(lowerer, arg))
                    .collect::<CypherPlanResult<_>>()?,
            }
        }
        Expr::CountStar => {
            return Err(CypherPlanError::Unsupported(
                "count(*) must be lowered through aggregate projection".to_string(),
            )
            .classified(CypherSemanticError::InvalidAggregation));
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            let case_expr = case
                .as_ref()
                .map(|expr| lower_expr(lowerer, expr))
                .transpose()?;
            IrExpr::Case {
                arms: arms
                    .iter()
                    .map(|(when, then)| {
                        let condition = if let Some(case_expr) = &case_expr {
                            IrExpr::Binary {
                                op: IrBinaryOp::Eq,
                                lhs: Box::new(case_expr.clone()),
                                rhs: Box::new(lower_expr(lowerer, when)?),
                            }
                        } else {
                            lower_expr(lowerer, when)?
                        };
                        Ok((condition, lower_expr(lowerer, then)?))
                    })
                    .collect::<CypherPlanResult<_>>()?,
                otherwise: otherwise
                    .as_ref()
                    .map(|expr| lower_expr(lowerer, expr).map(Box::new))
                    .transpose()?,
            }
        }
        Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::ListComprehension { .. }
        | Expr::PatternComprehension { .. }
        | Expr::Quantifier { .. } => {
            return Err(CypherPlanError::Unsupported(
                "scoped Cypher expression reached scalar lowering without materialization"
                    .to_string(),
            ));
        }
    })
}

pub(super) fn static_typeof_expr(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Literal(Literal::Null) => Some("NULL".to_string()),
        Expr::Literal(Literal::Bool(_)) => Some("BOOL".to_string()),
        Expr::Literal(Literal::Float(_)) => Some("DOUBLE".to_string()),
        Expr::Literal(Literal::String(_)) => Some("STRING".to_string()),
        Expr::Literal(Literal::Integer(value)) => Some(integer_literal_type(value).to_string()),
        Expr::List(items) => {
            let item_type = items
                .iter()
                .filter_map(static_typeof_expr)
                .reduce(unify_numeric_type)
                .unwrap_or_else(|| "ANY".to_string());
            Some(format!("{item_type}[]"))
        }
        Expr::Map(items) => {
            let fields = items
                .iter()
                .map(|(key, value)| {
                    format!(
                        "{key} {}",
                        static_typeof_expr(value).unwrap_or_else(|| "ANY".to_string())
                    )
                })
                .collect::<Vec<_>>();
            Some(format!("STRUCT({})", fields.join(", ")))
        }
        Expr::Function { name, args, .. }
            if name.eq_ignore_ascii_case("cast") && args.len() == 2 =>
        {
            match &args[1] {
                Expr::Literal(Literal::String(type_name)) => Some(type_name.to_ascii_uppercase()),
                _ => None,
            }
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("date") => {
            Some("DATE".to_string())
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("timestamp") => {
            Some("TIMESTAMP".to_string())
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("interval") => {
            Some("INTERVAL".to_string())
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("blob") => {
            Some("BLOB".to_string())
        }
        Expr::Function { name, .. } if name.eq_ignore_ascii_case("gen_random_uuid") => {
            Some("UUID".to_string())
        }
        Expr::Function { name, args, .. }
            if name.eq_ignore_ascii_case("map") && args.len() == 2 =>
        {
            Some(format!(
                "MAP({}, {})",
                static_collection_item_type(&args[0]).unwrap_or_else(|| "ANY".to_string()),
                static_collection_item_type(&args[1]).unwrap_or_else(|| "ANY".to_string())
            ))
        }
        Expr::Function { name, args, .. } if name.eq_ignore_ascii_case("union_value") => {
            if let [Expr::Literal(Literal::String(tag)), value] = args.as_slice() {
                Some(format!(
                    "UNION({tag} {})",
                    static_typeof_expr(value).unwrap_or_else(|| "ANY".to_string())
                ))
            } else if let [Expr::Map(fields)] = args.as_slice() {
                fields.first().map(|(tag, value)| {
                    format!(
                        "UNION({tag} {})",
                        static_typeof_expr(value).unwrap_or_else(|| "ANY".to_string())
                    )
                })
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(super) fn static_collection_item_type(expr: &Expr) -> Option<String> {
    static_typeof_expr(expr).map(|type_name| {
        type_name
            .strip_suffix("[]")
            .unwrap_or(type_name.as_str())
            .to_string()
    })
}

pub(super) fn integer_literal_type(value: &str) -> &'static str {
    if value.parse::<i64>().is_ok() {
        "INT64"
    } else if value.parse::<i128>().is_ok() {
        "INT128"
    } else {
        "UINT128"
    }
}

pub(super) fn unify_numeric_type(left: String, right: String) -> String {
    if left == right {
        return left;
    }
    let rank = |value: &str| match value {
        "UINT128" => 4,
        "INT128" => 3,
        "DOUBLE" | "FLOAT" => 2,
        "INT64" | "INT32" | "INT16" | "INT8" | "UINT64" | "UINT32" | "UINT16" | "UINT8" => 1,
        _ => 0,
    };
    if rank(&right) > rank(&left) {
        right
    } else {
        left
    }
}

/// True when a unary minus applies to an explicitly typed value
/// (`-CAST(x, 'UINT8')`, `-to_int8(x)`), where Kuzu's typed NEGATE
/// semantics (unsigned wrap-around, INT_MIN overflow error) differ
/// from plain `0 - x` arithmetic.
pub(super) fn is_typed_negate_operand(expr: &Expr) -> bool {
    match expr {
        Expr::Function { name, .. } => {
            let lower = name.to_ascii_lowercase();
            lower == "cast"
                || lower.starts_with("to_int")
                || lower.starts_with("to_uint")
                || matches!(
                    lower.as_str(),
                    "toint8"
                        | "toint16"
                        | "toint32"
                        | "toint64"
                        | "touint8"
                        | "touint16"
                        | "touint32"
                        | "touint64"
                )
        }
        _ => false,
    }
}

/// Render an expression the way Kuzu's binder prints it inside error
/// messages (`COUNT(COUNT_STAR())`, `SUM(SUM(a.age))`, ...). Best
/// effort: unhandled shapes render as `...`.
pub(super) fn render_kuzu_expr(expr: &Expr) -> String {
    match expr {
        Expr::CountStar => "COUNT_STAR()".to_string(),
        Expr::Variable(name) => name.clone(),
        Expr::Property { target, key } => format!("{}.{}", render_kuzu_expr(target), key),
        Expr::Function { name, args, .. } => format!(
            "{}({})",
            name.to_ascii_uppercase(),
            args.iter()
                .map(render_kuzu_expr)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => "...".to_string(),
    }
}

pub(super) fn lower_quantifier_kind(kind: QuantifierKind) -> IrQuantifierKind {
    match kind {
        QuantifierKind::All => IrQuantifierKind::All,
        QuantifierKind::Any => IrQuantifierKind::Any,
        QuantifierKind::None => IrQuantifierKind::None,
        QuantifierKind::Single => IrQuantifierKind::Single,
    }
}
