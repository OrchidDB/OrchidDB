//! Trino expands arrays in FROM, while the shared unparser emits SELECT UNNEST.
use super::*;
use datafusion::sql::sqlparser::parser::Parser;

pub(super) fn rewrite(query: &mut ast::Query) -> SqlResult<()> {
    let occupied = query.to_string();
    rewrite_body(&mut query.body, &occupied)?;
    let mut remaining = false;
    let _: std::ops::ControlFlow<()> = ast::visit_expressions_mut(query, |expr| {
        if let ast::Expr::Function(call) = expr {
            remaining |= call.name.to_string().trim_matches('"').eq_ignore_ascii_case("unnest");
        }
        std::ops::ControlFlow::Continue(())
    });
    if remaining {
        return Err(SqlError::Unsupported("Trino UNNEST must be a row-expanding projection".into()));
    }
    Ok(())
}

fn rewrite_body(body: &mut ast::SetExpr, occupied: &str) -> SqlResult<()> {
    if let ast::SetExpr::SetOperation { left, right, .. } = body {
        rewrite_body(left, occupied)?;
        return rewrite_body(right, occupied);
    }
    let ast::SetExpr::Select(select) = body else { return Ok(()) };
    let mut expansions = Vec::new();
    for item in &mut select.projection {
        let expr = match item {
            ast::SelectItem::ExprWithAlias { expr, .. } | ast::SelectItem::UnnamedExpr(expr) => expr,
            _ => continue,
        };
        let ast::Expr::Function(call) = expr else { continue };
        if !call.name.to_string().trim_matches('"').eq_ignore_ascii_case("unnest") { continue }
        let ast::FunctionArguments::List(args) = &call.args else {
            return Err(SqlError::Unsupported("invalid Trino UNNEST arguments".into()));
        };
        let [ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(array))] = args.args.as_slice() else {
            return Err(SqlError::Unsupported("Trino projection UNNEST requires one array".into()));
        };
        let array = array.clone();
        expansions.push((expr, array));
    }
    if expansions.is_empty() { return Ok(()) }
    let mut suffix = 0;
    let prefix = loop {
        let candidate = format!("__orchid_trino_expand_{suffix}");
        if !occupied.contains(&candidate) { break candidate }
        suffix += 1;
    };
    let arguments = expansions.iter().map(|(_, array)| {
        // UNNEST normally spreads ROW fields into separate columns. One extra
        // ROW layer preserves a struct-valued element as exactly one column.
        format!("transform({array}, {prefix}_item -> ROW({prefix}_item))")
    }).collect::<Vec<_>>().join(", ");
    let columns = (0..expansions.len()).map(|i| format!("{prefix}_value_{i}"))
        .collect::<Vec<_>>();
    let sql = format!("UNNEST({arguments}) AS {prefix}({})", columns.join(", "));
    let relation = Parser::new(&TrinoParser).try_with_sql(&sql)
        .and_then(|mut parser| parser.parse_table_factor())
        .map_err(|e| SqlError::Unsupported(e.to_string()))?;
    // One multi-array UNNEST zips arrays and pads shorter arrays with NULL.
    select.from.push(ast::TableWithJoins { relation, joins: vec![] });
    for ((expr, _), column) in expansions.into_iter().zip(columns) {
        *expr = ast::Expr::CompoundIdentifier(vec![
            ast::Ident::new(&prefix), ast::Ident::new(column),
        ]);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../../../../tests/unit/extracted/ir__rel__sql__trino__unnest__tests.rs"]
mod tests;
