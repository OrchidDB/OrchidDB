//! A statement-scoped view of the caller's DuckDB catalog. DuckDB, rather than
//! an Orchid signature inventory, binds each expression and selects overloads.
use super::{FunctionKind, FunctionOverload, OperatorTable};
use arrow::datatypes::DataType;
use datafusion::common::{DFSchema, DataFusionError, Result};
use datafusion::logical_expr::Expr;
use std::collections::BTreeMap;

pub struct HostCatalog {
    functions: BTreeMap<String, Vec<FunctionOverload>>,
    bind: Box<dyn Fn(&str) -> std::result::Result<(DataType, bool), String> + Send + Sync>,
}
impl HostCatalog {
    pub fn new(
        rows: &[serde_json::Value],
        bind: impl Fn(&str) -> std::result::Result<(DataType, bool), String> + Send + Sync + 'static,
    ) -> Self {
        let mut functions: BTreeMap<String, Vec<FunctionOverload>> = BTreeMap::new();
        for row in rows {
            let text = |i: usize| row[i].as_str().unwrap_or("").to_string();
            let name = text(0);
            let kind = match text(1).as_str() {
                "scalar" => FunctionKind::Scalar,
                "aggregate" => FunctionKind::Aggregate,
                "macro" => FunctionKind::Macro,
                "table" | "table_macro" => FunctionKind::Table,
                _ => FunctionKind::Other,
            };
            let overload = FunctionOverload {
                name: name.clone(),
                kind,
                parameter_types: vec![],
                varargs: None,
                return_type: None,
                stability: row[2].as_str().map(str::to_owned),
            };
            for alias in [
                name.clone(),
                format!("{}.{}", text(3), name),
                format!("{}.{}.{}", text(4), text(3), name),
            ] {
                functions
                    .entry(alias.to_ascii_lowercase())
                    .or_default()
                    .push(overload.clone());
            }
        }
        Self {
            functions,
            bind: Box::new(bind),
        }
    }
}
impl OperatorTable for HostCatalog {
    fn engine(&self) -> &str {
        "duckdb"
    }
    fn overloads(&self, name: &str) -> &[FunctionOverload] {
        self.functions
            .get(&name.to_ascii_lowercase())
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
    fn is_immutable(&self, name: &str, args: &[Expr], schema: &DFSchema) -> Result<bool> {
        let call = super::binding::call_sql(name, args, schema)?;
        (self.bind)(&format!("SELECT {call} AS value"))
            .map(|(_, immutable)| immutable)
            .map_err(DataFusionError::Plan)
    }
    fn bind(
        &self,
        name: &str,
        kind: FunctionKind,
        args: &[Expr],
        schema: &DFSchema,
    ) -> Result<DataType> {
        if !self
            .overloads(name)
            .iter()
            .any(|f| f.kind == kind || f.kind == FunctionKind::Macro)
        {
            return Err(DataFusionError::Plan(format!(
                "DuckDB function {name} is not a {kind:?} expression"
            )));
        }
        let call = super::binding::call_sql(name, args, schema)?;
        (self.bind)(&format!("SELECT {call} AS value"))
            .map(|(ty, _)| ty)
            .map_err(DataFusionError::Plan)
    }
}
