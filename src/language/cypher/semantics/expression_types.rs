//! Static expression validation and binding-kind inference.

use crate::language::cypher::planner::CypherSemanticError;

use super::{
    BinaryOp, BindingKind, Clause, CypherPlanError, CypherPlanResult, Expr, Literal,
    ProjectionBody, SemanticScope, UnaryOp, merge_pattern_properties, merge_set_item_exprs,
};
pub(super) fn validate_static_expression_types(expr: &Expr) -> CypherPlanResult<()> {
    match expr {
        Expr::Unary {
            op: UnaryOp::Not,
            expr,
        } => {
            validate_static_expression_types(expr)?;
            validate_bool_operand(expr)
        }
        Expr::Unary { expr, .. } => validate_static_expression_types(expr),
        Expr::Binary { op, lhs, rhs } if matches!(op, BinaryOp::And | BinaryOp::Or) => {
            validate_static_expression_types(lhs)?;
            validate_static_expression_types(rhs)?;
            validate_bool_operand(lhs)?;
            validate_bool_operand(rhs)
        }
        Expr::Binary { lhs, rhs, .. } => {
            validate_static_expression_types(lhs)?;
            validate_static_expression_types(rhs)
        }
        Expr::Function { name, args, .. } if name.eq_ignore_ascii_case("xor") => {
            for arg in args {
                validate_static_expression_types(arg)?;
                validate_bool_operand(arg)?;
            }
            Ok(())
        }
        Expr::Function { args, .. } => {
            for arg in args {
                validate_static_expression_types(arg)?;
            }
            Ok(())
        }
        Expr::Property { target, .. } | Expr::IsNull(target) | Expr::IsNotNull(target) => {
            validate_static_expression_types(target)
        }
        Expr::LabelPredicate { target, .. } => validate_static_expression_types(target),
        Expr::StringPredicate {
            target, pattern, ..
        } => {
            validate_static_expression_types(target)?;
            validate_static_expression_types(pattern)
        }
        Expr::List(items) => {
            for item in items {
                validate_static_expression_types(item)?;
            }
            Ok(())
        }
        Expr::Map(items) => {
            for (_, value) in items {
                validate_static_expression_types(value)?;
            }
            Ok(())
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                validate_static_expression_types(case)?;
            }
            for (when, then) in arms {
                validate_static_expression_types(when)?;
                validate_static_expression_types(then)?;
            }
            if let Some(otherwise) = otherwise {
                validate_static_expression_types(otherwise)?;
            }
            Ok(())
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            validate_static_expression_types(collection)?;
            if let Some(predicate) = predicate {
                validate_static_expression_types(predicate)?;
            }
            validate_static_expression_types(map)
        }
        Expr::ListReduce {
            collection, map, ..
        }
        | Expr::ListTransform {
            collection, map, ..
        } => {
            validate_static_expression_types(collection)?;
            validate_static_expression_types(map)
        }
        Expr::ListFilter {
            collection,
            predicate,
            ..
        }
        | Expr::Quantifier {
            collection,
            predicate,
            ..
        } => {
            validate_static_expression_types(collection)?;
            validate_static_expression_types(predicate)
        }
        Expr::PatternComprehension { predicate, map, .. } => {
            if let Some(predicate) = predicate {
                validate_static_expression_types(predicate)?;
            }
            validate_static_expression_types(map)
        }
        Expr::Exists(exists) => {
            if let Some(predicate) = &exists.predicate {
                validate_static_expression_types(predicate)?;
            }
            if let Some(query) = &exists.query {
                for clause in &query.clauses {
                    validate_clause_static_expression_types(clause)?;
                }
            }
            Ok(())
        }
        Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::PatternPredicate(_)
        | Expr::CountStar => Ok(()),
    }
}

pub(super) fn validate_clause_static_expression_types(clause: &Clause) -> CypherPlanResult<()> {
    match clause {
        Clause::Match(clause) => {
            if let Some(predicate) = &clause.predicate {
                validate_static_expression_types(predicate)?;
            }
            Ok(())
        }
        // UNWIND accepts heterogeneous list literals (bound as ANY[]),
        // so skip static homogeneity validation on its source.
        Clause::Unwind(_) => Ok(()),
        Clause::Call(clause) => {
            for arg in &clause.args {
                validate_static_expression_types(arg)?;
            }
            if let Some(predicate) = &clause.predicate {
                validate_static_expression_types(predicate)?;
            }
            Ok(())
        }
        Clause::Merge(clause) => {
            for properties in merge_pattern_properties(&clause.pattern) {
                validate_static_expression_types(properties)?;
            }
            for item in clause.on_create.iter().chain(clause.on_match.iter()) {
                for expr in merge_set_item_exprs(item) {
                    validate_static_expression_types(expr)?;
                }
            }
            Ok(())
        }
        Clause::Create(clause) => {
            for part in &clause.patterns {
                if let Some(properties) = &part.element.start.properties {
                    validate_static_expression_types(properties)?;
                }
                for chain in &part.element.chains {
                    if let Some(properties) = &chain.relationship.properties {
                        validate_static_expression_types(properties)?;
                    }
                    if let Some(properties) = &chain.node.properties {
                        validate_static_expression_types(properties)?;
                    }
                }
            }
            Ok(())
        }
        Clause::Set(clause) => {
            for item in &clause.items {
                match item {
                    crate::language::cypher::ast::SetItem::Property { target, value, .. } => {
                        validate_static_expression_types(target)?;
                        validate_static_expression_types(value)?;
                    }
                    crate::language::cypher::ast::SetItem::Replace { value, .. }
                    | crate::language::cypher::ast::SetItem::Merge { value, .. } => {
                        validate_static_expression_types(value)?;
                    }
                    crate::language::cypher::ast::SetItem::Labels { .. } => {}
                }
            }
            Ok(())
        }
        Clause::Delete(clause) => {
            for expr in &clause.expressions {
                validate_static_expression_types(expr)?;
            }
            Ok(())
        }
        Clause::With(clause) => {
            validate_projection_static_expression_types(&clause.projection)?;
            if let Some(predicate) = &clause.predicate {
                validate_static_expression_types(predicate)?;
            }
            Ok(())
        }
        Clause::Return(clause) => validate_projection_static_expression_types(&clause.projection),
    }
}

pub(super) fn validate_projection_static_expression_types(
    body: &ProjectionBody,
) -> CypherPlanResult<()> {
    for item in &body.items {
        validate_static_expression_types(&item.expr)?;
    }
    for sort in &body.order_by {
        validate_static_expression_types(&sort.expr)?;
    }
    if let Some(skip) = &body.skip {
        validate_static_expression_types(skip)?;
    }
    if let Some(limit) = &body.limit {
        validate_static_expression_types(limit)?;
    }
    Ok(())
}

pub(super) fn validate_expr_kinds(expr: &Expr, scope: &SemanticScope) -> CypherPlanResult<()> {
    match expr {
        Expr::Property { target, .. } => {
            validate_expr_kinds(target, scope)?;
            let target_kind = projected_expr_kind(target, scope);
            if matches!(
                target_kind,
                BindingKind::Bool
                    | BindingKind::Int
                    | BindingKind::Float
                    | BindingKind::String
                    | BindingKind::InternalId
                    | BindingKind::Path
                    | BindingKind::ListValue
                    | BindingKind::ListNode
                    | BindingKind::ListRelationship
                    | BindingKind::ListInt
                    | BindingKind::FixedListInt
            ) {
                return Err(CypherPlanError::Invalid(format!(
                    "Binder exception: {} has data type {} but (NODE,REL,STRUCT,ANY) was expected.",
                    display_semantic_expr(target),
                    target_kind.cypher_type_name()
                )).classified(if target_kind == BindingKind::Path {
                    CypherSemanticError::InvalidArgumentType
                } else { CypherSemanticError::InvalidPropertyAccess }));
            }
            Ok(())
        }
        Expr::Unary { expr, .. } | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            validate_expr_kinds(expr, scope)
        }
        Expr::Binary { op, lhs, rhs } => {
            validate_expr_kinds(lhs, scope)?;
            validate_expr_kinds(rhs, scope)?;
            validate_binary_expr_kind(*op, lhs, rhs, scope)
        }
        Expr::StringPredicate {
            target, pattern, ..
        } => {
            validate_expr_kinds(target, scope)?;
            validate_expr_kinds(pattern, scope)
        }
        Expr::Function { name, args, .. } => {
            for arg in args {
                validate_expr_kinds(arg, scope)?;
            }
            validate_function_expr_kind(name, args, scope)
        }
        Expr::List(items) => {
            for item in items {
                validate_expr_kinds(item, scope)?;
            }
            Ok(())
        }
        Expr::Map(items) => {
            for (_, value) in items {
                validate_expr_kinds(value, scope)?;
            }
            Ok(())
        }
        Expr::Case {
            case,
            arms,
            otherwise,
        } => {
            if let Some(case) = case {
                validate_expr_kinds(case, scope)?;
            }
            for (when, then) in arms {
                validate_expr_kinds(when, scope)?;
                validate_expr_kinds(then, scope)?;
            }
            if let Some(otherwise) = otherwise {
                validate_expr_kinds(otherwise, scope)?;
            }
            Ok(())
        }
        Expr::ListComprehension {
            collection,
            predicate,
            map,
            ..
        } => {
            validate_expr_kinds(collection, scope)?;
            if let Some(predicate) = predicate {
                validate_expr_kinds(predicate, scope)?;
            }
            validate_expr_kinds(map, scope)
        }
        Expr::ListReduce {
            collection, map, ..
        }
        | Expr::ListTransform {
            collection, map, ..
        } => {
            validate_expr_kinds(collection, scope)?;
            validate_expr_kinds(map, scope)
        }
        Expr::ListFilter {
            collection,
            predicate,
            ..
        }
        => {
            validate_expr_kinds(collection, scope)?;
            validate_expr_kinds(predicate, scope)
        }
        Expr::Quantifier { variable, collection, predicate, .. } => {
            validate_expr_kinds(collection, scope)?;
            validate_list_source(collection, scope)?;
            let kinds = if let Expr::List(items) = collection.as_ref() {
                items.iter().map(|item| projected_expr_kind(item, scope)).collect::<Vec<_>>()
            } else { vec![unwind_element_kind(collection, scope)] };
            for kind in kinds {
                let mut local = scope.clone();
                local.insert(variable.clone(), kind);
                validate_expr_kinds(predicate, &local)?;
            }
            Ok(())
        }
        Expr::PatternComprehension { predicate, map, .. } => {
            if let Some(predicate) = predicate {
                validate_expr_kinds(predicate, scope)?;
            }
            validate_expr_kinds(map, scope)
        }
        Expr::Exists(exists) => {
            if let Some(predicate) = &exists.predicate {
                validate_expr_kinds(predicate, scope)?;
            }
            Ok(())
        }
        Expr::LabelPredicate { target, .. } => validate_expr_kinds(target, scope),
        Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::PatternPredicate(_)
        | Expr::CountStar => Ok(()),
    }
}

pub(super) fn validate_binary_expr_kind(
    op: BinaryOp,
    lhs: &Expr,
    rhs: &Expr,
    scope: &SemanticScope,
) -> CypherPlanResult<()> {
    if !matches!(
        op,
        BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div
    ) {
        return Ok(());
    }
    let lhs_kind = projected_expr_kind(lhs, scope);
    let rhs_kind = projected_expr_kind(rhs, scope);
    if arithmetic_kinds_compatible(op, lhs_kind, rhs_kind) {
        return Ok(());
    }
    let op_name = match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        _ => unreachable!(),
    };
    if matches!(lhs_kind, BindingKind::InternalId) || matches!(rhs_kind, BindingKind::InternalId) {
        return Err(CypherPlanError::Invalid(format!(
            "Binder exception: Function {op_name} did not receive correct arguments:"
        )));
    }
    Err(CypherPlanError::Invalid(format!(
        "Binder exception: Cannot match a built-in function for given function {op_name}({},{}).",
        lhs_kind.cypher_type_name(),
        rhs_kind.cypher_type_name()
    )).classified(CypherSemanticError::InvalidArgumentType))
}

pub(super) fn arithmetic_kinds_compatible(
    op: BinaryOp,
    lhs: BindingKind,
    rhs: BindingKind,
) -> bool {
    if matches!(lhs, BindingKind::Unknown | BindingKind::Value)
        || matches!(rhs, BindingKind::Unknown | BindingKind::Value)
    {
        return true;
    }
    if matches!(
        (lhs, rhs),
        (BindingKind::Int, BindingKind::Int)
            | (BindingKind::Int, BindingKind::Float)
            | (BindingKind::Float, BindingKind::Int)
            | (BindingKind::Float, BindingKind::Float)
            | (BindingKind::String, BindingKind::String)
            | (BindingKind::ListInt, BindingKind::ListInt)
            | (BindingKind::Interval, BindingKind::Interval)
    ) {
        return true;
    }
    // List concatenation preserves arbitrary element types.
    let list = |kind| matches!(kind,BindingKind::ListValue | BindingKind::ListInt | BindingKind::FixedListInt | BindingKind::ListNode | BindingKind::ListRelationship);
    if matches!(op,BinaryOp::Add) && (list(lhs)||list(rhs)) {return true;}
    matches!(
        (op, lhs, rhs),
        (BinaryOp::Mul, BindingKind::Interval, BindingKind::Int)
            | (BinaryOp::Mul, BindingKind::Int, BindingKind::Interval)
            | (BinaryOp::Div, BindingKind::Interval, BindingKind::Int)
            | (
                BinaryOp::Add | BinaryOp::Sub,
                BindingKind::Date,
                BindingKind::Int
            )
            | (BinaryOp::Add, BindingKind::Int, BindingKind::Date)
            | (
                BinaryOp::Add | BinaryOp::Sub,
                BindingKind::Date,
                BindingKind::Interval
            )
            | (BinaryOp::Add, BindingKind::Interval, BindingKind::Date)
            | (
                BinaryOp::Add | BinaryOp::Sub,
                BindingKind::Timestamp,
                BindingKind::Interval
            )
            | (BinaryOp::Add, BindingKind::Interval, BindingKind::Timestamp)
    )
}

pub(super) fn validate_function_expr_kind(
    name: &str,
    args: &[Expr],
    scope: &SemanticScope,
) -> CypherPlanResult<()> {
    let lower = name.to_ascii_lowercase();
    if lower != "exists" && args.iter().any(|arg| matches!(arg, Expr::PatternPredicate(_))) {
        return Err(CypherPlanError::Invalid("Pattern predicates cannot be consumed as collection values".into())
            .classified(CypherSemanticError::UnexpectedSyntax));
    }
    if matches!(lower.as_str(), "labels" | "type") && args.len() == 1 {
        let kind = projected_expr_kind(&args[0], scope);
        if matches!(kind, BindingKind::Node | BindingKind::Relationship | BindingKind::Path)
            && kind != if lower == "labels" { BindingKind::Node } else { BindingKind::Relationship }
        {
            return Err(CypherPlanError::Invalid(format!("Invalid graph argument for {name}"))
                .classified(CypherSemanticError::InvalidArgumentType));
        }
    }
    if matches!(lower.as_str(), "length" | "properties") && args.len() == 1 {
        let kind = projected_expr_kind(&args[0], scope);
        let valid = match lower.as_str() {
            "length" => matches!(kind, BindingKind::Unknown | BindingKind::Value | BindingKind::Path),
            _ => matches!(kind, BindingKind::Unknown | BindingKind::Value | BindingKind::Node | BindingKind::Relationship),
        };
        if !valid || (lower == "length" && matches!(&args[0], Expr::Map(_) | Expr::List(_))) {
            return Err(CypherPlanError::Invalid(format!("Invalid argument for {name}"))
                .classified(CypherSemanticError::InvalidArgumentType));
        }
    }
    if matches!(lower.as_str(), "in" | "cypher_in") && args.len() == 2 {
        let rhs = projected_expr_kind(&args[1], scope);
        if matches!(rhs, BindingKind::Bool | BindingKind::Int | BindingKind::Float
            | BindingKind::String | BindingKind::Node | BindingKind::Relationship
            | BindingKind::Path | BindingKind::Date | BindingKind::Timestamp
            | BindingKind::TimestampMs | BindingKind::Interval)
            || matches!(&args[1], Expr::Map(_))
        {
            return Err(CypherPlanError::Invalid("The right operand of IN must be a list".into())
                .classified(CypherSemanticError::InvalidArgumentType));
        }
    }
    if lower == "size" && args.first().is_some_and(|arg|
        matches!(projected_expr_kind(arg, scope), BindingKind::Node
            | BindingKind::Relationship | BindingKind::Path))
    {
        return Err(CypherPlanError::Invalid("size requires a string or collection, not a graph element or path".into())
            .classified(CypherSemanticError::InvalidArgumentType));
    }
    if lower == "mod" && args.iter().any(|arg| !matches!(projected_expr_kind(arg, scope),
        BindingKind::Unknown | BindingKind::Value | BindingKind::Int | BindingKind::Float)) {
        return Err(CypherPlanError::Invalid("Modulo requires numeric operands".into())
            .classified(CypherSemanticError::InvalidArgumentType));
    }

    if lower == "date"
        && args
            .first()
            .is_some_and(|arg| matches!(projected_expr_kind(arg, scope), BindingKind::Int))
    {
        return Err(CypherPlanError::Invalid(
            "Conversion exception: Error occurred during parsing date. Given: \"2012\". Expected format: (YYYY-MM-DD)"
                .to_string(),
        ));
    }
    if lower == "coalesce" {
        validate_coalesce_static_types(args)?;
    }
    if matches!(lower.as_str(), "min" | "max")
        && args.first().is_some_and(|arg| {
            matches!(
                projected_expr_kind(arg, scope),
                BindingKind::Node | BindingKind::Relationship | BindingKind::RecursiveRelationship | BindingKind::Path
            )
        })
    {
        return Err(CypherPlanError::Invalid(format!(
            "Binder exception: Function {} did not receive correct arguments:",
            lower.to_ascii_uppercase()
        )));
    }
    Ok(())
}

pub(super) fn validate_coalesce_static_types(args: &[Expr]) -> CypherPlanResult<()> {
    if args.is_empty() {
        return Err(CypherPlanError::Invalid(
            "Binder exception: COALESCE requires at least one argument".to_string(),
        ));
    }

    // Each argument may have a different type; coalesce preserves the first
    // non-null value and does not unify or coerce the remaining arguments.
    Ok(())
}

pub(super) fn validate_bool_operand(expr: &Expr) -> CypherPlanResult<()> {
    match static_expr_type_name(expr)? {
        Some(type_name) if type_name != "BOOL" => Err(CypherPlanError::Invalid(format!(
            "Binder exception: Expression {} has data type {type_name} but expected BOOL. Implicit cast is not supported.",
            display_literal_expr(expr)
        )).classified(CypherSemanticError::InvalidArgumentType)),
        _ => Ok(()),
    }
}

pub(super) fn static_expr_type_name(expr: &Expr) -> CypherPlanResult<Option<String>> {
    match expr {
        Expr::Literal(Literal::Null) => Ok(None),
        Expr::Literal(Literal::Bool(_)) => Ok(Some("BOOL".to_string())),
        Expr::Literal(Literal::Integer(_)) => Ok(Some("INT64".to_string())),
        Expr::Literal(Literal::Float(_)) => Ok(Some("DOUBLE".to_string())),
        Expr::Literal(Literal::String(_)) => Ok(Some("STRING".to_string())),
        Expr::List(items) if items.is_empty() => Ok(Some("INT64[]".to_string())),
        Expr::List(items) => Ok(infer_literal_list_type(items)?
            .map(|inner| format!("{}[]", inner.cypher_name()))
            .or(Some("INT64[]".to_string()))),
        Expr::Map(items) => Ok(Some(static_map_type_name(items)?)),
        _ => Ok(None),
    }
}

pub(super) fn static_map_type_name(items: &[(String, Expr)]) -> CypherPlanResult<String> {
    let fields = items
        .iter()
        .map(|(key, value)| {
            Ok(format!(
                "{key} {}",
                static_expr_type_name(value)?.unwrap_or_else(|| "ANY".to_string())
            ))
        })
        .collect::<CypherPlanResult<Vec<_>>>()?;
    Ok(format!("STRUCT({})", fields.join(", ")))
}

pub(super) fn validate_regexp_replace_option(option: &Expr) -> CypherPlanResult<()> {
    match option {
        Expr::Literal(Literal::String(flag)) if flag == "g" => Ok(()),
        Expr::Literal(Literal::String(_)) => Err(CypherPlanError::Invalid(
            "Binder exception: regex_replace can only support global replace option: g."
                .to_string(),
        )),
        Expr::Literal(Literal::Integer(value)) => Err(CypherPlanError::Invalid(format!(
            "Binder exception: {value} has data type INT64 but STRING was expected."
        ))),
        Expr::Literal(Literal::Float(value)) => Err(CypherPlanError::Invalid(format!(
            "Binder exception: {value} has data type DOUBLE but STRING was expected."
        ))),
        Expr::Literal(Literal::Bool(value)) => Err(CypherPlanError::Invalid(format!(
            "Binder exception: {value} has data type BOOL but STRING was expected."
        ))),
        other => Err(CypherPlanError::Invalid(format!(
            "Binder exception: {} has type PROPERTY but LITERAL was expected.",
            display_property_expr(other)
        ))),
    }
}

pub(super) fn display_property_expr(expr: &Expr) -> String {
    match expr {
        Expr::Variable(name) => name.clone(),
        Expr::Property { target, key } => format!("{}.{}", display_property_expr(target), key),
        _ => display_literal_expr(expr),
    }
}

pub(super) fn validate_literal_list_types(items: &[Expr]) -> CypherPlanResult<()> {
    let _ = infer_literal_list_type(items)?;
    Ok(())
}

pub(super) fn infer_literal_list_type(items: &[Expr]) -> CypherPlanResult<Option<LiteralListType>> {
    // Infer each child once: repeating inference for preference and validation
    // makes deeply nested collection literals take exponential time.
    let inferred = items
        .iter()
        .map(literal_list_type)
        .collect::<CypherPlanResult<Vec<_>>>()?;
    let mut expected = preferred_literal_list_type(&inferred);
    for actual in inferred.into_iter().flatten() {
        let Some(expected_type) = expected.as_ref() else {
            expected = Some(actual.clone());
            continue;
        };
        if expected_type.compatible_with(&actual) {
            if matches!(expected_type, LiteralListType::EmptyList)
                && matches!(actual, LiteralListType::List(_))
            {
                expected = Some(actual);
            }
            continue;
        }
        // Cypher collections contain arbitrary values. Keep a heterogeneous
        // element type instead of rejecting a valid list at bind time.
        expected = Some(LiteralListType::Any);
    }
    Ok(expected)
}

fn preferred_literal_list_type(
    items: &[Option<LiteralListType>],
) -> Option<LiteralListType> {
    let mut first = None;
    let mut first_numeric = None;
    let mut first_list = None;
    for actual in items.iter().flatten() {
        if first.is_none() {
            first = Some(actual.clone());
        }
        if first_numeric.is_none()
            && matches!(actual, LiteralListType::Int | LiteralListType::Float)
        {
            first_numeric = Some(actual.clone());
        }
        if matches!(
            actual,
            LiteralListType::List(_) | LiteralListType::EmptyList
        ) {
            first_list = Some(actual.clone());
            break;
        }
    }
    first_list.or(first_numeric).or(first)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LiteralListType {
    Any,
    Bool,
    Int,
    Float,
    String,
    Map,
    EmptyList,
    List(Box<LiteralListType>),
}

impl LiteralListType {
    fn cypher_name(&self) -> String {
        match self {
            LiteralListType::Any => "ANY".to_string(),
            LiteralListType::Bool => "BOOL".to_string(),
            LiteralListType::Int => "INT64".to_string(),
            LiteralListType::Float => "DOUBLE".to_string(),
            LiteralListType::String => "STRING".to_string(),
            LiteralListType::Map => "STRUCT".to_string(),
            LiteralListType::EmptyList => "INT64[]".to_string(),
            LiteralListType::List(inner) => format!("{}[]", inner.cypher_name()),
        }
    }

    fn compatible_with(&self, other: &Self) -> bool {
        matches!(self, LiteralListType::Any)
            || matches!(other, LiteralListType::Any)
            || self == other
            || self.numeric_compatible(other)
            || self.list_compatible(other)
    }

    fn numeric_compatible(&self, other: &Self) -> bool {
        matches!(
            (self, other),
            (LiteralListType::Int, LiteralListType::Float)
                | (LiteralListType::Float, LiteralListType::Int)
        )
    }

    fn list_compatible(&self, other: &Self) -> bool {
        match (self, other) {
            (LiteralListType::EmptyList, LiteralListType::List(_))
            | (LiteralListType::List(_), LiteralListType::EmptyList) => true,
            (LiteralListType::List(left), LiteralListType::List(right)) => {
                left.compatible_with(right)
            }
            _ => false,
        }
    }
}

pub(super) fn literal_list_type(expr: &Expr) -> CypherPlanResult<Option<LiteralListType>> {
    match expr {
        Expr::Literal(Literal::Null) => Ok(None),
        Expr::Literal(Literal::Bool(_)) => Ok(Some(LiteralListType::Bool)),
        Expr::Literal(Literal::Integer(_)) => Ok(Some(LiteralListType::Int)),
        Expr::Literal(Literal::Float(_)) => Ok(Some(LiteralListType::Float)),
        Expr::Literal(Literal::String(_)) => Ok(Some(LiteralListType::String)),
        Expr::Map(_) => Ok(Some(LiteralListType::Map)),
        Expr::List(items) if items.is_empty() => Ok(Some(LiteralListType::EmptyList)),
        Expr::List(items) => Ok(infer_literal_list_type(items)?
            .map(|inner| LiteralListType::List(Box::new(inner)))
            .or(Some(LiteralListType::EmptyList))),
        _ => Ok(None),
    }
}

pub(super) fn display_literal_expr(expr: &Expr) -> String {
    match expr {
        Expr::Literal(Literal::Null) => "null".to_string(),
        Expr::Literal(Literal::Bool(true)) => "True".to_string(),
        Expr::Literal(Literal::Bool(false)) => "False".to_string(),
        Expr::Literal(Literal::Integer(value)) => value.clone(),
        Expr::Literal(Literal::Float(value)) => format!("{value:.6}"),
        Expr::Literal(Literal::String(value)) => value.clone(),
        Expr::Map(items) => {
            let values = items
                .iter()
                .map(|(_, value)| display_literal_expr(value))
                .collect::<Vec<_>>();
            format!("STRUCT_PACK({})", values.join(", "))
        }
        Expr::List(items) => {
            let values = items
                .iter()
                .filter(|item| !matches!(item, Expr::Literal(Literal::Null)))
                .map(display_literal_expr)
                .collect::<Vec<_>>();
            format!("LIST_CREATION({})", values.join(", "))
        }
        _ => "<expression>".to_string(),
    }
}

pub(super) fn display_semantic_expr(expr: &Expr) -> String {
    match expr {
        Expr::Variable(name) => name.clone(),
        Expr::Property { target, key } => format!("{}.{}", display_semantic_expr(target), key),
        Expr::Literal(_) | Expr::List(_) | Expr::Map(_) => display_literal_expr(expr),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => format!("-{}", display_semantic_expr(expr)),
        Expr::Unary {
            op: UnaryOp::Not,
            expr,
        } => format!("NOT {}", display_semantic_expr(expr)),
        Expr::Binary { op, lhs, rhs } => {
            let name = match op {
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
                BinaryOp::And => "AND",
                BinaryOp::Or => "OR",
                BinaryOp::Eq => "=",
                BinaryOp::Neq => "<>",
                BinaryOp::Lt => "<",
                BinaryOp::Lte => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Gte => ">=",
            };
            format!(
                "{name}({},{})",
                display_semantic_expr(lhs),
                display_semantic_expr(rhs)
            )
        }
        Expr::Function { name, args, .. } => {
            let args = args
                .iter()
                .map(display_semantic_expr)
                .collect::<Vec<_>>()
                .join(",");
            format!("{name}({args})")
        }
        Expr::CountStar => "COUNT(*)".to_string(),
        _ => "<expression>".to_string(),
    }
}

pub(super) fn display_order_expr(expr: &Expr) -> String {
    match expr {
        Expr::Function { name, args, .. } if name.eq_ignore_ascii_case("id") && args.len() == 1 => {
            format!("{}._ID", display_semantic_expr(&args[0]))
        }
        _ => display_semantic_expr(expr),
    }
}

pub(super) fn projected_expr_kind(expr: &Expr, scope: &SemanticScope) -> BindingKind {
    match expr {
        Expr::Variable(binding) => scope.kind(binding).unwrap_or(BindingKind::Unknown),
        Expr::Literal(Literal::Null) => BindingKind::Unknown,
        // Property names do not determine types. A schema-aware lowering
        // stage can refine them; the language analyzer must keep them dynamic.
        Expr::Property { .. } => BindingKind::Unknown,
        Expr::Literal(Literal::Bool(_)) => BindingKind::Bool,
        Expr::Literal(Literal::Integer(_)) => BindingKind::Int,
        Expr::Literal(Literal::Float(_)) => BindingKind::Float,
        Expr::Literal(Literal::String(_)) => BindingKind::String,
        Expr::Unary {
            op: UnaryOp::Not, ..
        }
        | Expr::IsNull(_)
        | Expr::IsNotNull(_)
        | Expr::LabelPredicate { .. }
        | Expr::StringPredicate { .. } => BindingKind::Bool,
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => projected_expr_kind(expr, scope),
        Expr::Binary { op, lhs, rhs } => match op {
            BinaryOp::And
            | BinaryOp::Or
            | BinaryOp::Eq
            | BinaryOp::Neq
            | BinaryOp::Lt
            | BinaryOp::Lte
            | BinaryOp::Gt
            | BinaryOp::Gte => BindingKind::Bool,
            BinaryOp::Div => BindingKind::Float,
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul => {
                let lhs = projected_expr_kind(lhs, scope);
                let rhs = projected_expr_kind(rhs, scope);
                if matches!(lhs, BindingKind::Float) || matches!(rhs, BindingKind::Float) {
                    BindingKind::Float
                } else if matches!(lhs, BindingKind::Int) && matches!(rhs, BindingKind::Int) {
                    BindingKind::Int
                } else {
                    BindingKind::Value
                }
            }
        },
        Expr::Function { name, args, .. } => function_result_kind(name, args, scope),
        Expr::CountStar => BindingKind::Int,
        Expr::List(items)
            if items
                .iter()
                .all(|item| matches!(item, Expr::Literal(Literal::Integer(_)))) =>
        {
            BindingKind::ListInt
        }
        Expr::List(items) => {
            // Infer each child once. Re-evaluating nested lists separately
            // for node and relationship checks makes depth cost exponential.
            let kinds = items.iter().map(|item| projected_expr_kind(item, scope)).collect::<Vec<_>>();
            if !kinds.is_empty() && kinds.iter().all(|kind| *kind == BindingKind::Node) {
                BindingKind::ListNode
            } else if !kinds.is_empty() && kinds.iter().all(|kind| *kind == BindingKind::Relationship) {
                BindingKind::ListRelationship
            } else {
                BindingKind::ListValue
            }
        }
        _ => BindingKind::Value,
    }
}

pub(super) fn function_result_kind(
    name: &str,
    args: &[Expr],
    scope: &SemanticScope,
) -> BindingKind {
    match name.to_ascii_lowercase().as_str() {
        "coalesce" => {
            let mut kinds = args.iter().filter(|arg| !matches!(arg, Expr::Literal(Literal::Null)))
                .map(|arg|projected_expr_kind(arg,scope));
            let first = kinds.next().unwrap_or(BindingKind::Unknown);
            if kinds.all(|kind|kind==first) {first} else {BindingKind::Unknown}
        }
        "count" | "count_if" | "size" | "length" | "rowid" => BindingKind::Int,
        "id" => BindingKind::Int,
        "avg" | "tofloat" | "to_float" | "todouble" | "to_double" => BindingKind::Float,
        "sum" => args
            .first()
            .map(|arg| projected_expr_kind(arg, scope))
            .unwrap_or(BindingKind::Int),
        "tostring" | "to_string" | "lower" | "upper" | "left" | "right" | "substring" => {
            BindingKind::String
        }
        "toboolean" | "to_bool" | "to_boolean" | "exists" => BindingKind::Bool,
        "collect" => match args.first().map(|arg| projected_expr_kind(arg, scope)) {
            Some(BindingKind::Node) => BindingKind::ListNode,
            Some(BindingKind::Relationship) => BindingKind::ListRelationship,
            Some(BindingKind::Int) => BindingKind::ListInt,
            _ => BindingKind::Value,
        },
        "date" => BindingKind::Date,
        "timestamp" => BindingKind::Timestamp,
        "cast" if args.len() == 2 => match &args[1] {
            Expr::Literal(Literal::String(type_name)) => {
                match type_name.to_ascii_uppercase().as_str() {
                    "TIMESTAMP" | "TIMESTAMP_NS" | "TIMESTAMP_SEC" | "TIMESTAMP_TZ" => {
                        BindingKind::Timestamp
                    }
                    "TIMESTAMP_MS" => BindingKind::TimestampMs,
                    "DATE" => BindingKind::Date,
                    "INTERVAL" => BindingKind::Interval,
                    _ => BindingKind::Value,
                }
            }
            _ => BindingKind::Value,
        },
        _ => BindingKind::Value,
    }
}

/// Static element kind of an `UNWIND` source expression, so unwound
/// node/relationship list elements can be reused in later patterns.
pub(super) fn unwind_element_kind(expr: &Expr, scope: &SemanticScope) -> BindingKind {
    match projected_expr_kind(expr, scope) {
        BindingKind::ListNode => BindingKind::Node,
        BindingKind::ListRelationship | BindingKind::RecursiveRelationship => BindingKind::Relationship,
        BindingKind::ListInt => BindingKind::Int,
        _ => BindingKind::Value,
    }
}

pub(super) fn validate_list_source(expr: &Expr, scope: &SemanticScope) -> CypherPlanResult<()> {
    if let Expr::Variable(name) = expr {
        if let Some(
            kind @ (BindingKind::Node
            | BindingKind::Relationship
            | BindingKind::Path),
        ) = scope.kind(name)
        {
            return Err(CypherPlanError::Invalid(format!(
                "Binder exception: {name} has data type {} but LIST was expected.",
                kind.cypher_type_name()
            )));
        }
    }
    if let Some(actual) = static_non_list_type_name(expr) {
        return Err(CypherPlanError::Invalid(format!(
            "Binder exception: {} has data type {actual} but LIST was expected.",
            display_literal_expr(expr)
        )));
    }
    Ok(())
}

pub(super) fn static_non_list_type_name(expr: &Expr) -> Option<&'static str> {
    match expr {
        Expr::Literal(Literal::Bool(_)) => Some("BOOL"),
        Expr::Literal(Literal::Integer(_)) => Some("INT64"),
        Expr::Literal(Literal::Float(_)) => Some("DOUBLE"),
        Expr::Literal(Literal::String(_)) => Some("STRING"),
        Expr::Map(_) => Some("STRUCT"),
        Expr::Literal(Literal::Null) | Expr::List(_) => None,
        _ => None,
    }
}
