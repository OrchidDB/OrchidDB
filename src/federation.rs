//! JSON-directed execution across caller-owned SQL and request sessions.
//!
//! Closed engine operations run on the engine owning their sources. Their results
//! are bound to query-scoped relations on the selected execution engine. No distributed snapshot or distributed transaction is implied.
use crate::compiler::{CompileRequest, CompiledSql};
use arrow::record_batch::RecordBatch;
use datafusion::sql::sqlparser::{ast, dialect::GenericDialect, parser::Parser};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicU64, Ordering},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Engine {
    pub dialect: String,
}

/// A SQL or service-request source, distinct from a relational completion engine.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub adapter: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndpointKind {
    Engine(crate::execution::ExecutableEngine),
    Source(String),
}
impl Engine {
    /// Legacy registry entries may name request sources, but never gain SQL
    /// execution capability. New schemas register these under `sources`.
    pub fn kind(&self) -> Result<EndpointKind, String> {
        if let Ok(engine) = crate::execution::ExecutableEngine::parse(&self.dialect) {
            return Ok(EndpointKind::Engine(engine));
        }
        if self.dialect == "trino" {
            return Ok(EndpointKind::Source(self.dialect.clone()));
        }
        if crate::operations::resolve(&self.dialect)?.is_some() {
            return Ok(EndpointKind::Source(self.dialect.clone()));
        }
        Err(format!(
            "unsupported executable engine {:?}; expected duckdb; register SQL services such as Trino under sources",
            self.dialect
        ))
    }
}
pub(crate) fn endpoint_adapter<'a>(request: &'a CompileRequest, name: &str) -> Option<&'a str> {
    request
        .engines
        .get(name)
        .map(|e| e.dialect.as_str())
        .or_else(|| request.sources.get(name).map(|s| s.adapter.as_str()))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Transfer {
    pub source_engine: String,
    pub source_dialect: String,
    pub sql: String,
    pub target_relation: String,
    pub columns: Vec<TransferColumn>,
    #[serde(default, alias = "search", skip_serializing_if = "Option::is_none")]
    pub operation: Option<DependentOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<RequestOperation>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DependentOperation {
    pub engine: String,
    pub input_columns: Vec<TransferColumn>,
    pub template: crate::ir::rel::sql::lowering::SqlTemplate,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RequestOperation {
    pub engine: String,
    pub input_columns: Vec<TransferColumn>,
    pub template: crate::operations::RequestTemplate,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransferColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

pub(crate) fn validate(request: &CompileRequest) -> Result<(), String> {
    if request.engine.is_some() {
        return crate::execution::plan::validate_request(request);
    }
    crate::execution::SqlDialect::resolve(&request.dialect).map_err(|e| e.to_string())?;
    for (name, source) in &request.sources {
        if name.is_empty() || request.engines.contains_key(name) {
            return Err(format!("duplicate or empty source name `{name}`"));
        }
        if crate::operations::resolve(&source.adapter)?.is_none() {
            return Err(format!(
                "unknown remote source adapter `{}`",
                source.adapter
            ));
        }
    }
    let metadata_owners = request
        .source_metadata
        .iter()
        .flat_map(|source| {
            source
                .options
                .get("source")
                .or_else(|| source.options.get("engine"))
                .into_iter()
                .chain(source.indexes.iter().filter_map(|index| {
                    index
                        .options
                        .get("source")
                        .or_else(|| index.options.get("engine"))
                }))
        })
        .map(|owner| {
            owner
                .as_str()
                .ok_or("source metadata engine must be a string")
        })
        .collect::<Result<Vec<_>, _>>()?;
    if request.engines.is_empty() && request.sources.is_empty() {
        if request.execution_engine.is_some()
            || request.tables.iter().any(|t| t.engine.is_some())
            || !metadata_owners.is_empty()
        {
            return Err("table/execution engine requires an engines registry".into());
        }
        return Ok(());
    }
    for owner in metadata_owners {
        if endpoint_adapter(request, owner).is_none() {
            return Err(format!("unknown source metadata engine `{owner}`"));
        }
    }
    for (name, engine) in &request.engines {
        if name.is_empty() {
            return Err("empty engine name".into());
        }
        engine.kind()?;
    }
    let target = request
        .execution_engine
        .as_ref()
        .ok_or("engines requires execution_engine")?;
    if request
        .engines
        .get(target)
        .ok_or("unknown execution_engine")?
        .dialect
        != request.dialect
    {
        return Err("execution_engine dialect does not match request dialect".into());
    }
    crate::execution::ExecutableEngine::parse(&request.dialect)?;
    let mut names = BTreeSet::new();
    for table in &request.tables {
        let normalized = parts(&table.name)?
            .into_iter()
            .map(|i| i.value)
            .collect::<Vec<_>>();
        if !names.insert(normalized) {
            return Err(format!("duplicate SQL table identity `{}`", table.name));
        }
        let engine = table.engine.as_ref().unwrap_or(target);
        if endpoint_adapter(request, engine).is_none() {
            return Err(format!(
                "unknown engine `{engine}` for table `{}`",
                table.name
            ));
        }
    }
    Ok(())
}

pub(crate) fn parts(name: &str) -> Result<Vec<ast::Ident>, String> {
    let mut parser = Parser::new(&GenericDialect)
        .try_with_sql(name)
        .map_err(|e| e.to_string())?;
    let name = parser.parse_object_name(false).map_err(|e| e.to_string())?;
    if parser.peek_token().token != datafusion::sql::sqlparser::tokenizer::Token::EOF {
        return Err("invalid table identifier".into());
    }
    name.0
        .into_iter()
        .map(|p| {
            p.as_ident()
                .cloned()
                .ok_or("invalid table identifier".into())
        })
        .collect()
}
/// Partition the relational DAG at engine boundaries. Try the largest closed
/// subtree on its owning engine; unsupported SQL falls through to smaller islands.
pub(crate) fn route(
    request: &CompileRequest,
    plan: datafusion::logical_expr::LogicalPlan,
) -> Result<(datafusion::logical_expr::LogicalPlan, Vec<Transfer>), String> {
    use datafusion::{
        common::{
            Column,
            tree_node::{Transformed, TreeNode, TreeNodeRecursion},
        },
        datasource::{empty::EmptyTable, provider_as_source},
        logical_expr::{Expr, LogicalPlan, LogicalPlanBuilder},
    };
    use std::sync::Arc;
    let Some(target) = &request.execution_engine else {
        return Ok((plan, vec![]));
    };
    let mut owners = BTreeMap::new();
    for table in &request.tables {
        let name = parts(&table.name)?
            .iter()
            .map(|i| i.value.clone())
            .collect::<Vec<_>>()
            .join(".");
        owners.insert(name, table.engine.as_ref().unwrap_or(target).clone());
    }
    let mut reserved = BTreeSet::new();
    plan.apply_with_subqueries(|node| {
        match node {
            LogicalPlan::TableScan(scan) => {
                reserved.insert(scan.table_name.table().to_string());
            }
            LogicalPlan::SubqueryAlias(alias) => {
                reserved.insert(alias.alias.table().to_string());
            }
            _ => {}
        }
        Ok(TreeNodeRecursion::Continue)
    })
    .map_err(|e: datafusion::common::DataFusionError| e.to_string())?;
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut transfers = vec![];
    let plan = plan
        .transform_down_with_subqueries(|node| {
            use crate::ir::rel::sql::{
                self,
                lowering::{self, LoweringContext, LoweringMode, RelationLowering},
            };
            let fail = datafusion::common::DataFusionError::Plan;
            let execution_dialect =
                sql::SqlDialect::resolve(&request.dialect).map_err(|e| fail(e.to_string()))?;
            let explicit_owner = crate::operations::owner(&node).map_err(fail)?;
            if let Some(engine) = &explicit_owner {
                let adapter_name = endpoint_adapter(request, engine)
                    .ok_or_else(|| fail(format!("unknown operation source `{engine}`")))?;
                if let Some(adapter) = crate::operations::resolve(adapter_name).map_err(fail)? {
                    let prepared = adapter.lower(&node).map_err(fail)?.ok_or_else(|| {
                        fail(format!("engine {engine} cannot lower relational operation"))
                    })?;
                    let replacement = route_request(
                        request,
                        &node,
                        engine,
                        prepared,
                        &mut transfers,
                        &mut reserved,
                        id,
                    )
                    .map_err(fail)?;
                    return Ok(Transformed::new(replacement, true, TreeNodeRecursion::Jump));
                }
            }
            let Some(placement) = lowering::placement_for(&node, execution_dialect)
                .map_err(|e| fail(e.to_string()))?
            else {
                return Ok(Transformed::no(node));
            };
            let owner_set = |plan: &LogicalPlan| -> datafusion::common::Result<BTreeSet<String>> {
                let mut found = BTreeSet::new();
                plan.apply_with_subqueries(|node| {
                    if let LogicalPlan::TableScan(scan) = node {
                        found.insert(
                            owners
                                .get(&scan.table_name.to_string())
                                .ok_or_else(|| {
                                    fail("relational input needs an engine owner".into())
                                })?
                                .clone(),
                        );
                    }
                    Ok(TreeNodeRecursion::Continue)
                })?;
                Ok(found)
            };
            let target_owners = placement
                .target
                .as_ref()
                .map(|plan| owner_set(plan))
                .transpose()?
                .unwrap_or_default();
            if target_owners.len() > 1 {
                return Err(fail(
                    "relational operation target must belong to one engine".into(),
                ));
            }
            let engine = explicit_owner
                .as_ref()
                .or_else(|| target_owners.first())
                .unwrap_or(target);
            let adapter_name = endpoint_adapter(request, engine)
                .ok_or_else(|| fail(format!("unknown operation source `{engine}`")))?;
            if let Some(adapter) = crate::operations::resolve(adapter_name).map_err(fail)? {
                let prepared = adapter.lower(&node).map_err(fail)?.ok_or_else(|| {
                    fail(format!("engine {engine} cannot lower relational operation"))
                })?;
                let replacement = route_request(
                    request,
                    &node,
                    engine,
                    prepared,
                    &mut transfers,
                    &mut reserved,
                    id,
                )
                .map_err(fail)?;
                return Ok(Transformed::new(replacement, true, TreeNodeRecursion::Jump));
            }
            let dialect = sql::SqlDialect::resolve(
                endpoint_adapter(request, engine).ok_or_else(|| fail("unknown endpoint".into()))?,
            )
            .map_err(|e| fail(e.to_string()))?;
            let source_owners = placement
                .source
                .as_ref()
                .map(|plan| owner_set(plan))
                .transpose()?
                .unwrap_or_default();
            let mode = if source_owners.iter().any(|owner| owner != engine) {
                LoweringMode::Bound
            } else {
                LoweringMode::InIsland
            };
            let lowered = lowering::lower_relation(&node, &LoweringContext { dialect, mode })
                .map_err(|e| fail(e.to_string()))?;
            let (input, template, schema) = match lowered {
                Some(RelationLowering::Dependent {
                    source,
                    template,
                    schema,
                }) => (source, template, schema),
                Some(RelationLowering::Rewrite(rewritten)) => {
                    return Ok(Transformed::yes(rewritten));
                }
                Some(RelationLowering::Sql(_)) => return Ok(Transformed::no(node)),
                None => {
                    return Err(fail(format!(
                        "engine {} cannot lower relational operation",
                        dialect.name()
                    )));
                }
            };
            if schema.as_ref() != node.schema().as_ref() {
                return Err(fail(
                    "dependent operation changed its logical output schema".into(),
                ));
            }
            if template.dialect != dialect.name() {
                return Err(fail(
                    "dependent operation changed its owning engine dialect".into(),
                ));
            }
            if template.parameters != input.schema().fields().len() {
                return Err(fail("dependent operation parameter schema mismatch".into()));
            }
            let (source, dependencies) = route(request, input.as_ref().clone()).map_err(fail)?;
            transfers.extend(dependencies);
            let source_sql =
                sql::unparse_plan(source, execution_dialect).map_err(|e| fail(e.to_string()))?;
            let mut name = format!("__orchiddb_operation_{id}_{}", transfers.len());
            while !reserved.insert(name.clone()) {
                name.push('_');
            }
            let columns = schema
                .fields()
                .iter()
                .map(|f| {
                    Ok(TransferColumn {
                        name: f.name().clone(),
                        data_type: type_name(f.data_type()).map_err(fail)?,
                        nullable: f.is_nullable(),
                    })
                })
                .collect::<datafusion::common::Result<Vec<_>>>()?;
            let scan = LogicalPlanBuilder::scan(
                name.clone(),
                provider_as_source(Arc::new(EmptyTable::new(Arc::new(
                    schema.as_arrow().clone(),
                )))),
                None,
            )?
            .build()?;
            let replacement = LogicalPlanBuilder::from(scan.clone())
                .project(
                    scan.schema()
                        .columns()
                        .into_iter()
                        .map(|c| {
                            let name = c.name.clone();
                            Expr::Column(c).alias(name)
                        })
                        .collect::<Vec<_>>(),
                )?
                .build()?;
            transfers.push(Transfer {
                source_engine: target.clone(),
                source_dialect: request.dialect.clone(),
                sql: source_sql,
                target_relation: name,
                columns,
                operation: Some(DependentOperation {
                    engine: engine.clone(),
                    input_columns: input
                        .schema()
                        .fields()
                        .iter()
                        .map(|f| {
                            Ok(TransferColumn {
                                name: f.name().clone(),
                                data_type: type_name(f.data_type()).map_err(fail)?,
                                nullable: f.is_nullable(),
                            })
                        })
                        .collect::<datafusion::common::Result<Vec<_>>>()?,
                    template,
                }),
                request: None,
            });
            Ok(Transformed::new(replacement, true, TreeNodeRecursion::Jump))
        })
        .map_err(|e| e.to_string())?
        .data;
    let result = plan
        .transform_down_with_subqueries(|node| {
            let mut sources = BTreeSet::new();
            let mut closed = true;
            node.apply_with_subqueries(|child| {
                if let LogicalPlan::TableScan(scan) = child {
                    match owners.get(&scan.table_name.to_string()) {
                        Some(engine) => {
                            sources.insert(engine.clone());
                        }
                        None => closed = false,
                    }
                }
                for expr in child.expressions() {
                    expr.apply(|e| {
                        if matches!(e, Expr::OuterReferenceColumn(..)) {
                            closed = false;
                        }
                        // Native declarations belong to the selected execution engine.
                        if let Expr::ScalarFunction(f) = e {
                            if f.name()
                                .starts_with(crate::ir::functions::ENGINE_FUNCTION_PREFIX)
                            {
                                closed = false;
                            }
                        }
                        if let Expr::AggregateFunction(f) = e {
                            if f.func
                                .name()
                                .starts_with(crate::ir::functions::ENGINE_FUNCTION_PREFIX)
                            {
                                closed = false;
                            }
                        }
                        Ok(TreeNodeRecursion::Continue)
                    })?;
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
            if closed && sources.len() == 1 && !sources.contains(target) {
                let source = sources.first().unwrap();
                if let Some(adapter) =
                    crate::operations::resolve(endpoint_adapter(request, source).ok_or_else(
                        || datafusion::common::DataFusionError::Plan("unknown endpoint".into()),
                    )?)
                    .map_err(datafusion::common::DataFusionError::Plan)?
                {
                    if let Some(prepared) = adapter
                        .lower(&node)
                        .map_err(datafusion::common::DataFusionError::Plan)?
                    {
                        let replacement = route_request(
                            request,
                            &node,
                            source,
                            prepared,
                            &mut transfers,
                            &mut reserved,
                            id,
                        )
                        .map_err(datafusion::common::DataFusionError::Plan)?;
                        return Ok(Transformed::new(replacement, true, TreeNodeRecursion::Jump));
                    }
                    return Ok(Transformed::no(node));
                }
                let dialect = crate::execution::SqlDialect::resolve(
                    endpoint_adapter(request, source).ok_or_else(|| {
                        datafusion::common::DataFusionError::Plan("unknown endpoint".into())
                    })?,
                )
                .map_err(|e| datafusion::common::DataFusionError::Plan(e.to_string()))?;
                let columns = node
                    .schema()
                    .fields()
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        Ok(TransferColumn {
                            name: format!("__c{i}"),
                            data_type: type_name(f.data_type())?,
                            nullable: f.is_nullable(),
                        })
                    })
                    .collect::<Result<Vec<_>, String>>();
                if let Ok(columns) = columns {
                    let projection = node
                        .schema()
                        .columns()
                        .into_iter()
                        .enumerate()
                        .map(|(i, c)| Expr::Column(c).alias(format!("__c{i}")))
                        .collect::<Vec<_>>();
                    let exported = LogicalPlanBuilder::from(node.clone())
                        .project(projection)?
                        .build()?;
                    if let Ok(sql) = crate::ir::rel::sql::unparse_plan(exported, dialect) {
                        let mut name = format!("__orchiddb_exchange_{id}_{}", transfers.len());
                        while !reserved.insert(name.clone()) {
                            name.push('_');
                        }
                        let schema = Arc::new(arrow::datatypes::Schema::new(
                            columns
                                .iter()
                                .map(|c| {
                                    Ok(arrow::datatypes::Field::new(
                                        &c.name,
                                        crate::compiler::data_type(&c.data_type)
                                            .map_err(datafusion::common::DataFusionError::Plan)?,
                                        c.nullable,
                                    ))
                                })
                                .collect::<datafusion::common::Result<Vec<_>>>()?,
                        ));
                        let scan = LogicalPlanBuilder::scan(
                            name.clone(),
                            provider_as_source(Arc::new(EmptyTable::new(schema))),
                            None,
                        )?
                        .build()?;
                        let expressions: Vec<_> = columns
                            .iter()
                            .zip(node.schema().fields())
                            .map(|(c, f)| {
                                Expr::Column(Column::new(Some(name.as_str()), &c.name))
                                    .alias(f.name())
                            })
                            .collect();
                        let qualifiers = node
                            .schema()
                            .iter()
                            .map(|(q, _)| q.cloned())
                            .collect::<BTreeSet<_>>();
                        let replacement = if qualifiers.len() == 1 {
                            let projected = LogicalPlanBuilder::from(scan)
                                .project(expressions)?
                                .build()?;
                            if let Some(Some(qualifier)) = qualifiers.first() {
                                LogicalPlanBuilder::from(projected)
                                    .alias(qualifier.clone())?
                                    .build()?
                            } else {
                                projected
                            }
                        } else {
                            // Multiple SQL namespaces cannot be represented by one
                            // exchange relation without rewriting all consumers.
                            // Descend until a projection or alias closes the scope.
                            return Ok(Transformed::no(node));
                        };
                        transfers.push(Transfer {
                            source_engine: source.clone(),
                            source_dialect: dialect.name().into(),
                            sql,
                            target_relation: name,
                            columns,
                            operation: None,
                            request: None,
                        });
                        return Ok(Transformed::new(replacement, true, TreeNodeRecursion::Jump));
                    }
                }
            }
            Ok(Transformed::no(node))
        })
        .map_err(|e| e.to_string())?;
    // A source that could not cross the boundary must never be resolved by
    // accident against a same-named table on the target connection.
    result.data.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            if let Some(owner) = owners.get(&scan.table_name.to_string()) {
                if owner != target {
                    return Err(datafusion::common::DataFusionError::Plan(format!(
                        "cannot transfer source `{}` from engine `{owner}`: no supported engine operation/exchange schema", scan.table_name)));
                }
            }
        }
        Ok(TreeNodeRecursion::Continue)
    }).map_err(|e|e.to_string())?;
    Ok((result.data, transfers))
}

/// Materialize a typed adapter request at the same boundary used by SQL islands.
fn route_request(
    request: &CompileRequest,
    node: &datafusion::logical_expr::LogicalPlan,
    engine: &str,
    prepared: crate::operations::PreparedOperation,
    transfers: &mut Vec<Transfer>,
    reserved: &mut BTreeSet<String>,
    id: u64,
) -> Result<datafusion::logical_expr::LogicalPlan, String> {
    use datafusion::common::tree_node::{Transformed, TreeNodeRecursion};
    use datafusion::{
        datasource::{empty::EmptyTable, provider_as_source},
        logical_expr::{Expr, LogicalPlan, LogicalPlanBuilder, Projection},
    };
    use std::sync::Arc;
    if prepared
        .replacement
        .as_ref()
        .map_or(&prepared.schema, |replacement| replacement.schema())
        != node.schema()
    {
        return Err("request operation changed its logical output schema".into());
    }
    if let Some(continuation) = &prepared.replacement {
        let mut markers = 0;
        continuation
            .apply_with_subqueries(|plan| {
                if let LogicalPlan::Extension(extension) = plan {
                    if let Some(marker) = extension
                        .node
                        .as_any()
                        .downcast_ref::<crate::operations::RequestResult>()
                    {
                        if marker.schema != prepared.schema {
                            return Err(datafusion::common::DataFusionError::Plan(
                                "request continuation schema mismatch".into(),
                            ));
                        }
                        markers += 1;
                    }
                }
                Ok(TreeNodeRecursion::Continue)
            })
            .map_err(|e| e.to_string())?;
        if markers != 1 {
            return Err("request continuation must contain exactly one result leaf".into());
        }
    }
    if prepared.template.adapter != endpoint_adapter(request, engine).ok_or("unknown source")? {
        return Err("request operation changed its owning adapter".into());
    }
    prepared.template.validate()?;
    let input_columns = prepared
        .source
        .as_ref()
        .map(|source| transfer_columns(source.schema()))
        .transpose()?
        .unwrap_or_default();
    if input_columns.len() != prepared.template.parameters {
        return Err("request operation parameter schema mismatch".into());
    }
    let columns = transfer_columns(&prepared.schema)?;
    let target = request
        .execution_engine
        .as_ref()
        .ok_or("request operation requires execution_engine")?;
    let (source_engine, source_dialect, sql) = if let Some(source) = &prepared.source {
        let (source, dependencies) = route(request, source.as_ref().clone())?;
        transfers.extend(dependencies);
        let dialect =
            crate::execution::SqlDialect::resolve(&request.dialect).map_err(|e| e.to_string())?;
        (
            target.clone(),
            request.dialect.clone(),
            crate::ir::rel::sql::unparse_plan(source, dialect).map_err(|e| e.to_string())?,
        )
    } else {
        (
            engine.to_owned(),
            prepared.template.adapter.clone(),
            String::new(),
        )
    };
    let mut name = format!("__orchiddb_request_{id}_{}", transfers.len());
    while !reserved.insert(name.clone()) {
        name.push('_');
    }
    // Exchange columns get positional names so duplicate names from different
    // input namespaces stay unambiguous. Restore the exact logical schema.
    let exchange_fields = prepared
        .schema
        .fields()
        .iter()
        .enumerate()
        .map(|(i, f)| {
            arrow::datatypes::Field::new(format!("__c{i}"), f.data_type().clone(), f.is_nullable())
        })
        .collect::<Vec<_>>();
    let scan = LogicalPlanBuilder::scan(
        name.clone(),
        provider_as_source(Arc::new(EmptyTable::new(Arc::new(
            arrow::datatypes::Schema::new(exchange_fields),
        )))),
        None,
    )
    .and_then(|builder| builder.build())
    .map_err(|e| e.to_string())?;
    let expressions = scan
        .schema()
        .columns()
        .into_iter()
        .zip(prepared.schema.iter())
        .map(|(column, (qualifier, field))| {
            Expr::Column(column).alias_qualified(qualifier.cloned(), field.name())
        })
        .collect();
    let replacement = LogicalPlan::Projection(
        Projection::try_new_with_schema(expressions, Arc::new(scan), prepared.schema.clone())
            .map_err(|e| e.to_string())?,
    );
    // The adapter receives logical names; the transfer uses positional names.
    // Both descriptions have identical types and positional order.
    let exchange_columns = columns
        .iter()
        .enumerate()
        .map(|(i, c)| TransferColumn {
            name: format!("__c{i}"),
            ..c.clone()
        })
        .collect();
    transfers.push(Transfer {
        source_engine,
        source_dialect,
        sql,
        target_relation: name,
        columns: exchange_columns,
        operation: None,
        request: Some(RequestOperation {
            engine: engine.into(),
            input_columns,
            template: prepared.template,
        }),
    });
    if let Some(continuation) = prepared.replacement {
        let continuation = continuation.transform_down_with_subqueries(|plan| {
            if matches!(&plan, LogicalPlan::Extension(extension) if extension.node.as_any().is::<crate::operations::RequestResult>()) {
                return Ok(Transformed::new(replacement.clone(), true, TreeNodeRecursion::Jump));
            }
            Ok(Transformed::no(plan))
        }).map_err(|e|e.to_string())?.data;
        let (continuation, dependencies) = route(request, continuation)?;
        transfers.extend(dependencies);
        Ok(continuation)
    } else {
        Ok(replacement)
    }
}
fn transfer_columns(
    schema: &datafusion::common::DFSchemaRef,
) -> Result<Vec<TransferColumn>, String> {
    schema
        .fields()
        .iter()
        .map(|field| {
            Ok(TransferColumn {
                name: field.name().clone(),
                data_type: type_name(field.data_type())?,
                nullable: field.is_nullable(),
            })
        })
        .collect()
}

pub(crate) fn type_name(ty: &arrow::datatypes::DataType) -> Result<String, String> {
    use arrow::datatypes::DataType::*;
    if crate::ir::functions::domain::is_json(ty) {
        return Ok("json".into());
    }
    if let Some((name, storage)) = crate::ir::functions::domain::descriptor(ty) {
        return Ok(format!("domain:{name}:{}", type_name(storage)?));
    }
    Ok(match ty {
        Null => "null".into(),
        Boolean => "boolean".into(),
        Int8 => "int8".into(),
        Int16 => "int16".into(),
        Int32 => "int32".into(),
        Int64 => "int64".into(),
        UInt8 => "uint8".into(),
        UInt16 => "uint16".into(),
        UInt32 => "uint32".into(),
        UInt64 => "uint64".into(),
        Float32 => "float32".into(),
        Float64 => "float64".into(),
        Utf8 | LargeUtf8 | Utf8View => "string".into(),
        Binary | LargeBinary => "binary".into(),
        Date32 | Date64 => "date".into(),
        Time32(_) | Time64(_) => "time".into(),
        Timestamp(_, None) => "timestamp".into(),
        Timestamp(_, Some(zone)) if zone.as_ref() == "UTC" => "timestamptz".into(),
        Decimal128(p, s) => format!("decimal:{p}:{s}"),
        List(f) | LargeList(f) | FixedSizeList(f, _) => {
            format!("list:{}", type_name(f.data_type())?)
        }
        Struct(fields) => format!(
            "struct_fields:{}",
            serde_json::to_string(
                &fields
                    .iter()
                    .map(|f| Ok((f.name().clone(), type_name(f.data_type())?)))
                    .collect::<Result<Vec<_>, String>>()?
            )
            .map_err(|e| e.to_string())?
        ),
        _ => return Err(format!("unsupported exchange type {ty}")),
    })
}

/// Bind a completed SQL island as a query-scoped CTE. This emits one read-only
/// statement and never creates a database table, view, or other catalog object.
pub fn bind_batches(
    sql: &str,
    dialect: &str,
    transfer: &Transfer,
    batches: &[RecordBatch],
) -> Result<String, String> {
    bind_region_batches(sql, dialect, &[(transfer, batches)])
}

pub(crate) fn bind_region_batches(
    sql: &str,
    dialect: &str,
    inputs: &[(&Transfer, &[RecordBatch])],
) -> Result<String, String> {
    let dialect = crate::execution::SqlDialect::resolve(dialect).map_err(|e| e.to_string())?;
    let parser_dialect = dialect.parser_dialect();
    let mut statements = Parser::new(parser_dialect.as_ref())
        .with_recursion_limit(1024)
        .try_with_sql(sql)
        .map_err(|e| e.to_string())?
        .parse_statements()
        .map_err(|e| e.to_string())?;
    if statements.len() != 1 {
        return Err("binding requires exactly one query".into());
    }
    let ast::Statement::Query(query) = &mut statements[0] else {
        return Err("binding requires a read query".into());
    };
    let mut names = query
        .with
        .as_ref()
        .map(|with| {
            with.cte_tables
                .iter()
                .map(|cte| cte.alias.name.value.clone())
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();
    let mut bindings = Vec::with_capacity(inputs.len());
    for (index, (transfer, batches)) in inputs.iter().enumerate() {
        if !names.insert(transfer.target_relation.clone()) {
            return Err("duplicate exchange binding".into());
        }
        let shared = inputs[..index].iter().find(|(previous, values)| {
            previous.columns.len() == transfer.columns.len()
                && previous
                    .columns
                    .iter()
                    .zip(&transfer.columns)
                    .all(|(a, b)| a.data_type == b.data_type)
                && values.len() == batches.len()
                && values
                    .iter()
                    .zip(*batches)
                    .all(|(a, b)| a.num_rows() == b.num_rows() && a.columns() == b.columns())
        });
        if let Some((previous, _)) = shared {
            for batch in *batches {
                for (column, field) in batch.columns().iter().zip(&transfer.columns) {
                    if !field.nullable && column.null_count() != 0 {
                        return Err("NULL in non-nullable exchange column".into());
                    }
                }
            }
            let columns = transfer
                .columns
                .iter()
                .map(|column| dialect.quote_ident(&column.name))
                .collect::<Vec<_>>()
                .join(", ");
            let source = previous
                .columns
                .iter()
                .map(|column| dialect.quote_ident(&column.name))
                .collect::<Vec<_>>()
                .join(", ");
            let sql = format!(
                "WITH {} ({columns}) AS (SELECT {source} FROM {}) SELECT 1",
                dialect.quote_ident(&transfer.target_relation),
                dialect.quote_ident(&previous.target_relation)
            );
            let mut statements = Parser::parse_sql(parser_dialect.as_ref(), &sql)
                .map_err(|error| error.to_string())?;
            let ast::Statement::Query(query) = statements.remove(0) else {
                unreachable!()
            };
            bindings.push(query.with.unwrap().cte_tables.remove(0));
        } else {
            bindings.push(input_binding(dialect, transfer, batches)?);
        }
    }
    if let Some(existing) = &mut query.with {
        bindings.append(&mut existing.cte_tables);
        existing.cte_tables = bindings;
    } else if !bindings.is_empty() {
        query.with = Some(ast::With {
            with_token: ast::helpers::attached_token::AttachedToken::empty(),
            recursive: false,
            cte_tables: bindings,
        });
    }
    Ok(statements.remove(0).to_string())
}

fn input_binding(
    dialect: crate::execution::SqlDialect,
    transfer: &Transfer,
    batches: &[RecordBatch],
) -> Result<ast::Cte, String> {
    use datafusion::common::ScalarValue;
    let types = transfer
        .columns
        .iter()
        .map(|c| crate::compiler::data_type(&c.data_type))
        .collect::<Result<Vec<_>, _>>()?;
    if types.is_empty() {
        return Err("exchange must have columns".into());
    }
    let mut rows = Vec::new();
    for batch in batches {
        if batch.num_columns() != types.len() {
            return Err("exchange column count mismatch".into());
        }
        for row in 0..batch.num_rows() {
            let mut cells = Vec::new();
            for (index, ty) in types.iter().enumerate() {
                let value = ScalarValue::try_from_array(batch.column(index).as_ref(), row)
                    .map_err(|e| e.to_string())?;
                let value = exchange_scalar(value, ty)?;
                if value.is_null() && !transfer.columns[index].nullable {
                    return Err("NULL in non-nullable exchange column".into());
                }
                cells.push(
                    crate::ir::rel::sql::exchange_literal(value, ty.clone(), dialect)
                        .map_err(|e| e.to_string())?,
                );
            }
            rows.push(format!("({})", cells.join(", ")));
        }
    }
    let columns = transfer
        .columns
        .iter()
        .map(|c| dialect.quote_ident(&c.name))
        .collect::<Vec<_>>()
        .join(", ");
    let relation = if rows.is_empty() {
        let cells = types
            .iter()
            .map(|ty| {
                let value = ScalarValue::try_from(ty).map_err(|e| e.to_string())?;
                crate::ir::rel::sql::exchange_literal(value, ty.clone(), dialect)
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        format!("SELECT {} WHERE FALSE", cells.join(", "))
    } else {
        format!("VALUES {}", rows.join(", "))
    };
    let prefix = format!(
        "WITH {} ({columns}) AS ({relation}) SELECT 1",
        dialect.quote_ident(&transfer.target_relation)
    );
    let parser_dialect = dialect.parser_dialect();
    let mut binding =
        Parser::parse_sql(parser_dialect.as_ref(), &prefix).map_err(|e| e.to_string())?;
    let ast::Statement::Query(binding) = binding.remove(0) else {
        unreachable!()
    };
    let mut cte = binding.with.unwrap().cte_tables.remove(0);
    dialect
        .rewrite_input_binding(&mut cte)
        .map_err(|e| e.to_string())?;
    Ok(cte)
}

/// Shared protocol for bindings which transport Arrow IPC or typed JSON rows.
pub fn bind_command(command: serde_json::Value) -> Result<serde_json::Value, String> {
    let plan = command.get("plan").cloned().ok_or("missing bind plan")?;
    if plan["version"] != 1 {
        return Err("unsupported bind plan version".into());
    }
    let name = command["relation"]
        .as_str()
        .ok_or("missing bind relation")?;
    let transfers = plan["transfers"].as_array().ok_or("missing transfers")?;
    let index = transfers
        .iter()
        .position(|t| t["target_relation"].as_str() == Some(name))
        .ok_or("unknown exchange relation")?;
    let transfer: Transfer =
        serde_json::from_value(transfers[index].clone()).map_err(|e| e.to_string())?;
    let batches = exchange_batches(&command, &transfer.columns)?;
    bind_plan_batches(plan, name, &batches)
}

fn exchange_batches(
    command: &serde_json::Value,
    columns: &[TransferColumn],
) -> Result<Vec<RecordBatch>, String> {
    let forms = ["ipc", "rows", "batches"]
        .iter()
        .filter(|key| command.get(**key).is_some())
        .count();
    if forms != 1 {
        return Err("provide exactly one of rows, IPC, or batches".into());
    }
    if let Some(chunks) = command.get("batches") {
        let mut output = vec![];
        for chunk in chunks
            .as_array()
            .ok_or("exchange batches must be an array")?
        {
            output.extend(exchange_batches(chunk, columns)?);
        }
        return Ok(output);
    }
    use base64::Engine;
    use datafusion::common::ScalarValue;
    let batches = if let Some(ipc) = command["ipc"].as_str() {
        if command.get("rows").is_some() {
            return Err("provide rows or IPC, not both".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(ipc)
            .map_err(|e| e.to_string())?;
        arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?
    } else {
        let rows = command["rows"]
            .as_array()
            .ok_or("missing bind rows or IPC")?;
        let types = columns
            .iter()
            .map(|c| crate::compiler::data_type(&c.data_type))
            .collect::<Result<Vec<_>, _>>()?;
        let mut values = vec![Vec::new(); types.len()];
        for row in rows {
            let row = row.as_array().ok_or("exchange row must be an array")?;
            if row.len() != types.len() {
                return Err("exchange row width mismatch".into());
            }
            for (index, ty) in types.iter().enumerate() {
                values[index].push(json_scalar(&row[index], ty)?);
            }
        }
        let arrays = values
            .into_iter()
            .zip(&types)
            .map(|(v, ty)| {
                if v.is_empty() {
                    Ok(arrow::array::new_empty_array(ty))
                } else {
                    ScalarValue::iter_to_array(v).map_err(|e| e.to_string())
                }
            })
            .collect::<Result<Vec<_>, String>>()?;
        let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(
            columns
                .iter()
                .zip(types)
                .map(|(c, ty)| arrow::datatypes::Field::new(&c.name, ty, c.nullable))
                .collect::<Vec<_>>(),
        ));
        vec![
            RecordBatch::try_new_with_options(
                schema,
                arrays,
                &arrow::record_batch::RecordBatchOptions::new().with_row_count(Some(rows.len())),
            )
            .map_err(|e| e.to_string())?,
        ]
    };
    Ok(batches)
}

/// Complete a transfer and update every dependent island using the same batches.
/// Shared by the JSON/IPC protocol and the zero-copy Arrow C stream entry point.
pub fn bind_plan_batches(
    mut plan: serde_json::Value,
    name: &str,
    batches: &[RecordBatch],
) -> Result<serde_json::Value, String> {
    if plan["version"] != 1 {
        return Err("unsupported bind plan version".into());
    }
    let transfers = plan["transfers"].as_array().ok_or("missing transfers")?;
    let index = transfers
        .iter()
        .position(|t| t["target_relation"].as_str() == Some(name))
        .ok_or("unknown exchange relation")?;
    let transfer: Transfer =
        serde_json::from_value(transfers[index].clone()).map_err(|e| e.to_string())?;
    let sql = bind_batches(
        plan["sql"].as_str().ok_or("missing SQL")?,
        plan["dialect"].as_str().ok_or("missing dialect")?,
        &transfer,
        &batches,
    )?;
    plan["sql"] = sql.into();
    plan["transfers"].as_array_mut().unwrap().remove(index);
    for pending in plan["transfers"].as_array_mut().unwrap() {
        if !pending["request"].is_null() && pending["sql"].as_str() == Some("") {
            continue;
        }
        let query = bind_batches(
            pending["sql"].as_str().ok_or("missing dependent SQL")?,
            pending["source_dialect"]
                .as_str()
                .ok_or("missing dependent dialect")?,
            &transfer,
            &batches,
        )?;
        pending["sql"] = query.into();
    }
    Ok(plan)
}

pub(crate) fn json_scalar(
    value: &serde_json::Value,
    ty: &arrow::datatypes::DataType,
) -> Result<datafusion::common::ScalarValue, String> {
    use arrow::datatypes::DataType;
    use base64::Engine;
    use datafusion::common::ScalarValue;
    use std::sync::Arc;
    if value.is_null() {
        return ScalarValue::try_from(ty).map_err(|e| e.to_string());
    }
    if crate::ir::functions::domain::is_json(ty) {
        let text = value.as_str().ok_or(
            "JSON exchange cells must contain serialized JSON text (SQL null uses a null cell)",
        )?;
        return crate::ir::functions::domain::json_scalar(text).map_err(|e| e.to_string());
    }
    if let DataType::Struct(fields) = ty {
        let object = value.as_object().ok_or("expected struct exchange object")?;
        let columns = fields
            .iter()
            .map(|f| {
                json_scalar(
                    object.get(f.name()).unwrap_or(&serde_json::Value::Null),
                    f.data_type(),
                )?
                .to_array_of_size(1)
                .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        return Ok(ScalarValue::Struct(Arc::new(
            arrow::array::StructArray::try_new(fields.clone(), columns, None)
                .map_err(|e| e.to_string())?,
        )));
    }
    if let DataType::List(field) | DataType::LargeList(field) = ty {
        if let Some(text) = value.as_str() {
            return json_scalar(
                &serde_json::from_str(text).map_err(|e| format!("invalid JSON list: {e}"))?,
                ty,
            );
        }
        let values = value
            .as_array()
            .ok_or("expected list exchange value")?
            .iter()
            .map(|v| json_scalar(v, field.data_type()))
            .collect::<Result<Vec<_>, _>>()?;
        // Preserve the declared offset width and child field metadata.
        let values = if values.is_empty() {
            arrow::array::new_empty_array(field.data_type())
        } else {
            ScalarValue::iter_to_array(values).map_err(|e| e.to_string())?
        };
        return match ty {
            DataType::LargeList(_) => Ok(ScalarValue::LargeList(Arc::new(
                arrow::array::LargeListArray::try_new(
                    field.clone(),
                    arrow::buffer::OffsetBuffer::from_lengths([values.len()]),
                    values,
                    None,
                )
                .map_err(|e| e.to_string())?,
            ))),
            _ => Ok(ScalarValue::List(Arc::new(
                arrow::array::ListArray::try_new(
                    field.clone(),
                    arrow::buffer::OffsetBuffer::from_lengths([values.len()]),
                    values,
                    None,
                )
                .map_err(|e| e.to_string())?,
            ))),
        };
    }
    if matches!(ty, DataType::Binary | DataType::LargeBinary) {
        let text = value.as_str().ok_or("expected encoded binary")?;
        let bytes = if let Some(hex) = text.strip_prefix("\\x") {
            if hex.len() % 2 != 0 {
                return Err("invalid hexadecimal binary".into());
            }
            hex.as_bytes()
                .chunks(2)
                .map(|pair| {
                    std::str::from_utf8(pair)
                        .map_err(|e| e.to_string())
                        .and_then(|pair| u8::from_str_radix(pair, 16).map_err(|e| e.to_string()))
                })
                .collect::<Result<Vec<_>, _>>()?
        } else {
            base64::engine::general_purpose::STANDARD
                .decode(text)
                .map_err(|e| e.to_string())?
        };
        return Ok(if *ty == DataType::LargeBinary {
            ScalarValue::LargeBinary(Some(bytes))
        } else {
            ScalarValue::Binary(Some(bytes))
        });
    }
    let text = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => value.to_string(),
        _ => return Err("unsupported exchange scalar".into()),
    };
    ScalarValue::Utf8(Some(text))
        .cast_to(ty)
        .map_err(|e| e.to_string())
}

/// A caller-owned session for SQL queries or typed adapter requests. Federation
/// never imports data into database objects or changes the caller's transaction.
#[async_trait::async_trait(?Send)]
pub trait Session {
    /// Shared connection identity across aliases/clones in a session registry.
    /// Returning None treats this registry entry as independently owned.
    fn concurrency_key(&self) -> Option<String> { None }

    /// Whether another statement can run before this session’s stream finishes.
    fn supports_concurrent_statements(&self) -> bool {
        false
    }

    /// Stream a SQL island with typed input relations. Implementations must not
    /// collect inputs or substitute rows into SQL text. SQL sources execute
    /// without inputs; only the DuckDB completion adapter consumes relations.
    async fn query_stream(
        &mut self,
        _sql: &str,
        _inputs: Vec<crate::execution::stream::InputRelation>,
    ) -> Result<crate::execution::stream::BatchStream, String> {
        Err("session does not implement streaming SQL execution".into())
    }
    /// Consume bounded batches of bound requests lazily. Remote sources implement
    /// this operation; they are not SQL executable engines.
    async fn execute_requests_stream(
        &mut self,
        _requests: RequestStream,
        _columns: Vec<TransferColumn>,
    ) -> Result<crate::execution::stream::BatchStream, String> {
        Err("source does not implement streaming batched requests".into())
    }
    async fn query_requests_stream(
        &mut self,
        _queries: QueryStream,
        _columns: Vec<TransferColumn>,
    ) -> Result<crate::execution::stream::BatchStream, String> {
        Err("engine does not implement streaming dependent SQL requests".into())
    }
    fn dialect(&self) -> &str;
    async fn query(&mut self, sql: &str) -> Result<Vec<RecordBatch>, String> {
        self.query_stream(sql, vec![])
            .await?
            .collect_batches()
            .await
    }
    async fn execute_request(
        &mut self,
        _request: &serde_json::Value,
        _columns: &[TransferColumn],
    ) -> Result<Vec<RecordBatch>, String> {
        Err("session does not support prepared requests".into())
    }
    /// Batch independently bound operations without losing per-input boundaries.
    async fn execute_requests(
        &mut self,
        requests: &[serde_json::Value],
        columns: &[TransferColumn],
    ) -> Result<Vec<Vec<RecordBatch>>, String> {
        let mut results = Vec::with_capacity(requests.len());
        for request in requests {
            results.push(self.execute_request(request, columns).await?);
        }
        Ok(results)
    }
}

/// Request batches preserve per-input correlation encoded by the bound template.
pub type RequestStream = std::pin::Pin<
    Box<dyn futures::Stream<Item = Result<Vec<serde_json::Value>, String>> + Send + 'static>,
>;
pub type QueryStream =
    std::pin::Pin<Box<dyn futures::Stream<Item = Result<String, String>> + Send + 'static>>;

/// Terminal compatibility API. Intermediate islands always remain streams.
pub async fn execute(
    plan: &CompiledSql,
    sessions: &mut BTreeMap<String, Box<dyn Session>>,
) -> Result<Vec<RecordBatch>, String> {
    execute_stream(plan, sessions)
        .await?
        .collect_batches()
        .await
}

#[derive(Debug, Clone)]
pub struct ExchangeOptions {
    pub request_batch_size: usize,
    /// Explicit replay and same-connection reentrancy boundaries use disk IPC.
    /// Independent single-consumer edges remain single-pass.
    pub replay_directory: std::path::PathBuf,
}
impl Default for ExchangeOptions {
    fn default() -> Self {
        Self {
            request_batch_size: 64,
            replay_directory: std::env::temp_dir().join("orchiddb-exchanges"),
        }
    }
}

pub async fn execute_stream(
    plan: &CompiledSql,
    sessions: &mut BTreeMap<String, Box<dyn Session>>,
) -> Result<crate::execution::stream::BatchStream, String> {
    execute_stream_with_options(plan, sessions, &ExchangeOptions::default()).await
}

pub async fn execute_stream_with_options(
    plan: &CompiledSql,
    sessions: &mut BTreeMap<String, Box<dyn Session>>,
    options: &ExchangeOptions,
) -> Result<crate::execution::stream::BatchStream, String> {
    use crate::execution::{ExecutableEngine, stream::BatchSource};
    if plan.version != 1 {
        return Err("unsupported compiled SQL version".into());
    }
    if options.request_batch_size == 0 {
        return Err("request batch size must be positive".into());
    }
    ExecutableEngine::parse(&plan.dialect)?;
    let target = plan
        .execution_engine
        .as_ref()
        .ok_or("plan has no execution_engine")?;
    for (id, dialect) in std::iter::once((target, &plan.dialect)).chain(
        plan.transfers
            .iter()
            .map(|t| (&t.source_engine, &t.source_dialect)),
    ) {
        if sessions
            .get(id)
            .ok_or_else(|| format!("missing engine or source `{id}`"))?
            .dialect()
            != dialect
        {
            return Err(format!("adapter mismatch for `{id}`"));
        }
    }
    // Fail before executing any source when a dependent target is missing.
    for transfer in &plan.transfers {
        if let Some(request) = &transfer.request {
            if transfer.operation.is_some() {
                return Err("transfer cannot contain SQL and request operations".into());
            }
            request.template.validate()?;
            if request.input_columns.len() != request.template.parameters {
                return Err("request parameter schema mismatch".into());
            }
            if sessions
                .get(&request.engine)
                .ok_or_else(|| format!("missing source `{}`", request.engine))?
                .dialect()
                != request.template.adapter
            {
                return Err("request source adapter mismatch".into());
            }
        }
        if let Some(operation) = &transfer.operation {
            crate::execution::SqlDialect::resolve(&operation.template.dialect).map_err(|e| e.to_string())?;
            if sessions
                .get(&operation.engine)
                .ok_or_else(|| format!("missing engine `{}`", operation.engine))?
                .dialect()
                != operation.template.dialect
            {
                return Err("dependent SQL engine mismatch".into());
            }
        }
        if !transfer.sql.is_empty() {
            crate::execution::SqlDialect::resolve(&transfer.source_dialect).map_err(|e| e.to_string())?;
        }
    }
    let mut uses = BTreeMap::<String, usize>::new();
    for (sql, dialect) in plan
        .transfers
        .iter()
        .map(|t| (t.sql.as_str(), t.source_dialect.as_str()))
        .chain(std::iter::once((plan.sql.as_str(), plan.dialect.as_str())))
    {
        for name in relation_names(sql, dialect)? {
            *uses.entry(name).or_default() += 1;
        }
    }
    let connection_keys = sessions.iter().map(|(name, session)| {
        (name.clone(), session.concurrency_key().map(|key| (true,key)).unwrap_or_else(|| (false,name.clone())))
    }).collect::<BTreeMap<_,_>>();
    let same_connection = |a: &str, b: &str| connection_keys[a] == connection_keys[b];
    let mut completed = BTreeMap::<String, BatchSource>::new();
    for (index, transfer) in plan.transfers.iter().enumerate() {
        let reused_later = |endpoint: &str| {
            same_connection(endpoint, target)
                || plan.transfers[index + 1..].iter().any(|next| {
                    same_connection(&next.source_engine, endpoint)
                        || next.operation.as_ref().is_some_and(|op| same_connection(&op.engine, endpoint))
                        || next.request.as_ref().is_some_and(|op| same_connection(&op.engine, endpoint))
                })
        };
        let stream = if let Some(request) = transfer
            .request
            .as_ref()
            .filter(|_| transfer.sql.is_empty())
        {
            let requests: RequestStream = Box::pin(futures::stream::iter([Ok(vec![
                request.template.bind(&[])?,
            ])]));
            sessions
                .get_mut(&request.engine)
                .unwrap()
                .execute_requests_stream(requests, transfer.columns.clone())
                .await?
        } else {
            let inputs = streaming_inputs(&transfer.sql, &transfer.source_dialect, &completed)?;
            let input = sessions
                .get_mut(&transfer.source_engine)
                .unwrap()
                .query_stream(&transfer.sql, inputs)
                .await?;
            // A caller-owned connection cannot have two active result cursors.
            // Drain only this explicit reentrancy boundary to bounded disk IPC.
            let same_dependent = transfer
                .operation
                .as_ref()
                .is_some_and(|op| same_connection(&op.engine, &transfer.source_engine));
            let input = if !sessions[&transfer.source_engine].supports_concurrent_statements()
                && (same_dependent || reused_later(&transfer.source_engine))
            {
                BatchSource::spool(input, &options.replay_directory)
                    .await?
                    .open()?
            } else {
                input
            };
            if let Some(request) = &transfer.request {
                let requests =
                    bind_request_stream(input, request.clone(), options.request_batch_size)?;
                sessions
                    .get_mut(&request.engine)
                    .unwrap()
                    .execute_requests_stream(requests, transfer.columns.clone())
                    .await?
            } else if let Some(operation) = &transfer.operation {
                let queries = bind_query_stream(input, operation.clone())?;
                sessions
                    .get_mut(&operation.engine)
                    .unwrap()
                    .query_requests_stream(queries, transfer.columns.clone())
                    .await?
            } else {
                input
            }
        };
        let stream = normalize_transfer_stream(stream, &transfer.columns)?;
        // Replay is explicit and disk backed; never silently repeat remote work.
        let dependent_reentry = transfer.operation.as_ref().is_some_and(|op| {
            !sessions[&op.engine].supports_concurrent_statements() && reused_later(&op.engine)
        });
        let source =
            if dependent_reentry || uses.get(&transfer.target_relation).copied().unwrap_or(0) > 1 {
                BatchSource::spool(stream, &options.replay_directory).await?
            } else {
                BatchSource::once(stream)
            };
        if completed
            .insert(transfer.target_relation.clone(), source)
            .is_some()
        {
            return Err("duplicate transfer relation".into());
        }
    }
    let inputs = streaming_inputs(&plan.sql, &plan.dialect, &completed)?;
    sessions
        .get_mut(target)
        .unwrap()
        .query_stream(&plan.sql, inputs)
        .await
}

fn relation_names(sql: &str, dialect: &str) -> Result<Vec<String>, String> {
    if sql.is_empty() {
        return Ok(vec![]);
    }
    let dialect = crate::execution::SqlDialect::resolve(dialect).map_err(|e| e.to_string())?.parser_dialect();
    let statements = Parser::new(dialect.as_ref())
        .with_recursion_limit(1024)
        .try_with_sql(sql)
        .map_err(|e| e.to_string())?
        .parse_statements()
        .map_err(|e| e.to_string())?;
    let mut names = Vec::new();
    let _ = ast::visit_relations(&statements, |relation| {
        if relation.0.len() == 1 {
            if let Some(name) = relation.0[0].as_ident() {
                names.push(name.value.clone());
            }
        }
        std::ops::ControlFlow::<()>::Continue(())
    });
    Ok(names)
}
fn streaming_inputs(
    sql: &str,
    dialect: &str,
    completed: &BTreeMap<String, crate::execution::stream::BatchSource>,
) -> Result<Vec<crate::execution::stream::InputRelation>, String> {
    let names = relation_names(sql, dialect)?.into_iter().collect::<BTreeSet<_>>();
    Ok(names
        .into_iter()
        .filter_map(|name| {
            completed
                .get(&name)
                .map(|source| crate::execution::stream::InputRelation {
                    name,
                    source: source.clone(),
                })
        })
        .collect())
}
pub(crate) fn transfer_schema(
    columns: &[TransferColumn],
) -> Result<arrow::datatypes::SchemaRef, String> {
    Ok(std::sync::Arc::new(arrow::datatypes::Schema::new(
        columns
            .iter()
            .map(|c| {
                Ok(arrow::datatypes::Field::new(
                    &c.name,
                    crate::compiler::data_type(&c.data_type)?,
                    c.nullable,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?,
    )))
}
pub(crate) fn normalize_transfer_stream(
    stream: crate::execution::stream::BatchStream,
    columns: &[TransferColumn],
) -> Result<crate::execution::stream::BatchStream, String> {
    use futures::StreamExt;
    let schema = transfer_schema(columns)?;
    let output = schema.clone();
    Ok(crate::execution::stream::BatchStream::new(
        output,
        stream.map(move |batch| {
            let batch = batch?;
            if batch.num_columns() != schema.fields().len() {
                return Err("exchange column count mismatch".into());
            }
            let arrays = batch
                .columns()
                .iter()
                .zip(schema.fields())
                .map(|(a, f)| {
                    if a.data_type() == f.data_type() {
                        Ok(a.clone())
                    } else {
                        let cast =
                            arrow::compute::cast(a, f.data_type()).map_err(|e| e.to_string())?;
                        let restored = arrow::compute::cast(&cast, a.data_type())
                            .map_err(|e| e.to_string())?;
                        if restored.as_ref() != a.as_ref() {
                            return Err(format!(
                                "exchange conversion would lose information in column {}",
                                f.name()
                            ));
                        }
                        Ok(cast)
                    }
                })
                .collect::<Result<Vec<_>, String>>()?;
            RecordBatch::try_new_with_options(
                schema.clone(),
                arrays,
                &arrow::record_batch::RecordBatchOptions::new()
                    .with_row_count(Some(batch.num_rows())),
            )
            .map_err(|e| e.to_string())
        }),
    ))
}
fn bound_rows(
    input: crate::execution::stream::BatchStream,
    columns: Vec<TransferColumn>,
) -> Result<
    impl futures::Stream<Item = Result<Vec<datafusion::common::ScalarValue>, String>> + Send + 'static,
    String,
> {
    use futures::StreamExt;
    let types = columns
        .iter()
        .map(|c| crate::compiler::data_type(&c.data_type))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(futures::stream::try_unfold(
        (input, None::<RecordBatch>, 0usize, types),
        |(mut input, mut batch, mut row, types)| async move {
            loop {
                if let Some(current) = &batch {
                    if row < current.num_rows() {
                        if current.num_columns() != types.len() {
                            return Err("request source row width mismatch".into());
                        }
                        let values = current
                            .columns()
                            .iter()
                            .zip(&types)
                            .map(|(a, t)| {
                                exchange_scalar(
                                    datafusion::common::ScalarValue::try_from_array(
                                        a.as_ref(),
                                        row,
                                    )
                                    .map_err(|e| e.to_string())?,
                                    t,
                                )
                            })
                            .collect::<Result<Vec<_>, _>>()?;
                        row += 1;
                        return Ok(Some((values, (input, batch, row, types))));
                    }
                }
                batch = match input.next().await {
                    Some(batch) => Some(batch?),
                    None => return Ok(None),
                };
                row = 0;
            }
        },
    ))
}
fn bind_request_stream(
    input: crate::execution::stream::BatchStream,
    operation: RequestOperation,
    batch_size: usize,
) -> Result<RequestStream, String> {
    use futures::StreamExt;
    let rows = Box::pin(bound_rows(input, operation.input_columns)?);
    Ok(Box::pin(
        rows.map(move |values| operation.template.bind(&values?))
            .chunks(batch_size)
            .map(|batch| batch.into_iter().collect()),
    ))
}
fn bind_query_stream(
    input: crate::execution::stream::BatchStream,
    operation: DependentOperation,
) -> Result<QueryStream, String> {
    use futures::StreamExt;
    Ok(Box::pin(bound_rows(input, operation.input_columns)?.map(
        move |values| operation.template.bind(&values?).map_err(|e| e.to_string()),
    )))
}

/// Lossless JSON representation used inside PostgreSQL child-list cells.
pub(crate) fn scalar_json(
    value: &datafusion::common::ScalarValue,
) -> datafusion::common::Result<serde_json::Value> {
    use datafusion::common::ScalarValue;
    if value.is_null() {
        return Ok(serde_json::Value::Null);
    }
    if crate::ir::functions::domain::is_json(&value.data_type()) {
        return Ok(crate::ir::functions::domain::json_text(value)?
            .map(serde_json::Value::String)
            .unwrap_or(serde_json::Value::Null));
    }
    Ok(match value {
        ScalarValue::Struct(a) => serde_json::Value::Object(
            a.fields()
                .iter()
                .zip(a.columns())
                .map(|(f, a)| {
                    Ok((
                        f.name().clone(),
                        scalar_json(&ScalarValue::try_from_array(a, 0)?)?,
                    ))
                })
                .collect::<datafusion::common::Result<_>>()?,
        ),
        ScalarValue::List(a) => {
            let values = a.value(0);
            serde_json::Value::Array(
                (0..values.len())
                    .map(|i| scalar_json(&ScalarValue::try_from_array(values.as_ref(), i)?))
                    .collect::<datafusion::common::Result<Vec<_>>>()?,
            )
        }
        ScalarValue::Utf8(Some(s))
        | ScalarValue::LargeUtf8(Some(s))
        | ScalarValue::Utf8View(Some(s)) => s.clone().into(),
        ScalarValue::Binary(Some(bytes)) | ScalarValue::LargeBinary(Some(bytes)) => format!(
            "\\x{}",
            bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
        .into(),
        _ => {
            let array = value.to_array_of_size(1)?;
            let text = arrow::util::display::array_value_to_string(array.as_ref(), 0)?;
            serde_json::from_str(&text).unwrap_or_else(|_| text.into())
        }
    })
}

// PostgreSQL ADBC drivers transport JSONB cells as UTF-8 Arrow values.
// Decode them against the declared exchange type, without guessing a schema.
fn exchange_scalar(
    value: datafusion::common::ScalarValue,
    ty: &arrow::datatypes::DataType,
) -> Result<datafusion::common::ScalarValue, String> {
    use datafusion::common::ScalarValue;
    if value.is_null() {
        return ScalarValue::try_from(ty).map_err(|e| e.to_string());
    }
    if value.data_type() == *ty {
        return Ok(value);
    }
    if crate::ir::functions::domain::descriptor(ty).is_some() {
        let array = crate::ir::functions::domain::restore(
            &value.to_array_of_size(1).map_err(|e| e.to_string())?,
            ty,
        )
        .map_err(|e| e.to_string())?;
        return ScalarValue::try_from_array(&array, 0).map_err(|e| e.to_string());
    }
    if let arrow::datatypes::DataType::List(field) = ty {
        if let ScalarValue::Utf8(Some(text))
        | ScalarValue::LargeUtf8(Some(text))
        | ScalarValue::Utf8View(Some(text)) = &value
        {
            return json_scalar(
                &serde_json::from_str(text).map_err(|e| format!("invalid JSON list: {e}"))?,
                ty,
            );
        }
        let items = match &value {
            ScalarValue::List(a) => Some(a.value(0)),
            ScalarValue::LargeList(a) => Some(a.value(0)),
            ScalarValue::FixedSizeList(a) => Some(a.value(0)),
            _ => None,
        };
        if let Some(items) = items {
            let values = (0..items.len())
                .map(|i| {
                    exchange_scalar(
                        ScalarValue::try_from_array(items.as_ref(), i)
                            .map_err(|e| e.to_string())?,
                        field.data_type(),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(ScalarValue::List(ScalarValue::new_list(
                &values,
                field.data_type(),
                true,
            )));
        }
    }
    value.cast_to(ty).map_err(|e| e.to_string())
}

#[cfg(test)]
mod audit_regressions {
    #[test]
    fn exchange_discovery_preserves_trino_compound_casts() {
        let sql = "SELECT CAST(xs AS ARRAY(ROW(k BIGINT, v MAP(VARCHAR, BIGINT)))) FROM incoming";
        assert_eq!(super::relation_names(sql, "trino").unwrap(), vec!["incoming"]);
    }

    use super::json_scalar;
    use arrow::datatypes::{DataType, Field};
    use datafusion::common::ScalarValue;
    use serde_json::json;
    use std::sync::Arc;
    #[test]
    fn domain_and_nested_json_exchange_preserve_types_and_nulls() {
        use crate::ir::functions::domain;
        let document = domain::json_scalar(r#"{"n":9007199254740993}"#).unwrap();
        let json_null = domain::json_scalar("null").unwrap();
        let sql_null = ScalarValue::try_from(&domain::json_type()).unwrap();
        let list = ScalarValue::List(ScalarValue::new_list(
            &[document.clone(), json_null, sql_null],
            &domain::json_type(),
            true,
        ));
        // Preserve declared field order, including nested domains, rather than
        // reconstructing the schema from JSON object key ordering.
        let fields = vec![
            Field::new("z_document", domain::json_type(), true),
            Field::new("a_items", list.data_type(), true),
        ]
        .into();
        let record = ScalarValue::Struct(Arc::new(
            arrow::array::StructArray::try_new(
                fields,
                vec![
                    document.to_array_of_size(1).unwrap(),
                    list.to_array_of_size(1).unwrap(),
                ],
                None,
            )
            .unwrap(),
        ));
        let geometry =
            domain::scalar("geometry", ScalarValue::Binary(Some(vec![0, 255, 128]))).unwrap();
        for value in [record, geometry] {
            let ty = value.data_type();
            assert_eq!(
                crate::compiler::data_type(&super::type_name(&ty).unwrap()).unwrap(),
                ty
            );
            let encoded = super::scalar_json(&value).unwrap();
            assert_eq!(json_scalar(&encoded, &ty).unwrap(), value);
        }
    }

    #[test]
    fn audit_large_binary_decodes_bytes() {
        for (input, expected) in [
            (json!("\\x00ff80"), vec![0, 255, 128]),
            (json!("AP+A"), vec![0, 255, 128]),
            (json!("\\x"), vec![]),
        ] {
            assert_eq!(
                json_scalar(&input, &DataType::LargeBinary).unwrap(),
                ScalarValue::LargeBinary(Some(expected))
            );
        }
        assert_eq!(
            json_scalar(&json!(null), &DataType::LargeBinary).unwrap(),
            ScalarValue::LargeBinary(None)
        );
    }
    #[test]
    fn audit_large_list_decodes_nested_empty_and_null() {
        let inner = DataType::LargeList(Arc::new(Field::new("item", DataType::Int64, true)));
        let outer = DataType::LargeList(Arc::new(Field::new("item", inner.clone(), true)));
        let expected = ScalarValue::LargeList(ScalarValue::new_large_list(
            &[
                ScalarValue::LargeList(ScalarValue::new_large_list(
                    &[ScalarValue::Int64(Some(1)), ScalarValue::Int64(None)],
                    &DataType::Int64,
                )),
                ScalarValue::LargeList(ScalarValue::new_large_list(&[], &DataType::Int64)),
                ScalarValue::try_from(&inner).unwrap(),
            ],
            &inner,
        ));
        assert_eq!(
            json_scalar(&json!([[1, null], [], null]), &outer).unwrap(),
            expected
        );
        assert_eq!(
            json_scalar(&json!("[[1,null],[],null]"), &outer).unwrap(),
            expected
        );
        assert_eq!(
            json_scalar(&json!(null), &outer).unwrap(),
            ScalarValue::try_from(&outer).unwrap()
        );
    }
}

/// Bind source-row values for a dependent relational operation. The returned SQL
/// statements or request payloads execute on `engine`; their concatenated results
/// complete the named transfer.
pub fn bind_operation_command(command: serde_json::Value) -> Result<serde_json::Value, String> {
    if let Some(ipc) = command["ipc"].as_str() {
        use base64::Engine;
        if command.get("rows").is_some() {
            return Err("provide rows or IPC, not both".into());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(ipc)
            .map_err(|e| e.to_string())?;
        let batches = arrow::ipc::reader::StreamReader::try_new(std::io::Cursor::new(bytes), None)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        return bind_operation_batches(command, &batches);
    }
    let plan = &command["plan"];
    if plan["version"] != 1 {
        return Err("unsupported operation plan version".into());
    }
    let name = command["relation"]
        .as_str()
        .ok_or("missing operation relation")?;
    let transfer = plan["transfers"]
        .as_array()
        .ok_or("missing transfers")?
        .iter()
        .find(|t| t["target_relation"].as_str() == Some(name))
        .ok_or("unknown operation relation")?;
    let transfer: Transfer = serde_json::from_value(transfer.clone()).map_err(|e| e.to_string())?;
    if let Some(operation) = &transfer.request {
        if transfer.operation.is_some() {
            return Err("transfer cannot contain both SQL and request operations".into());
        }
        let types = operation
            .input_columns
            .iter()
            .map(|c| crate::compiler::data_type(&c.data_type))
            .collect::<Result<Vec<_>, _>>()?;
        let rows = command["rows"].as_array().ok_or("missing source rows")?;
        let mut requests = vec![];
        for row in rows {
            let row = row.as_array().ok_or("source row must be an array")?;
            if row.len() != types.len() {
                return Err("operation source row width mismatch".into());
            }
            let values = row
                .iter()
                .zip(&types)
                .map(|(value, ty)| json_scalar(value, ty))
                .collect::<Result<Vec<_>, _>>()?;
            requests.push(operation.template.bind(&values)?);
        }
        return Ok(
            serde_json::json!({"version":1,"engine":operation.engine,"relation":name,"adapter":operation.template.adapter,"requests":requests,"columns":transfer.columns}),
        );
    }
    let search = transfer
        .operation
        .ok_or("transfer is not a dependent operation")?;
    let types = search
        .input_columns
        .iter()
        .map(|c| crate::compiler::data_type(&c.data_type))
        .collect::<Result<Vec<_>, _>>()?;
    let rows = command["rows"].as_array().ok_or("missing source rows")?;
    let mut statements = vec![];
    for row in rows {
        let row = row.as_array().ok_or("source row must be an array")?;
        if row.len() != types.len() {
            return Err("operation source row width mismatch".into());
        }
        let values = row
            .iter()
            .zip(&types)
            .map(|(v, t)| json_scalar(v, t))
            .collect::<Result<Vec<_>, _>>()?;
        statements.push(search.template.bind(&values).map_err(|e| e.to_string())?);
    }
    Ok(
        serde_json::json!({"version":1,"engine":search.engine,"relation":name,"dialect":search.template.dialect,"sql":statements}),
    )
}

/// Bind operation parameters directly from Arrow, respecting their declared
/// domain types rather than the physical JSON/string representation of a driver.
pub fn bind_operation_batches(
    mut command: serde_json::Value,
    batches: &[RecordBatch],
) -> Result<serde_json::Value, String> {
    let name = command["relation"]
        .as_str()
        .ok_or("missing operation relation")?;
    let transfer = command["plan"]["transfers"]
        .as_array()
        .ok_or("missing transfers")?
        .iter()
        .find(|t| t["target_relation"].as_str() == Some(name))
        .ok_or("unknown operation relation")?;
    let transfer: Transfer = serde_json::from_value(transfer.clone()).map_err(|e| e.to_string())?;
    let columns = if let Some(op) = &transfer.request {
        &op.input_columns
    } else if let Some(op) = &transfer.operation {
        &op.input_columns
    } else {
        return Err("transfer is not a dependent operation".into());
    };
    let types = columns
        .iter()
        .map(|c| crate::compiler::data_type(&c.data_type))
        .collect::<Result<Vec<_>, _>>()?;
    let mut rows = vec![];
    for batch in batches {
        if batch.num_columns() != types.len() {
            return Err("operation source row width mismatch".into());
        }
        for row in 0..batch.num_rows() {
            rows.push(
                batch
                    .columns()
                    .iter()
                    .zip(&types)
                    .map(|(a, ty)| {
                        let value = datafusion::common::ScalarValue::try_from_array(a, row)
                            .map_err(|e| e.to_string())?;
                        let value = exchange_scalar(value, ty)?;
                        scalar_json(&value).map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            );
        }
    }
    command
        .as_object_mut()
        .ok_or("invalid operation command")?
        .remove("ipc");
    command["rows"] = serde_json::json!(rows);
    bind_operation_command(command)
}

/// Compatibility name for protocol-v1 clients.
pub type DependentSearch = DependentOperation;

/// Compatibility entry point; dependent operations are not limited to search.
pub fn bind_search_command(command: serde_json::Value) -> Result<serde_json::Value, String> {
    bind_operation_command(command)
}
