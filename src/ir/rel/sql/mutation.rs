//! Effects on existing mapped tables. Values are bound parameters; identifiers
//! come from mapping metadata. The caller owns the transaction boundary.
use super::SqlDialect;
#[cfg(feature = "duckdb")]
use super::DuckDbExecutor;

#[derive(Debug, Clone)]
pub enum RowCondition {
    Equal(String, Option<String>),
    NotNull(String),
}

#[derive(Debug, Clone)]
pub enum MappedMutation {
    Upsert {
        table: String,
        key: Vec<(String, Option<String>)>,
        values: Vec<(String, Option<String>)>,
    },
    Assign {
        table: String,
        values: Vec<(String, Option<String>)>,
        conditions: Vec<RowCondition>,
    },
    Clear {
        table: String,
        columns: Vec<String>,
        conditions: Vec<RowCondition>,
    },
    InsertAbsent {
        table: String,
        values: Vec<(String, Option<String>)>,
    },
    Delete {
        table: String,
        conditions: Vec<RowCondition>,
    },
}

pub trait MutationHost {
    fn count(&mut self, sql: &str, parameters: Vec<Option<String>>) -> Result<i64, String>;
    fn execute(&mut self, sql: &str, parameters: Vec<Option<String>>) -> Result<(), String>;
}
#[cfg(feature = "duckdb")]
impl MutationHost for DuckDbExecutor {
    fn count(&mut self, sql: &str, parameters: Vec<Option<String>>) -> Result<i64, String> {
        self.connection().map_err(|e| e.to_string())?
            .query_row(sql, duckdb::params_from_iter(parameters), |row| row.get(0)).map_err(|e| e.to_string())
    }
    fn execute(&mut self, sql: &str, parameters: Vec<Option<String>>) -> Result<(), String> {
        self.connection().map_err(|e| e.to_string())?
            .execute(sql, duckdb::params_from_iter(parameters)).map_err(|e| e.to_string())?;
        Ok(())
    }
}
impl MappedMutation {
    #[cfg(feature = "duckdb")]
    pub fn execute(&self, executor: &mut DuckDbExecutor) -> Result<(), String> { self.execute_in(executor) }
    pub fn execute_in(&self, host: &mut impl MutationHost) -> Result<(), String> {
        let quote = |name: &str| SqlDialect::DuckDb.quote_ident(name);
        let table = |name: &str| {
            datafusion::common::TableReference::from(name)
                .to_vec()
                .iter()
                .map(|part| quote(part))
                .collect::<Vec<_>>()
                .join(".")
        };
        let (sql, parameters) = match self {
            Self::Upsert {
                table: name,
                key,
                values,
            } => {
                if key.is_empty() {
                    return Err("Mapped writes require a complete row key".into());
                }
                let predicates = key
                    .iter()
                    .map(|(c, _)| format!("{} IS NOT DISTINCT FROM ?", quote(c)))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let params = key.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>();
                let count = host.count(&format!("SELECT count(*) FROM {} WHERE {predicates}", table(name)), params.clone())?;
                if count > 1 {
                    return Err(format!("Declared RDF key is not unique in {name}"));
                }
                if count == 0 {
                    let all = key.iter().chain(values).collect::<Vec<_>>();
                    (
                        format!(
                            "INSERT INTO {} ({}) VALUES ({})",
                            table(name),
                            all.iter()
                                .map(|(c, _)| quote(c))
                                .collect::<Vec<_>>()
                                .join(","),
                            vec!["?"; all.len()].join(",")
                        ),
                        all.iter().map(|(_, v)| v.clone()).collect(),
                    )
                } else {
                    if values.is_empty() {
                        return Ok(());
                    }
                    for (column, value) in values {
                        let sql = format!(
                            "SELECT count(*) FROM {} WHERE {predicates} AND {} IS NOT NULL AND {} IS DISTINCT FROM ?",
                            table(name),
                            quote(column),
                            quote(column)
                        );
                        let mismatch = host.count(&sql, params.iter().cloned().chain(std::iter::once(value.clone())).collect())?;
                        if mismatch > 0 {
                            return Err(format!(
                                "RDF insert would assign multiple values to {name}.{column}; delete the old value first"
                            ));
                        }
                    }
                    (
                        format!(
                            "UPDATE {} SET {} WHERE {predicates}",
                            table(name),
                            values
                                .iter()
                                .map(|(c, _)| format!("{} = ?", quote(c)))
                                .collect::<Vec<_>>()
                                .join(",")
                        ),
                        values
                            .iter()
                            .map(|(_, v)| v.clone())
                            .chain(params)
                            .collect(),
                    )
                }
            }
            Self::Assign {
                table: name,
                values,
                conditions,
            } => {
                let mut params = values.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>();
                let predicates = conditions
                    .iter()
                    .map(|c| match c {
                        RowCondition::Equal(c, v) => {
                            params.push(v.clone());
                            format!("{} IS NOT DISTINCT FROM ?", quote(c))
                        }
                        RowCondition::NotNull(c) => format!("{} IS NOT NULL", quote(c)),
                    })
                    .collect::<Vec<_>>()
                    .join(" AND ");
                (
                    format!(
                        "UPDATE {} SET {} WHERE {}",
                        table(name),
                        values
                            .iter()
                            .map(|(c, _)| format!("{} = ?", quote(c)))
                            .collect::<Vec<_>>()
                            .join(","),
                        if predicates.is_empty() {
                            "TRUE"
                        } else {
                            &predicates
                        }
                    ),
                    params,
                )
            }
            Self::Clear {
                table: name,
                columns,
                conditions,
            } => {
                if columns.is_empty() {
                    return Ok(());
                }
                let mut params = Vec::new();
                let predicates = conditions
                    .iter()
                    .map(|c| match c {
                        RowCondition::Equal(c, v) => {
                            params.push(v.clone());
                            format!("{} IS NOT DISTINCT FROM ?", quote(c))
                        }
                        RowCondition::NotNull(c) => format!("{} IS NOT NULL", quote(c)),
                    })
                    .collect::<Vec<_>>()
                    .join(" AND ");
                (
                    format!(
                        "UPDATE {} SET {} WHERE {}",
                        table(name),
                        columns
                            .iter()
                            .map(|c| format!("{} = NULL", quote(c)))
                            .collect::<Vec<_>>()
                            .join(","),
                        if predicates.is_empty() {
                            "TRUE"
                        } else {
                            &predicates
                        }
                    ),
                    params,
                )
            }
            Self::InsertAbsent {
                table: name,
                values,
            } => {
                if values.is_empty() {
                    return Err("Mapped insertion has no columns".into());
                }
                let columns = values
                    .iter()
                    .map(|(column, _)| quote(column))
                    .collect::<Vec<_>>();
                let params = values
                    .iter()
                    .map(|(_, value)| value.clone())
                    .collect::<Vec<_>>();
                let predicates = columns
                    .iter()
                    .map(|column| format!("{column} IS NOT DISTINCT FROM ?"))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                (
                    format!(
                        "INSERT INTO {} ({}) SELECT {} WHERE NOT EXISTS (SELECT 1 FROM {} WHERE {predicates})",
                        table(name),
                        columns.join(","),
                        vec!["?"; values.len()].join(","),
                        table(name)
                    ),
                    params
                        .iter()
                        .chain(params.iter())
                        .cloned()
                        .collect::<Vec<_>>(),
                )
            }
            Self::Delete {
                table: name,
                conditions,
            } => {
                let mut params = Vec::new();
                let predicates = conditions
                    .iter()
                    .map(|condition| match condition {
                        RowCondition::Equal(column, value) => {
                            params.push(value.clone());
                            format!("{} IS NOT DISTINCT FROM ?", quote(column))
                        }
                        RowCondition::NotNull(column) => format!("{} IS NOT NULL", quote(column)),
                    })
                    .collect::<Vec<_>>();
                (
                    format!(
                        "DELETE FROM {} WHERE {}",
                        table(name),
                        if predicates.is_empty() {
                            "TRUE".into()
                        } else {
                            predicates.join(" AND ")
                        }
                    ),
                    params,
                )
            }
        };
        host.execute(&sql, parameters)?;
        Ok(())
    }
}
