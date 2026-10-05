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
    /// A bare __args function argument expands to all positional arguments.
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
    /// Arity-specific mappings take precedence over a dialect-wide mapping.
    pub sql_overloads: BTreeMap<(String, usize), SqlFunctionMapping>,
    pub(crate) portable_builtin: bool,
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
            sql_overloads: BTreeMap::new(),
            portable_builtin: false,
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
    pub fn sql_mapping(&self, dialect: &str, arity: usize) -> Option<&SqlFunctionMapping> {
        self.sql_overloads
            .get(&(dialect.to_owned(), arity))
            .or_else(|| self.sql.get(dialect))
    }
    pub fn has_sql_mapping(&self, dialect: &str, arity: usize) -> bool {
        self.sql_mapping(dialect, arity).is_some()
            || (self.portable_builtin
                && matches!(dialect, "duckdb" | "postgres")
                && super::portable::structured::handles(self.logical_name()))
    }
    /// Resolve mappings that depend on Arrow field names or literal options.
    pub fn sql_mapping_for_call(
        &self,
        dialect: &str,
        args: &[datafusion::logical_expr::Expr],
        schema: &datafusion::common::DFSchema,
    ) -> Result<Option<SqlFunctionMapping>> {
        if let Some(mapping) = self.sql_mapping(dialect, args.len()) {
            return Ok(Some(mapping.clone()));
        }
        if self.has_sql_mapping(dialect, args.len()) {
            return Ok(Some(SqlFunctionMapping {
                value: super::portable::structured::mapping(
                    self.logical_name(),
                    args,
                    schema,
                    dialect == "postgres",
                )?,
                ordering: None,
            }));
        }
        Ok(None)
    }
    pub(crate) fn specialized(&self, dialect: &str, arity: usize, value: String) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        value.hash(&mut hash);
        let mut result = self.clone();
        result.name = format!("{}_typed_{:x}", self.name, hash.finish());
        result.sql_overloads.insert(
            (dialect.into(), arity),
            SqlFunctionMapping {
                value,
                ordering: None,
            },
        );
        result
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
    fn with_updated_config(
        &self,
        config: &datafusion::common::config::ConfigOptions,
    ) -> Option<ScalarUDF> {
        let native = self.native.inner().with_updated_config(config)?;
        let mut updated = self.clone();
        updated.native = Arc::new(native);
        Some(ScalarUDF::new_from_impl(updated))
    }
    fn coerce_types(&self, args: &[DataType]) -> Result<Vec<DataType>> {
        self.native.coerce_types(args)
    }
    fn invoke_with_args(&self, mut args: ScalarFunctionArgs) -> Result<ColumnarValue> {
        if self.portable_builtin && self.native.name() == "overlay" {
            return super::portable::native_overlay(args);
        }
        if self.portable_builtin
            && self.native.name() == "map"
            && args
                .args
                .iter()
                .any(|a| matches!(a, ColumnarValue::Array(_)))
        {
            // Upstream map does not broadcast scalar list arguments alongside
            // array arguments, unlike the rest of the native scalar catalog.
            args.args = ColumnarValue::values_to_arrays(&args.args)?
                .into_iter()
                .map(ColumnarValue::Array)
                .collect();
        }
        self.native.inner().invoke_with_args(args)
    }
    fn short_circuits(&self) -> bool {
        self.native.inner().short_circuits()
    }
    fn conditional_arguments<'a>(
        &self,
        args: &'a [datafusion::logical_expr::Expr],
    ) -> Option<(
        Vec<&'a datafusion::logical_expr::Expr>,
        Vec<&'a datafusion::logical_expr::Expr>,
    )> {
        self.native.inner().conditional_arguments(args)
    }
    fn simplify(
        &self,
        args: Vec<datafusion::logical_expr::Expr>,
        info: &datafusion::logical_expr::simplify::SimplifyContext,
    ) -> Result<datafusion::logical_expr::simplify::ExprSimplifyResult> {
        if self.portable_builtin
            && !matches!(
                self.native.name(),
                "coalesce"
                    | "nvl"
                    | "nvl2"
                    | "arrow_cast"
                    | "arrow_typeof"
                    | "arrow_metadata"
                    | "version"
                    | "current_date"
                    | "current_time"
                    | "now"
            )
        {
            // Keep the portable identity until placement. Native rewrites such
            // as regexp_like -> regex operator would bypass dialect guards.
            return Ok(datafusion::logical_expr::simplify::ExprSimplifyResult::Original(args));
        }
        self.native.inner().simplify(args, info)
    }
}
pub fn definition(udf: &ScalarUDF) -> Option<&LogicalFunction> {
    udf.inner().as_any().downcast_ref()
}
