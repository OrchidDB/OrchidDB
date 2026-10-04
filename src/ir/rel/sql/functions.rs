//! Target-engine expression adaptation, analogous to Calcite's dialect
//! `unparseCall`/`prepareUnparse` boundary. Operates on syntax nodes, never SQL
//! substrings: a function spelling inside data or an identifier is untouched.
use std::ops::ControlFlow;

#[cfg(feature = "duckdb")]
use datafusion::common::DFSchema;
#[cfg(feature = "duckdb")]
use datafusion::logical_expr::Expr;
use datafusion::sql::sqlparser::{ast, parser::Parser};
#[cfg(test)]
use datafusion::sql::sqlparser::dialect::DuckDbDialect;
#[cfg(feature = "duckdb")]
use datafusion::sql::unparser::Unparser;

use super::{SqlDialect, SqlError, SqlResult};

/// Render a scalar expression using the same rules as complete queries. The
/// catalog binder uses this after replacing column references by typed NULLs.
#[cfg(feature = "duckdb")]
pub(crate) fn expression_sql(expr: &Expr, _schema: &DFSchema) -> SqlResult<String> {
    let dialect = SqlDialect::DuckDb.unparser_dialect();
    let encoded =
        super::unparse::encode_expression_literals(expr.clone(), SqlDialect::DuckDb)?.data;
    let mut expression = Unparser::new(dialect.as_ref()).expr_to_sql(&encoded)?;
    prepare_ast(&mut expression, SqlDialect::DuckDb)?;
    Ok(expression.to_string())
}

pub(super) fn prepare_ast<T: ast::VisitMut>(tree: &mut T, dialect: SqlDialect) -> SqlResult<()> {
    prepare_scoped_ast(tree, dialect, dialect == SqlDialect::Postgres)
}

pub(super) fn prepare_scoped_ast<T: ast::VisitMut>(tree: &mut T, dialect: SqlDialect, repair_qualifiers: bool) -> SqlResult<()> {
    struct Reserved(std::collections::BTreeSet<String>);
    impl ast::VisitorMut for Reserved {
        type Break = ();
        fn post_visit_table_factor(&mut self, factor: &mut ast::TableFactor) -> ControlFlow<()> {
            let alias = match factor {
                ast::TableFactor::Table { name, alias, .. } => {
                    for part in &name.0 {
                        if let Some(id) = part.as_ident() {
                            self.0.insert(id.value.clone());
                        }
                    }
                    alias.as_ref()
                }
                ast::TableFactor::Derived { alias, .. } => alias.as_ref(),
                _ => None,
            };
            if let Some(alias) = alias {
                self.0.insert(alias.name.value.clone());
            }
            ControlFlow::Continue(())
        }
    }
    let mut reserved = Reserved(Default::default());
    let _ = tree.visit(&mut reserved);
    struct Scope {
        names: std::collections::BTreeSet<String>,
        derived: std::collections::BTreeMap<Vec<String>, Vec<ast::Ident>>,
    }
    struct UnitProjection {
        repair_qualifiers: bool,
        next_alias: usize,
        scopes: Vec<Scope>,
        reserved: std::collections::BTreeSet<String>,
        visibility: Vec<usize>,
    }
    impl ast::VisitorMut for UnitProjection {
        type Break = SqlError;
        fn pre_visit_table_factor(&mut self, factor: &mut ast::TableFactor) -> ControlFlow<Self::Break> {
            if matches!(factor, ast::TableFactor::Derived { lateral: false, .. }) {
                self.visibility.push(self.scopes.len());
            }
            ControlFlow::Continue(())
        }
        fn post_visit_table_factor(&mut self, factor: &mut ast::TableFactor) -> ControlFlow<Self::Break> {
            if matches!(factor, ast::TableFactor::Derived { lateral: false, .. }) {
                self.visibility.pop();
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_query(&mut self, query: &mut ast::Query) -> ControlFlow<Self::Break> {
            let mut scope = Scope {
                names: Default::default(),
                derived: Default::default(),
            };
            if !self.repair_qualifiers {
                self.scopes.push(scope);
                return ControlFlow::Continue(());
            }
            if let ast::SetExpr::Select(select) = query.body.as_mut() {
                for from in &mut select.from {
                    for factor in std::iter::once(&mut from.relation)
                        .chain(from.joins.iter_mut().map(|j| &mut j.relation))
                    {
                        match factor {
                            ast::TableFactor::Table { name, alias, .. } => {
                                if let Some(alias) = alias {
                                    scope.names.insert(alias.name.value.clone());
                                } else if let Some(id) = name.0.last().and_then(|p| p.as_ident()) {
                                    scope.names.insert(id.value.clone());
                                }
                            }
                            ast::TableFactor::Derived {
                                subquery, alias, ..
                            } => {
                                if alias
                                    .as_ref()
                                    .is_none_or(|a| a.name.value.starts_with("derived_"))
                                {
                                    let fresh = loop {
                                        self.next_alias += 1;
                                        let name =
                                            format!("__orchiddb_derived_{}", self.next_alias);
                                        if self.reserved.insert(name.clone()) {
                                            break name;
                                        }
                                    };
                                    *alias = Some(ast::TableAlias {
                                        name: ast::Ident::with_quote('"', fresh),
                                        columns: vec![],
                                        explicit: true,
                                    });
                                }
                                let alias = alias.as_ref().unwrap();
                                scope.names.insert(alias.name.value.clone());
                                if let ast::SetExpr::Select(inner) = subquery.body.as_ref() {
                                    for item in &inner.projection {
                                        let (expr, output) = match item {
                                            ast::SelectItem::UnnamedExpr(
                                                ast::Expr::CompoundIdentifier(parts),
                                            ) => (parts, parts.last().unwrap()),
                                            ast::SelectItem::ExprWithAlias {
                                                expr: ast::Expr::CompoundIdentifier(parts),
                                                alias,
                                            } => (parts, alias),
                                            _ => continue,
                                        };
                                        let key = expr.iter().map(|i| i.value.clone()).collect();
                                        let mapped = vec![alias.name.clone(), output.clone()];
                                        // A source column may be projected both by its
                                        // original name and under an additional alias.
                                        // Preserve the original output when available.
                                        if expr.last().is_some_and(|id| id.value == output.value) {
                                            scope.derived.insert(key, mapped);
                                        } else {
                                            scope.derived.entry(key).or_insert(mapped);
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            self.scopes.push(scope);
            ControlFlow::Continue(())
        }
        fn post_visit_expr(&mut self, expr: &mut ast::Expr) -> ControlFlow<Self::Break> {
            if !self.repair_qualifiers { return ControlFlow::Continue(()); }
            if let ast::Expr::CompoundIdentifier(parts) = expr {
                if parts.len() >= 2 {
                    let key = parts.iter().map(|i| i.value.clone()).collect::<Vec<_>>();
                    if let Some(scope) = self.scopes.last() {
                        if scope.names.contains(&parts[parts.len() - 2].value) {
                            return ControlFlow::Continue(());
                        }
                        // EXISTS may correlate with an outer table also projected
                        // by its inner relation. Preserve that outer reference
                        // before considering the inner projection's aliases.
                        let floor = self.visibility.last().copied().unwrap_or(0);
                        if self.scopes[floor..self.scopes.len() - 1].iter().rev()
                            .any(|outer| outer.names.contains(&parts[parts.len() - 2].value)) {
                            return ControlFlow::Continue(());
                        }
                        if let Some(mapped) = scope.derived.get(&key) {
                            *parts = mapped.clone();
                            return ControlFlow::Continue(());
                        }
                    }
                    // A derived relation cannot refer to the alias that its
                    // parent assigns to that same relation. Only original
                    // outer table names can be correlated SQL references.
                    if !parts[parts.len() - 2].value.starts_with("__orchiddb_derived_")
                        && self.scopes.iter().rev().skip(1).any(|scope| scope.names.contains(&parts[parts.len() - 2].value)) {
                        return ControlFlow::Continue(());
                    }
                    *expr = ast::Expr::Identifier(parts.last().unwrap().clone());
                }
            }
            ControlFlow::Continue(())
        }
        fn post_visit_query(&mut self, query: &mut ast::Query) -> ControlFlow<Self::Break> {
            self.scopes.pop();
            if let ast::SetExpr::Select(select) = query.body.as_mut() {
                if select.projection.is_empty() {
                    // DataFusion can omit the projection around a limited join.
                    // Retain its input columns; only a FROM-less empty tuple
                    // needs a unit column to preserve cardinality.
                    if select.from.is_empty() {
                        select.projection.push(ast::SelectItem::ExprWithAlias {
                            expr: ast::Expr::Value(ast::Value::Number("1".into(), false).into()),
                            alias: ast::Ident::new("__orchiddb_unit"),
                        });
                    } else {
                        select.projection.push(ast::SelectItem::Wildcard(Default::default()));
                    }
                }
            }
            ControlFlow::Continue(())
        }
    }
    if let ControlFlow::Break(error) = tree.visit(&mut UnitProjection {
        repair_qualifiers,
        next_alias: 0,
        scopes: vec![],
        reserved: reserved.0,
        visibility: vec![],
    }) { return Err(error); }
    super::logical_functions::adapt_ordering(tree, dialect)?;
    if let ControlFlow::Break(error) = ast::visit_expressions_mut(tree, |expr| match adapt_expression(expr, dialect) {
        Ok(()) => ControlFlow::Continue(()),
        Err(error) => ControlFlow::Break(error),
    }) { return Err(error); }
    if dialect == SqlDialect::Postgres {
        struct EmptyArrays;
        impl ast::VisitorMut for EmptyArrays {
            type Break = SqlError;
            fn pre_visit_expr(&mut self, expr: &mut ast::Expr) -> ControlFlow<Self::Break> {
                if let ast::Expr::Cast { expr: inner, .. } = expr {
                    if let ast::Expr::Array(a) = inner.as_ref() {
                        if a.elem.iter().all(|v| matches!(v, ast::Expr::Value(v) if v.value == ast::Value::Null)) {
                            **inner = ast::Expr::Value(ast::Value::SingleQuotedString(format!("{{{}}}", vec!["NULL"; a.elem.len()].join(","))).into());
                        }
                    }
                }
                ControlFlow::Continue(())
            }
            fn post_visit_expr(&mut self, expr: &mut ast::Expr) -> ControlFlow<Self::Break> {
                if let ast::Expr::Array(a) = expr {
                    if !a.elem.iter().all(|v| matches!(v, ast::Expr::Value(v) if v.value == ast::Value::Null)) { return ControlFlow::Continue(()); }
                    let literal = format!("CAST('{{{}}}' AS INTEGER[])", vec!["NULL"; a.elem.len()].join(","));
                    match template(&literal, &[]) {
                        Ok(value) => *expr = value,
                        Err(error) => return ControlFlow::Break(error),
                    }
                }
                ControlFlow::Continue(())
            }
        }
        if let ControlFlow::Break(error) = tree.visit(&mut EmptyArrays) { return Err(error); }
        // PostgreSQL pulls simple derived projections into their consumers.
        // Reusing a projected scalar subquery can then expand it exponentially
        // across nested CASE/cast expressions. Preserve that evaluation boundary.
        struct ScalarProjectionBoundary;
        impl ast::VisitorMut for ScalarProjectionBoundary {
            type Break = ();
            fn post_visit_table_factor(&mut self, factor: &mut ast::TableFactor) -> ControlFlow<()> {
                if let ast::TableFactor::Derived { subquery, .. } = factor {
                    let unlimited = subquery.limit_clause.is_none() || matches!(&subquery.limit_clause,
                        Some(ast::LimitClause::LimitOffset { limit: None, offset: None, limit_by }) if limit_by.is_empty());
                    if unlimited && subquery.fetch.is_none() {
                        if let ast::SetExpr::Select(select) = subquery.body.as_mut() {
                            let mut scalar_subquery = false;
                            let _: ControlFlow<()> = ast::visit_expressions_mut(&mut select.projection, |expr| {
                                scalar_subquery |= matches!(expr, ast::Expr::Subquery(_));
                                ControlFlow::Continue(())
                            });
                            if scalar_subquery {
                                subquery.limit_clause = Some(ast::LimitClause::LimitOffset {
                                    limit: None, limit_by: vec![], offset: Some(ast::Offset {
                                    value: ast::Expr::Value(ast::Value::Number("0".into(), false).into()),
                                    rows: ast::OffsetRows::None,
                                }) });
                            }
                        }
                    }
                }
                ControlFlow::Continue(())
            }
        }
        let _ = tree.visit(&mut ScalarProjectionBoundary);
    }
    if let SqlDialect::Custom(adapter) = dialect {
        struct RewriteQueries(&'static dyn super::DialectAdapter);
        impl ast::VisitorMut for RewriteQueries {
            type Break = SqlError;
            fn post_visit_query(&mut self, query: &mut ast::Query) -> ControlFlow<Self::Break> {
                match self.0.rewrite_query(query) { Ok(()) => ControlFlow::Continue(()), Err(error) => ControlFlow::Break(error) }
            }
        }
        if let ControlFlow::Break(error) = tree.visit(&mut RewriteQueries(adapter)) { return Err(error); }
    }
    Ok(())
}

fn adapt_expression(expr: &mut ast::Expr, dialect: SqlDialect) -> SqlResult<()> {
    if dialect == SqlDialect::Postgres {
        if let ast::Expr::Dictionary(fields) = expr {
            let args = fields.iter().map(|field| field.value.as_ref().clone()).collect::<Vec<_>>();
            let placeholders = (0..args.len()).map(|i|format!("__arg{i}")).collect::<Vec<_>>().join(", ");
            *expr = template(&format!("ROW({placeholders})"), &args)?;
            return Ok(());
        }
    }
    if let ast::Expr::IsNull(operand) | ast::Expr::IsNotNull(operand)
        | ast::Expr::IsTrue(operand) | ast::Expr::IsFalse(operand)
        | ast::Expr::IsNotTrue(operand) | ast::Expr::IsNotFalse(operand) = expr {
        if matches!(operand.as_ref(), ast::Expr::UnaryOp { .. } | ast::Expr::BinaryOp { .. }) {
            **operand = ast::Expr::Nested(Box::new(operand.as_ref().clone()));
        }
    }
    // SQL postfix predicates bind differently from comparisons. The upstream
    // unparser omits these parentheses, changing `(a IS NULL) = (b IS NULL)`
    // into `(a IS NULL = b) IS NULL` when the target parses it again.
    if let ast::Expr::BinaryOp { left, right, .. } = expr {
        for operand in [left, right] {
            if matches!(operand.as_ref(), ast::Expr::UnaryOp { .. }) {
                **operand = ast::Expr::Nested(Box::new(operand.as_ref().clone()));
            }
            if matches!(
                operand.as_ref(),
                ast::Expr::IsNull(_) | ast::Expr::IsNotNull(_)
                | ast::Expr::IsTrue(_) | ast::Expr::IsFalse(_)
                | ast::Expr::IsNotTrue(_) | ast::Expr::IsNotFalse(_)) {
                **operand = ast::Expr::Nested(Box::new(operand.as_ref().clone()));
            }
        }
    }
    if dialect == SqlDialect::Postgres {
        if let ast::Expr::Cast { kind: ast::CastKind::Cast, expr: value, data_type, .. } = expr {
            // DF's FLOAT spelling denotes Arrow Float32; PostgreSQL FLOAT
            // without a precision denotes Float64. Preserve the Arrow width.
            let ty = match data_type.to_string().as_str() {
                "FLOAT" | "REAL" => Some("REAL"),
                "DOUBLE PRECISION" | "DOUBLE" => Some("DOUBLE PRECISION"),
                _ => None,
            };
            if let Some(ty) = ty {
                let safe = super::postgres_functions::safe_cast(ty)?;
                let safe = safe.replace("__arg0", "v");
                *expr = template(&format!("(SELECT coalesce({safe}, CAST(v AS {ty})) FROM (SELECT __arg0 AS v) AS __local5)"), &[value.as_ref().clone()])?;
                return Ok(());
            }
        }
    }
    if dialect == SqlDialect::Postgres {
        if let ast::Expr::Cast {
            kind: ast::CastKind::TryCast,
            expr: value,
            data_type,
            ..
        } = expr
        {
            let ty = match data_type.to_string().as_str() { "FLOAT" => "REAL".to_string(), other => other.to_string() };
            let arg = value.as_ref().clone();
            let rule = super::postgres_functions::safe_cast(&ty)?;
            *expr = template(&rule, &[arg])?;
            return Ok(());
        }
    }
    if dialect == SqlDialect::Postgres {
        if let ast::Expr::Case {
            conditions,
            else_result,
            ..
        } = expr
        {
            let nonempty = conditions
                .iter()
                .map(|branch| &branch.result)
                .chain(else_result.iter().map(|e| e.as_ref()))
                .find(|e| matches!(e, ast::Expr::Array(a) if !a.elem.is_empty()))
                .cloned();
            if let Some(nonempty) = nonempty {
                for branch in conditions
                    .iter_mut()
                    .map(|branch| &mut branch.result)
                    .chain(else_result.iter_mut().map(|e| e.as_mut()))
                {
                    if matches!(branch, ast::Expr::Array(a) if a.elem.is_empty()) {
                        *branch = template("(__arg0)[1:0]", &[nonempty.clone()])?;
                    }
                }
            }
        }
    }
    if dialect == SqlDialect::Postgres {
        if let ast::Expr::Array(array) = expr {
            array.named = true;
        }
        if let ast::Expr::CompoundFieldAccess { root, access_chain } = expr {
            if let [ast::AccessExpr::Subscript(ast::Subscript::Index { index })] =
                access_chain.as_slice()
            {
                let args = [root.as_ref().clone(), index.clone()];
                *expr = template(
                    "(SELECT a[CASE WHEN i < 0 THEN cardinality(a) + i + 1 ELSE i END] FROM (SELECT __arg0 AS a, __arg1 AS i) AS __local0)",
                    &args,
                )?;
                return Ok(());
            }
        }
    }
    let ast::Expr::Function(function) = expr else {
        if let SqlDialect::Custom(adapter) = dialect { return adapter.rewrite_expression(expr); }
        return Ok(());
    };
    let name = function.name.to_string();
    if let Some(lowered) = dialect.lower_function(function)? {
        *expr = lowered;
        return Ok(());
    }
    if let ast::FunctionArguments::List(arguments) = &function.args {
        let positional = arguments
            .args
            .iter()
            .map(|argument| match argument {
                ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(value)) => Some(value.clone()),
                _ => None,
            })
            .collect::<Option<Vec<_>>>();
        if let Some(arguments) = positional {
            if let Some(lowered) =
                dialect.lower_scalar_function(name.trim_matches('"'), &arguments)?
            {
                *expr = lowered;
                return Ok(());
            }
        }
    }
    if name.trim_matches('"').starts_with("__orchiddb_logical_") {
        let implementation = super::logical_functions::mapping(&name)
            .ok_or_else(|| SqlError::Unsupported(format!("logical function {name} has no {} mapping", dialect.name())))?;
        let ast::FunctionArguments::List(arguments) = &function.args else {
            return Err(SqlError::Unsupported("invalid logical function arguments".into()));
        };
        let args = arguments.args.iter().map(|arg| match arg {
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(expr)) => Ok(expr.clone()),
            _ => Err(SqlError::Unsupported("logical functions require positional expressions".into())),
        }).collect::<SqlResult<Vec<_>>>()?;
        *expr = portable_template(&implementation.value, &args, dialect)?;
        return Ok(());
    }
    if name == crate::ir::functions::ENGINE_CAST_FUNCTION {
        let ast::FunctionArguments::List(arguments) = &function.args else {
            return Err(SqlError::Unsupported(
                "invalid declared UDF argument cast".into(),
            ));
        };
        let [
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(value)),
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(ast::Expr::Value(type_value))),
        ] = arguments.args.as_slice()
        else {
            return Err(SqlError::Unsupported(
                "invalid declared UDF argument cast".into(),
            ));
        };
        let ast::Value::SingleQuotedString(sql_type) = &type_value.value else {
            return Err(SqlError::Unsupported(
                "invalid declared UDF argument type".into(),
            ));
        };
        *expr = portable_template(&format!("CAST(__arg0 AS {sql_type})"), &[value.clone()], dialect)?;
        return Ok(());
    }
    if let Some(native) = name.strip_prefix(crate::ir::functions::ENGINE_FUNCTION_PREFIX) {
        if dialect == SqlDialect::Postgres && matches!(native, "quantile_cont" | "quantile_disc") {
            if let ast::FunctionArguments::List(arguments) = &function.args {
                let args = arguments.args.iter().filter_map(|arg| match arg {
                    ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(expr)) => Some(expr.clone()), _ => None,
                }).collect::<Vec<_>>();
                return super::postgres_functions::adapt(expr, native, &args);
            }
        }
        let components = native.split('.').collect::<Vec<_>>();
        if components.iter().any(|part| part.is_empty()) {
            return Err(SqlError::Unsupported(format!(
                "invalid engine function path {native:?}"
            )));
        }
        function.name = ast::ObjectName::from(
            components
                .into_iter()
                .map(|part| dialect.identifier(part))
                .collect::<Vec<_>>(),
        );
        return Ok(());
    }
    let ast::FunctionArguments::List(arguments) = &function.args else {
        if let SqlDialect::Custom(adapter) = dialect { return adapter.rewrite_expression(expr); }
        return Ok(());
    };
    let args: Option<Vec<_>> = arguments
        .args
        .iter()
        .map(|arg| match arg {
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(expr)) => Some(expr.clone()),
            _ => None,
        })
        .collect();
    let Some(args) = args else {
        if let SqlDialect::Custom(adapter) = dialect { return adapter.rewrite_expression(expr); }
        return Ok(());
    };
    if args.len() == 2 && matches!(name.as_str(), "__orchiddb_is_not_distinct_from" | "__orchiddb_is_distinct_from") {
        *expr = if name == "__orchiddb_is_not_distinct_from" {
            ast::Expr::IsNotDistinctFrom(Box::new(args[0].clone()), Box::new(args[1].clone()))
        } else {
            ast::Expr::IsDistinctFrom(Box::new(args[0].clone()), Box::new(args[1].clone()))
        };
        return Ok(());
    }
    if let SqlDialect::Custom(adapter) = dialect { return adapter.rewrite_expression(expr); }
    if dialect == SqlDialect::Postgres {
        return super::postgres_functions::adapt(expr, &name, &args);
    }
    match (name.as_str(), args.len()) {
        ("btrim", 1 | 2) => rename(function, "trim"),
        // DuckDB reverse uses grapheme clusters; DataFusion/Cypher reverse
        // uses Unicode code points. Splitting on the empty separator keeps
        // code points separate, including combining marks and ZWJ.
        ("reverse", 1) => {
            *expr = template("array_to_string(list_reverse(string_split(__arg0, '')), '')", &args)?;
        }
        ("array_position", 3) if args[2].to_string() == "1" => {
            *expr = template("array_position(__arg0, __arg1)", &args)?;
        }

        ("encode", 2) if matches!(&args[1],
            ast::Expr::Value(value) if value.value == ast::Value::SingleQuotedString("hex".into())) => {
            // Arrow casts text to raw UTF-8 bytes. DuckDB's CAST(text AS BLOB)
            // instead parses backslash escapes and rejects non-ASCII text.
            fn unnest(expr:&ast::Expr)->&ast::Expr {match expr {ast::Expr::Nested(inner)=>unnest(inner),_=>expr}}
            let input=match unnest(&args[0]) {
                ast::Expr::Cast {expr:inner,data_type,..} if data_type.to_string()=="BLOB" => match unnest(inner) {
                        ast::Expr::Cast {data_type,..} if data_type.to_string().starts_with("VARCHAR")=>
                    {
                        template("encode(__arg0)",&[inner.as_ref().clone()])?
                    }
                    _=>args[0].clone(),
                    },
                _=>args[0].clone(),
            };
            *expr = template("lower(hex(__arg0))", &[input])?;
        }
        ("__orchiddb_utf16_length", 1) => {
            *expr = template(r"CAST(length(regexp_replace(__arg0, '[\x{10000}-\x{10FFFF}]', 'xx', 'g')) AS INTEGER)", &args)?;
        }
        ("__orchiddb_utf16_substring", 2 | 3) => {
            let mut args = args;
            if args.len() == 2 {
                args.push(ast::Expr::Value(ast::Value::Number("9223372036854775807".into(), false).into()));
            }
            *expr = template(r"
                (SELECT CASE WHEN __local0.s IS NULL THEN NULL ELSE COALESCE(
                    (SELECT string_agg(CASE WHEN p >= lo AND p + w <= hi THEN ch ELSE '�' END, '' ORDER BY p)
                     FROM (SELECT ch, w, sum(w) OVER (ORDER BY ord ROWS UNBOUNDED PRECEDING) - w AS p
                           FROM (SELECT ch, ord, CASE WHEN unicode(ch) > 65535 THEN 2 ELSE 1 END AS w
                                 FROM unnest(regexp_extract_all(__local0.s, '(?s).')) WITH ORDINALITY AS __local1(ch, ord)) AS __local2) AS __local3
                     WHERE hi > lo AND p < hi AND p + w > lo), '') END
                 FROM (SELECT s,
                         greatest(0, least(n, CASE WHEN a < 0 THEN n + a ELSE a END)) AS lo,
                         greatest(0, least(n, CASE WHEN b < 0 THEN n + b ELSE b END)) AS hi
                       FROM (SELECT s, a, b, length(regexp_replace(s, '[\x{10000}-\x{10FFFF}]', 'xx', 'g')) AS n
                             FROM (SELECT __arg0 AS s, coalesce(__arg1, 0) AS a, coalesce(__arg2, 9223372036854775807) AS b) AS __local2) AS __local1) AS __local0)", &args)?;
        }
        ("array_min", 1) => rename(function, "list_min"),
        ("array_max", 1) => rename(function, "list_max"),
        ("regexp_like", 2 | 3) => rename(function, "regexp_matches"),
        ("nanvl", 2) => {
            *expr = template(
                "list_extract(list_transform([struct_pack(lhs := __arg0, rhs := __arg1)], lambda __local0: CASE WHEN isnan(__local0.lhs) THEN __local0.rhs ELSE __local0.lhs END), 1)",
                &args,
            )?
        }
        ("log", 2) => *expr = template("ln(__arg1) / ln(__arg0)", &args)?,
        // DuckDB sign(NaN)=0, whereas DataFusion preserves NaN. Bind the
        // argument once so volatile calls keep their evaluation count.
        ("signum", 1) => {
            *expr = template(
                "list_extract(list_transform([__arg0], lambda __local0: CASE WHEN isnan(__local0) THEN __local0 ELSE sign(__local0) END), 1)",
                &args,
            )?
        }
        ("trunc", 2) => {
            *expr = template(
                "list_extract(list_transform([power(10.0, __arg1)], lambda __local0: trunc(__arg0 * __local0) / __local0), 1)",
                &args,
            )?
        }
        ("array_distinct", 1) => {
            *expr = template(
                "list_extract(list_transform([__arg0], lambda __local0: list_filter(__local0, lambda __local1, __local2: list_position(__local0, __local1) = __local2)), 1)",
                &args,
            )?
        }
        // DuckDB has no list_replace. Null matches are intentional, as in
        // DataFusion's compare_element_to_list(..., true).
        ("array_replace_all", 3) => {
            *expr = template(
                "list_extract(list_transform([struct_pack(items := __arg0, old := __arg1, new := __arg2)], lambda __local0: list_transform(__local0.items, lambda __local1: CASE WHEN __local1 IS NOT DISTINCT FROM __local0.old THEN __local0.new ELSE __local1 END)), 1)",
                &args,
            )?
        }
        // list_intersect loses NULL elements and reverses order. DataFusion
        // probes the longer list, preserves its order, and includes NULL.
        ("array_intersect", 2) => {
            *expr = template(
                "list_extract(list_transform([struct_pack(lhs := __arg0, rhs := __arg1)], lambda __local0: CASE WHEN __local0.lhs IS NULL OR __local0.rhs IS NULL THEN NULL ELSE list_extract(list_transform([struct_pack(probe := CASE WHEN len(__local0.lhs) < len(__local0.rhs) THEN __local0.rhs ELSE __local0.lhs END, lookup := CASE WHEN len(__local0.lhs) < len(__local0.rhs) THEN __local0.lhs ELSE __local0.rhs END)], lambda __local1: list_filter(__local1.probe, lambda __local2, __local3: list_position(__local1.probe, __local2) = __local3 AND list_position(__local1.lookup, __local2) IS NOT NULL)), 1) END), 1)",
                &args,
            )?
        }
        _ => {}
    }
    Ok(())
}

fn rename(function: &mut ast::Function, name: &str) {
    function.name = ast::ObjectName::from(vec![ast::Ident::new(name)]);
}

/// Parse only trusted rule templates, then substitute argument ASTs. Fresh
/// lambda names prevent capturing outer columns or nested lambdas.
pub(super) fn template(source: &str, args: &[ast::Expr]) -> SqlResult<ast::Expr> {
    portable_template(source, args, SqlDialect::DuckDb)
}
pub(super) fn portable_template(source: &str, args: &[ast::Expr], dialect: SqlDialect) -> SqlResult<ast::Expr> {
    let rendered = args
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
    let parser_dialect = dialect.parser_dialect();
    let mut tokens = datafusion::sql::sqlparser::tokenizer::Tokenizer::new(parser_dialect.as_ref(), source)
        .tokenize().map_err(|err| SqlError::Unsupported(format!("dialect expression template: {err}")))?;
    let mut locals = std::collections::BTreeMap::new();
    for token in &mut tokens {
        let datafusion::sql::sqlparser::tokenizer::Token::Word(word) = token else { continue; };
        if word.quote_style.is_some() { continue; }
        let Some(index) = word.value.strip_prefix("__local").and_then(|s| s.parse::<usize>().ok()) else { continue; };
        // Rename only generated identifier tokens, never JSON keys, text
        // literals, quoted user identifiers, or substrings of another name.
        let fresh = locals.entry(index).or_insert_with(|| {
            let mut suffix = 0;
            loop {
                let name = format!("__graph_dialect_{index}_{suffix}");
                if !rendered.contains(&name) && !source.contains(&name) { break name; }
                suffix += 1;
            }
        });
        word.value = fresh.clone();
    }
    let mut parser = Parser::new(parser_dialect.as_ref()).with_tokens(tokens);
    let mut expression = parser
        .parse_expr()
        .map_err(|err| SqlError::Unsupported(format!("dialect expression template: {err}")))?;
    if parser.peek_token().token != datafusion::sql::sqlparser::tokenizer::Token::EOF {
        return Err(SqlError::Unsupported(format!("SQL expression template has trailing token {}: {source}", parser.peek_token())));
    }
    struct Arguments<'a>(&'a [ast::Expr]);
    impl ast::VisitorMut for Arguments<'_> {
        type Break = SqlError;
        fn post_visit_expr(&mut self, expr: &mut ast::Expr) -> ControlFlow<Self::Break> {
            if let ast::Expr::Identifier(ident) = expr {
                if let Some(index) = ident.value.strip_prefix("__arg").and_then(|s|s.parse::<usize>().ok()) {
                    let Some(value) = self.0.get(index) else {
                        return ControlFlow::Break(SqlError::Unsupported(format!("unbound function mapping argument {}",ident.value)));
                    };
                    // Post-order substitution never revisits caller expressions.
                    *expr = ast::Expr::Nested(Box::new(value.clone()));
                }
            }
            ControlFlow::Continue(())
        }
    }
    if let ControlFlow::Break(error) = ast::VisitMut::visit(&mut expression, &mut Arguments(args)) { return Err(error); }
    Ok(ast::Expr::Nested(Box::new(expression)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_preserve_argument_identifiers_and_reject_missing_arguments() {
        let column = ast::Expr::Identifier(ast::Ident::with_quote('"', "__arg1"));
        assert_eq!(template("__arg0", &[column]).unwrap().to_string(), "((\"__arg1\"))");
        assert!(template("__arg1", &[]).is_err());
        assert!(template("__arg0; SELECT 1", &[ast::Expr::Identifier(ast::Ident::new("x"))]).is_err());
        let sql = template("(SELECT '__local12' AS \"__local12\" FROM (SELECT __arg0 AS value) AS __local12)", &[ast::Expr::Identifier(ast::Ident::new("payload"))]).unwrap().to_string();
        assert!(sql.contains("'__local12'") && sql.contains("\"__local12\"") && sql.contains("AS __graph_dialect_12_0"), "{sql}");
    }

    fn rewrite(sql: &str, dialect: SqlDialect) -> String {
        let mut statements = Parser::parse_sql(&DuckDbDialect {}, sql).unwrap();
        prepare_ast(&mut statements[0], dialect).unwrap();
        statements[0].to_string()
    }

    #[test]
    fn correlated_exists_keeps_outer_key_when_inner_projects_it_under_an_alias() {
        let sql = rewrite("SELECT l.id FROM source l WHERE EXISTS (SELECT 1 FROM (SELECT l.id AS right_id FROM source l WHERE l.id = 1) derived_1 WHERE l.id = derived_1.right_id)", SqlDialect::Postgres);
        assert!(sql.contains("WHERE l.id = right_id"), "{sql}");
        assert!(!sql.contains("WHERE right_id = right_id"), "{sql}");
    }

    #[test]
    fn postgres_preserves_reused_scalar_projection_boundary() {
        let sql = rewrite("SELECT x + x FROM (SELECT TRY_CAST(v AS DOUBLE) AS x FROM source) t", SqlDialect::Postgres);
        assert!(sql.contains("FROM source OFFSET 0"), "{sql}");
    }

    #[cfg(feature = "postgres")]
    #[test]
    fn postgres_lenient_casts_do_not_abort_on_numeric_overflow() {
        let Ok(url) = std::env::var("GRAPH_PG_URL") else { return; };
        let mut client = postgres::Client::connect(&url, postgres::NoTls).unwrap();
        let huge = "9".repeat(200_000);
        let padded = format!("{}1", "0".repeat(200_000));
        for (ty, value, expected) in [
            ("BIGINT", huge.as_str(), None),
            ("BIGINT", padded.as_str(), Some("1")),
            ("BIGINT", "9223372036854775808", None),
            ("BIGINT", "-9223372036854775808", Some("-9223372036854775808")),
            ("DECIMAL(5,2)", huge.as_str(), None),
            ("DECIMAL(5,2)", "1e99999999999999999999", None),
            ("DECIMAL(5,2)", "1e-99999999999999999999", Some("0.00")),
            ("DECIMAL(5,2)", "999.995", None),
            ("DECIMAL(5,2)", "12.345", Some("12.35")),
            ("DOUBLE", "1e99999999999999999999", Some("Infinity")),
            ("DOUBLE", "-1e99999999999999999999", Some("-Infinity")),
            ("DOUBLE", "1e-99999999999999999999", Some("0")),
            ("REAL", "1e39", Some("Infinity")),
            ("REAL", "1e-60", Some("0")),
            ("DOUBLE", "invalid", None),
        ] {
            let sql = rewrite(&format!("SELECT CAST(TRY_CAST(v AS {ty}) AS VARCHAR) FROM (VALUES (CAST($1 AS VARCHAR))) AS input(v)"), SqlDialect::Postgres);
            let row = client.query_one(&sql, &[&value]).unwrap_or_else(|e| panic!("{ty}: {e}"));
            assert_eq!(row.get::<_, Option<String>>(0).as_deref(), expected, "{ty}");
        }
    }

    #[test]
    fn function_adaptation_preserves_literals_and_identifiers_in_both_dialects() {
        let original = "SELECT array_min(xs), 'array_min(xs)', xs AS \"array_min(xs)\" FROM t";
        let duck = rewrite(original, SqlDialect::DuckDb);
        assert!(duck.contains("list_min(xs)"), "{duck}");
        assert!(duck.contains("'array_min(xs)'"), "{duck}");
        assert!(duck.contains("AS \"array_min(xs)\""), "{duck}");
        let postgres = rewrite(original, SqlDialect::Postgres);
        assert!(
            postgres.contains("SELECT min(v) FROM unnest((xs))"),
            "{postgres}"
        );
        assert!(postgres.contains("'array_min(xs)'"), "{postgres}");
        assert!(postgres.contains("AS \"array_min(xs)\""), "{postgres}");
    }

    #[test]
    fn native_calls_bypass_standard_rules_including_aggregates() {
        let sql = rewrite(
            "SELECT __engine_function_log(100), __engine_function_sum(x), log(2, 8) FROM t",
            SqlDialect::DuckDb,
        );
        assert!(sql.contains("\"log\"(100)"), "{sql}");
        assert!(sql.contains("\"sum\"(x)"), "{sql}");
        assert!(sql.contains("ln((8)) / ln((2))"), "{sql}");
        let mut statement =
            Parser::parse_sql(&DuckDbDialect {}, "SELECT __engine_function_log(100)")
                .unwrap()
                .remove(0);
        prepare_ast(&mut statement, SqlDialect::Postgres).unwrap();
        assert!(statement.to_string().contains("\"log\"(100)"));
    }

    #[test]
    fn native_schema_paths_quote_components_and_keep_sql_data_inert() {
        let sql = rewrite(
            "SELECT __engine_function_app.score('x); DROP TABLE t; --')",
            SqlDialect::DuckDb,
        );
        assert_eq!(sql, "SELECT \"app\".\"score\"('x); DROP TABLE t; --')");
        let mut expression = Parser::new(&DuckDbDialect {})
            .try_with_sql("f('unchanged')")
            .unwrap()
            .parse_expr()
            .unwrap();
        let ast::Expr::Function(function) = &mut expression else {
            unreachable!()
        };
        function.name = ast::ObjectName::from(vec![ast::Ident::new(
            "__engine_function_app.score); DROP TABLE t; --",
        )]);
        prepare_ast(&mut expression, SqlDialect::DuckDb).unwrap();
        assert_eq!(
            expression.to_string(),
            "\"app\".\"score); DROP TABLE t; --\"('unchanged')"
        );
    }

    #[test]
    fn lambda_variables_do_not_capture_outer_columns() {
        let sql = rewrite(
            "SELECT array_replace_all(xs, __graph_dialect_0_0, __graph_dialect_1_0) FROM t",
            SqlDialect::DuckDb,
        );
        assert!(sql.contains("lambda __graph_dialect_0_1"), "{sql}");
        assert!(sql.contains("lambda __graph_dialect_1_1"), "{sql}");
        assert!(sql.contains("old := (__graph_dialect_0_0)"), "{sql}");
    }

    #[cfg(feature = "duckdb")]
    #[test]
    fn expression_binding_preserves_special_literals() {
        use datafusion::prelude::lit;
        let connection = duckdb::Connection::open_in_memory().unwrap();
        let schema = DFSchema::empty();
        for value in [
            "double \" quotes",
            "it''s",
            "a\\'b",
            "nul\0byte",
            "\0''\\'\"",
        ] {
            let sql = expression_sql(&lit(value), &schema).unwrap();
            let returned: String = connection
                .query_row(&format!("SELECT {sql}"), [], |row| row.get(0))
                .unwrap();
            assert_eq!(returned, value, "{sql}");
        }
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let sql = expression_sql(&lit(value), &schema).unwrap();
            let returned: f64 = connection
                .query_row(&format!("SELECT {sql}"), [], |row| row.get(0))
                .unwrap();
            assert!(
                returned == value || returned.is_nan() && value.is_nan(),
                "{sql}: {returned}"
            );
        }
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let sql = expression_sql(&lit(value), &schema).unwrap();
            let returned: f32 = connection
                .query_row(&format!("SELECT {sql}"), [], |row| row.get(0))
                .unwrap();
            assert!(
                returned == value || returned.is_nan() && value.is_nan(),
                "{sql}: {returned}"
            );
        }
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn standard_expressions_match_datafusion_results_in_duckdb() {
        use datafusion::common::ScalarValue;
        use datafusion::prelude::SessionContext;
        let ctx = SessionContext::new();
        let conn = duckdb::Connection::open_in_memory().unwrap();
        let queries = [
            "SELECT log(2.0, 8.0), log(100.0), trunc(-12.345, 2), trunc(123.4, -1), 2.0 / log(2.0, 8.0)",
            "SELECT signum(-0.0), signum(-2.0), signum(3.0), signum(CAST('NaN' AS DOUBLE)), signum(CAST(NULL AS DOUBLE))",
            "SELECT array_replace_all([1, 2, 1, NULL], 1, 9), array_replace_all([1, NULL, 2], CAST(NULL AS INT), 9)",
            "SELECT array_replace_all([1, 2, 1], 1, CAST(NULL AS INT)), array_replace_all(CAST(NULL AS INT[]), 1, 9)",
            "SELECT array_intersect([1, NULL, 2, 2], [NULL, 2]), array_intersect([2, 1], [1, 2, 3]), array_intersect(CAST(NULL AS INT[]), [1])",
            "SELECT array_distinct([2, NULL, 1, 2, NULL]), array_distinct(CAST(NULL AS INT[])), array_distinct(CAST([] AS INT[]))",
            "SELECT array_min([2, NULL, 1]), array_max([2, NULL, 1]), array_min(CAST([] AS INT[]))",
            "SELECT nanvl(CAST('NaN' AS DOUBLE), 9.0), nanvl(2.0, 9.0), nanvl(CAST(NULL AS DOUBLE), 9.0)",
            "SELECT regexp_like('alphabet', 'pha'), regexp_like('ABC', 'abc', 'i'), regexp_like(CAST(NULL AS VARCHAR), 'x')",
        ];
        for query in queries {
            let frame = ctx.sql(query).await.unwrap();
            let plan = frame.logical_plan();
            let dialect = SqlDialect::DuckDb.unparser_dialect();
            // DF53 cannot unparse explicit list casts. Exercise those
            // boundary cases directly through the identical SQL AST adapter.
            let mut statement = if query.contains("AS INT[]") {
                Parser::parse_sql(&DuckDbDialect {}, query)
                    .unwrap()
                    .remove(0)
            } else {
                Unparser::new(dialect.as_ref()).plan_to_sql(plan).unwrap()
            };
            prepare_ast(&mut statement, SqlDialect::DuckDb).unwrap();
            let sql = statement.to_string();
            let expected = frame.collect().await.unwrap();
            let mut prepared = conn
                .prepare(&sql)
                .unwrap_or_else(|err| panic!("{query}\n{sql}\n{err}"));
            let actual: Vec<_> = prepared.query_arrow([]).unwrap().collect();
            for col in 0..expected[0].num_columns() {
                let actual_column = arrow::compute::cast(
                    actual[0].column(col),
                    expected[0].column(col).data_type(),
                )
                .unwrap();
                let a = ScalarValue::try_from_array(&actual_column, 0).unwrap();
                let e = ScalarValue::try_from_array(expected[0].column(col), 0).unwrap();
                // SQL engines choose different integer widths for constants;
                // displayed scalar values compare values including list order.
                assert_eq!(a.to_string(), e.to_string(), "{query}\n{sql}\ncolumn {col}");
            }
        }
    }
}
