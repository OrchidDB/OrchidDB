//! JSON-directed execution across caller-owned SQL sessions.
//!
//! Closed SQL islands run on the engine owning their sources. Their results
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

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Transfer {
    pub source_engine: String,
    pub source_dialect: String,
    pub sql: String,
    pub target_relation: String,
    pub columns: Vec<TransferColumn>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransferColumn {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

pub(crate) fn validate(request: &CompileRequest) -> Result<(), String> {
    if request.engines.is_empty() {
        if request.execution_engine.is_some() || request.tables.iter().any(|t| t.engine.is_some()) {
            return Err("table/execution engine requires an engines registry".into());
        }
        return Ok(());
    }
    for (name, engine) in &request.engines {
        if name.is_empty() || !matches!(engine.dialect.as_str(), "postgres" | "duckdb") {
            return Err(format!(
                "invalid engine `{name}` or dialect `{}`",
                engine.dialect
            ));
        }
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
        if !request.engines.contains_key(engine) {
            return Err(format!(
                "unknown engine `{engine}` for table `{}`",
                table.name
            ));
        }
    }
    Ok(())
}

fn parts(name: &str) -> Result<Vec<ast::Ident>, String> {
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
                let dialect = if request.engines[source].dialect == "postgres" {
                    crate::execution::SqlDialect::Postgres
                } else {
                    crate::execution::SqlDialect::DuckDb
                };
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
                        "cannot transfer source `{}` from engine `{owner}`: no supported SQL island/exchange schema", scan.table_name)));
                }
            }
        }
        Ok(TreeNodeRecursion::Continue)
    }).map_err(|e|e.to_string())?;
    Ok((result.data, transfers))
}

pub(crate) fn type_name(ty: &arrow::datatypes::DataType) -> Result<String, String> {
    use arrow::datatypes::DataType::*;
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
        Decimal128(p, s) => format!("decimal:{p}:{s}"),
        List(f) | LargeList(f) => format!("list:{}", type_name(f.data_type())?),
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
    use datafusion::common::ScalarValue;
    let dialect = match dialect {
        "duckdb" => crate::execution::SqlDialect::DuckDb,
        "postgres" => crate::execution::SqlDialect::Postgres,
        _ => return Err("unsupported binding dialect".into()),
    };
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
    let parser_dialect: Box<dyn datafusion::sql::sqlparser::dialect::Dialect> = match dialect {
        crate::execution::SqlDialect::DuckDb => {
            Box::new(datafusion::sql::sqlparser::dialect::DuckDbDialect {})
        }
        crate::execution::SqlDialect::Postgres => {
            Box::new(datafusion::sql::sqlparser::dialect::PostgreSqlDialect {})
        }
    };
    let mut statements =
        Parser::new(parser_dialect.as_ref()).with_recursion_limit(1024)
            .try_with_sql(sql).map_err(|e| e.to_string())?
            .parse_statements().map_err(|e| e.to_string())?;
    if statements.len() != 1 {
        return Err("binding requires exactly one query".into());
    }
    let ast::Statement::Query(query) = &mut statements[0] else {
        return Err("binding requires a read query".into());
    };
    let mut binding =
        Parser::parse_sql(parser_dialect.as_ref(), &prefix).map_err(|e| e.to_string())?;
    let ast::Statement::Query(binding) = binding.remove(0) else {
        unreachable!()
    };
    let mut with = binding.with.unwrap();
    if let Some(existing) = &mut query.with {
        if existing
            .cte_tables
            .iter()
            .any(|c| c.alias.name.value == transfer.target_relation)
        {
            return Err("duplicate exchange binding".into());
        }
        existing.cte_tables.insert(0, with.cte_tables.remove(0));
    } else {
        query.with = Some(with);
    }
    Ok(statements.remove(0).to_string())
}

/// Shared protocol for bindings which transport Arrow IPC or typed JSON rows.
pub fn bind_command(command: serde_json::Value) -> Result<serde_json::Value, String> {
    use base64::Engine;
    use datafusion::common::ScalarValue;
    let mut plan = command.get("plan").cloned().ok_or("missing bind plan")?;
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
        let types = transfer
            .columns
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
        let columns = values
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
            transfer
                .columns
                .iter()
                .zip(types)
                .map(|(c, ty)| arrow::datatypes::Field::new(&c.name, ty, c.nullable))
                .collect::<Vec<_>>(),
        ));
        vec![RecordBatch::try_new(schema, columns).map_err(|e| e.to_string())?]
    };
    let sql = bind_batches(
        plan["sql"].as_str().ok_or("missing SQL")?,
        plan["dialect"].as_str().ok_or("missing dialect")?,
        &transfer,
        &batches,
    )?;
    plan["sql"] = sql.into();
    plan["transfers"].as_array_mut().unwrap().remove(index);
    Ok(plan)
}

pub(crate) fn json_scalar(
    value: &serde_json::Value,
    ty: &arrow::datatypes::DataType,
) -> Result<datafusion::common::ScalarValue, String> {
    use arrow::datatypes::DataType;
    use base64::Engine;
    use datafusion::common::ScalarValue;
    if value.is_null() {
        return ScalarValue::try_from(ty).map_err(|e| e.to_string());
    }
    if let DataType::List(field) = ty {
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
        return Ok(ScalarValue::List(ScalarValue::new_list(
            &values,
            field.data_type(),
            true,
        )));
    }
    if *ty == DataType::Binary {
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
        return Ok(ScalarValue::Binary(Some(bytes)));
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

/// A caller-owned session. Federation only submits SELECT queries; it never
/// imports data into database objects or changes the caller's transaction.
#[async_trait::async_trait(?Send)]
pub trait Session {
    fn dialect(&self) -> &str;
    async fn query(&mut self, sql: &str) -> Result<Vec<RecordBatch>, String>;
}

pub async fn execute(
    plan: &CompiledSql,
    sessions: &mut BTreeMap<String, Box<dyn Session>>,
) -> Result<Vec<RecordBatch>, String> {
    if plan.version != 1 {
        return Err("unsupported compiled SQL version".into());
    }
    let target = plan
        .execution_engine
        .as_ref()
        .ok_or("plan has no execution_engine")?;
    for (id, dialect) in std::iter::once((target, &plan.dialect)).chain(
        plan.transfers
            .iter()
            .map(|t| (&t.source_engine, &t.source_dialect)),
    ) {
        let session = sessions
            .get(id)
            .ok_or_else(|| format!("missing engine `{id}`"))?;
        if session.dialect() != dialect {
            return Err(format!("dialect mismatch for engine `{id}`"));
        }
    }
    let mut sql = plan.sql.clone();
    for transfer in &plan.transfers {
        let batches = sessions
            .get_mut(&transfer.source_engine)
            .unwrap()
            .query(&transfer.sql)
            .await?;
        sql = bind_batches(&sql, &plan.dialect, transfer, &batches)?;
    }
    sessions.get_mut(target).unwrap().query(&sql).await
}

/// Lossless JSON representation used inside PostgreSQL child-list cells.
pub(crate) fn scalar_json(
    value: &datafusion::common::ScalarValue,
) -> datafusion::common::Result<serde_json::Value> {
    use datafusion::common::ScalarValue;
    if value.is_null() {
        return Ok(serde_json::Value::Null);
    }
    Ok(match value {
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
