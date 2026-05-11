use crate::grammar::generated::cypher::cypherparser::{
    Cypher_T__2, Cypher_T__4, Cypher_T__11, Cypher_T__12, Cypher_T__13, Cypher_T__14, Cypher_T__15,
    Cypher_T__16, Cypher_T__17, Cypher_T__18, Cypher_T__19, Cypher_T__20,
    OC_AddOrSubtractExpressionContext, OC_AddOrSubtractExpressionContextAttrs,
    OC_AndExpressionContext, OC_AndExpressionContextAttrs, OC_AtomContext, OC_AtomContextAttrs,
    OC_BooleanLiteralContextAttrs, OC_CaseAlternativeContext, OC_CaseAlternativeContextAttrs,
    OC_CaseExpressionContext, OC_CaseExpressionContextAttrs, OC_ComparisonExpressionContext,
    OC_ComparisonExpressionContextAttrs, OC_ExpressionContext, OC_ExpressionContextAttrs,
    OC_FunctionInvocationContext, OC_FunctionInvocationContextAttrs, OC_FunctionNameContext,
    OC_FunctionNameContextAttrs, OC_ListLiteralContextAttrs, OC_ListOperatorExpressionContext,
    OC_ListOperatorExpressionContextAttrs, OC_ListPredicateExpressionContextAttrs,
    OC_LiteralContextAttrs, OC_MapLiteralContextAttrs, OC_MultiplyDivideModuloExpressionContext,
    OC_MultiplyDivideModuloExpressionContextAttrs, OC_NamespaceContextAttrs,
    OC_NodeLabelsContext, OC_NodeLabelsContextAttrs, OC_NonArithmeticOperatorExpressionContext,
    OC_NonArithmeticOperatorExpressionContextAttrs, OC_NotExpressionContext,
    OC_NotExpressionContextAttrs, OC_NullPredicateExpressionContextAttrs,
    OC_NumberLiteralContextAttrs, OC_OrExpressionContext, OC_OrExpressionContextAttrs,
    OC_ParameterContextAttrs, OC_ParenthesizedExpressionContextAttrs,
    OC_PartialComparisonExpressionContext, OC_PartialComparisonExpressionContextAttrs,
    OC_PowerOfExpressionContextAttrs, OC_PropertyExpressionContext,
    OC_PropertyExpressionContextAttrs, OC_PropertyLookupContextAttrs,
    OC_StringListNullPredicateExpressionContext, OC_StringListNullPredicateExpressionContextAttrs,
    OC_StringPredicateExpressionContextAttrs, OC_UnaryAddOrSubtractExpressionContextAttrs,
    OC_XorExpressionContextAttrs,
};
use crate::language::cypher::ast::{BinaryOp, Expr, Literal, StringPredicateOp, UnaryOp};
use crate::language::cypher::parser::{CypherParseError, Result};
use antlr4rust::parser_rule_context::ParserRuleContext;
use antlr4rust::token::Token;
use antlr4rust::tree::ParseTree;

use super::{collections, context, names, predicates, subqueries};

pub(crate) fn lower_expression(ctx: &OC_ExpressionContext<'_>) -> Result<Expr> {
    let Some(or_expr) = ctx.oC_OrExpression() else {
        return context::missing("expression missing OR expression");
    };
    lower_or_expression(or_expr.as_ref())
}

pub(crate) fn lower_or_expression(ctx: &OC_OrExpressionContext<'_>) -> Result<Expr> {
    fold_left(
        ctx.oC_XorExpression_all()
            .into_iter()
            .map(|expr| lower_xor_expression(expr.as_ref()))
            .collect::<Result<Vec<_>>>()?,
        BinaryOp::Or,
    )
}

pub(crate) fn lower_xor_expression(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_XorExpressionContext<'_>,
) -> Result<Expr> {
    let mut expressions = ctx
        .oC_AndExpression_all()
        .into_iter()
        .map(|expr| lower_and_expression(expr.as_ref()))
        .collect::<Result<Vec<_>>>()?
        .into_iter();
    let Some(mut expr) = expressions.next() else {
        return context::missing("XOR expression missing child expression");
    };
    for rhs in expressions {
        expr = Expr::Function {
            name: "xor".to_string(),
            distinct: false,
            args: vec![expr, rhs],
        };
    }
    Ok(expr)
}

pub(crate) fn lower_and_expression(ctx: &OC_AndExpressionContext<'_>) -> Result<Expr> {
    fold_left(
        ctx.oC_NotExpression_all()
            .into_iter()
            .map(|expr| lower_not_expression(expr.as_ref()))
            .collect::<Result<Vec<_>>>()?,
        BinaryOp::And,
    )
}

pub(crate) fn lower_not_expression(ctx: &OC_NotExpressionContext<'_>) -> Result<Expr> {
    let Some(comparison) = ctx.oC_ComparisonExpression() else {
        return context::missing("NOT expression missing comparison");
    };
    let mut expr = lower_comparison_expression(comparison.as_ref())?;
    for _ in ctx.NOT_all() {
        expr = Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(expr),
        };
    }
    Ok(expr)
}

pub(crate) fn lower_comparison_expression(
    ctx: &OC_ComparisonExpressionContext<'_>,
) -> Result<Expr> {
    let Some(first) = ctx.oC_StringListNullPredicateExpression() else {
        return context::missing("comparison missing left expression");
    };
    let mut previous = lower_string_list_null_predicate_expression(first.as_ref())?;
    let mut comparisons = Vec::new();
    for partial in ctx.oC_PartialComparisonExpression_all() {
        let Some(rhs) = partial.oC_StringListNullPredicateExpression() else {
            return context::missing("comparison missing right expression");
        };
        let rhs = lower_string_list_null_predicate_expression(rhs.as_ref())?;
        let comparison = if partial.get_text().trim_start().starts_with("=~") {
            Expr::Function {
                name: "regex_match".to_string(),
                distinct: false,
                args: vec![previous.clone(), rhs.clone()],
            }
        } else {
            Expr::Binary {
                op: partial_comparison_op(partial.as_ref())?,
                lhs: Box::new(previous.clone()),
                rhs: Box::new(rhs.clone()),
            }
        };
        comparisons.push(comparison);
        previous = rhs;
    }
    if comparisons.is_empty() {
        Ok(previous)
    } else {
        fold_left(comparisons, BinaryOp::And)
    }
}

pub(crate) fn lower_string_list_null_predicate_expression(
    ctx: &OC_StringListNullPredicateExpressionContext<'_>,
) -> Result<Expr> {
    let Some(base) = ctx.oC_AddOrSubtractExpression() else {
        return context::missing("predicate expression missing base expression");
    };
    let mut expr = lower_add_or_subtract_expression(base.as_ref())?;

    let mut predicates = Vec::new();
    for pred in ctx.oC_StringPredicateExpression_all() {
        let op = if pred.get_text().trim_start().starts_with("=~") {
            StringPredicateOp::Regex
        } else if pred.STARTS().is_some() {
            StringPredicateOp::StartsWith
        } else if pred.ENDS().is_some() {
            StringPredicateOp::EndsWith
        } else {
            StringPredicateOp::Contains
        };
        let Some(rhs) = pred.oC_AddOrSubtractExpression() else {
            return context::missing("string predicate missing right expression");
        };
        predicates.push((
            pred.start().get_token_index(),
            PredicatePostfix::String {
                op,
                rhs: lower_add_or_subtract_expression(rhs.as_ref())?,
            },
        ));
    }

    for pred in ctx.oC_ListPredicateExpression_all() {
        let Some(rhs) = pred.oC_AddOrSubtractExpression() else {
            return context::missing("IN predicate missing list expression");
        };
        predicates.push((
            pred.start().get_token_index(),
            PredicatePostfix::In(lower_add_or_subtract_expression(rhs.as_ref())?),
        ));
    }

    for pred in ctx.oC_NullPredicateExpression_all() {
        predicates.push((
            pred.start().get_token_index(),
            PredicatePostfix::Null {
                negated: pred.NOT().is_some(),
            },
        ));
    }

    predicates.sort_by_key(|(index, _)| *index);
    for (_, predicate) in predicates {
        expr = match predicate {
            PredicatePostfix::String { op, rhs } => Expr::StringPredicate {
                op,
                target: Box::new(expr),
                pattern: Box::new(rhs),
            },
            PredicatePostfix::In(rhs) => Expr::Function {
                name: "in".to_string(),
                distinct: false,
                args: vec![expr, rhs],
            },
            PredicatePostfix::Null { negated } => {
                if negated {
                    Expr::IsNotNull(Box::new(expr))
                } else {
                    Expr::IsNull(Box::new(expr))
                }
            }
        };
    }

    Ok(expr)
}

enum PredicatePostfix {
    String { op: StringPredicateOp, rhs: Expr },
    In(Expr),
    Null { negated: bool },
}

pub(crate) fn lower_add_or_subtract_expression(
    ctx: &OC_AddOrSubtractExpressionContext<'_>,
) -> Result<Expr> {
    let children = ctx.oC_MultiplyDivideModuloExpression_all();
    let mut ops = Vec::new();
    ops.extend(
        ctx.get_tokens(Cypher_T__17)
            .into_iter()
            .map(|token| (token.symbol.get_token_index(), BinaryOp::Add)),
    );
    ops.extend(
        ctx.get_tokens(Cypher_T__18)
            .into_iter()
            .map(|token| (token.symbol.get_token_index(), BinaryOp::Sub)),
    );
    ops.sort_by_key(|(index, _)| *index);
    let mut ops = ops.into_iter().map(|(_, op)| op);
    let mut exprs = children
        .into_iter()
        .map(|expr| lower_multiply_divide_modulo_expression(expr.as_ref()))
        .collect::<Result<Vec<_>>>()?
        .into_iter();
    let Some(mut expr) = exprs.next() else {
        return context::missing("addition expression missing child expression");
    };
    for rhs in exprs {
        let Some(op) = ops.next() else {
            return context::missing("addition expression missing operator");
        };
        expr = Expr::Binary {
            op,
            lhs: Box::new(expr),
            rhs: Box::new(rhs),
        };
    }
    Ok(expr)
}

pub(crate) fn lower_multiply_divide_modulo_expression(
    ctx: &OC_MultiplyDivideModuloExpressionContext<'_>,
) -> Result<Expr> {
    let children = ctx.oC_PowerOfExpression_all();
    let mut ops = Vec::new();
    ops.extend(
        ctx.get_tokens(Cypher_T__4)
            .into_iter()
            .map(|token| (token.symbol.get_token_index(), MultiplyOp::Mul)),
    );
    ops.extend(
        ctx.get_tokens(Cypher_T__19)
            .into_iter()
            .map(|token| (token.symbol.get_token_index(), MultiplyOp::Div)),
    );
    ops.extend(
        ctx.get_tokens(Cypher_T__20)
            .into_iter()
            .map(|token| (token.symbol.get_token_index(), MultiplyOp::Mod)),
    );
    ops.sort_by_key(|(index, _)| *index);
    let mut ops = ops.into_iter().map(|(_, op)| op);
    let mut exprs = children
        .into_iter()
        .map(|expr| lower_power_expression(expr.as_ref()))
        .collect::<Result<Vec<_>>>()?
        .into_iter();
    let Some(mut expr) = exprs.next() else {
        return context::missing("multiplication expression missing child expression");
    };
    for rhs in exprs {
        let Some(op) = ops.next() else {
            return context::missing("multiplication expression missing operator");
        };
        expr = match op {
            MultiplyOp::Div => Expr::Binary {
                op: BinaryOp::Div,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            },
            MultiplyOp::Mod => Expr::Function {
                name: "mod".to_string(),
                distinct: false,
                args: vec![expr, rhs],
            },
            MultiplyOp::Mul => Expr::Binary {
                op: BinaryOp::Mul,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            },
        };
    }
    Ok(expr)
}

pub(crate) fn lower_power_expression(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_PowerOfExpressionContext<'_>,
) -> Result<Expr> {
    let mut exprs = ctx
        .oC_UnaryAddOrSubtractExpression_all()
        .into_iter()
        .map(|expr| lower_unary_add_or_subtract_expression(expr.as_ref()))
        .collect::<Result<Vec<_>>>()?
        .into_iter();
    let Some(mut expr) = exprs.next() else {
        return context::missing("power expression missing child expression");
    };
    for rhs in exprs {
        expr = Expr::Function {
            name: "pow".to_string(),
            distinct: false,
            args: vec![expr, rhs],
        };
    }
    Ok(expr)
}

#[derive(Clone, Copy)]
enum MultiplyOp {
    Mul,
    Div,
    Mod,
}

pub(crate) fn lower_unary_add_or_subtract_expression(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_UnaryAddOrSubtractExpressionContext<
        '_,
    >,
) -> Result<Expr> {
    let Some(inner) = ctx.oC_NonArithmeticOperatorExpression() else {
        return context::missing("unary expression missing operand");
    };
    let mut expr = lower_non_arithmetic_operator_expression(inner.as_ref())?;
    if ctx.get_tokens(Cypher_T__18).len() % 2 == 1 {
        expr = Expr::Unary {
            op: UnaryOp::Neg,
            expr: Box::new(expr),
        };
    }
    Ok(expr)
}

pub(crate) fn lower_property_expression(ctx: &OC_PropertyExpressionContext<'_>) -> Result<Expr> {
    let Some(atom) = ctx.oC_Atom() else {
        return context::missing("property expression missing atom");
    };
    let mut expr = lower_atom(atom.as_ref())?;
    for lookup in ctx.oC_PropertyLookup_all() {
        expr = Expr::Property {
            target: Box::new(expr),
            key: lower_property_lookup(lookup.as_ref())?,
        };
    }
    Ok(expr)
}

pub(crate) fn lower_non_arithmetic_operator_expression(
    ctx: &OC_NonArithmeticOperatorExpressionContext<'_>,
) -> Result<Expr> {
    let Some(atom) = ctx.oC_Atom() else {
        return context::missing("non-arithmetic expression missing atom");
    };
    let mut expr = lower_atom(atom.as_ref())?;
    let mut operations = Vec::new();
    for lookup in ctx.oC_PropertyLookup_all() {
        operations.push((
            lookup.start().get_token_index(),
            NonArithmeticOp::Property(lower_property_lookup(lookup.as_ref())?),
        ));
    }
    for list_op in ctx.oC_ListOperatorExpression_all() {
        operations.push((
            list_op.start().get_token_index(),
            NonArithmeticOp::List(lower_list_operator_parts(list_op.as_ref())?),
        ));
    }
    operations.sort_by_key(|(token, _)| *token);
    for (_, operation) in operations {
        match operation {
            NonArithmeticOp::Property(key) => {
                expr = Expr::Property {
                    target: Box::new(expr),
                    key,
                };
            }
            NonArithmeticOp::List((name, args)) => {
                expr = Expr::Function {
                    name,
                    distinct: false,
                    args: std::iter::once(expr).chain(args).collect(),
                };
            }
        }
    }
    if let Some(labels) = ctx.oC_NodeLabels() {
        expr = Expr::LabelPredicate {
            target: Box::new(expr),
            labels: lower_node_labels(labels.as_ref())?,
        };
    }
    Ok(expr)
}

enum NonArithmeticOp {
    Property(String),
    List((String, Vec<Expr>)),
}

fn lower_list_operator_parts(
    ctx: &OC_ListOperatorExpressionContext<'_>,
) -> Result<(String, Vec<Expr>)> {
    let expressions = ctx.oC_Expression_all();
    let Some(dot_dot) = ctx.get_token(Cypher_T__11, 0) else {
        let Some(index) = expressions.first() else {
            return context::missing("list index operator missing expression");
        };
        return Ok((
            "cypher_subscript".to_string(),
            vec![lower_expression(index.as_ref())?],
        ));
    };

    let dot_index = dot_dot.symbol.get_token_index();
    let start = expressions
        .iter()
        .find(|expr| expr.start().get_token_index() < dot_index)
        .map(|expr| lower_expression(expr.as_ref()))
        .transpose()?
        .unwrap_or(Expr::Literal(Literal::Null));
    let end = expressions
        .iter()
        .find(|expr| expr.start().get_token_index() > dot_index)
        .map(|expr| lower_expression(expr.as_ref()))
        .transpose()?
        .unwrap_or(Expr::Literal(Literal::Null));
    Ok(("list_slice".to_string(), vec![start, end]))
}

fn lower_node_labels(ctx: &OC_NodeLabelsContext<'_>) -> Result<Vec<String>> {
    let labels = ctx
        .oC_NodeLabel_all()
        .into_iter()
        .map(|label| names::lower_node_label(label.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    if labels.is_empty() {
        return context::missing("label predicate missing label");
    }
    Ok(labels)
}

pub(crate) fn lower_atom(ctx: &OC_AtomContext<'_>) -> Result<Expr> {
    if let Some(literal) = ctx.oC_Literal() {
        return lower_literal_expr(literal.as_ref());
    }
    if let Some(parameter) = ctx.oC_Parameter() {
        return lower_parameter(parameter.as_ref());
    }
    if let Some(parenthesized) = ctx.oC_ParenthesizedExpression() {
        let Some(expression) = parenthesized.oC_Expression() else {
            return context::missing("parenthesized expression missing expression");
        };
        return lower_expression(expression.as_ref());
    }
    if let Some(case) = ctx.oC_CaseExpression() {
        return lower_case_expression(case.as_ref());
    }
    if let Some(list) = ctx.oC_ListComprehension() {
        return collections::lower_list_comprehension(list.as_ref());
    }
    if let Some(pattern) = ctx.oC_PatternComprehension() {
        return collections::lower_pattern_comprehension(pattern.as_ref());
    }
    if let Some(quantifier) = ctx.oC_Quantifier() {
        return collections::lower_quantifier(quantifier.as_ref());
    }
    if let Some(pattern_predicate) = ctx.oC_PatternPredicate() {
        return predicates::lower_pattern_predicate(pattern_predicate.as_ref());
    }
    if let Some(function) = ctx.oC_FunctionInvocation() {
        return lower_function_invocation(function.as_ref());
    }
    if let Some(exists) = ctx.oC_ExistentialSubquery() {
        return subqueries::lower_existential_subquery(exists.as_ref());
    }
    if ctx.COUNT().is_some() {
        return Ok(Expr::CountStar);
    }
    if let Some(variable) = ctx.oC_Variable() {
        return Ok(Expr::Variable(names::clean_identifier(
            &variable.get_text(),
        )));
    }
    context::unsupported(format!(
        "Cypher atom `{}` is not lowerable to the current graph IR yet",
        ctx.get_text()
    ))
}

pub(crate) fn lower_function_invocation(ctx: &OC_FunctionInvocationContext<'_>) -> Result<Expr> {
    let name = ctx
        .oC_FunctionName()
        .map(|name| lower_function_name(name.as_ref()))
        .transpose()?
        .ok_or_else(|| CypherParseError::Parse("function invocation missing name".to_string()))?;
    let args = ctx
        .oC_Expression_all()
        .into_iter()
        .map(|arg| lower_expression(arg.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    Ok(Expr::Function {
        name,
        distinct: ctx.DISTINCT().is_some(),
        args,
    })
}

pub(crate) fn lower_function_name(ctx: &OC_FunctionNameContext<'_>) -> Result<String> {
    let mut parts = Vec::new();
    if let Some(namespace) = ctx.oC_Namespace() {
        parts.extend(
            namespace
                .oC_SymbolicName_all()
                .into_iter()
                .map(|name| names::clean_identifier(&name.get_text())),
        );
    }
    if let Some(name) = ctx.oC_SymbolicName() {
        parts.push(names::clean_identifier(&name.get_text()));
    }
    if parts.is_empty() {
        return context::missing("function name missing symbolic name");
    }
    Ok(parts.join("."))
}

pub(crate) fn lower_case_expression(ctx: &OC_CaseExpressionContext<'_>) -> Result<Expr> {
    let direct_exprs = ctx.oC_Expression_all();
    let first_arm_start = ctx
        .oC_CaseAlternative_all()
        .first()
        .map(|alt| alt.start().get_token_index());
    let case = if let Some(first_arm_start) = first_arm_start {
        direct_exprs
            .iter()
            .find(|expr| expr.start().get_token_index() < first_arm_start)
            .map(|expr| lower_expression(expr.as_ref()).map(Box::new))
            .transpose()?
    } else {
        None
    };
    let otherwise = if ctx.ELSE().is_some() {
        let else_index = ctx
            .ELSE()
            .map(|token| token.symbol.get_token_index())
            .unwrap_or(isize::MAX);
        direct_exprs
            .iter()
            .find(|expr| expr.start().get_token_index() > else_index)
            .map(|expr| lower_expression(expr.as_ref()).map(Box::new))
            .transpose()?
    } else {
        None
    };
    let arms = ctx
        .oC_CaseAlternative_all()
        .into_iter()
        .map(|alt| lower_case_alternative(alt.as_ref()))
        .collect::<Result<Vec<_>>>()?;
    Ok(Expr::Case {
        case,
        arms,
        otherwise,
    })
}

pub(crate) fn lower_case_alternative(ctx: &OC_CaseAlternativeContext<'_>) -> Result<(Expr, Expr)> {
    let exprs = ctx.oC_Expression_all();
    let Some(when) = exprs.first() else {
        return context::missing("CASE alternative missing WHEN expression");
    };
    let Some(then) = exprs.get(1) else {
        return context::missing("CASE alternative missing THEN expression");
    };
    Ok((
        lower_expression(when.as_ref())?,
        lower_expression(then.as_ref())?,
    ))
}

pub(crate) fn lower_literal_expr(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_LiteralContext<'_>,
) -> Result<Expr> {
    if let Some(boolean) = ctx.oC_BooleanLiteral() {
        return Ok(Expr::Literal(Literal::Bool(boolean.TRUE().is_some())));
    }
    if ctx.NULL().is_some() {
        return Ok(Expr::Literal(Literal::Null));
    }
    if let Some(number) = ctx.oC_NumberLiteral() {
        if let Some(integer) = number.oC_IntegerLiteral() {
            return Ok(Expr::Literal(Literal::Integer(lower_integer_literal(
                integer.as_ref(),
            )?)));
        }
        if let Some(double) = number.oC_DoubleLiteral() {
            return Ok(Expr::Literal(Literal::Float(parse_f64(
                &double.get_text(),
            )?)));
        }
    }
    if let Some(string) = ctx.StringLiteral() {
        return Ok(Expr::Literal(Literal::String(unquote_string(
            &string.get_text(),
        ))));
    }
    if let Some(list) = ctx.oC_ListLiteral() {
        return Ok(Expr::List(
            list.oC_Expression_all()
                .into_iter()
                .map(|item| lower_expression(item.as_ref()))
                .collect::<Result<Vec<_>>>()?,
        ));
    }
    if let Some(map) = ctx.oC_MapLiteral() {
        return lower_map_literal(map.as_ref());
    }
    context::unsupported(format!("unrecognized Cypher literal `{}`", ctx.get_text()))
}

pub(crate) fn lower_map_literal(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_MapLiteralContext<'_>,
) -> Result<Expr> {
    let keys = ctx.oC_PropertyKeyName_all();
    let values = ctx.oC_Expression_all();
    if keys.len() != values.len() {
        return context::missing("map literal key/value count mismatch");
    }
    keys.into_iter()
        .zip(values)
        .map(|(key, value)| {
            Ok((
                lower_property_key_name(key.as_ref())?,
                lower_expression(value.as_ref())?,
            ))
        })
        .collect::<Result<Vec<_>>>()
        .map(Expr::Map)
}

pub(crate) fn lower_parameter(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_ParameterContext<'_>,
) -> Result<Expr> {
    let name = ctx
        .oC_SymbolicName()
        .map(|name| name.get_text())
        .or_else(|| ctx.DecimalInteger().map(|number| number.get_text()))
        .ok_or_else(|| CypherParseError::Parse("parameter missing name".to_string()))?;
    Ok(Expr::Parameter(names::clean_identifier(&name)))
}

pub(crate) fn lower_property_lookup(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_PropertyLookupContext<'_>,
) -> Result<String> {
    if let Some(key) = ctx.oC_PropertyKeyName() {
        return lower_property_key_name(key.as_ref());
    }
    if ctx.get_token(Cypher_T__4, 0).is_some() {
        return Ok("*".to_string());
    }
    context::missing("property lookup missing property key")
}

pub(crate) fn lower_property_key_name(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_PropertyKeyNameContext<'_>,
) -> Result<String> {
    Ok(names::clean_identifier(&ctx.get_text()))
}

pub(crate) fn lower_integer_literal(
    ctx: &crate::grammar::generated::cypher::cypherparser::OC_IntegerLiteralContext<'_>,
) -> Result<String> {
    let text = ctx.get_text();
    let text = text.trim_start();
    if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        return u128::from_str_radix(hex, 16)
            .map(|value| value.to_string())
            .map_err(|err| parse_error("integer", text, err));
    }
    if let Some(octal) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
        return u128::from_str_radix(octal, 8)
            .map(|value| value.to_string())
            .map_err(|err| parse_error("integer", text, err));
    }
    Ok(text.to_string())
}

fn fold_left(mut exprs: Vec<Expr>, op: BinaryOp) -> Result<Expr> {
    if exprs.is_empty() {
        return context::missing("expression missing child expression");
    }
    let mut expr = exprs.remove(0);
    for rhs in exprs {
        expr = Expr::Binary {
            op,
            lhs: Box::new(expr),
            rhs: Box::new(rhs),
        };
    }
    Ok(expr)
}

pub(crate) fn partial_comparison_op(
    ctx: &OC_PartialComparisonExpressionContext<'_>,
) -> Result<BinaryOp> {
    if ctx.get_token(Cypher_T__12, 0).is_some() {
        Ok(BinaryOp::Neq)
    } else if ctx.get_token(Cypher_T__15, 0).is_some() {
        Ok(BinaryOp::Lte)
    } else if ctx.get_token(Cypher_T__16, 0).is_some() {
        Ok(BinaryOp::Gte)
    } else if ctx.get_token(Cypher_T__2, 0).is_some() {
        Ok(BinaryOp::Eq)
    } else if ctx.get_token(Cypher_T__13, 0).is_some() {
        Ok(BinaryOp::Lt)
    } else if ctx.get_token(Cypher_T__14, 0).is_some() {
        Ok(BinaryOp::Gt)
    } else if ctx.get_text().trim_start().starts_with("=~") {
        context::unsupported("regular expression comparison is lowered before operator lookup")
    } else {
        context::unsupported(format!("comparison operator in `{}`", ctx.get_text()))
    }
}

fn parse_f64(text: &str) -> Result<f64> {
    text.parse::<f64>()
        .map_err(|err| parse_error("float", text, err))
}

fn parse_error<T: std::fmt::Display>(kind: &str, text: &str, err: T) -> CypherParseError {
    CypherParseError::Parse(format!("invalid {kind} literal `{text}`: {err}"))
}

fn unquote_string(text: &str) -> String {
    let trimmed = text.trim();
    let body = trimmed
        .strip_prefix(['\'', '"'])
        .and_then(|s| s.strip_suffix(['\'', '"']))
        .unwrap_or(trimmed);
    let mut result = String::new();
    let mut chars = body.chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(next) = chars.next() {
                match next {
                    'b' | 'B' => result.push('\u{0008}'),
                    'f' | 'F' => result.push('\u{000c}'),
                    'n' | 'N' => result.push('\n'),
                    'r' | 'R' => result.push('\r'),
                    't' | 'T' => result.push('\t'),
                    'x' | 'X' => {
                        let mut hex = String::new();
                        for _ in 0..2 {
                            if let Some(digit) = chars.next() {
                                hex.push(digit);
                            }
                        }
                        if hex.len() == 2 {
                            if let Ok(value) = u32::from_str_radix(&hex, 16) {
                                if let Some(ch) = char::from_u32(value) {
                                    result.push(ch);
                                    continue;
                                }
                            }
                        }
                        result.push(next);
                        result.push_str(&hex);
                    }
                    'u' | 'U' => {
                        if let Some(ch) = read_unicode_escape(&mut chars) {
                            result.push(ch);
                            continue;
                        }
                        result.push(next);
                    }
                    other => result.push(other),
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn read_unicode_escape(chars: &mut std::str::Chars<'_>) -> Option<char> {
    for width in [8, 4] {
        let mut probe = chars.clone();
        let mut hex = String::new();
        let mut complete = true;
        for _ in 0..width {
            let Some(digit) = probe.next() else {
                complete = false;
                break;
            };
            if !digit.is_ascii_hexdigit() {
                complete = false;
                break;
            }
            hex.push(digit);
        }
        if !complete {
            continue;
        }
        if let Ok(value) = u32::from_str_radix(&hex, 16) {
            if let Some(ch) = char::from_u32(value) {
                *chars = probe;
                return Some(ch);
            }
        }
    }
    None
}
