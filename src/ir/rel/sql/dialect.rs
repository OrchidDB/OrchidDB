//! In-code engine integration. Adapters own syntax, codecs and relational rewrites.
use super::{SqlDialect, SqlError, SqlResult};
use arrow::datatypes::DataType;
use datafusion::common::ScalarValue;
use datafusion::logical_expr::LogicalPlan;
use datafusion::sql::{
    sqlparser::{ast, dialect::Dialect as ParserDialect},
    unparser::dialect::Dialect as UnparserDialect,
};
use std::{
    collections::BTreeMap,
    sync::{OnceLock, RwLock},
};

/// An application-supplied SQL engine implementation. Register a static adapter
/// once; runtime sessions and dataset/index options remain caller owned.
/// Complex rewrites are ordinary Rust code operating on typed IR and SQL ASTs.
/// Unknown functions and value encodings fail unless the adapter supports them.
pub trait DialectAdapter: std::any::Any + std::fmt::Debug + Send + Sync {
    fn name(&self) -> &'static str;
    fn parser_dialect(&self) -> Box<dyn ParserDialect>;
    fn unparser_dialect(&self) -> Box<dyn UnparserDialect>;
    fn sql_type(&self, data_type: &DataType) -> SqlResult<String>;
    /// Encode a typed value for transport into this engine, including a cast
    /// when needed to preserve null, empty collection and numeric types.
    fn exchange_literal(&self, value: &ScalarValue, data_type: &DataType) -> SqlResult<String> {
        Err(SqlError::Unsupported(format!(
            "{} has no literal codec for {data_type}: {}",
            self.name(),
            value.data_type()
        )))
    }
    fn quote_ident(&self, ident: &str) -> String {
        match self.unparser_dialect().identifier_quote_style(ident) {
            Some(quote) => ast::Ident::with_quote(quote, ident).to_string(),
            None => ast::Ident::new(ident).to_string(),
        }
    }
    fn requires_scope_repair(&self) -> bool { false }
    fn create_table_keyword(&self) -> &'static str {
        "CREATE TEMPORARY TABLE"
    }
    fn double_type(&self) -> &'static str {
        "DOUBLE PRECISION"
    }
    fn finalize_sql(&self, sql: String) -> SqlResult<String> { Ok(self.fixup_query(sql)) }
    fn fixup_query(&self, sql: String) -> String {
        sql
    }
    /// Called for ordinary expressions after OrchidDB's logical-function and
    /// internal operator placeholders have been resolved. Functions require an
    /// explicit implementation; the default accepts standard non-function ASTs.
    fn rewrite_expression(&self, expression: &mut ast::Expr) -> SqlResult<()> {
        if let ast::Expr::Function(function) = expression {
            return Err(SqlError::Unsupported(format!(
                "{} has no mapping for function {}",
                self.name(),
                function.name
            )));
        }
        Ok(())
    }
    /// Lower a complete call, retaining aggregate ordering, filters, windows,
    /// and DISTINCT. Scalar-only implementations can use lower_scalar_function.
    fn lower_function(&self, _function: &ast::Function) -> SqlResult<Option<ast::Expr>> {
        Ok(None)
    }
    /// Whether code lowering can handle this UDF identity. Argument-specific
    /// support is still checked before placing an island.
    fn supports_scalar_function(&self, _name: &str) -> bool {
        false
    }
    /// Lower a bound scalar function as an expression or scalar subquery.
    /// The identity is the native UDF name; no backend spelling is inferred.
    /// Returning None leaves ordinary adaptation in place.
    fn lower_scalar_function(
        &self,
        _name: &str,
        _arguments: &[ast::Expr],
    ) -> SqlResult<Option<ast::Expr>> {
        Ok(None)
    }
    fn rewrite_input_binding(&self, _binding: &mut ast::Cte) -> SqlResult<()> {
        Ok(())
    }
    fn rewrite_query(&self, _query: &mut ast::Query) -> SqlResult<()> {
        Ok(())
    }
    /// Describe ownership-bearing inputs of an adapter-defined relational
    /// node. Discovery can be requested by a different coordinator engine;
    /// return None for nodes this adapter does not recognize.
    fn relation_placement(
        &self,
        _plan: &LogicalPlan,
    ) -> Option<super::lowering::RelationPlacement> {
        None
    }
    /// Transform a typed relational operation into SQL AST, another logical
    /// plan, or an explicit dependent operation. Preserve the operation's
    /// output schema and semantics. None leaves ordinary relational nodes to
    /// the shared unparser; an unhandled extension remains unsupported.
    fn lower_relation(
        &self,
        _plan: &LogicalPlan,
        _context: &super::lowering::LoweringContext,
    ) -> SqlResult<Option<super::lowering::RelationLowering>> {
        Ok(None)
    }
    /// Optional expression/ordering templates supplied by the engine package.
    /// Full relational rewrites may consume scoring expressions without any
    /// scalar mapping; mappings are required only for emitted scalar calls.
    fn function_mapping(
        &self,
        _logical_name: &str,
    ) -> Option<crate::ir::functions::logical::SqlFunctionMapping> {
        None
    }
}

/// Engine adapters include both SQL dialect behavior and relational lowering.
pub use DialectAdapter as EngineAdapter;

fn registry() -> &'static RwLock<BTreeMap<&'static str, &'static dyn DialectAdapter>> {
    static REGISTRY: OnceLock<RwLock<BTreeMap<&'static str, &'static dyn DialectAdapter>>> =
        OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}

impl SqlDialect {
    pub(crate) fn requires_scope_repair(self) -> bool {
        match self { Self::Postgres => true, Self::Custom(adapter) => adapter.requires_scope_repair(), _ => false }
    }
    /// Snapshot registered adapters before invoking application callbacks. No
    /// registry lock is held while an adapter examines a relational operation.
    pub fn registered_adapters() -> SqlResult<Vec<&'static dyn DialectAdapter>> {
        Ok(registry()
            .read()
            .map_err(|_| SqlError::Setup("dialect registry poisoned".into()))?
            .values()
            .copied()
            .collect())
    }

    /// Construct an identifier using this engine's AST quoting rules.
    pub fn identifier(self, name: &str) -> ast::Ident {
        match self {
            Self::Custom(adapter) => {
                match adapter.unparser_dialect().identifier_quote_style(name) {
                    Some(quote) => ast::Ident::with_quote(quote, name),
                    None => ast::Ident::new(name),
                }
            }
            _ => ast::Ident::with_quote('"', name),
        }
    }

    pub fn supports_scalar_function(self, name: &str) -> bool {
        match self {
            Self::Custom(adapter) => adapter.supports_scalar_function(name),
            Self::DuckDb | Self::Postgres => name.starts_with("__orchiddb_json_"),
        }
    }
    pub fn lower_function(self, function: &ast::Function) -> SqlResult<Option<ast::Expr>> {
        match self {
            Self::Custom(adapter) => adapter.lower_function(function),
            _ => super::json::lower_function(function, self),
        }
    }
    pub fn lower_scalar_function(
        self,
        name: &str,
        arguments: &[ast::Expr],
    ) -> SqlResult<Option<ast::Expr>> {
        match self {
            Self::Custom(adapter) => adapter.lower_scalar_function(name, arguments),
            Self::DuckDb | Self::Postgres => super::json::lower(name, arguments, self),
        }
    }
    pub fn lower_relation(
        self,
        plan: &LogicalPlan,
        context: &super::lowering::LoweringContext,
    ) -> SqlResult<Option<super::lowering::RelationLowering>> {
        match self {
            Self::Custom(adapter) => adapter.lower_relation(plan, context),
            Self::DuckDb | Self::Postgres => super::lowering::builtin_lower_relation(plan, context),
        }
    }

    /// Register an implementation for configuration/protocol name resolution.
    /// Re-registering the same instance is idempotent; replacement is rejected.
    pub fn register(adapter: &'static dyn DialectAdapter) -> SqlResult<Self> {
        let name = adapter.name();
        if name.is_empty() || matches!(name, "postgres" | "duckdb" | "starrocks") {
            return Err(SqlError::Setup(format!(
                "reserved or empty engine dialect name {name:?}"
            )));
        }
        let mut registered = registry()
            .write()
            .map_err(|_| SqlError::Setup("dialect registry poisoned".into()))?;
        if let Some(existing) = registered.get(name) {
            if !(std::ptr::addr_eq(*existing, adapter)
                && std::any::Any::type_id(*existing) == std::any::Any::type_id(adapter))
            {
                return Err(SqlError::Setup(format!(
                    "engine dialect {name:?} already registered"
                )));
            }
        } else {
            registered.insert(name, adapter);
        }
        Ok(Self::Custom(adapter))
    }
    pub fn resolve(name: &str) -> SqlResult<Self> {
        match name {
            "duckdb" => Ok(Self::DuckDb),
            "postgres" => Ok(Self::Postgres),
            "starrocks" => Ok(Self::Custom(&super::starrocks::STARROCKS)),
            _ => registry()
                .read()
                .map_err(|_| SqlError::Setup("dialect registry poisoned".into()))?
                .get(name)
                .copied()
                .map(Self::Custom)
                .ok_or_else(|| {
                    SqlError::Unsupported(format!(
                        "unsupported SQL dialect {name:?}: engine is not registered"
                    ))
                }),
        }
    }
    pub fn parser_dialect(self) -> Box<dyn ParserDialect> {
        match self {
            Self::DuckDb => Box::new(datafusion::sql::sqlparser::dialect::DuckDbDialect {}),
            Self::Postgres => Box::new(datafusion::sql::sqlparser::dialect::PostgreSqlDialect {}),
            Self::Custom(adapter) => adapter.parser_dialect(),
        }
    }
    pub fn sql_type(self, data_type: &DataType) -> SqlResult<String> {
        match self {
            Self::DuckDb => Ok(crate::ir::functions::duckdb_type(data_type)?),
            Self::Postgres => Ok(crate::ir::functions::postgres_type(data_type)?),
            Self::Custom(adapter) => adapter.sql_type(data_type),
        }
    }
    pub fn rewrite_input_binding(self, binding: &mut ast::Cte) -> SqlResult<()> {
        match self {
            Self::Custom(adapter) => adapter.rewrite_input_binding(binding),
            Self::DuckDb | Self::Postgres => Ok(()),
        }
    }
    pub fn exchange_literal(self, value: ScalarValue, data_type: DataType) -> SqlResult<String> {
        super::exchange_literal(value, data_type, self)
    }
}
