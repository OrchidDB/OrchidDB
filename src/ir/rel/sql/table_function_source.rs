//! Declared table functions. Input columns bind explicit arguments; output
//! predicates remain filters on the returned rows.
use super::{SqlDialect, SqlError, SqlResult};
use arrow::datatypes::SchemaRef;
use datafusion::{
    catalog::TableProvider,
    common::{
        Result,
        tree_node::{Transformed, TreeNode},
    },
    datasource::source_as_provider,
    logical_expr::{Expr, LogicalPlan, Operator, TableProviderFilterPushDown, TableType},
    sql::{
        sqlparser::{ast, parser::Parser},
        unparser::Unparser,
    },
};
use serde::Deserialize;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Options {
    function: Vec<String>,
    #[serde(default)]
    arguments: Vec<Argument>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Argument {
    /// Virtual input column; never read from the function's returned rows.
    column: String,
    /// Omit for positional arguments.
    name: Option<String>,
    /// A JSON scalar default. Required arguments may instead be bound by equality.
    value: Option<serde_json::Value>,
    #[serde(default)]
    optional: bool,
}
#[derive(Debug)]
struct Provider {
    options: Options,
    schema: SchemaRef,
}
fn invalid(message: impl std::fmt::Display) -> SqlError {
    SqlError::Unsupported(format!("table function source: {message}"))
}
fn ident(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn scalar(value: &serde_json::Value) -> SqlResult<String> {
    Ok(match value {
        serde_json::Value::String(s) => format!("'{}'", s.replace('\'', "''")),
        serde_json::Value::Bool(v) => v.to_string(),
        serde_json::Value::Number(v) => v.to_string(),
        _ => return Err(invalid("argument defaults must be non-null JSON scalars")),
    })
}
pub(crate) fn provider(
    metadata: &crate::ir::rel::source_metadata::SourceMetadata,
    schema: SchemaRef,
) -> SqlResult<Option<Arc<dyn TableProvider>>> {
    if metadata.format.as_deref() != Some("table_function") {
        return Ok(None);
    }
    let options: Options =
        serde_json::from_value(serde_json::to_value(&metadata.options).map_err(invalid)?)
            .map_err(invalid)?;
    if options.function.is_empty() || options.function.iter().any(String::is_empty) {
        return Err(invalid("function requires nonempty identifier components"));
    }
    let mut columns = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut named = false;
    for arg in &options.arguments {
        if !columns.insert(&arg.column) || schema.field_with_name(&arg.column).is_err() {
            return Err(invalid(
                "argument columns must be distinct declared columns",
            ));
        }
        if let Some(name) = &arg.name {
            if name.is_empty() || !names.insert(name) {
                return Err(invalid("argument names must be nonempty and distinct"));
            }
            named = true;
        } else if named || arg.optional {
            return Err(invalid(
                "positional arguments must precede named arguments and cannot be optional",
            ));
        }
        if let Some(v) = &arg.value {
            scalar(v)?;
        }
    }
    if columns.len() == schema.fields().len() {
        return Err(invalid("declare at least one result column"));
    }
    Ok(Some(Arc::new(Provider { options, schema })))
}
impl TableProvider for Provider {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    fn table_type(&self) -> TableType {
        TableType::Base
    }
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> Result<Vec<TableProviderFilterPushDown>> {
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }
}
// Only a direct equality with a non-null typed literal supplies an input.
// OR, ranges, expressions, and correlations stay outside this boundary.
fn binding<'a>(expr: &'a Expr, column: &str) -> Option<&'a Expr> {
    let Expr::BinaryExpr(binary) = expr else {
        return None;
    };
    if binary.op != Operator::Eq {
        return None;
    }
    for (left, right) in [
        (&*binary.left, &*binary.right),
        (&*binary.right, &*binary.left),
    ] {
        if let (Expr::Column(c), Expr::Literal(value, _)) = (left, right) {
            if c.name == column && !value.is_null() {
                return Some(right);
            }
        }
    }
    None
}
fn bound(provider: &Arc<dyn TableProvider>) -> Option<&Provider> {
    if let Some(p) = provider.as_any().downcast_ref::<Provider>() {
        return Some(p);
    }
    if let Some(p) = provider
        .as_any()
        .downcast_ref::<crate::ir::rel::constraints::ConstrainedProvider>()
    {
        return bound(&p.inner);
    }
    if let Some(p) = crate::ir::rel::statistics::statistics_provider(provider) {
        return bound(&p.inner);
    }
    None
}
pub(super) fn push_filters(plan: LogicalPlan) -> SqlResult<(LogicalPlan, bool)> {
    use datafusion::common::tree_node::TreeNodeRecursion;
    let mut found = false;
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            if let Ok(provider) = source_as_provider(&scan.source) {
                found |= bound(&provider).is_some();
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    if !found {
        return Ok((plan, false));
    }
    use datafusion::optimizer::{Optimizer, OptimizerContext, push_down_filter::PushDownFilter};
    Ok((
        Optimizer::with_rules(vec![Arc::new(PushDownFilter::new())]).optimize(
            plan,
            &OptimizerContext::new().with_skip_failing_rules(false),
            |_, _| {},
        )?,
        true,
    ))
}
pub(super) fn lower(plan: &LogicalPlan, dialect: SqlDialect) -> SqlResult<Option<ast::Statement>> {
    let LogicalPlan::TableScan(scan) = plan else {
        return Ok(None);
    };
    let Ok(source) = source_as_provider(&scan.source) else {
        return Ok(None);
    };
    let Some(p) = bound(&source) else {
        return Ok(None);
    };
    if !matches!(dialect, SqlDialect::DuckDb | SqlDialect::Postgres) {
        return Err(invalid(
            "declared table functions currently require DuckDB or PostgreSQL",
        ));
    }
    let unparser_dialect = dialect.unparser_dialect();
    let unparser = Unparser::new(unparser_dialect.as_ref());
    let mut arguments = Vec::new();
    let mut inputs = std::collections::BTreeMap::new();
    for arg in &p.options.arguments {
        let field = p.schema.field_with_name(&arg.column).map_err(invalid)?;
        let value = if let Some(default) = &arg.value {
            Some(scalar(default)?)
        } else {
            scan.filters
                .iter()
                .find_map(|f| binding(f, &arg.column))
                .map(|e| unparser.expr_to_sql(e).map(|e| e.to_string()))
                .transpose()?
        };
        if value.is_none() && !arg.optional {
            return Err(invalid(format!(
                "required argument {} needs a default or an equality predicate",
                arg.column
            )));
        }
        let typed = format!(
            "CAST({} AS {})",
            value.as_deref().unwrap_or("NULL"),
            dialect.sql_type(field.data_type())?
        );
        if value.is_some() {
            arguments.push(match &arg.name {
                Some(name) => format!("{} := {typed}", ident(name)),
                None => typed.clone(),
            });
        }
        inputs.insert(arg.column.as_str(), typed);
    }
    let columns = p
        .schema
        .fields()
        .iter()
        .map(|f| match inputs.get(f.name().as_str()) {
            Some(value) => format!("{value} AS {}", ident(f.name())),
            None => format!("__function.{}", ident(f.name())),
        })
        .collect::<Vec<_>>();
    let mut sql = format!(
        "SELECT {} FROM {}({}) AS __function",
        columns.join(", "),
        p.options
            .function
            .iter()
            .map(|n| ident(n))
            .collect::<Vec<_>>()
            .join("."),
        arguments.join(", ")
    );
    let filters = scan
        .filters
        .iter()
        .map(|filter| {
            let filter = filter
                .clone()
                .transform_up(|e| match e {
                    Expr::Column(mut c) => {
                        c.relation = None;
                        Ok(Transformed::yes(Expr::Column(c)))
                    }
                    _ => Ok(Transformed::no(e)),
                })?
                .data;
            let mut expr = unparser.expr_to_sql(&filter)?;
            super::functions::prepare_scoped_ast(&mut expr, dialect, false)?;
            Ok(format!("({expr})"))
        })
        .collect::<SqlResult<Vec<_>>>()?;
    // Filter the full schema before projection: a pushed predicate can reference
    // columns absent from the requested output.
    let selected = scan
        .projected_schema
        .fields()
        .iter()
        .map(|f| ident(f.name()))
        .collect::<Vec<_>>();
    sql = format!(
        "SELECT {} FROM ({sql}) AS __table_function{}",
        if selected.is_empty() {
            "1".into()
        } else {
            selected.join(", ")
        },
        if filters.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", filters.join(" AND "))
        }
    );
    if let Some(limit) = scan.fetch {
        sql.push_str(&format!(" LIMIT {limit}"));
    }
    let mut statements =
        Parser::parse_sql(dialect.parser_dialect().as_ref(), &sql).map_err(invalid)?;
    Ok(Some(statements.remove(0)))
}
