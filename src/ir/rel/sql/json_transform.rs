//! Typed JSON conversion is a recursive SQL transformation, not a name mapping.
use super::{SqlDialect, SqlError, SqlResult};
use arrow::datatypes::DataType;
use datafusion::sql::sqlparser::ast;
use serde_json::Value;
fn error(message: impl Into<String>) -> SqlError {
    SqlError::Unsupported(message.into())
}
fn string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
use super::literals::literal_text as literal;
fn failure(dialect: SqlDialect, doc: &str, ty: &str) -> String {
    if dialect == SqlDialect::DuckDb {
        format!("CAST(error('invalid JSON transform value') AS {ty})")
    } else {
        format!(
            "CAST(CAST(CAST('invalid JSON transform value: ' || CAST({doc} AS TEXT) AS INTEGER) AS TEXT) AS {ty})"
        )
    }
}
fn kind(dialect: SqlDialect, doc: &str) -> String {
    if dialect == SqlDialect::DuckDb {
        format!("json_type({doc})")
    } else {
        format!("jsonb_typeof({doc})")
    }
}
fn type_match(dialect: SqlDialect, doc: &str, duck: &str, pg: &str) -> String {
    format!(
        "{} = {}",
        kind(dialect, doc),
        string(if dialect == SqlDialect::DuckDb {
            duck
        } else {
            pg
        })
    )
}
fn extract(dialect: SqlDialect, doc: &str, key: &str) -> String {
    if dialect == SqlDialect::DuckDb {
        format!(
            "json_extract({doc}, {})",
            string(&format!("$.{}", serde_json::to_string(key).unwrap()))
        )
    } else {
        format!("({doc} -> {})", string(key))
    }
}
fn render(doc: &str, schema: &Value, dialect: SqlDialect, next: &mut usize) -> SqlResult<String> {
    let ty = crate::ir::functions::json::schema_type(schema).map_err(|e| error(e.to_string()))?;
    let sql_type = dialect.sql_type(&ty)?;
    if crate::ir::functions::domain::is_json(&ty) {
        return Ok(format!("CAST({doc} AS {sql_type})"));
    }
    let null_kind = type_match(dialect, doc, "NULL", "null");
    let invalid = failure(dialect, doc, &sql_type);
    let (condition, value) = match schema {
        Value::Object(fields) => {
            if dialect == SqlDialect::Postgres {
                return Err(error(
                    "PostgreSQL JSON transform with struct output requires native execution; use typed json.value projections for SQL columns",
                ));
            }
            if fields.is_empty() {
                return Err(error(
                    "DuckDB JSON transform cannot represent an empty anonymous struct",
                ));
            }
            let fields = fields
                .iter()
                .map(|(key, child)| {
                    Ok(format!(
                        "{} := {}",
                        dialect.quote_ident(key),
                        render(&extract(dialect, doc, key), child, dialect, next)?
                    ))
                })
                .collect::<SqlResult<Vec<_>>>()?
                .join(", ");
            (
                type_match(dialect, doc, "OBJECT", "object"),
                format!("struct_pack({fields})"),
            )
        }
        Value::Array(schema) => {
            let child = &schema[0];
            let id = *next;
            *next += 1;
            let value = if dialect == SqlDialect::DuckDb {
                let variable = format!("__orchiddb_json_index_{id}");
                let selected =
                    format!("json_extract({doc}, '$[' || CAST({variable} AS VARCHAR) || ']')");
                format!(
                    "list_transform(range(CAST(json_array_length({doc}) AS BIGINT)), {variable} -> {})",
                    render(&selected, child, dialect, next)?
                )
            } else {
                let alias = format!("__orchiddb_json_element_{id}");
                let selected = format!("{alias}.value");
                let value = render(&selected, child, dialect, next)?;
                let child_type = crate::ir::functions::json::schema_type(child)
                    .map_err(|e| error(e.to_string()))?;
                let value = if matches!(
                    child_type,
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(..)
                ) {
                    // Nested lists use a JSONB[] exchange representation. JSON
                    // leaves must be serialized text inside that envelope so a
                    // JSON null remains distinct from an SQL-null list cell.
                    let field = match &child_type {
                        DataType::List(f)
                        | DataType::LargeList(f)
                        | DataType::FixedSizeList(f, _) => f,
                        _ => unreachable!(),
                    };
                    if crate::ir::functions::domain::is_json(field.data_type()) {
                        let item = format!("__orchiddb_json_transport_{id}");
                        format!(
                            "CASE WHEN ({value}) IS NULL THEN NULL ELSE to_jsonb(ARRAY(SELECT CAST({item}.value AS TEXT) FROM unnest({value}) WITH ORDINALITY AS {item}(value,position) ORDER BY {item}.position)) END"
                        )
                    } else {
                        format!("to_jsonb({value})")
                    }
                } else {
                    value
                };
                format!(
                    "ARRAY(SELECT {value} FROM jsonb_array_elements({doc}) WITH ORDINALITY AS {alias}(value,position) ORDER BY {alias}.position)"
                )
            };
            (type_match(dialect, doc, "ARRAY", "array"), value)
        }
        Value::String(_) => {
            let text = if dialect == SqlDialect::DuckDb {
                format!("json_extract_string({doc}, '$')")
            } else {
                format!("({doc} #>> '{{}}')")
            };
            let numeric = if dialect == SqlDialect::DuckDb {
                format!("{} IN ('BIGINT','UBIGINT','DOUBLE')", kind(dialect, doc))
            } else {
                type_match(dialect, doc, "", "number")
            };
            let condition = match &ty {
                DataType::Utf8 => format!(
                    "{} NOT IN ({}, {}, {})",
                    kind(dialect, doc),
                    string(if dialect == SqlDialect::DuckDb {
                        "NULL"
                    } else {
                        "null"
                    }),
                    string(if dialect == SqlDialect::DuckDb {
                        "ARRAY"
                    } else {
                        "array"
                    }),
                    string(if dialect == SqlDialect::DuckDb {
                        "OBJECT"
                    } else {
                        "object"
                    })
                ),
                DataType::Boolean => type_match(dialect, doc, "BOOLEAN", "boolean"),
                ty if ty.is_integer() => {
                    let lexical = if dialect == SqlDialect::DuckDb {
                        format!("regexp_full_match({text}, '-?[0-9]+')")
                    } else {
                        format!("({text} ~ '^-?[0-9]+$')")
                    };
                    format!("({numeric} AND {lexical})")
                }
                ty if ty.is_numeric() => numeric,
                _ => {
                    return Err(error(format!(
                        "JSON transform does not support conversion to {ty}"
                    )));
                }
            };
            let cast = format!("CAST({text} AS {sql_type})");
            let value = if matches!(ty, DataType::Float32 | DataType::Float64) {
                if dialect == SqlDialect::DuckDb {
                    format!("CASE WHEN isfinite({cast}) THEN {cast} ELSE {invalid} END")
                } else {
                    format!(
                        "CASE WHEN {cast} IN (CAST('Infinity' AS {sql_type}), CAST('-Infinity' AS {sql_type})) THEN {invalid} ELSE {cast} END"
                    )
                }
            } else {
                cast
            };
            (condition, value)
        }
        _ => return Err(error("invalid JSON transform schema")),
    };
    Ok(format!(
        "CASE WHEN {doc} IS NULL OR {null_kind} THEN CAST(NULL AS {sql_type}) WHEN {condition} THEN {value} ELSE {invalid} END"
    ))
}
pub(crate) fn lower(args: &[ast::Expr], dialect: SqlDialect) -> SqlResult<ast::Expr> {
    if args.len() != 2 {
        return Err(error(
            "json.transform requires document and constant schema",
        ));
    }
    let schema = literal(&args[1])
        .ok_or_else(|| error("JSON transform schema must be constant JSON/text"))?;
    let schema: Value =
        serde_json::from_str(&schema).map_err(|e| error(format!("JSON transform schema: {e}")))?;
    let canonical = super::json::canonical_sql("__arg0", dialect)?;
    let value = render("__local0.document", &schema, dialect, &mut 0)?;
    let sql = format!("(SELECT {value} FROM (SELECT {canonical} AS document) AS __local0)");
    super::functions::portable_template(&sql, &args[..1], dialect)
}

#[cfg(test)]
#[path = "../../../../tests/unit/extracted/ir__rel__sql__json_transform__tests.rs"]
mod tests;
