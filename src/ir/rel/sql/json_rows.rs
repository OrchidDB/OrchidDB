//! Engine-owned relational JSON transformations. Native list expressions remain
//! executable in DataFusion, while SQL uses actual backend row functions.
use super::lowering::{LoweringContext, LoweringMode, RelationLowering, SqlTemplate};
use super::*;
use crate::ir::functions::json::path::{self, Step};
use crate::ir::rel::dependent::{ArgumentBinding, TableFunction};
use datafusion::sql::{
    sqlparser::{ast, parser::Parser},
    unparser::Unparser,
};

pub(super) fn lower(
    function: &TableFunction,
    ctx: &LoweringContext,
) -> SqlResult<Option<RelationLowering>> {
    let name = function.name.join(".");
    let Some(op) = crate::ir::functions::json::operation(&name) else {
        return Ok(None);
    };
    if !matches!(op, "elements" | "entries" | "tree") {
        return Ok(None);
    }
    if !matches!(ctx.dialect, SqlDialect::DuckDb | SqlDialect::Postgres) {
        return Ok(None);
    }
    if function.arguments.is_empty()
        || function.arguments.len() > 2
        || function.output_schema.fields().len() != 6
    {
        return Err(SqlError::Unsupported(
            "JSON row functions require a document, optional path, and six row fields".into(),
        ));
    }
    let steps = if let Some(expr) = function.arguments.get(1) {
        let Expr::Literal(ScalarValue::Utf8(Some(text)), _) = expr else {
            return native(
                function,
                "dynamic JSON row paths execute in a native island",
            );
        };
        let steps = path::parse(text).map_err(|e| SqlError::Unsupported(e.to_string()))?;
        if !path::singular(&steps) {
            return native(
                function,
                "JSON row wildcard/filter/slice paths execute in a native island",
            );
        }
        steps
    } else {
        vec![]
    };
    super::logical_functions::with_plan(&function.clone().into_plan(), ctx.dialect, || {
        render(function, op, &steps, ctx)
    })
    .map(Some)
}
fn native(function: &TableFunction, reason: &str) -> SqlResult<Option<RelationLowering>> {
    if function.native_list.is_some() {
        return function
            .native_plan()
            .map(RelationLowering::Rewrite)
            .map(Some)
            .map_err(SqlError::from);
    }
    Err(SqlError::Unsupported(reason.into()))
}
fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}
fn expr_sql(expr: Expr, d: SqlDialect) -> SqlResult<String> {
    let expr = expr
        .transform_up(|expr| {
            if let Expr::Column(mut column) = expr {
                column.relation = Some(datafusion::common::TableReference::bare("__json_source"));
                return Ok(Transformed::yes(Expr::Column(column)));
            }
            Ok(Transformed::no(expr))
        })?
        .data;
    let expr = super::unparse::encode_expression_literals(expr, d)?.data;
    let dialect = d.unparser_dialect();
    let mut ast = Unparser::new(dialect.as_ref()).expr_to_sql(&expr)?;
    super::functions::prepare_scoped_ast(&mut ast, d, false)?;
    Ok(ast.to_string())
}
fn children(document: &str, d: SqlDialect) -> String {
    match d {
        SqlDialect::DuckDb => format!(
            "SELECT value,idx,key,CAST(row_number() OVER (ORDER BY idx,key) AS BIGINT) AS position FROM (SELECT value, CASE WHEN json_type({document}) = 'ARRAY' THEN CAST(key AS BIGINT) ELSE NULL END AS idx, CASE WHEN json_type({document}) = 'OBJECT' THEN key ELSE NULL END AS key FROM (SELECT key,value,id FROM json_each(CASE WHEN json_type({document}) IN ('ARRAY','OBJECT') THEN {document} ELSE CAST('{{}}' AS JSON) END) QUALIFY row_number() OVER (PARTITION BY key ORDER BY id DESC) = 1) AS __json_unique) AS __json_fields"
        ),
        SqlDialect::Postgres => {
            format!(
                "SELECT value, CAST(ordinality-1 AS BIGINT) AS idx, CAST(NULL AS TEXT) AS key, CAST(ordinality AS BIGINT) AS position FROM jsonb_array_elements(CASE WHEN jsonb_typeof({document}) = 'array' THEN {document} ELSE '[]'::jsonb END) WITH ORDINALITY"
            ) + &format!(
                " UNION ALL SELECT value, CAST(NULL AS BIGINT) AS idx, key, CAST(row_number() OVER (ORDER BY key COLLATE \"C\") AS BIGINT) AS position FROM jsonb_each(CASE WHEN jsonb_typeof({document}) = 'object' THEN {document} ELSE '{{}}'::jsonb END)"
            )
        }
        _ => unreachable!(),
    }
}
fn child_path(parent: &str, index: &str, key: &str, d: SqlDialect) -> String {
    let encoded = if d == SqlDialect::Postgres {
        format!("CAST(to_jsonb({key}) AS TEXT)")
    } else {
        format!("CAST(to_json({key}) AS VARCHAR)")
    };
    format!(
        "({parent} || '[' || CASE WHEN {index} IS NOT NULL THEN CAST({index} AS VARCHAR) ELSE {encoded} END || ']')"
    )
}
fn render(
    function: &TableFunction,
    op: &str,
    steps: &[Step],
    ctx: &LoweringContext,
) -> SqlResult<RelationLowering> {
    let d = ctx.dialect;
    let pg = d == SqlDialect::Postgres;
    let document = expr_sql(function.arguments[0].clone(), d)?;
    let mut selected = if pg {
        format!("CAST({document} AS JSONB)")
    } else {
        format!("CAST({document} AS JSON)")
    };
    let mut root_path = quote("$");
    let mut parent_path = "CAST(NULL AS VARCHAR)".to_owned();
    let mut key = "CAST(NULL AS VARCHAR)".to_owned();
    let mut index = "CAST(NULL AS BIGINT)".to_owned();
    for step in steps {
        parent_path = root_path.clone();
        let kind = if pg {
            format!("jsonb_typeof({selected})")
        } else {
            format!("lower(json_type({selected}))")
        };
        let length = if pg {
            format!(
                "CASE WHEN {kind}='array' THEN CAST(jsonb_array_length({selected}) AS BIGINT) ELSE 0 END"
            )
        } else {
            format!(
                "CASE WHEN {kind}='array' THEN CAST(json_array_length({selected}) AS BIGINT) ELSE 0 END"
            )
        };
        match step {
            Step::Field(field) => {
                key = format!("CAST({} AS VARCHAR)", quote(field));
                index = "CAST(NULL AS BIGINT)".into();
            }
            Step::Index(i) => {
                index = if *i < 0 {
                    format!("(({length}) + ({i}))")
                } else {
                    format!("CAST({i} AS BIGINT)")
                };
                key = "CAST(NULL AS VARCHAR)".into();
            }
            Step::Pointer(field) => {
                key = format!(
                    "CASE WHEN {kind}='object' THEN CAST({} AS VARCHAR) ELSE NULL END",
                    quote(field)
                );
                index = match field
                    .parse::<i64>()
                    .ok()
                    .filter(|i| *i >= 0 && i.to_string() == *field)
                {
                    Some(i) => {
                        format!("CASE WHEN {kind}='array' THEN CAST({i} AS BIGINT) ELSE NULL END")
                    }
                    None => "CAST(NULL AS BIGINT)".into(),
                };
            }
            _ => unreachable!(),
        }
        root_path = child_path(&parent_path, &index, &key, d);
        selected = super::json::access_steps(&selected, std::slice::from_ref(step), d)?;
    }
    let depth = steps.len();
    let rows = if op == "tree" {
        let base = &root_path;
        let parent = &parent_path;
        let empty = if pg {
            "CAST(ARRAY[] AS BIGINT[])"
        } else {
            "CAST([] AS BIGINT[])"
        };
        let append = if pg {
            "__walk.order_path || ARRAY[__child.position]"
        } else {
            "list_append(__walk.order_path,__child.position)"
        };
        let children = children("__walk.value", d);
        let path = child_path("__walk.path", "__child.idx", "__child.key", d);
        format!(
            "WITH RECURSIVE __walk(value,idx,key,path,parent_path,depth,order_path) AS (SELECT {selected}, {index}, {key}, CAST({base} AS VARCHAR), CAST({parent} AS VARCHAR), CAST({depth} AS BIGINT), {empty} WHERE {selected} IS NOT NULL UNION ALL SELECT * FROM (SELECT __child.value,__child.idx,__child.key,{path},__walk.path,__walk.depth+1,{append} FROM __walk CROSS JOIN LATERAL ({children}) AS __child) AS __walk_step) SELECT value,idx,key,path,parent_path,depth,CAST(row_number() OVER (ORDER BY order_path) AS BIGINT) AS position FROM __walk"
        )
    } else {
        let (kind, empty) = if op == "elements" {
            ("array", "[]")
        } else {
            ("object", "{}")
        };
        let checked = if pg {
            format!(
                "CASE WHEN {selected} IS NULL THEN '{empty}'::jsonb WHEN jsonb_typeof({selected}) = '{kind}' THEN {selected} ELSE CAST('JSON row function requires {kind}: ' || jsonb_typeof({selected}) AS JSONB) END"
            )
        } else {
            format!(
                "CASE WHEN {selected} IS NULL THEN CAST('{empty}' AS JSON) WHEN json_type({selected}) = '{}' THEN {selected} ELSE error('JSON row function requires {kind}') END",
                kind.to_ascii_uppercase()
            )
        };
        let children = children("__root.doc", d);
        let path = child_path(&root_path, "__child.idx", "__child.key", d);
        format!(
            "SELECT __child.value,__child.idx,__child.key,{path} AS path,{} AS parent_path,CAST({} AS BIGINT) AS depth,__child.position FROM (SELECT {checked} AS doc) AS __root CROSS JOIN LATERAL ({children}) AS __child",
            root_path,
            depth + 1
        )
    };
    let bound = function.source.is_some()
        && (ctx.mode == LoweringMode::Bound || function.binding == ArgumentBinding::PrepareTime);
    let mut projection = Vec::new();
    let from = if let Some(source) = &function.source {
        projection.extend(source.schema().fields().iter().map(|f| {
            format!(
                "__json_source.{} AS {}",
                d.quote_ident(f.name()),
                d.quote_ident(f.name())
            )
        }));
        let source = if bound {
            format!(
                "SELECT {}",
                source
                    .schema()
                    .fields()
                    .iter()
                    .enumerate()
                    .map(|(i, f)| format!("${} AS {}", i + 1, d.quote_ident(f.name())))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            super::unparse_plan(source.as_ref().clone(), d)?
        };
        format!(
            "({source}) AS __json_source {} JOIN LATERAL ({rows}) AS __json_rows{}",
            if function.outer { "LEFT" } else { "CROSS" },
            if function.outer { " ON TRUE" } else { "" }
        )
    } else if function.outer {
        format!("(SELECT 1) AS __json_source LEFT JOIN LATERAL ({rows}) AS __json_rows ON TRUE")
    } else {
        format!("({rows}) AS __json_rows")
    };
    projection.extend(
        ["value", "idx", "key", "path", "parent_path", "depth"]
            .iter()
            .zip(function.output_schema.fields())
            .map(|(physical, field)| {
                format!(
                    "__json_rows.{} AS {}",
                    d.quote_ident(physical),
                    d.quote_ident(field.name())
                )
            }),
    );
    if let Some(name) = &function.ordinality {
        projection.push(format!("__json_rows.position AS {}", d.quote_ident(name)));
    }
    let sql = format!("SELECT {} FROM {from}", projection.join(", "));
    let parser = d.parser_dialect();
    let mut statements = Parser::parse_sql(parser.as_ref(), &sql)
        .map_err(|e| SqlError::Unsupported(format!("JSON rows: {e}")))?;
    let mut statement = statements.remove(0);
    if bound {
        let source = function.source.as_ref().unwrap();
        let _ = ast::visit_expressions_mut(&mut statement, |expr| {
            if let ast::Expr::CompoundIdentifier(parts) = expr {
                if parts.len() == 2 && parts[0].value == "__json_source" {
                    if let Some(i) = source
                        .schema()
                        .fields()
                        .iter()
                        .position(|f| f.name() == &parts[1].value)
                    {
                        *expr =
                            ast::Expr::Value(ast::Value::Placeholder(format!("${}", i + 1)).into());
                    }
                }
            }
            std::ops::ControlFlow::<()>::Continue(())
        });
        Ok(RelationLowering::Dependent {
            source: source.clone(),
            template: SqlTemplate {
                sql: statement.to_string(),
                parameters: source.schema().fields().len(),
                dialect: d.name().into(),
            },
            schema: function.schema.clone(),
        })
    } else {
        Ok(RelationLowering::Sql(statement))
    }
}
