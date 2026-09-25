//! Effects on existing mapped tables. Values are bound parameters; identifiers
//! come from mapping metadata. The caller owns the transaction boundary.
use super::{DuckDbExecutor, SqlDialect};

#[derive(Debug, Clone)]
pub enum RowCondition {
    Equal(String, Option<String>),
    NotNull(String),
}

#[derive(Debug, Clone)]
pub enum MappedMutation {
    InsertAbsent {
        table: String,
        values: Vec<(String, Option<String>)>,
    },
    Delete {
        table: String,
        conditions: Vec<RowCondition>,
    },
}

impl MappedMutation {
    pub fn execute(&self, executor: &mut DuckDbExecutor) -> Result<(), String> {
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
        executor
            .connection()
            .map_err(|e| e.to_string())?
            .execute(&sql, duckdb::params_from_iter(parameters))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
