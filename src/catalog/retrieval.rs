use super::CypherRelationship;
use crate::ir::rel::mapping::{
    ComputedRelationship, GraphMapping, MappedSource, NullOrder, RelationshipOrder, RetrievalMode,
    SortDirection,
};
use crate::language::cypher::{ast, parameters, parser};
use datafusion::logical_expr::{ExprFunctionExt, LogicalPlan, LogicalPlanBuilder};
use datafusion::prelude::col;
use std::collections::BTreeMap;

fn sql(expr: &ast::Expr, target: &str, aliases: &BTreeMap<String, String>) -> Option<String> {
    use ast::{BinaryOp, Expr, Literal, UnaryOp};
    Some(match expr {
        Expr::Variable(name) => aliases.get(name)?.clone(),
        Expr::Property { target: value, key } => {
            let Expr::Variable(name) = value.as_ref() else {
                return None;
            };
            let side = if name == "source" {
                "source"
            } else if name == target {
                "target"
            } else {
                return None;
            };
            format!("{side}.\"{}\"", key.replace('"', "\"\""))
        }
        Expr::Literal(Literal::Integer(value)) => value.clone(),
        Expr::Literal(Literal::Float(value)) if value.is_finite() => value.to_string(),
        Expr::Literal(Literal::String(value)) => format!("'{}'", value.replace('\'', "''")),
        Expr::Literal(Literal::Bool(value)) => value.to_string(),
        Expr::Literal(Literal::Null) => "NULL".into(),
        Expr::Unary { op, expr } => format!(
            "({} {})",
            match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "NOT",
            },
            sql(expr, target, aliases)?
        ),
        Expr::Binary { op, lhs, rhs } => format!(
            "({} {} {})",
            sql(lhs, target, aliases)?,
            match op {
                BinaryOp::Or => "OR",
                BinaryOp::And => "AND",
                BinaryOp::Eq => "=",
                BinaryOp::Neq => "<>",
                BinaryOp::Lt => "<",
                BinaryOp::Lte => "<=",
                BinaryOp::Gt => ">",
                BinaryOp::Gte => ">=",
                BinaryOp::Add => "+",
                BinaryOp::Sub => "-",
                BinaryOp::Mul => "*",
                BinaryOp::Div => "/",
            },
            sql(rhs, target, aliases)?
        ),
        Expr::IsNull(expr) => format!("({} IS NULL)", sql(expr, target, aliases)?),
        Expr::IsNotNull(expr) => format!("({} IS NOT NULL)", sql(expr, target, aliases)?),
        Expr::Function {
            name,
            args,
            distinct: false,
        } if matches!(
            name.as_str(),
            "vector.cosine_similarity" | "vector.dot" | "vector.l2_distance"
        ) =>
        {
            format!(
                "{}({})",
                name,
                args.iter()
                    .map(|expr| sql(expr, target, aliases))
                    .collect::<Option<Vec<_>>>()?
                    .join(",")
            )
        }
        Expr::List(values) => format!(
            "ARRAY[{}]",
            values
                .iter()
                .map(|expr| sql(expr, target, aliases))
                .collect::<Option<Vec<_>>>()?
                .join(",")
        ),
        _ => return None,
    })
}

fn rejects_null(
    expr: &ast::Expr,
    score: &str,
    target: &str,
    aliases: &BTreeMap<String, String>,
) -> bool {
    use ast::{BinaryOp, Expr};
    match expr {
        Expr::Binary {
            op: BinaryOp::And,
            lhs,
            rhs,
        } => rejects_null(lhs, score, target, aliases) || rejects_null(rhs, score, target, aliases),
        Expr::Binary {
            op: BinaryOp::Or,
            lhs,
            rhs,
        } => rejects_null(lhs, score, target, aliases) && rejects_null(rhs, score, target, aliases),
        Expr::Binary {
            op:
                BinaryOp::Eq
                | BinaryOp::Neq
                | BinaryOp::Lt
                | BinaryOp::Lte
                | BinaryOp::Gt
                | BinaryOp::Gte,
            lhs,
            rhs,
        } => {
            sql(lhs, target, aliases).as_deref() == Some(score)
                || sql(rhs, target, aliases).as_deref() == Some(score)
        }
        Expr::IsNotNull(value) => sql(value, target, aliases).as_deref() == Some(score),
        _ => false,
    }
}

pub(super) fn plan(
    mapping: &GraphMapping,
    rule: &CypherRelationship,
) -> Result<Option<LogicalPlan>, String> {
    let target_node = mapping
        .node(&rule.target)
        .ok_or("unknown derived edge target")?;
    let MappedSource::Table(table) = &target_node.source else {
        return Ok(None);
    };
    let Some(metadata) = mapping.source_metadata.get(table) else {
        return Ok(None);
    };
    if metadata.indexes.is_empty() {
        return Ok(None);
    }
    let values = rule
        .arguments(&BTreeMap::new())?
        .iter()
        .map(|(name, value)| Ok((name.clone(), crate::compiler::parameter(value)?)))
        .collect::<Result<_, String>>()?;
    let mut query = parser::parse_query(&rule.cypher).map_err(|error| error.to_string())?;
    parameters::bind_parameters_with_diagnostics(&mut query, &values)
        .map_err(|error| error.to_string())?;
    if !query.unions.is_empty() {
        return Ok(None);
    }
    let recognize = || -> Option<ComputedRelationship> {
        let mut clauses = query.clauses.iter();
        let ast::Clause::With(source) = clauses.next()? else {
            return None;
        };
        if source.predicate.is_some()
            || source.projection.distinct
            || source.projection.include_existing
            || source.projection.items.len() != 1
            || source.projection.skip.is_some()
            || source.projection.limit.is_some()
            || !source.projection.order_by.is_empty()
            || source.projection.items[0].expr != ast::Expr::Variable("source".into())
            || source.projection.items[0]
                .alias
                .as_deref()
                .is_some_and(|name| name != "source")
        {
            return None;
        }
        let ast::Clause::Match(matched) = clauses.next()? else {
            return None;
        };
        if matched.optional || matched.patterns.len() != 1 {
            return None;
        }
        let pattern = &matched.patterns[0];
        if pattern.variable.is_some() || !pattern.element.chains.is_empty() {
            return None;
        }
        let node = &pattern.element.start;
        if node.labels != [rule.target.clone()] || node.properties.is_some() {
            return None;
        }
        let target = node.variable.as_deref()?;
        let mut aliases = BTreeMap::new();
        let mut predicates = Vec::new();
        let mut null_predicates = Vec::new();
        if let Some(predicate) = &matched.predicate {
            predicates.push(sql(predicate, target, &aliases)?);
            null_predicates.push((predicate.clone(), aliases.clone()));
        }
        let mut projection = clauses.next()?;
        if let ast::Clause::With(with) = projection {
            if with.projection.distinct
                || with.projection.include_existing
                || with.projection.skip.is_some()
                || with.projection.limit.is_some()
                || !with.projection.order_by.is_empty()
            {
                return None;
            }
            let mut has_target = false;
            for item in &with.projection.items {
                if item.expr == ast::Expr::Variable(target.into()) {
                    if item.alias.as_deref().is_some_and(|name| name != target) {
                        return None;
                    }
                    has_target = true;
                    continue;
                }
                aliases.insert(item.alias.clone()?, sql(&item.expr, target, &aliases)?);
            }
            if !has_target {
                return None;
            }
            if let Some(predicate) = &with.predicate {
                predicates.push(sql(predicate, target, &aliases)?);
                null_predicates.push((predicate.clone(), aliases.clone()));
            }
            projection = clauses.next()?;
        }
        let ast::Clause::Return(returned) = projection else {
            return None;
        };
        if clauses.next().is_some()
            || returned.projection.distinct
            || returned.projection.include_existing
            || returned.projection.skip.is_some()
            || returned.projection.order_by.len() != 1
        {
            return None;
        }
        let limit = returned.projection.limit.as_ref()?;
        let limit = match limit {
            ast::Expr::Function { name, args, .. }
                if name == "cypher_slice_bound" && args.len() == 1 =>
            {
                &args[0]
            }
            other => other,
        };
        let limit = super::lowering::constant(limit).ok()?.as_u64()?;
        let mut properties = BTreeMap::new();
        let mut has_target = false;
        for item in &returned.projection.items {
            if item.expr == ast::Expr::Variable(target.into())
                && item.alias.as_deref().unwrap_or(target) == rule.returns.target
            {
                has_target = true;
                continue;
            }
            let name = item.alias.clone().or_else(|| {
                if let ast::Expr::Variable(name) = &item.expr {
                    Some(name.clone())
                } else {
                    None
                }
            })?;
            if !rule.returns.properties.contains_key(&name) {
                return None;
            }
            properties.insert(name, sql(&item.expr, target, &aliases)?);
        }
        if !has_target || properties.len() != rule.returns.properties.len() {
            return None;
        }
        let order = &returned.projection.order_by[0];
        let order_expression = if let ast::Expr::Variable(name) = &order.expr {
            properties.get(name)?.clone()
        } else {
            sql(&order.expr, target, &aliases)?
        };
        if !order_expression.starts_with("vector.") {
            return None;
        }
        let output_name = properties.iter().find_map(|(name, expression)| {
            if expression == &order_expression {
                Some(name.clone())
            } else {
                None
            }
        })?;
        let metric = if order_expression.starts_with("vector.cosine_similarity(") {
            crate::ir::rel::search::SearchMetric::Cosine
        } else if order_expression.starts_with("vector.dot(") {
            crate::ir::rel::search::SearchMetric::Dot
        } else {
            crate::ir::rel::search::SearchMetric::L2
        };
        let column = target_node.properties.iter().find_map(|(name, column)| {
            order_expression
                .ends_with(&format!(",target.\"{}\")", name.replace('"', "\"\"")))
                .then_some(column)
        })?;
        let index = metadata
            .indexes
            .iter()
            .find(|index| index.metric == metric && &index.column == column)?;
        let nulls = if matches!(order.direction, ast::SortDirection::Asc)
            || null_predicates.iter().any(|(predicate, aliases)| {
                rejects_null(predicate, &order_expression, target, aliases)
            }) {
            NullOrder::Last
        } else {
            NullOrder::First
        };
        let retrieval = if index
            .options
            .get("retrieval")
            .or_else(|| metadata.options.get("retrieval"))
            .and_then(serde_json::Value::as_str)
            == Some("approximate_allowed")
        {
            RetrievalMode::ApproximateAllowed
        } else {
            RetrievalMode::Exact
        };
        Some(ComputedRelationship {
            name: rule.name.clone(),
            source: rule.source.clone(),
            target: rule.target.clone(),
            predicate: if predicates.is_empty() {
                None
            } else {
                Some(predicates.join(" AND "))
            },
            properties,
            order_by: vec![RelationshipOrder {
                expression: output_name,
                direction: match order.direction {
                    ast::SortDirection::Asc => SortDirection::Asc,
                    ast::SortDirection::Desc => SortDirection::Desc,
                },
                nulls,
            }],
            limit_per_source: Some(limit),
            retrieval,
            candidates: None,
        })
    };
    let Some(computed) = recognize() else {
        return Ok(None);
    };
    let plan = crate::ir::rel::mapping::computed::plan(mapping, &computed)
        .map_err(|error| error.to_string())?;
    let mut fields = Vec::new();
    for (label, side, prefix) in [
        (&rule.source, "source", "__src"),
        (&rule.target, "target", "__dst"),
    ] {
        for index in 0..mapping.node(label).unwrap().id_column.len() {
            fields.push(
                col(format!("__relationship_{side}_key_{index}"))
                    .alias(format!("{prefix}_{index}")),
            );
        }
    }
    for (index, name) in rule.returns.properties.keys().enumerate() {
        fields.push(
            col(crate::ir::rel::mapping::computed::prop("edge", name))
                .alias(format!("__property_{index}")),
        );
    }
    let plan = LogicalPlanBuilder::from(plan)
        .project(fields)
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())?;
    let order = plan
        .schema()
        .fields()
        .iter()
        .filter(|field| field.name().starts_with("__src_") || field.name().starts_with("__dst_"))
        .map(|field| col(field.name()).sort(true, true))
        .collect::<Vec<_>>();
    let row = datafusion::functions_window::row_number::row_number()
        .order_by(order)
        .build()
        .map_err(|error| error.to_string())?
        .alias("__edge_id");
    Ok(Some(
        LogicalPlanBuilder::from(plan)
            .window(vec![row])
            .map_err(|error| error.to_string())?
            .build()
            .map_err(|error| error.to_string())?,
    ))
}
