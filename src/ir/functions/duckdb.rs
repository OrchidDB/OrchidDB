//! DuckDB adapter: enumerate its actual catalog and ask its binder to choose
//! overloads. No duplicate, inevitably stale list of DuckDB signatures.
use super::{FunctionKind, FunctionOverload, OperatorTable};
use arrow::datatypes::DataType;
use datafusion::common::{DFSchema, DataFusionError, Result};
use datafusion::logical_expr::Expr;
use duckdb::Connection;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, OnceLock},
};

pub struct DuckDbCatalog {
    connection: Mutex<Connection>,
    functions: BTreeMap<String, Vec<FunctionOverload>>,
}

fn error(err: impl std::fmt::Display) -> DataFusionError {
    DataFusionError::Plan(format!("DuckDB function binding: {err}"))
}

impl DuckDbCatalog {
    pub fn new() -> Result<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(error)?)
    }

    /// Snapshot functions after loading extensions or registering macros on
    /// this connection. The same connection binds their calls.
    pub fn from_connection(connection: Connection) -> Result<Self> {
        let mut functions: BTreeMap<String, Vec<FunctionOverload>> = BTreeMap::new();
        {
            let mut statement = connection.prepare(
                "SELECT function_name, function_type, array_to_string(parameter_types, chr(9)), varargs, return_type, stability, schema_name FROM duckdb_functions() ORDER BY function_name, function_type, parameter_types::VARCHAR"
            ).map_err(error)?;
            let mut rows = statement.query([]).map_err(error)?;
            while let Some(row) = rows.next().map_err(error)? {
                let raw_name: String = row.get(0).map_err(error)?;
                let namespace: String = row.get(6).map_err(error)?;
                let name = if namespace == "main" {
                    raw_name
                } else {
                    format!("{namespace}.{raw_name}")
                };
                let kind: String = row.get(1).map_err(error)?;
                let parameters: Option<String> = row.get(2).map_err(error)?;
                let overload = FunctionOverload {
                    name: name.clone(),
                    kind: match kind.as_str() {
                        "scalar" => FunctionKind::Scalar,
                        "aggregate" => FunctionKind::Aggregate,
                        "macro" => FunctionKind::Macro,
                        "table" | "table_macro" => FunctionKind::Table,
                        _ => FunctionKind::Other,
                    },
                    parameter_types: parameters
                        .filter(|v| !v.is_empty())
                        .map(|v| v.split('\t').map(str::to_owned).collect())
                        .unwrap_or_default(),
                    varargs: row.get(3).map_err(error)?,
                    return_type: row.get(4).map_err(error)?,
                    stability: row.get(5).map_err(error)?,
                };
                functions
                    .entry(name.to_ascii_lowercase())
                    .or_default()
                    .push(overload);
            }
        }
        Ok(Self {
            connection: Mutex::new(connection),
            functions,
        })
    }

    pub fn functions(&self) -> impl Iterator<Item = &FunctionOverload> {
        self.functions.values().flatten()
    }
}

impl OperatorTable for DuckDbCatalog {
    fn engine(&self) -> &str {
        "duckdb"
    }
    fn overloads(&self, name: &str) -> &[FunctionOverload] {
        self.functions
            .get(&name.to_ascii_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    fn bind(
        &self,
        name: &str,
        kind: FunctionKind,
        args: &[Expr],
        schema: &DFSchema,
    ) -> Result<DataType> {
        let overloads = self.overloads(name);
        if overloads.is_empty() {
            return Err(error(format!("unknown function `{name}`")));
        }
        if !overloads.iter().any(|f| {
            f.kind == kind
                || matches!(kind, FunctionKind::Scalar | FunctionKind::Aggregate)
                    && f.kind == FunctionKind::Macro
        }) {
            return Err(error(format!(
                "function `{name}` cannot be used as a {kind:?} expression (table-valued functions are not expression calls)"
            )));
        }
        let call = super::binding::call_sql(name, args, schema)?;
        let sql = format!("DESCRIBE SELECT {call} AS value");
        let connection = self.connection.lock().map_err(error)?;
        let type_name: String = connection
            .query_row(&sql, [], |row| row.get(1))
            .map_err(error)?;
        if type_name == "NULL" {
            return Ok(DataType::Null);
        }
        // DuckDB's cast_to_type bind callback replaces itself with a cast of
        // its FIRST argument and discards the second expression entirely.
        // A scalar subquery keeps aggregate binding out of the outer query.
        // Thus only NULL is executed, even for side-effecting UDFs; unnamed
        // STRUCTs and extension types retain their actual Arrow schema without
        // trying to parse DESCRIBE's sometimes non-roundtrippable type text.
        let mut statement = match connection.prepare(&format!(
            "SELECT cast_to_type(NULL, (SELECT {call})) AS value WHERE false"
        )) {
            Ok(statement) => statement,
            // DESCRIBE renders DuckDB's SQLNULL pseudo-type as INTEGER. The
            // cast binder distinguishes it without evaluating error()/NULL.
            Err(err)
                if err
                    .to_string()
                    .contains("cast_to_type cannot be used to cast to NULL") =>
            {
                return Ok(DataType::Null);
            }
            Err(err) => return Err(error(err)),
        };
        let result = statement.query_arrow([]).map_err(error)?;
        Ok(result.get_schema().field(0).data_type().clone())
    }
}

pub(super) fn default_catalog() -> Result<Arc<DuckDbCatalog>> {
    static CATALOG: OnceLock<std::result::Result<Arc<DuckDbCatalog>, String>> = OnceLock::new();
    CATALOG
        .get_or_init(|| {
            DuckDbCatalog::new()
                .map(Arc::new)
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .cloned()
        .map_err(error)
}
