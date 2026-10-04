//! Backend SQL for a first-class ranked search join.
use super::*;
use crate::ir::rel::search::{RankedJoin, SCORE_COLUMN, SearchMetric};
use datafusion::sql::{
    sqlparser::{
        ast,
        dialect::{DuckDbDialect, PostgreSqlDialect},
        parser::Parser,
    },
    unparser::Unparser,
};

/// Built-in engine transformations. Physical source facts select a rule;
/// engine ownership and dependent execution are handled by generic placement.
pub(crate) fn lower_relation(
    plan: &LogicalPlan,
    ctx: &super::lowering::LoweringContext,
) -> SqlResult<Option<super::lowering::RelationLowering>> {
    use super::lowering::{LoweringMode, RelationLowering};
    let LogicalPlan::Extension(extension) = plan else {
        return Ok(None);
    };
    let Some(search) = extension.node.as_any().downcast_ref::<RankedJoin>() else {
        return Ok(None);
    };
    let format = search
        .source_metadata
        .as_ref()
        .and_then(|m| m.format.as_deref());
    if format == Some("lance") {
        if ctx.dialect != SqlDialect::DuckDb {
            return Err(SqlError::Unsupported(
                "Lance source requires DuckDB ownership".into(),
            ));
        }
        return Ok(Some(RelationLowering::Dependent {
            source: search.source.clone(),
            template: lance_template(search)?,
            schema: search.schema.clone(),
        }));
    }
    if search.index.is_none()
        && (ctx.dialect != SqlDialect::Postgres || search.metric() == Some(SearchMetric::Bm25))
    {
        return Ok(Some(RelationLowering::Rewrite(
            search
                .native_plan()
                .map_err(|e| SqlError::Unsupported(e.to_string()))?,
        )));
    }
    if format.is_some_and(|f| !matches!(f, "pgvector" | "postgres")) {
        return Err(SqlError::Unsupported(format!(
            "{} does not lower indexed access for source format {:?}",
            ctx.dialect.name(),
            format
        )));
    }
    if ctx.dialect != SqlDialect::Postgres {
        return Err(SqlError::Unsupported(
            "indexed vector access requires a registered transformation for the owning engine"
                .into(),
        ));
    }
    if ctx.mode == LoweringMode::Bound {
        Ok(Some(RelationLowering::Dependent {
            source: search.source.clone(),
            template: postgres_template(search)?,
            schema: search.schema.clone(),
        }))
    } else {
        statement(search, ctx.dialect)
            .map(RelationLowering::Sql)
            .map(Some)
    }
}
pub(crate) fn statement(search: &RankedJoin, dialect: SqlDialect) -> SqlResult<ast::Statement> {
    let sql = stacker::maybe_grow(4 * 1024 * 1024, 64 * 1024 * 1024, || {
        super::logical_functions::with_plan(&search.clone().into_plan(), dialect, || {
            render(search, dialect)
        })
    })?;
    let dialect_parser = dialect.parser_dialect();
    Parser::parse_sql(dialect_parser.as_ref(), &sql)
        .map_err(|e| SqlError::Unsupported(e.to_string()))?
        .into_iter()
        .next()
        .ok_or_else(|| SqlError::Unsupported("empty search SQL".into()))
}
pub(crate) fn expression(
    search: &RankedJoin,
    expr: Expr,
    dialect: SqlDialect,
) -> SqlResult<String> {
    let expr = expr
        .transform_up(|e| {
            if let Expr::Column(mut c) = e {
                let alias = if search.source.schema().has_column(&c) {
                    "__search_source"
                } else {
                    "__search_target"
                };
                c.relation = Some(datafusion::common::TableReference::bare(alias));
                return Ok(Transformed::yes(Expr::Column(c)));
            }
            Ok(Transformed::no(e))
        })?
        .data;
    let sql_dialect = dialect.unparser_dialect();
    let mut ast = Unparser::new(sql_dialect.as_ref()).expr_to_sql(&expr)?;
    super::functions::prepare_scoped_ast(&mut ast, dialect, false)?;
    Ok(ast.to_string())
}
pub(crate) fn render(search: &RankedJoin, dialect: SqlDialect) -> SqlResult<String> {
    let source = super::recursive::unparse_plan(search.source.as_ref().clone(), dialect)?;
    let target = super::recursive::unparse_plan(search.target.as_ref().clone(), dialect)?;
    let query = expression(search, search.query().clone(), dialect)?;
    let document = expression(search, search.document().clone(), dialect)?;
    let operator = match search.metric() {
        Some(SearchMetric::Cosine) => "<=>",
        Some(SearchMetric::Dot) => "<#>",
        Some(SearchMetric::L2) => "<->",
        _ => {
            return Err(SqlError::Unsupported(
                "no indexed operator for search function".into(),
            ));
        }
    };
    let order = format!("CAST({document} AS vector) {operator} CAST({query} AS vector)");
    let order = if search.exact {
        format!("({order}) + 0.0")
    } else {
        order
    };
    let predicate = search
        .predicate
        .clone()
        .map(|p| expression(search, p, dialect))
        .transpose()?
        .unwrap_or_else(|| "TRUE".into());
    let score = expression(search, search.score.clone(), dialect)?;
    let tie = if search.exact {
        search
            .target_keys
            .iter()
            .map(|e| {
                expression(search, e.clone(), dialect).map(|s| format!(", {s} ASC NULLS LAST"))
            })
            .collect::<SqlResult<Vec<_>>>()?
            .join("")
    } else {
        String::new()
    };
    let source_cols = search
        .source
        .schema()
        .fields()
        .iter()
        .map(|f| format!("__search_source.{}", dialect.quote_ident(f.name())))
        .collect::<Vec<_>>()
        .join(", ");
    let target_cols = search
        .target
        .schema()
        .fields()
        .iter()
        .map(|f| format!("__search_target.{}", dialect.quote_ident(f.name())))
        .collect::<Vec<_>>()
        .join(", ");
    let hit_columns = search
        .target
        .schema()
        .fields()
        .iter()
        .map(|f| {
            let column = format!("__search_hits.{}", dialect.quote_ident(f.name()));
            if super::postgres_lists::nested(f.data_type()) {
                format!(
                    "__orchiddb_pg_list_normalize({column}) AS {}",
                    dialect.quote_ident(f.name())
                )
            } else {
                column
            }
        })
        .chain(std::iter::once(format!(
            "__search_hits.{}",
            dialect.quote_ident(SCORE_COLUMN)
        )))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "SELECT {source_cols}, {hit_columns} FROM ({source}) AS __search_source CROSS JOIN LATERAL (SELECT {target_cols}, {score} AS {} FROM ({target}) AS __search_target WHERE {predicate} ORDER BY {order} ASC NULLS LAST{tie} LIMIT {}) AS __search_hits",
        dialect.quote_ident(SCORE_COLUMN),
        search.limit
    ))
}

/// Compatibility name for the general dependent SQL template.
pub type SearchTemplate = super::lowering::SqlTemplate;

pub(crate) fn lance_template(search: &RankedJoin) -> SqlResult<SearchTemplate> {
    stacker::maybe_grow(4 * 1024 * 1024, 64 * 1024 * 1024, || {
        super::logical_functions::with_plan(&search.clone().into_plan(), SqlDialect::DuckDb, || {
            lance_template_inner(search)
        })
    })
}
fn lance_template_inner(search: &RankedJoin) -> SqlResult<SearchTemplate> {
    let index = search
        .index
        .as_ref()
        .ok_or_else(|| SqlError::Unsupported("missing Lance index binding".into()))?;
    let metadata = search
        .source_metadata
        .as_ref()
        .ok_or_else(|| SqlError::Unsupported("missing Lance source metadata".into()))?;
    let uri = metadata
        .options
        .get("uri")
        .and_then(serde_json::Value::as_str)
        .filter(|uri| !uri.is_empty())
        .ok_or_else(|| SqlError::Unsupported("Lance source requires a dataset uri".into()))?;
    let option = |name: &str| -> SqlResult<Option<u64>> {
        index
            .options
            .get(name)
            .or_else(|| metadata.options.get(name))
            .map(|value| {
                value
                    .as_u64()
                    .filter(|n| *n > 0 && *n <= u32::MAX as u64)
                    .ok_or_else(|| {
                        SqlError::Unsupported(format!("Lance {name} must be a positive u32"))
                    })
            })
            .transpose()
    };
    let nprobes = option("nprobes")?;
    let refine_factor = option("refine_factor")?;
    if search.limit == 0 {
        let columns = search
            .schema
            .fields()
            .iter()
            .map(|f| {
                let value = ScalarValue::try_from(f.data_type())?;
                let value =
                    super::exchange_literal(value, f.data_type().clone(), SqlDialect::DuckDb)?;
                Ok(format!(
                    "{value} AS {}",
                    SqlDialect::DuckDb.quote_ident(f.name())
                ))
            })
            .collect::<SqlResult<Vec<_>>>()?;
        return Ok(SearchTemplate {
            sql: format!("SELECT {} WHERE FALSE", columns.join(", ")),
            parameters: search.source.schema().fields().len(),
            dialect: "duckdb".into(),
        });
    }
    // duckdb-lance chooses the index's metric. Without an index its exact
    // scanner uses L2; do not silently change cosine/dot into L2.
    if search.exact && index.metric != SearchMetric::L2 && index.metric != SearchMetric::Bm25 {
        return Err(SqlError::Unsupported("duckdb-lance exact vector search currently exposes only L2; cosine/dot require a matching index and approximate_allowed".into()));
    }
    validate_lance_predicate(search)?;
    let d = SqlDialect::DuckDb;
    let query = expression(search, search.query().clone(), d)?;
    let string = |s: &str| format!("'{}'", s.replace('\'', "''"));
    let k = search.limit;
    let function = if index.metric == SearchMetric::Bm25 {
        format!(
            "lance_fts({}, {}, {query}, k = {k}, prefilter = true)",
            string(uri),
            string(&index.column)
        )
    } else {
        let controls = nprobes
            .map(|n| format!(", nprobs = {n}"))
            .unwrap_or_default()
            + &refine_factor
                .map(|n| format!(", refine_factor = {n}"))
                .unwrap_or_default();
        format!(
            "lance_vector_search({}, {}, {query}, k = {k}, use_index = {}, prefilter = true{controls})",
            string(uri),
            string(&index.column),
            !search.exact
        )
    };
    let mut target = Parser::parse_sql(
        &DuckDbDialect {},
        &super::unparse_plan(search.target.as_ref().clone(), d)?,
    )
    .map_err(|e| SqlError::Unsupported(e.to_string()))?;
    let mut parsed = Parser::parse_sql(&DuckDbDialect {}, &format!("SELECT * FROM {function}"))
        .map_err(|e| SqlError::Unsupported(e.to_string()))?;
    let ast::Statement::Query(q) = parsed.remove(0) else {
        unreachable!()
    };
    let ast::SetExpr::Select(select) = *q.body else {
        unreachable!()
    };
    let relation = select.from[0].relation.clone();
    struct Replace {
        table: String,
        relation: ast::TableFactor,
        count: usize,
    }
    impl ast::VisitorMut for Replace {
        type Break = SqlError;
        fn post_visit_table_factor(
            &mut self,
            factor: &mut ast::TableFactor,
        ) -> std::ops::ControlFlow<Self::Break> {
            if let ast::TableFactor::Table {
                name,
                alias,
                args: None,
                ..
            } = factor
            {
                let normalized = name
                    .0
                    .iter()
                    .filter_map(|p| p.as_ident())
                    .map(|p| p.value.as_str())
                    .collect::<Vec<_>>()
                    .join(".");
                if normalized != self.table {
                    return std::ops::ControlFlow::Break(SqlError::Unsupported(
                        "Lance search target must be its declared physical table".into(),
                    ));
                }
                let alias = alias.clone().or_else(|| {
                    Some(ast::TableAlias {
                        name: name.0.last().unwrap().as_ident().unwrap().clone(),
                        columns: vec![],
                        explicit: true,
                    })
                });
                *factor = self.relation.clone();
                if let ast::TableFactor::Table { alias: dest, .. } = factor {
                    *dest = alias;
                }
                self.count += 1;
            }
            std::ops::ControlFlow::Continue(())
        }
    }
    let mut replace = Replace {
        table: metadata.table.clone(),
        relation,
        count: 0,
    };
    use ast::VisitMut;
    if let std::ops::ControlFlow::Break(e) = target.visit(&mut replace) {
        return Err(e);
    }
    if replace.count != 1 {
        return Err(SqlError::Unsupported(
            "Lance search requires exactly one target scan".into(),
        ));
    }
    if index.metric == SearchMetric::Bm25 {
        let ast::Statement::Query(q) = &mut target[0] else {
            unreachable!()
        };
        let ast::SetExpr::Select(select) = q.body.as_mut() else {
            return Err(SqlError::Unsupported(
                "Lance target requires a projection".into(),
            ));
        };
        select
            .projection
            .push(ast::SelectItem::UnnamedExpr(ast::Expr::Identifier(
                ast::Ident::with_quote('"', "_score"),
            )));
    }
    let source_cols = search
        .source
        .schema()
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| format!("${} AS {}", i + 1, d.quote_ident(f.name())))
        .collect::<Vec<_>>();
    let target_cols = search
        .target
        .schema()
        .fields()
        .iter()
        .map(|f| format!("__search_target.{}", d.quote_ident(f.name())))
        .collect::<Vec<_>>();
    let score = if index.metric == SearchMetric::Bm25 {
        "__search_target._score".into()
    } else {
        expression(search, search.score.clone(), d)?
    };
    let predicate = search
        .predicate
        .clone()
        .map(|p| expression(search, p, d))
        .transpose()?
        .unwrap_or_else(|| "TRUE".into());

    let sql = format!(
        "SELECT {}, {}, {score} AS {} FROM ({}) AS __search_target WHERE {predicate}",
        source_cols.join(", "),
        target_cols.join(", "),
        d.quote_ident(SCORE_COLUMN),
        target[0]
    );
    let mut statements = Parser::parse_sql(&DuckDbDialect {}, &sql)
        .map_err(|e| SqlError::Unsupported(e.to_string()))?;
    let flow = ast::visit_expressions_mut(&mut statements, |e| {
        if let ast::Expr::CompoundIdentifier(ids) = e {
            if ids.len() == 2 && ids[0].value == "__search_source" {
                let Some(i) = search
                    .source
                    .schema()
                    .fields()
                    .iter()
                    .position(|f| f.name() == &ids[1].value)
                else {
                    return std::ops::ControlFlow::Break(SqlError::Unsupported(
                        "unbound search input".into(),
                    ));
                };
                *e = ast::Expr::Value(ast::Value::Placeholder(format!("${}", i + 1)).into());
            }
        }
        std::ops::ControlFlow::Continue(())
    });
    if let std::ops::ControlFlow::Break(e) = flow {
        return Err(e);
    }
    Ok(SearchTemplate {
        sql: statements[0].to_string(),
        parameters: source_cols.len(),
        dialect: "duckdb".into(),
    })
}

fn validate_lance_predicate(search: &RankedJoin) -> SqlResult<()> {
    // These predicates become DuckDB table filters after binding the source
    // row. Anything else needs an explicit post-retrieval stage; accepting it
    // here could apply a filter after top-k and silently change the edge set.
    fn scalar(e: &Expr, s: &RankedJoin) -> bool {
        e.column_refs()
            .iter()
            .all(|c| s.source.schema().has_column(c))
    }
    fn column(e: &Expr, s: &RankedJoin) -> bool {
        matches!(e, Expr::Column(c) if s.target.schema().has_column(c))
    }
    fn valid(e: &Expr, s: &RankedJoin) -> bool {
        use datafusion::logical_expr::Operator;
        match e {
            Expr::BinaryExpr(b) if b.op == Operator::And => valid(&b.left, s) && valid(&b.right, s),
            Expr::BinaryExpr(b)
                if matches!(
                    b.op,
                    Operator::Eq
                        | Operator::NotEq
                        | Operator::Lt
                        | Operator::LtEq
                        | Operator::Gt
                        | Operator::GtEq
                ) =>
            {
                (column(&b.left, s) && scalar(&b.right, s))
                    || (scalar(&b.left, s) && column(&b.right, s))
                    || scalar(e, s)
            }
            Expr::IsNull(e) | Expr::IsNotNull(e) => column(e, s) || scalar(e, s),
            _ => scalar(e, s),
        }
    }
    if search.predicate.as_ref().is_some_and(|e| !valid(e, search)) {
        return Err(SqlError::Unsupported("Lance candidate predicates require pushable column comparisons, null checks, and AND; put other conditions in a final relationship stage".into()));
    }
    Ok(())
}

/// Cross-engine pgvector access binds source rows on the target owner. A
/// same-engine search remains one correlated SQL island instead.
pub(crate) fn postgres_template(search: &RankedJoin) -> SqlResult<SearchTemplate> {
    stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024, || {
        super::logical_functions::with_plan(
            &search.clone().into_plan(),
            SqlDialect::Postgres,
            || {
                let d = SqlDialect::Postgres;
                // Preserve the physical target projection here. Normalizing nested
                // payloads inside it introduces scalar subqueries, preventing
                // PostgreSQL from flattening the projection into an index scan.
                // The exchange decoder already reads nested arrays/JSON losslessly.
                let target = super::recursive::unparse_plan(search.target.as_ref().clone(), d)?;
                let score = expression(search, search.score.clone(), d)?;
                let query = expression(search, search.query().clone(), d)?;
                let document = expression(search, search.document().clone(), d)?;
                let operator = match search.metric() {
                    Some(SearchMetric::Cosine) => "<=>",
                    Some(SearchMetric::Dot) => "<#>",
                    Some(SearchMetric::L2) => "<->",
                    _ => {
                        return Err(SqlError::Unsupported(
                            "pgvector does not implement this metric".into(),
                        ));
                    }
                };
                let distance =
                    format!("CAST({document} AS vector) {operator} CAST({query} AS vector)");
                let distance = if search.exact {
                    format!("({distance}) + 0.0")
                } else {
                    distance
                };
                let ties = if search.exact {
                    search
                        .target_keys
                        .iter()
                        .map(|e| {
                            expression(search, e.clone(), d)
                                .map(|e| format!(", {e} ASC NULLS LAST"))
                        })
                        .collect::<SqlResult<Vec<_>>>()?
                        .join("")
                } else {
                    String::new()
                };
                let predicate = search
                    .predicate
                    .clone()
                    .map(|p| expression(search, p, d))
                    .transpose()?
                    .unwrap_or_else(|| "TRUE".into());
                let source_cols = search
                    .source
                    .schema()
                    .fields()
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("${} AS {}", i + 1, d.quote_ident(f.name())))
                    .collect::<Vec<_>>();
                let target_cols = search
                    .target
                    .schema()
                    .fields()
                    .iter()
                    .map(|f| format!("__search_target.{}", d.quote_ident(f.name())))
                    .collect::<Vec<_>>();
                let sql = format!(
                    "SELECT {}, {}, {score} AS {} FROM ({target}) AS __search_target WHERE {predicate} ORDER BY {distance} ASC NULLS LAST{ties} LIMIT {}",
                    source_cols.join(", "),
                    target_cols.join(", "),
                    d.quote_ident(SCORE_COLUMN),
                    search.limit
                );
                let mut statements = Parser::parse_sql(&PostgreSqlDialect {}, &sql)
                    .map_err(|e| SqlError::Unsupported(e.to_string()))?;
                let flow = ast::visit_expressions_mut(&mut statements, |e| {
                    if let ast::Expr::CompoundIdentifier(ids) = e {
                        if ids.len() == 2 && ids[0].value == "__search_source" {
                            let Some(i) = search
                                .source
                                .schema()
                                .fields()
                                .iter()
                                .position(|f| f.name() == &ids[1].value)
                            else {
                                return std::ops::ControlFlow::Break(SqlError::Unsupported(
                                    "unbound search source".into(),
                                ));
                            };
                            *e = ast::Expr::Value(
                                ast::Value::Placeholder(format!("${}", i + 1)).into(),
                            );
                        }
                    }
                    std::ops::ControlFlow::Continue(())
                });
                if let std::ops::ControlFlow::Break(e) = flow {
                    return Err(e);
                }
                Ok(SearchTemplate {
                    sql: statements[0].to_string(),
                    parameters: source_cols.len(),
                    dialect: "postgres".into(),
                })
            },
        )
    })
}
