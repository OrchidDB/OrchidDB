//! Optional DuckDB adapters. Engines consume `ConstraintCatalog`; they never
//! discover database constraints implicitly. Names are explicit to avoid search-path ambiguity.
use super::*;
use ::duckdb::{Connection, params};
#[derive(Debug, Clone)]
pub struct TableBinding {
    pub name: String,
    pub database: String,
    pub schema: String,
    pub table: String,
}
impl TableBinding {
    pub fn new(
        name: impl Into<String>,
        database: impl Into<String>,
        schema: impl Into<String>,
        table: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            database: database.into(),
            schema: schema.into(),
            table: table.into(),
        }
    }
    fn sql(&self) -> String {
        format!(
            "{}.{}.{}",
            quote(&self.database),
            quote(&self.schema),
            quote(&self.table)
        )
    }
}
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
/// Extract PK/UNIQUE/NOT NULL/FK metadata only for the explicit bindings.
/// Foreign keys to tables outside the supplied set are omitted.
pub fn extract(
    connection: &Connection,
    tables: &[TableBinding],
) -> Result<ConstraintCatalog, String> {
    let collation: String = connection
        .query_row("SELECT current_setting('default_collation')", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?;
    if !collation.is_empty() && !collation.eq_ignore_ascii_case("binary") {
        return Err("constraint extraction requires binary comparison semantics; supply explicit compatible facts for collated sources".into());
    }
    // Key enforcement and query equality must use the same domain. DuckDB
    // exposes collations in table definitions, not Arrow field metadata.
    for t in tables {
        let mut definition=connection.prepare("SELECT sql FROM duckdb_tables() WHERE database_name=? AND schema_name=? AND table_name=?").map_err(|e|e.to_string())?;
        let definitions = definition
            .query_map(params![t.database, t.schema, t.table], |r| {
                r.get::<_, String>(0)
            })
            .map_err(|e| e.to_string())?;
        for ddl in definitions {
            use datafusion::sql::sqlparser::{
                dialect::DuckDbDialect,
                tokenizer::{Token, Tokenizer},
            };
            let ddl = ddl.map_err(|e| e.to_string())?;
            let tokens = Tokenizer::new(&DuckDbDialect {}, &ddl)
                .tokenize()
                .map_err(|e| e.to_string())?;
            if tokens.iter().any(|t|matches!(t,Token::Word(w) if w.quote_style.is_none()&&w.value.eq_ignore_ascii_case("collate"))) {return Err(format!("collated source {} requires explicit equality-compatible constraints",t.name));}
        }
    }
    let mut result = ConstraintCatalog::default();
    let mut stmt=connection.prepare("SELECT constraint_name, constraint_type, to_json(constraint_column_names)::VARCHAR, referenced_table, to_json(referenced_column_names)::VARCHAR FROM duckdb_constraints() WHERE database_name=? AND schema_name=? AND table_name=? ORDER BY constraint_index").map_err(|e|e.to_string())?;
    for t in tables {
        let rows = stmt
            .query_map(params![t.database, t.schema, t.table], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for row in rows {
            let (name, kind, cols, target, refs) = row.map_err(|e| e.to_string())?;
            let columns: Vec<String> = serde_json::from_str(&cols).map_err(|e| e.to_string())?;
            let fact = match kind.as_str() {
                "PRIMARY KEY" => {
                    result.insert(
                        &t.name,
                        Constraint::enforced(
                            format!("{name}:non_null"),
                            Fact::NonNull {
                                columns: columns.clone(),
                            },
                        ),
                    );
                    Fact::Unique {
                        columns,
                        nulls_equal: false,
                    }
                }
                "UNIQUE" => Fact::Unique {
                    columns,
                    nulls_equal: false,
                },
                "NOT NULL" => Fact::NonNull { columns },
                "FOREIGN KEY" => {
                    let Some(target) = tables.iter().find(|b| {
                        b.database == t.database
                            && b.schema == t.schema
                            && Some(&b.table) == target.as_ref()
                    }) else {
                        continue;
                    };
                    Fact::ForeignKey {
                        columns,
                        target: target.name.clone(),
                        references: serde_json::from_str(
                            &refs.ok_or("missing FK reference columns")?,
                        )
                        .map_err(|e| e.to_string())?,
                    }
                }
                _ => continue,
            };
            result.insert(&t.name, Constraint::enforced(name, fact));
        }
    }
    use sha2::{Digest, Sha256};
    result.revision = format!(
        "duckdb:{:x}",
        Sha256::digest(serde_json::to_vec(&result).map_err(|e| e.to_string())?)
    );
    Ok(result)
}
/// Check every supplied fact in the caller's current transaction. No data leaves
/// DuckDB except one boolean per fact. The returned facts are snapshot-scoped;
/// callers must keep that snapshot immutable while planning and executing them.
pub fn validate(
    connection: &Connection,
    tables: &[TableBinding],
    catalog: &ConstraintCatalog,
    scope: &str,
) -> Result<ConstraintCatalog, String> {
    if scope.is_empty() {
        return Err("validation requires a nonempty snapshot scope".into());
    }
    let table = |name: &str| {
        tables
            .iter()
            .find(|t| t.name == name)
            .map(TableBinding::sql)
            .ok_or_else(|| format!("missing DuckDB binding {name}"))
    };
    let mut schemas = BTreeMap::new();
    for t in tables {
        let mut stmt = connection
            .prepare(&format!("SELECT * FROM {} WHERE false", t.sql()))
            .map_err(|e| e.to_string())?;
        let reader = stmt.query_arrow([]).map_err(|e| e.to_string())?;
        schemas.insert(t.name.clone(), reader.get_schema());
    }
    catalog.validate(&schemas)?;
    let mut validated = catalog.clone();
    for (name, facts) in &mut validated.tables {
        let source = table(name)?;
        for c in facts {
            let sql = match &c.fact {
                Fact::NonNull { columns } => format!(
                    "SELECT EXISTS(SELECT 1 FROM {source} WHERE {})",
                    columns
                        .iter()
                        .map(|c| format!("{} IS NULL", quote(c)))
                        .collect::<Vec<_>>()
                        .join(" OR ")
                ),
                Fact::Unique {
                    columns,
                    nulls_equal,
                } => {
                    let filter = if *nulls_equal {
                        String::new()
                    } else {
                        format!(
                            " WHERE {}",
                            columns
                                .iter()
                                .map(|c| format!("{} IS NOT NULL", quote(c)))
                                .collect::<Vec<_>>()
                                .join(" AND ")
                        )
                    };
                    format!(
                        "SELECT EXISTS(SELECT 1 FROM {source}{filter} GROUP BY {} HAVING count(*) > 1)",
                        columns
                            .iter()
                            .map(|c| quote(c))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                }
                Fact::ForeignKey {
                    columns,
                    target,
                    references,
                } => format!(
                    "SELECT EXISTS(SELECT 1 FROM {source} l WHERE {} AND NOT EXISTS(SELECT 1 FROM {} r WHERE {}))",
                    columns
                        .iter()
                        .map(|c| format!("l.{} IS NOT NULL", quote(c)))
                        .collect::<Vec<_>>()
                        .join(" AND "),
                    table(target)?,
                    columns
                        .iter()
                        .zip(references)
                        .map(|(a, b)| format!("l.{} = r.{}", quote(a), quote(b)))
                        .collect::<Vec<_>>()
                        .join(" AND ")
                ),
                Fact::FunctionalDependency {
                    determinant,
                    dependent,
                } => {
                    let eq = determinant
                        .iter()
                        .map(|c| format!("l.{0} IS NOT DISTINCT FROM r.{0}", quote(c)))
                        .collect::<Vec<_>>()
                        .join(" AND ");
                    let ne = dependent
                        .iter()
                        .map(|c| format!("l.{0} IS DISTINCT FROM r.{0}", quote(c)))
                        .collect::<Vec<_>>()
                        .join(" OR ");
                    format!(
                        "SELECT EXISTS(SELECT 1 FROM {source} l JOIN {source} r ON {eq} WHERE {ne})"
                    )
                }
            };
            let violated: bool = connection
                .query_row(&sql, [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            if violated {
                return Err(format!("constraint {name}.{} is violated", c.name));
            }
            c.evidence = Evidence::Validated {
                scope: scope.into(),
            };
        }
    }
    Ok(validated)
}
