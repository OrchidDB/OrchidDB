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
    for (index, item) in select.projection.iter().enumerate() {
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
        expansions.push((index, array.clone()));
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
    let sql = format!("SELECT * FROM UNNEST({arguments}) AS {prefix}({})", columns.join(", "));
    let mut statements = Parser::parse_sql(&TrinoParser, &sql)
        .map_err(|e| SqlError::Unsupported(e.to_string()))?;
    let ast::Statement::Query(parsed) = statements.remove(0) else { unreachable!() };
    let ast::SetExpr::Select(mut parsed) = *parsed.body else { unreachable!() };
    // A single multi-array UNNEST zips arrays and pads shorter arrays with NULL,
    // matching the shared plan's row expansion rather than a Cartesian product.
    select.from.append(&mut parsed.from);
    for ((index, _), column) in expansions.into_iter().zip(columns) {
        let replacement = ast::Expr::CompoundIdentifier(vec![
            ast::Ident::new(&prefix), ast::Ident::new(column),
        ]);
        match &mut select.projection[index] {
            ast::SelectItem::ExprWithAlias { expr, .. } | ast::SelectItem::UnnamedExpr(expr) => *expr = replacement,
            _ => unreachable!(),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn query(sql: &str) -> Box<ast::Query> {
        let ast::Statement::Query(query) = Parser::parse_sql(&TrinoParser, sql).unwrap().remove(0)
            else { panic!("query expected") };
        query
    }
    #[test]
    fn paired_expansions_share_one_zip_relation_and_keep_output_aliases() {
        let mut query = query("SELECT unnest(a) AS x, unnest(b) AS y FROM input");
        rewrite(&mut query).unwrap();
        let ast::SetExpr::Select(select) = query.body.as_ref() else { panic!() };
        assert_eq!(select.from.len(), 2);
        let (arity, alias) = match &select.from[1].relation {
            ast::TableFactor::UNNEST { array_exprs, alias, .. } => (array_exprs.len(), alias),
            ast::TableFactor::Table { args: Some(args), alias, .. } => (args.args.len(), alias),
            other => panic!("unexpected expansion: {other:?}"),
        };
        assert_eq!(arity, 2);
        assert_eq!(alias.as_ref().unwrap().columns.len(), 2);
        assert!(query.to_string().contains(" AS x"));
        assert!(query.to_string().contains(" AS y"));
        let once = query.to_string();
        rewrite(&mut query).unwrap();
        assert_eq!(query.to_string(), once);
    }
    #[test]
    fn expansion_names_do_not_shadow_existing_identifiers() {
        let mut query = query("SELECT __orchid_trino_expand_0, unnest(a) AS x FROM input");
        rewrite(&mut query).unwrap();
        let ast::SetExpr::Select(select) = query.body.as_ref() else { panic!() };
        let alias = match &select.from[1].relation {
            ast::TableFactor::UNNEST { alias, .. } | ast::TableFactor::Table { alias, .. } => alias,
            other => panic!("unexpected expansion: {other:?}"),
        };
        assert_eq!(alias.as_ref().unwrap().name.value, "__orchid_trino_expand_1");
    }
    #[test]
    fn union_arms_expand_independently_and_wrap_row_elements() {
        let mut query = query("SELECT unnest(ARRAY[ROW(1, 2)]) AS x UNION ALL SELECT unnest(ARRAY[ROW(3, 4)]) AS x");
        rewrite(&mut query).unwrap();
        let sql = query.to_string();
        assert_eq!(sql.matches("FROM UNNEST(transform(").count(), 2);
        assert_eq!(sql.matches(" -> ROW(").count(), 2);
        Parser::parse_sql(&TrinoParser, &sql).unwrap();
    }
    #[test]
    fn scalar_consumers_and_ambiguous_unnest_arguments_are_rejected() {
        for sql in ["SELECT 1 + unnest(a) FROM input", "SELECT unnest(a, b) FROM input"] {
            assert!(rewrite(&mut query(sql)).is_err(), "{sql}");
        }
    }
}
