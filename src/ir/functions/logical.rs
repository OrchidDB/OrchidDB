//! Portable scalar functions: one executable identity with per-dialect SQL.
//! SQL templates are parsed into ASTs by the SQL adapter; arguments are never
//! interpolated as strings. Definitions are attached to UDFs, not global state.
use arrow::datatypes::DataType;
use datafusion::{
    common::Result,
    logical_expr::{
        ColumnarValue, ReturnFieldArgs, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
    },
};
use serde::{Deserialize, Serialize};
use std::{any::Any, collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqlFunctionMapping {
    /// SQL expression with __arg0, __arg1, ... expression placeholders.
    pub value: String,
    /// Equivalent ordering key; reversal is explicit rather than inferred from SQL.
    #[serde(default)]
    pub ordering: Option<SqlOrderingMapping>,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SqlOrderingMapping {
    pub expression: String,
    pub reverse: bool,
}

/// Register on GraphMapping to use a custom portable function in relationship
/// expressions. The supplied native UDF owns type checking and batch execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LogicalFunction {
    name: String,
    aliases: Vec<String>,
    native: Arc<ScalarUDF>,
    pub sql: BTreeMap<String, SqlFunctionMapping>,
    /// Optional indexed-access contract. The function's first two arguments
    /// must be query and document, and its ordering must match this metric.
    /// Additional arguments may carry native corpus statistics.
    pub search_metric: Option<crate::ir::rel::search::SearchMetric>,
}
impl LogicalFunction {
    pub fn new(
        name: impl Into<String>,
        native: Arc<ScalarUDF>,
        sql: BTreeMap<String, SqlFunctionMapping>,
    ) -> Self {
        let name = name.into().to_ascii_lowercase();
        // Hex encoding preserves arbitrary logical names without dots or SQL quoting.
        let encoded = name
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        Self {
            name: format!("__orchiddb_logical_{encoded}"),
            aliases: vec![name],
            native,
            sql,
            search_metric: None,
        }
    }
    pub fn with_search_metric(mut self, metric: crate::ir::rel::search::SearchMetric) -> Self {
        self.search_metric = Some(metric);
        self
    }
    pub fn logical_name(&self) -> &str {
        &self.aliases[0]
    }
    pub fn into_udf(self) -> Arc<ScalarUDF> {
        Arc::new(ScalarUDF::new_from_impl(self))
    }
}
impl ScalarUDFImpl for LogicalFunction {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn aliases(&self) -> &[String] {
        &self.aliases
    }
    fn signature(&self) -> &Signature {
        self.native.signature()
    }
    fn return_type(&self, args: &[DataType]) -> Result<DataType> {
        self.native.return_type(args)
    }
    fn return_field_from_args(&self, args: ReturnFieldArgs) -> Result<arrow::datatypes::FieldRef> {
        self.native.return_field_from_args(args)
    }
    fn coerce_types(&self, args: &[DataType]) -> Result<Vec<DataType>> {
        self.native.coerce_types(args)
    }
    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        self.native.inner().invoke_with_args(args)
    }
}
pub fn definition(udf: &ScalarUDF) -> Option<&LogicalFunction> {
    udf.inner().as_any().downcast_ref()
}
