//! Source-first execution with DuckDB completion. Pushdown can eliminate every
//! completion operation. This protocol is internal to Orchid's client bindings.
//!
//! SQL audit [L31](../../tools/diagnostics/sql-lowering-audit.md#l31-source-ownership-and-sql-region-placement): Try source SQL first and use DuckDB for residual relational work.
//! Pushdown can eliminate completion entirely; there is no second completion mode.
use super::stream::{BatchSource, BatchStream, InputRelation};
use crate::{compiler::CompileRequest, federation::TransferColumn};
use arrow::{
    array::{ArrayRef, RecordBatch, RecordBatchOptions},
    datatypes::{Field, Schema, SchemaRef},
};
use datafusion::{
    common::{
        DFSchema, ScalarValue,
        tree_node::{TreeNode, TreeNodeRecursion},
    },
    logical_expr::{Expr, LogicalPlan, LogicalPlanBuilder},
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub version: u32,
    pub nodes: Vec<Node>,
    pub result: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub columns: Vec<TransferColumn>,
    pub schema_ipc: Vec<u8>,
    #[serde(flatten)]
    pub operation: Operation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Operation {
    SourceSql {
        source: String,
        dialect: String,
        sql: String,
    },
    SourceRequest {
        source: String,
        input: Option<usize>,
        template: crate::operations::RequestTemplate,
    },
    LocalProject {
        input: usize,
        expressions: Vec<LocalExpr>,
    },
    Input {
        one_row: bool,
    },
    Duckdb {
        sql: String,
        inputs: Vec<Relation>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub name: String,
    pub node: usize,
    /// Recursive SQL may rescan an input inside one statement. Preserve its
    /// snapshot in bounded Arrow IPC storage instead of reopening the source.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replay: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LocalExpr {
    /// Decode a typed graph descriptor into the standard native value carrier.
    /// This is representation conversion only; source SQL owns row selection.
    GraphElement {
        input: Box<LocalExpr>,
        data_type: String,
    },
    Column {
        index: usize,
    },
    Cast {
        input: Box<LocalExpr>,
        data_type: String,
    },
    /// One-element Arrow IPC stream preserves exact scalar types across clients.
    Literal {
        ipc: Vec<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        json: Option<serde_json::Value>,
        data_type: String,
    },
    Call {
        function: String,
        arguments: Vec<LocalExpr>,
        returns: String,
    },
}
const LOCAL: &str = "__engine_function___orchid_local_";

impl LocalExpr {
    fn column_demand(&self, demand: &mut BTreeSet<usize>) {
        match self {
            Self::Column { index } => { demand.insert(*index); }
            Self::GraphElement { input, .. } | Self::Cast { input, .. } => input.column_demand(demand),
            Self::Call { arguments, .. } => for arg in arguments { arg.column_demand(demand); },
            Self::Literal { .. } => {}
        }
    }
    fn remap_columns(&mut self, mapping: &BTreeMap<usize, usize>) {
        match self {
            Self::Column { index } => *index = mapping[index],
            Self::GraphElement { input, .. } | Self::Cast { input, .. } => input.remap_columns(mapping),
            Self::Call { arguments, .. } => for arg in arguments { arg.remap_columns(mapping); },
            Self::Literal { .. } => {}
        }
    }
}

pub(crate) fn validate_request(request: &CompileRequest) -> Result<(), String> {
    if !request.engines.is_empty() || request.execution_engine.is_some() {
        return Err("source execution uses sources, not engines/execution_engine; register Trino as a SQL source; DuckDB owns completion".into());
    }
    for (name, source) in &request.sources {
        if name.is_empty() {
            return Err("source name cannot be empty".into());
        }
        if super::SqlDialect::resolve(&source.adapter).is_err()
            && crate::operations::resolve(&source.adapter)?.is_none()
        {
            return Err(format!("unknown source adapter {}", source.adapter));
        }
    }
    let mut tables = BTreeSet::new();
    for table in &request.tables {
        let normalized = crate::federation::parts(&table.name)?
            .into_iter()
            .map(|i| i.value)
            .collect::<Vec<_>>();
        if !tables.insert(normalized) {
            return Err(format!("duplicate SQL table identity {}", table.name));
        }
        let source = table
            .engine
            .as_ref()
            .ok_or_else(|| format!("table {} requires an explicit source", table.name))?;
        if !request.sources.contains_key(source) {
            return Err(format!("unknown source {source}"));
        }
    }
    for metadata in &request.source_metadata {
        for options in
            std::iter::once(&metadata.options).chain(metadata.indexes.iter().map(|i| &i.options))
        {
            if let Some(owner) = options.get("source").or_else(|| options.get("engine")) {
                let owner = owner
                    .as_str()
                    .ok_or("source metadata owner must be a string")?;
                if !request.sources.contains_key(owner) {
                    return Err(format!("unknown source metadata owner {owner}"));
                }
            }
        }
    }
    Ok(())
}
fn columns(plan: &LogicalPlan) -> Result<Vec<TransferColumn>, String> {
    plan.schema()
        .fields()
        .iter()
        .map(|f| {
            Ok(TransferColumn {
                name: f.name().clone(),
                data_type: crate::federation::type_name(f.data_type())?,
                nullable: f.is_nullable(),
            })
        })
        .collect()
}
fn schema_ipc(schema: &Schema) -> Result<Vec<u8>, String> {
    let mut bytes = vec![];
    {
        let mut writer = arrow::ipc::writer::StreamWriter::try_new(&mut bytes, schema)
            .map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
    }
    Ok(bytes)
}
fn schema(columns: &[TransferColumn]) -> Result<SchemaRef, String> {
    Ok(Arc::new(Schema::new(
        columns
            .iter()
            .map(|c| {
                Ok(Field::new(
                    &c.name,
                    crate::compiler::data_type(&c.data_type)?,
                    c.nullable,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?,
    )))
}
fn expr_has_local(expr: &Expr) -> bool {
    let mut found = false;
    let _ = expr.apply(|e| {
        if let Expr::ScalarFunction(f) = e {
            found |= f.name().starts_with(LOCAL);
        }
        Ok(TreeNodeRecursion::Continue)
    });
    found
}
fn has_local(plan: &LogicalPlan) -> Result<bool, String> {
    let mut found = false;
    plan.apply_with_subqueries(|p| {
        for e in p.expressions() {
            e.apply(|e| {
                if let Expr::ScalarFunction(f) = e {
                    found |= f.name().starts_with(LOCAL);
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        Ok(TreeNodeRecursion::Continue)
    })
    .map_err(|e| e.to_string())?;
    Ok(found)
}
fn local_expr(expr: &Expr, input: &DFSchema) -> Result<LocalExpr, String> {
    match expr {
        Expr::Alias(a) => local_expr(&a.expr, input),
        Expr::Column(c) => Ok(LocalExpr::Column {
            index: input.index_of_column(c).map_err(|e| e.to_string())?,
        }),
        Expr::Literal(value, _) => {
            let batch = RecordBatch::try_from_iter([(
                "value",
                value.to_array().map_err(|e| e.to_string())?,
            )])
            .map_err(|e| e.to_string())?;
            let mut ipc = vec![];
            {
                let mut writer =
                    arrow::ipc::writer::StreamWriter::try_new(&mut ipc, &batch.schema())
                        .map_err(|e| e.to_string())?;
                writer.write(&batch).map_err(|e| e.to_string())?;
                writer.finish().map_err(|e| e.to_string())?;
            }
            Ok(LocalExpr::Literal {
                ipc,
                json: crate::operations::parameter_json(value).ok(),
                data_type: crate::federation::type_name(&value.data_type())?,
            })
        }
        Expr::ScalarFunction(f) if f.name() == "__engine_declared_argument_cast" => {
            use datafusion::logical_expr::ExprSchemable;
            Ok(LocalExpr::Cast {
                input: Box::new(local_expr(&f.args[0], input)?),
                data_type: crate::federation::type_name(
                    &expr.get_type(input).map_err(|e| e.to_string())?,
                )?,
            })
        }
        Expr::ScalarFunction(f) if f.name().starts_with(LOCAL) => {
            use datafusion::logical_expr::ExprSchemable;
            Ok(LocalExpr::Call {
                function: f.name()[LOCAL.len()..].into(),
                arguments: f
                    .args
                    .iter()
                    .map(|a| local_expr(a, input))
                    .collect::<Result<_, _>>()?,
                returns: crate::federation::type_name(
                    &expr.get_type(input).map_err(|e| e.to_string())?,
                )?,
            })
        }
        _ => Err(format!(
            "expression requires source or DuckDB execution: {expr}"
        )),
    }
}

pub(crate) fn build(
    request: &CompileRequest,
    plan: &LogicalPlan,
) -> Result<ExecutionPlan, String> {
    let mut builder = Builder {
        request,
        bound: BTreeMap::new(),
        plan: ExecutionPlan {
            version: 1,
            nodes: vec![],
            result: 0,
        },
    };
    builder.plan.result = builder.lower(plan)?;
    Ok(builder.plan)
}
struct Builder<'a> {
    request: &'a CompileRequest,
    bound: BTreeMap<String, usize>,
    plan: ExecutionPlan,
}
impl Builder<'_> {
    fn source_sql(&mut self, plan: &LogicalPlan, source: String, dialect: String, sql: String) -> Result<usize, String> {
        let id = self.push(plan, Operation::SourceSql { source, dialect: dialect.clone(), sql })?;
        if dialect != "trino" || !plan.schema().fields().iter().any(|f| crate::ir::rel::native_values::is_value(f.data_type())) {
            return Ok(id);
        }
        let expressions = plan.schema().fields().iter().enumerate().map(|(index, field)| {
            let column = LocalExpr::Column { index };
            Ok(if crate::ir::rel::native_values::is_value(field.data_type()) {
                LocalExpr::GraphElement { input: Box::new(column), data_type: crate::federation::type_name(field.data_type())? }
            } else { column })
        }).collect::<Result<Vec<_>, String>>()?;
        self.push(plan, Operation::LocalProject { input: id, expressions })
    }
    fn push(&mut self, plan: &LogicalPlan, operation: Operation) -> Result<usize, String> {
        let id = self.plan.nodes.len();
        self.plan.nodes.push(Node {
            columns: columns(plan)?,
            schema_ipc: schema_ipc(plan.schema().as_arrow())?,
            operation,
        });
        Ok(id)
    }
    fn lower(&mut self, plan: &LogicalPlan) -> Result<usize, String> {
        self.lower_with_demand(plan, true)
    }
    fn lower_with_demand(&mut self, plan: &LogicalPlan, prune_local_input: bool) -> Result<usize, String> {
        if let LogicalPlan::TableScan(scan) = plan {
            if let Some(id) = self.bound.get(&scan.table_name.to_string()) {
                return Ok(*id);
            }
        }
        let mut owners = BTreeSet::new();
        let mut recursive_tables = BTreeSet::new();
        plan.apply_with_subqueries(|p| {
            if let LogicalPlan::RecursiveQuery(query) = p { recursive_tables.insert(query.name.clone()); }
            Ok(TreeNodeRecursion::Continue)
        }).map_err(|e| e.to_string())?;
        plan.apply_with_subqueries(|p| {
            if let LogicalPlan::TableScan(scan) = p {
                // Recursive work tables are internal dependencies of their
                // owning query, not an independent completion-engine source.
                if datafusion::datasource::source_as_provider(&scan.source)?
                    .as_any().is::<datafusion::datasource::cte_worktable::CteWorkTable>()
                    && recursive_tables.contains(&scan.table_name.to_string()) {
                    return Ok(TreeNodeRecursion::Continue);
                }
                let table = self
                    .request
                    .tables
                    .iter()
                    .find(|t| t.name == scan.table_name.to_string());
                if let Some(owner) = table.and_then(|t| t.engine.as_ref()) {
                    owners.insert(owner.clone());
                } else {
                    owners.insert(String::new());
                }
            }
            Ok(TreeNodeRecursion::Continue)
        })
        .map_err(|e| e.to_string())?;
        let local = has_local(plan)?;
        let mut source_error = None;
        if !local && owners.len() == 1 && !owners.contains("") {
            let source = owners.first().unwrap();
            let adapter = &self.request.sources[source].adapter;
            if let Ok(dialect) = super::SqlDialect::resolve(adapter) {
                match crate::ir::rel::sql::unparse_plan_with_context(plan.clone(), dialect, &self.request.rule_context) {
                    Ok(sql) => {
                    return self.source_sql(plan, source.clone(), adapter.clone(), sql);
                    }
                    // A source capability miss leaves residual work for DuckDB.
                    // This is planning, never a retry after source execution.
                    Err(error) => source_error = Some(format!("source {source} ({adapter}): {error}")),
                }
            }
        }
        // Request adapters may describe dependent inputs containing local UDFs.
        // The adapter owns request lowering; those inputs are planned separately.
        {
            let owner = crate::operations::owner(plan)?
                .or_else(|| (owners.len() == 1).then(|| owners.first().unwrap().clone()));
            if let Some(source) = owner.filter(|s| self.request.sources.contains_key(s)) {
                if let Some(adapter) =
                    crate::operations::resolve(&self.request.sources[&source].adapter)?
                {
                    if let Some(prepared) = adapter.lower(plan)? {
                        prepared.template.validate()?;
                        let input = prepared
                            .source
                            .as_ref()
                            .map(|p| self.lower(p))
                            .transpose()?;
                        let operation = Operation::SourceRequest {
                            source,
                            input,
                            template: prepared.template,
                        };
                        if let Some(replacement) = prepared.replacement {
                            use datafusion::common::tree_node::Transformed;
                            let id = self.plan.nodes.len();
                            let cols = prepared
                                .schema
                                .fields()
                                .iter()
                                .map(|f| {
                                    Ok(TransferColumn {
                                        name: f.name().clone(),
                                        data_type: crate::federation::type_name(f.data_type())?,
                                        nullable: f.is_nullable(),
                                    })
                                })
                                .collect::<Result<Vec<_>, String>>()?;
                            self.plan.nodes.push(Node {
                                columns: cols,
                                schema_ipc: schema_ipc(prepared.schema.as_arrow())?,
                                operation,
                            });
                            let name = format!("__orchid_request_{id}");
                            self.bound.insert(name.clone(), id);
                            let mut markers = 0;
                            let replacement = replacement.transform_down_with_subqueries(|node| {
                                if let LogicalPlan::Extension(ext) = &node {
                                    if let Some(marker) = ext.node.as_any().downcast_ref::<crate::operations::RequestResult>() {
                                        if marker.schema != prepared.schema { return Err(datafusion::common::DataFusionError::Plan("request continuation schema mismatch".into())); }
                                        markers += 1;
                                        let scan = LogicalPlanBuilder::scan(&name, datafusion::datasource::provider_as_source(Arc::new(datafusion::datasource::empty::EmptyTable::new(Arc::new(prepared.schema.as_arrow().clone())))), None)?.build()?;
                                        let exprs = scan.schema().columns().into_iter().zip(prepared.schema.iter()).map(|(c,(q,f))| Expr::Column(c).alias_qualified(q.cloned(), f.name())).collect::<Vec<_>>();
                                        return Ok(Transformed::yes(LogicalPlanBuilder::from(scan).project(exprs)?.build()?));
                                    }
                                }
                                Ok(Transformed::no(node))
                            }).map_err(|e| e.to_string())?.data;
                            if markers != 1 {
                                return Err(
                                    "request continuation must contain exactly one result leaf"
                                        .into(),
                                );
                            }
                            return self.lower(&replacement);
                        }
                        return self.push(plan, operation);
                    }
                }
            }
        }
        match plan {
            LogicalPlan::EmptyRelation(p) => {
                return self.push(
                    plan,
                    Operation::Input {
                        one_row: p.produce_one_row,
                    },
                );
            }
            LogicalPlan::Projection(p) => {
                if let Ok(mut expressions) = p
                    .expr
                    .iter()
                    .map(|e| local_expr(e, p.input.schema()))
                    .collect::<Result<Vec<_>, _>>()
                {
                    // SQL audit G13: propagate actual argument/output demand
                    // through the Arrow boundary and remap positional indices.
                    let mut demand = BTreeSet::new();
                    for expr in &expressions { expr.column_demand(&mut demand); }
                    let input = if !prune_local_input || demand.len() == p.input.schema().fields().len() {
                        self.lower(&p.input)?
                    } else {
                        let mapping = demand.iter().enumerate().map(|(new, old)| (*old, new)).collect();
                        for expr in &mut expressions { expr.remap_columns(&mapping); }
                        let mut columns = demand.iter().map(|i| Expr::Column(p.input.schema().columns()[*i].clone())).collect::<Vec<_>>();
                        if columns.is_empty() {
                            // Preserve row multiplicity for an argument-free UDF.
                            columns.push(datafusion::logical_expr::lit(1_i64).alias("__orchid_input_row"));
                        }
                        let input = LogicalPlanBuilder::from(p.input.as_ref().clone()).project(columns)
                            .and_then(|b| b.build()).map_err(|e| e.to_string())?;
                        // This projection already expresses demand; do not wrap
                        // it in another identical projection when it spans UDFs.
                        self.lower_with_demand(&input, false)?
                    };
                    return self.push(plan, Operation::LocalProject { input, expressions });
                }
            }
            LogicalPlan::SubqueryAlias(p) => return self.lower(&p.input),
            _ => {}
        }
        // Materialize explicitly local calls before a residual relational
        // operator. Do not hoist through conditionals or short-circuit operators.
        if plan.inputs().len() == 1 && plan.expressions().iter().any(expr_has_local) {
            use datafusion::common::tree_node::Transformed;
            let input = plan.inputs()[0];
            let mut lifted = Vec::new();
            let mut safe = true;
            let expressions = plan
                .expressions()
                .into_iter()
                .map(|expr| {
                    expr.transform_down(|expr| {
                        match &expr {
                            Expr::Case(_) => {
                                safe = false;
                                return Ok(Transformed::no(expr));
                            }
                            Expr::BinaryExpr(b)
                                if matches!(
                                    b.op,
                                    datafusion::logical_expr::Operator::And
                                        | datafusion::logical_expr::Operator::Or
                                ) =>
                            {
                                safe = false;
                                return Ok(Transformed::no(expr));
                            }
                            Expr::ScalarFunction(f) if f.name().starts_with(LOCAL) => {
                                if local_expr(&expr, input.schema()).is_err() {
                                    safe = false;
                                    return Ok(Transformed::no(expr));
                                }
                                let mut name = format!("__orchid_local_{}", lifted.len());
                                while input.schema().fields().iter().any(|f| f.name() == &name) {
                                    name.push('_');
                                }
                                lifted.push(expr.alias(&name));
                                return Ok(Transformed::new(
                                    Expr::Column(datafusion::common::Column::from_name(name)),
                                    true,
                                    TreeNodeRecursion::Jump,
                                ));
                            }
                            Expr::ScalarFunction(_) if expr_has_local(&expr) => {
                                safe = false;
                            }
                            _ => {}
                        }
                        Ok(Transformed::no(expr))
                    })
                    .map(|result| result.data)
                    .map_err(|e| e.to_string())
                })
                .collect::<Result<Vec<_>, String>>()?;
            if safe && !lifted.is_empty() {
                let mut projection = input
                    .schema()
                    .columns()
                    .into_iter()
                    .map(Expr::Column)
                    .collect::<Vec<_>>();
                projection.extend(lifted);
                let input = LogicalPlanBuilder::from(input.clone())
                    .project(projection)
                    .and_then(|b| b.build())
                    .map_err(|e| e.to_string())?;
                let rebuilt = plan
                    .with_new_exprs(expressions, vec![input])
                    .map_err(|e| e.to_string())?;
                let visible = plan
                    .schema()
                    .columns()
                    .into_iter()
                    .map(Expr::Column)
                    .collect::<Vec<_>>();
                let rebuilt = LogicalPlanBuilder::from(rebuilt)
                    .project(visible)
                    .and_then(|b| b.build())
                    .map_err(|e| e.to_string())?;
                return self.lower(&rebuilt);
            }
        }
        // Source-free expressions can also finish on the sole SQL source.
        // Completion stays lazy even though DuckDB is always the residual engine.
        if !local && owners.is_empty() && self.request.sources.len() == 1 {
            let (source, binding) = self.request.sources.iter().next().unwrap();
            if let Ok(dialect) = super::SqlDialect::resolve(&binding.adapter) {
                match crate::ir::rel::sql::unparse_plan_with_context(plan.clone(), dialect, &self.request.rule_context) {
                    Ok(sql) => return self.source_sql(plan, source.clone(), binding.adapter.clone(), sql),
                    Err(error) => source_error = Some(format!("source {source} ({}): {error}", binding.adapter)),
                }
            }
        }
        self.complete_in_duckdb(plan).map_err(|error| match source_error {
            Some(source) => format!("{source}; DuckDB completion: {error}"),
            None => error,
        })
    }

    fn complete_in_duckdb(&mut self, plan: &LogicalPlan) -> Result<usize, String> {
        // Local calls must never leak into generated DuckDB SQL.
        if plan
            .expressions()
            .iter()
            .any(|e| e.to_string().contains(LOCAL))
        {
            return Err(
                "local UDF must be an explicit projection before relational completion".into(),
            );
        }
        let inputs = plan.inputs();
        if inputs.is_empty() {
            return Err(format!("unsupported source operation {}", plan.display()));
        }
        let mut relations = vec![];
        // A recursive query owns its work-table scope. Keep that region whole
        // in DuckDB and expose only real source scans as Arrow inputs; never
        // schedule a recursive work table as an independent source operation.
        let recursive = plan.exists(|node| Ok(matches!(node, LogicalPlan::RecursiveQuery(_))))
            .map_err(|e| e.to_string())?;
        let residual = if recursive {
            use datafusion::common::tree_node::Transformed;
            plan.clone().transform_down_with_subqueries(|node| {
                if let LogicalPlan::TableScan(scan) = &node {
                    if datafusion::datasource::source_as_provider(&scan.source)?
                        .as_any().is::<datafusion::datasource::cte_worktable::CteWorkTable>() {
                        return Ok(Transformed::no(node));
                    }
                    let replacement = self.completion_input(&node, &mut relations)
                        .map_err(datafusion::common::DataFusionError::Plan)?;
                    return Ok(Transformed::new(replacement, true, TreeNodeRecursion::Jump));
                }
                Ok(Transformed::no(node))
            }).map_err(|e| e.to_string())?.data
        } else {
            let replaced = inputs.into_iter().map(|input| self.completion_input(input, &mut relations))
                .collect::<Result<Vec<_>, _>>()?;
            // UNNEST reconstructs its dependencies from exec_columns.
            let expressions = if matches!(plan, LogicalPlan::Unnest(_)) { vec![] } else { plan.expressions() };
            plan.with_new_exprs(expressions, replaced).map_err(|e| e.to_string())?
        };
        if recursive { for relation in &mut relations { relation.replay = true; } }
        let sql = crate::ir::rel::sql::unparse_plan_with_context(residual, super::SqlDialect::DuckDb, &self.request.rule_context)
            .map_err(|e| e.to_string())?;
        self.push(
            plan,
            Operation::Duckdb {
                sql,
                inputs: relations,
            },
        )
    }
    fn completion_input(&mut self, input: &LogicalPlan, relations: &mut Vec<Relation>) -> Result<LogicalPlan, String> {
        let id = self.lower(input)?;
        let name = format!("__orchid_input_{}_{id}", self.plan.nodes.len());
        let scan = LogicalPlanBuilder::scan(&name,
            datafusion::datasource::provider_as_source(Arc::new(
                datafusion::datasource::empty::EmptyTable::new(Arc::new(input.schema().as_arrow().clone())))), None)
            .and_then(|b| b.build()).map_err(|e| e.to_string())?;
        let expressions = scan.schema().columns().into_iter().zip(input.schema().iter())
            .map(|(column, (qualifier, field))| Expr::Column(column).alias_qualified(qualifier.cloned(), field.name()))
            .collect::<Vec<_>>();
        let result = LogicalPlanBuilder::from(scan).project(expressions).and_then(|b| b.build()).map_err(|e| e.to_string())?;
        relations.push(Relation { name, node: id, replay: false });
        Ok(result)
    }

}

/// Application-owned batch UDF. It must return exactly one value per input row.
/// Effectful functions are never duplicated or retried by this executor.
pub trait LocalFunction: Send + Sync {
    fn call(&self, arguments: &[ArrayRef], rows: usize) -> Result<ArrayRef, String>;
}
pub type Functions = BTreeMap<String, Arc<dyn LocalFunction>>;

fn validate_expr(expr: &LocalExpr, functions: &Functions, width: usize) -> Result<(), String> {
    match expr {
        LocalExpr::Column { index } if *index >= width => {
            Err("local input column out of bounds".into())
        }
        LocalExpr::Call {
            function,
            arguments,
            returns,
        } => {
            if !functions.contains_key(function) {
                return Err(format!("missing local function {function}"));
            }
            crate::compiler::data_type(returns)?;
            for argument in arguments {
                validate_expr(argument, functions, width)?;
            }
            Ok(())
        }
        LocalExpr::Cast { input, data_type } | LocalExpr::GraphElement { input, data_type } => {
            crate::compiler::data_type(data_type)?;
            validate_expr(input, functions, width)
        }
        LocalExpr::Literal { ipc, .. } => {
            literal(ipc)?;
            Ok(())
        }
        _ => Ok(()),
    }
}
fn literal(ipc: &[u8]) -> Result<ScalarValue, String> {
    let mut reader = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(ipc), None)
        .map_err(|e| e.to_string())?;
    let batch = reader
        .next()
        .ok_or("missing literal")?
        .map_err(|e| e.to_string())?;
    if batch.num_columns() != 1 || batch.num_rows() != 1 {
        return Err("invalid literal shape".into());
    }
    ScalarValue::try_from_array(batch.column(0), 0).map_err(|e| e.to_string())
}
fn evaluate(
    expr: &LocalExpr,
    batch: &RecordBatch,
    functions: &Functions,
) -> Result<ArrayRef, String> {
    match expr {
        LocalExpr::Column { index } => Ok(batch.column(*index).clone()),
        LocalExpr::GraphElement { input, data_type } => {
            let values = evaluate(input, batch, functions)?;
            if values.data_type() != &crate::compiler::data_type(data_type)? {
                return Err("graph descriptor schema mismatch".into());
            }
            crate::execution::graph_values::decode_array(values)
        }
        LocalExpr::Cast { input, data_type } => arrow::compute::cast(
            &evaluate(input, batch, functions)?,
            &crate::compiler::data_type(data_type)?,
        )
        .map_err(|e| e.to_string()),
        LocalExpr::Literal { ipc, .. } => literal(ipc)?
            .to_array_of_size(batch.num_rows())
            .map_err(|e| e.to_string()),
        LocalExpr::Call {
            function,
            arguments,
            returns,
        } => {
            let args = arguments
                .iter()
                .map(|a| evaluate(a, batch, functions))
                .collect::<Result<Vec<_>, _>>()?;
            let result = functions[function].call(&args, batch.num_rows())?;
            if result.len() != batch.num_rows()
                || result.data_type() != &crate::compiler::data_type(returns)?
            {
                return Err(format!(
                    "local function {function} returned an invalid type or row count"
                ));
            }
            Ok(result)
        }
    }
}

/// Executes only locations present in the plan. A DuckDB binding may lazily open
/// its session on first query_stream; it is never touched by source-only plans.
pub async fn execute(
    plan: &ExecutionPlan,
    sources: &mut BTreeMap<String, Box<dyn crate::federation::Session>>,
    functions: &Functions,
    duckdb: Option<&mut dyn crate::federation::Session>,
) -> Result<BatchStream, String> {
    if plan.version != 1 || plan.result >= plan.nodes.len() {
        return Err("invalid execution plan protocol/result".into());
    }
    for (id, node) in plan.nodes.iter().enumerate() {
        schema(&node.columns)?;
        let dependency = |input: usize| {
            if input < id {
                Ok(())
            } else {
                Err("execution plan is not topologically ordered".to_string())
            }
        };
        match &node.operation {
            Operation::SourceSql {
                source, dialect, ..
            } => {
                if sources
                    .get(source)
                    .ok_or_else(|| format!("missing source {source}"))?
                    .dialect()
                    != dialect
                {
                    return Err(format!("source dialect mismatch for {source}"));
                }
                super::SqlDialect::resolve(dialect).map_err(|e| e.to_string())?;
            }
            Operation::SourceRequest {
                source,
                input,
                template,
            } => {
                if sources
                    .get(source)
                    .ok_or_else(|| format!("missing source {source}"))?
                    .dialect()
                    != template.adapter
                {
                    return Err("request source adapter mismatch".into());
                }
                template.validate()?;
                if let Some(input) = input {
                    dependency(*input)?;
                    if plan.nodes[*input].columns.len() != template.parameters {
                        return Err("request parameter count mismatch".into());
                    }
                } else if template.parameters != 0 {
                    return Err("request has unbound parameters".into());
                }
            }
            Operation::LocalProject { input, expressions } => {
                dependency(*input)?;
                if expressions.len() != node.columns.len() {
                    return Err("projection width mismatch".into());
                }
                for expr in expressions {
                    validate_expr(expr, functions, plan.nodes[*input].columns.len())?;
                }
            }
            Operation::Duckdb { inputs, .. } => {
                if duckdb.as_ref().is_none_or(|d| d.dialect() != "duckdb") {
                    return Err("plan requires a DuckDB completion binding".into());
                }
                for relation in inputs {
                    dependency(relation.node)?;
                }
            }
            Operation::Input { .. } => {}
        }
    }
    let mut duckdb = duckdb;
    let mut outputs = BTreeMap::<usize, BatchSource>::new();
    for (id, node) in plan.nodes.iter().enumerate() {
        let output_schema = schema(&node.columns)?;
        let stream = match &node.operation {
            Operation::SourceSql { source, sql, .. } => {
                sources
                    .get_mut(source)
                    .unwrap()
                    .query_stream(sql, vec![])
                    .await?
            }
            Operation::SourceRequest {
                source,
                input,
                template,
            } => {
                let requests: crate::federation::RequestStream = if let Some(input) = input {
                    let template = template.clone();
                    Box::pin(outputs[input].open()?.flat_map(move |batch| {
                        let template = template.clone();
                        match batch {
                            Err(error) => futures::stream::iter([Err(error)]).boxed(),
                            Ok(batch) => {
                                let count = batch.num_rows();
                                futures::stream::iter((0..count).step_by(64).map(move |start| {
                                    (start..(start + 64).min(count))
                                        .map(|row| {
                                            let args = batch
                                                .columns()
                                                .iter()
                                                .map(|c| {
                                                    ScalarValue::try_from_array(c, row)
                                                        .map_err(|e| e.to_string())
                                                })
                                                .collect::<Result<Vec<_>, _>>()?;
                                            template.bind(&args)
                                        })
                                        .collect::<Result<Vec<_>, String>>()
                                }))
                                .boxed()
                            }
                        }
                    }))
                } else {
                    Box::pin(futures::stream::iter([Ok(vec![template.bind(&[])?])]))
                };
                sources
                    .get_mut(source)
                    .unwrap()
                    .execute_requests_stream(requests, node.columns.clone())
                    .await?
            }
            Operation::LocalProject { input, expressions } => {
                let expressions = expressions.clone();
                let functions = functions.clone();
                let out = output_schema.clone();
                BatchStream::new(
                    output_schema.clone(),
                    outputs[input].open()?.map(move |batch| {
                        let batch = batch?;
                        let arrays = expressions
                            .iter()
                            .map(|e| evaluate(e, &batch, &functions))
                            .collect::<Result<Vec<_>, _>>()?;
                        RecordBatch::try_new_with_options(
                            out.clone(),
                            arrays,
                            &RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
                        )
                        .map_err(|e| e.to_string())
                    }),
                )
            }
            Operation::Input { one_row } => {
                let batch = RecordBatch::try_new_with_options(
                    output_schema.clone(),
                    vec![],
                    &RecordBatchOptions::new().with_row_count(Some(usize::from(*one_row))),
                )
                .map_err(|e| e.to_string())?;
                BatchStream::new(output_schema.clone(), futures::stream::iter([Ok(batch)]))
            }
            Operation::Duckdb { sql, inputs } => {
                let inputs = inputs
                    .iter()
                    .map(|r| InputRelation {
                        name: r.name.clone(),
                        source: outputs[&r.node].clone(),
                    })
                    .collect();
                duckdb
                    .as_deref_mut()
                    .unwrap()
                    .query_stream(sql, inputs)
                    .await?
            }
        };
        // Normalize names by position, but reject type/width changes.
        if stream.schema().fields().len() != output_schema.fields().len()
            || stream
                .schema()
                .fields()
                .iter()
                .zip(output_schema.fields())
                .any(|(a, b)| a.data_type() != b.data_type())
        {
            return Err(format!(
                "source result schema mismatch at node {id}: expected {output_schema:?}, received {:?}",
                stream.schema()
            ));
        }
        let out = output_schema.clone();
        let stream = BatchStream::new(
            output_schema,
            stream.map(move |batch| {
                let batch = batch?;
                RecordBatch::try_new_with_options(
                    out.clone(),
                    batch.columns().to_vec(),
                    &RecordBatchOptions::new().with_row_count(Some(batch.num_rows())),
                )
                .map_err(|e| e.to_string())
            }),
        );
        // A source cursor cannot remain active when a dependent step reuses
        // the same caller-owned session. Spool only explicit reentry boundaries.
        let source_id = |op: &Operation| match op {
            Operation::SourceSql { source, .. } | Operation::SourceRequest { source, .. } => {
                Some(source.clone())
            }
            _ => None,
        };
        let reentry = if let Some(source) = source_id(&node.operation) {
            let current = &sources[&source];
            !current.supports_concurrent_statements()
                && plan.nodes[id + 1..]
                    .iter()
                    .filter_map(|n| source_id(&n.operation))
                    .any(|later| {
                        source == later
                            || current.concurrency_key().is_some_and(|key| {
                                sources[&later].concurrency_key().as_ref() == Some(&key)
                            })
                    })
        } else {
            matches!(node.operation, Operation::Duckdb { .. })
                && plan.nodes[id + 1..]
                    .iter()
                    .any(|n| matches!(n.operation, Operation::Duckdb { .. }))
        };
        let replay = plan.nodes.iter().any(|consumer| match &consumer.operation {
            Operation::Duckdb { inputs, .. } => inputs.iter().any(|input| input.node == id && input.replay),
            _ => false,
        });
        let output = if reentry || replay {
            BatchSource::spool(stream, &std::env::temp_dir().join("orchiddb-exchanges")).await?
        } else {
            BatchSource::once(stream)
        };
        outputs.insert(id, output);
    }
    outputs[&plan.result].open()
}

/// Lazily creates a completion adapter. Merely configuring DuckDB does not open
/// a database, register functions, or initialize its runtime.
pub struct LazyDuckdb {
    factory: Box<dyn FnMut() -> Result<Box<dyn crate::federation::Session>, String>>,
    session: Option<Box<dyn crate::federation::Session>>,
}
impl LazyDuckdb {
    pub fn new(
        factory: impl FnMut() -> Result<Box<dyn crate::federation::Session>, String> + 'static,
    ) -> Self {
        Self {
            factory: Box::new(factory),
            session: None,
        }
    }
}
#[async_trait::async_trait(?Send)]
impl crate::federation::Session for LazyDuckdb {
    fn dialect(&self) -> &str {
        "duckdb"
    }
    async fn query_stream(
        &mut self,
        sql: &str,
        inputs: Vec<InputRelation>,
    ) -> Result<BatchStream, String> {
        if self.session.is_none() {
            let session = (self.factory)()?;
            if session.dialect() != "duckdb" {
                return Err("completion factory must return a DuckDB adapter".into());
            }
            self.session = Some(session);
        }
        self.session
            .as_mut()
            .unwrap()
            .query_stream(sql, inputs)
            .await
    }
}
