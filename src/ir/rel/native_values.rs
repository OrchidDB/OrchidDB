//! Lossless host scalar boundary. Expressions use the shared evaluator and
//! values use the existing snapshot codec; SQL owns scans and row operators.
use super::*;
use crate::ir::catalog::snapshot::binary;
use base64::{Engine, engine::general_purpose::STANDARD};
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use std::any::Any;

mod elements;
mod rows;
pub use rows::{decode_rows, result_batch, result_schema};
use elements::Context;
pub const FIELD: &str = "__orchiddb_value_v1";
pub const VALUE: &str = "__orchiddb_value";
pub const PREDICATE: &str = "__orchiddb_predicate";
pub const JSON: &str = "__orchiddb_value_json";
pub const ITEMS: &str = "__orchiddb_value_items";
pub const KEY: &str = "__orchiddb_value_key";
pub const PROJECT: &str = "__orchiddb_value_project";
pub const PROCEDURE: &str = "__orchiddb_value_procedure";
pub const SORT: &str = "__orchiddb_value_sort";
pub const AGGREGATE: &str = "__orchiddb_value_aggregate";
pub(crate) fn value_type() -> DataType {
    DataType::Struct(vec![Field::new(FIELD, DataType::Utf8, true)].into())
}
pub fn is_value(ty: &DataType) -> bool {
    ty == &value_type()
}
fn encode(value: &Value) -> String {
    STANDARD.encode(binary::encode_value_bytes(value))
}
fn decode(text: &str) -> Result<Value, String> {
    binary::decode_value_bytes(&STANDARD.decode(text).map_err(|e| e.to_string())?)
}
pub(super) fn literal(value: &Value) -> RelResult<Expr> {
    if matches!(value, Value::Null) {
        return Ok(lit(ScalarValue::try_from(&value_type())?));
    }
    Ok(df_core::named_struct(vec![lit(FIELD), lit(encode(value))]))
}
fn from_json(value: &serde_json::Value, context: &mut Context) -> Result<Value, String> {
    Ok(match value {
        serde_json::Value::Object(map) if map.len() == 1 && map.contains_key(elements::FIELD) => {
            let descriptor = from_json(&map[elements::FIELD], context)?;
            context.attach(descriptor)?
        }
        serde_json::Value::Object(map)
            if map.len() == 1 && map.contains_key("__orchiddb_number") =>
        {
            let (kind, text) = (
                map["__orchiddb_number"][0]
                    .as_str()
                    .ok_or("Missing numeric type")?,
                map["__orchiddb_number"][1]
                    .as_str()
                    .ok_or("Missing numeric value")?,
            );
            match kind {
                "TINYINT" => Value::Byte(text.parse().map_err(|_| "Invalid byte")?),
                "SMALLINT" => Value::Short(text.parse().map_err(|_| "Invalid short")?),
                "INTEGER" => Value::Int(text.parse().map_err(|_| "Invalid integer")?),
                "BIGINT" => Value::Long(text.parse().map_err(|_| "Invalid long")?),
                "HUGEINT" => match text.parse::<i64>() {
                    Ok(value) => Value::Long(value),
                    Err(_) => Value::BigInt(text.parse().map_err(|_| "Invalid huge integer")?),
                },
                "FLOAT" => Value::Float32(text.parse().map_err(|_| "Invalid float")?),
                "DOUBLE" => Value::Float(text.parse().map_err(|_| "Invalid double")?),
                _ => return Err("Unsupported numeric type".into()),
            }
        }
        serde_json::Value::Object(map)
            if map.len() == 1 && map.contains_key("__orchiddb_float") =>
        {
            Value::Float(match map["__orchiddb_float"].as_str() {
                Some("NaN") => f64::NAN,
                Some("Infinity") => f64::INFINITY,
                Some("-Infinity") => f64::NEG_INFINITY,
                _ => return Err("Invalid native float".into()),
            })
        }
        serde_json::Value::Object(map) if map.len() == 1 && map.contains_key(FIELD) => {
            match map[FIELD].as_str() {
                Some(text) => context.decode(text)?,
                None if map[FIELD].is_null() => Value::Null,
                _ => return Err("Invalid native value storage".into()),
            }
        }
        serde_json::Value::Object(map) => Value::Map(
            map.iter()
                .map(|(k, v)| Ok((k.clone(), from_json(v, context)?)))
                .collect::<Result<_, String>>()?,
        ),
        serde_json::Value::Array(items) => Value::List(
            items
                .iter()
                .map(|v| from_json(v, context))
                .collect::<Result<_, _>>()?,
        ),
        _ => crate::compiler::parameter(value)?,
    })
}
/// Export typed values using the same client protocol as the native runtime.
pub fn json(value: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut context = Context::default();
    let value = from_json(value, &mut context)?;
    Ok(crate::ir::runtime::output::gremlin_typed_value(
        &value,
        &context.graph,
    ))
}
/// Equality keys reuse the language runtime's identity rules, excluding
/// detached property payloads and normalizing Cypher numeric equivalence.
pub fn key(input: &serde_json::Value, cypher: bool) -> Result<String, String> {
    let mut context = Context::default();
    let value = from_json(input, &mut context)?;
    let bytes = if cypher {
        crate::ir::runtime::ops::distinct::encode_cypher_equivalence(&value)
    } else {
        crate::ir::runtime::ops::distinct::encode_value(&value)
    };
    Ok(STANDARD.encode(bytes))
}

pub(super) fn key_expr(value: Expr, cypher: bool) -> Expr {
    function(KEY, DataType::Utf8, vec![value, lit(cypher)])
}

pub fn evaluate(
    expression: &str,
    bindings: &serde_json::Value,
    predicate: bool,
    statement_micros: i64,
    transaction_micros: i64,
) -> Result<serde_json::Value, crate::ir::diagnostics::QueryExecutionError> {
    let expr: IrExpr = serde_json::from_str(expression).map_err(|e| e.to_string())?;
    let mut context = Context::default();
    let Value::Map(bindings) = from_json(bindings, &mut context)? else {
        return Err("Native expression bindings must be a struct".into());
    };
    let graph = &context.graph;
    graph.set_host_clocks(
        chrono::DateTime::from_timestamp_micros(statement_micros)
            .ok_or("Invalid statement clock")?,
        chrono::DateTime::from_timestamp_micros(transaction_micros)
            .ok_or("Invalid transaction clock")?,
    );
    let value = eval_scalar(&expr, &KernelRow { bindings, bulk: 1 }, &graph)
        .map_err(crate::ir::diagnostics::QueryExecutionError::from_error)?;
    if predicate {
        return match value {
            Value::Null => Ok(serde_json::Value::Null),
            Value::Bool(v) => Ok(v.into()),
            _ => Err("Expected a boolean predicate".into()),
        };
    }
    Ok(context.wire(&value))
}

/// A current projection returns zero or one values. An array containing SQL
/// null is productive; an empty array drops the input traverser. The shared
/// kernel owns that distinction, including Map.get and element properties.
pub fn project(
    expression: &str,
    bindings: &serde_json::Value,
    statement_micros: i64,
    transaction_micros: i64,
) -> Result<serde_json::Value, crate::ir::diagnostics::QueryExecutionError> {
    let expr: IrExpr = serde_json::from_str(expression).map_err(|e| e.to_string())?;
    let mut context = Context::default();
    let Value::Map(bindings) = from_json(bindings, &mut context)? else {
        return Err("Native projection bindings must be a struct".into());
    };
    context.graph.set_host_clocks(
        chrono::DateTime::from_timestamp_micros(statement_micros).ok_or("Invalid statement clock")?,
        chrono::DateTime::from_timestamp_micros(transaction_micros).ok_or("Invalid transaction clock")?,
    );
    let rows = crate::ir::runtime::ops::project::current_project_op(
        &expr, vec![KernelRow { bindings, bulk: 1 }], &context.graph,
    ).map_err(crate::ir::diagnostics::QueryExecutionError::from_error)?;
    Ok(serde_json::Value::Array(rows.iter().map(|row| context.wire(&row.get("current"))).collect()))
}

impl LoweringContext<'_> {
    pub(super) fn native_current_projection(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        Ok(function(PROJECT, DataType::List(Arc::new(Field::new("item", value_type(), true))), vec![
            lit(serde_json::to_string(expr).map_err(|e| RelError::Unsupported(e.to_string()))?),
            self.native_bindings(plan, expr)?,
        ]))
    }
}

pub fn items(input: &serde_json::Value) -> Result<serde_json::Value, String> {
    let mut context = Context::default();
    let value = from_json(input, &mut context)?;
    let rows = crate::ir::runtime::ops::unwind::unwind_op(
        &IrExpr::binding("value"), "item", false,
        vec![KernelRow::new().with("value", value)], &context.graph,
    ).map_err(|error| error.to_string())?;
    Ok(serde_json::Value::Array(rows.iter().map(|row| context.wire(&row.get("item"))).collect()))
}

pub fn aggregate(
    specification: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let (agg, cypher): (AggCall, bool) =
        serde_json::from_str(specification).map_err(|e| e.to_string())?;
    let mut context = Context::default();
    let values = match from_json(input, &mut context)? {
        Value::Null => vec![],
        Value::List(items) => items,
        _ => return Err("Expected aggregate list".into()),
    };
    let rows = values
        .into_iter()
        .map(|value| KernelRow::new().with("value", value))
        .collect();
    let alias=agg.alias.clone();
    let kind=agg.kind;
    let rows = crate::ir::runtime::ops::aggregate::aggregate_op_with_cypher_equivalence(
        &[],
        &[agg],
        rows,
        &context.graph,
        cypher,
    )
    .map_err(|e| e.to_string())?;
    let value=rows.first().map(|row|row.get(&alias)).unwrap_or(Value::Null);
    let value=match (kind,alias.as_str()) {
        (AggKind::CollectTraversers,"__group_flatten_value")=>crate::ir::runtime::ops::aggregate::flatten_group_lists(value),
        (AggKind::CollectTraversers,"__group_unwrap_value")=>crate::ir::runtime::ops::aggregate::unwrap_single_group_value(value),
        _=>value,
    };
    Ok(context.wire(&value))
}
pub(super) fn predicate(expr: Expr) -> Expr {
    function(
        PREDICATE,
        DataType::Boolean,
        vec![
            lit(r#"{"Binding":"value"}"#),
            df_core::named_struct(vec![lit("value"), expr]),
        ],
    )
}
pub(super) fn list(expr: Expr) -> Expr {
    function(
        ITEMS,
        DataType::List(Arc::new(Field::new("item", value_type(), true))),
        vec![expr],
    )
}
pub(super) fn aggregate_expr(agg: &AggCall, arg: Expr, cypher: bool) -> RelResult<Expr> {
    let mut spec = agg.clone();
    spec.arg = Some(IrExpr::binding("value"));
    Ok(function(
        AGGREGATE,
        value_type(),
        vec![
            lit(serde_json::to_string(&(spec, cypher))
                .map_err(|e| RelError::Unsupported(e.to_string()))?),
            df_array_agg(arg),
        ],
    ))
}
fn function(name: &str, result: DataType, args: Vec<Expr>) -> Expr {
    ScalarUDF::from(NativeFunction {
        name: name.into(),
        result,
        signature: Signature::any(args.len(), Volatility::Volatile),
    })
    .call(args)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SortSpecification {
    keys: Vec<SortKey>,
    scopes: Vec<String>,
}

/// Return a permutation, so the host retains the original SQL row types.
pub fn sort(specification: &str, input: &serde_json::Value) -> Result<Vec<usize>, String> {
    let specification: SortSpecification = serde_json::from_str(specification).map_err(|e| e.to_string())?;
    let mut context = Context::default();
    let values = match from_json(input, &mut context)? {
        Value::Null => vec![],
        Value::List(items) => items,
        _ => return Err("Expected sort rows".into()),
    };
    let mut index = "__orchid_row_index".to_string();
    while values
        .iter()
        .any(|value| matches!(value,Value::Map(map) if map.contains_key(&index)))
    {
        index.push('_');
    }
    let rows = values
        .into_iter()
        .enumerate()
        .map(|(i, value)| {
            let Value::Map(mut bindings) = value else {
                return Err("Expected sort row struct".to_string());
            };
            // Reconstruct only the logical bindings referenced by sort keys.
            // The original expressions retain their language comparator tags.
            for scope in &specification.scopes {
                let Some(Value::Map(captured)) = bindings.remove(scope) else {
                    return Err("Missing sort binding scope".to_string());
                };
                bindings.extend(captured);
            }
            bindings.insert(index.clone(), Value::Int(i as i64));
            Ok(KernelRow { bindings, bulk: 1 })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let rows = crate::ir::runtime::ops::sort::sort_op(&specification.keys, rows, &context.graph)
        .map_err(|e| e.to_string())?;
    rows.into_iter()
        .map(|row| match row.get(&index) {
            Value::Int(i) => Ok(i as usize),
            _ => Err("Missing sort row index".into()),
        })
        .collect()
}
impl LoweringContext<'_> {
    pub(super) fn native_sort(
        &mut self,
        input: LoweredNode,
        keys: &[SortKey],
    ) -> RelResult<LoweredNode> {
        let names = output_fields(&input.plan);
        let mut packed = names.iter().flat_map(|name| [lit(name.clone()), col_exact(name)]).collect::<Vec<_>>();
        let mut scopes = Vec::new();
        for (i, key) in keys.iter().enumerate() {
            let mut scope = format!("__orchid_sort_scope_{i}");
            while names.contains(&scope) { scope.push('_'); }
            packed.extend([lit(scope.clone()), self.native_bindings(&input.plan, &key.expr)?]);
            scopes.push(scope);
        }
        let packed = df_core::named_struct(packed);
        let specification = SortSpecification { keys: keys.to_vec(), scopes };
        let list_type = DataType::List(Arc::new(Field::new(
            "item",
            packed.get_type(input.plan.schema())?,
            true,
        )));
        let alias = format!("__orchid_sorted_{}", self.scan_counter);
        self.scan_counter += 1;
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .aggregate(Vec::<Expr>::new(), vec![df_array_agg(packed).alias(&alias)])?
            .build()?;
        let sorted = function(
            SORT,
            list_type,
            vec![
                lit(serde_json::to_string(&specification)
                    .map_err(|e| RelError::Unsupported(e.to_string()))?),
                col_exact(&alias),
            ],
        )
        .alias(&alias);
        let plan = LogicalPlanBuilder::from(plan)
            .project(vec![sorted])?
            .build()?;
        let plan = collections::unnest_scope(
            plan,
            format!("__w_sql_cte_native_sort_{}", self.scan_counter),
        )?;
        let plan = collections::unnest_input(plan)?;
        let plan = LogicalPlanBuilder::from(plan)
            .unnest_column(Column::new_unqualified(&alias))?
            .build()?;
        let plan = collections::unnest_scope(
            plan,
            format!("__w_sql_cte_native_sorted_{}", self.scan_counter),
        )?;
        let plan = LogicalPlanBuilder::from(plan)
            .project(
                names
                    .iter()
                    .map(|name| df_core::get_field(col_exact(&alias), name.as_str()).alias(name))
                    .collect::<Vec<_>>(),
            )?
            .build()?;
        Ok(input.with_plan(plan))
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct ProcedureSpec {
    name: String,
    signature: crate::ir::procedures::ProcedureSignature,
    rows: Vec<Vec<String>>,
    args: Vec<crate::ir::plan::ProcedureArg>,
    yields: Vec<String>,
}
pub fn procedure(
    specification: &str,
    input: &serde_json::Value,
) -> Result<serde_json::Value, crate::ir::diagnostics::QueryExecutionError> {
    let spec: ProcedureSpec = serde_json::from_str(specification).map_err(|e| e.to_string())?;
    let procedure = crate::ir::procedures::TableProcedure {
        signature: spec.signature,
        rows: spec
            .rows
            .iter()
            .map(|row| row.iter().map(|value| decode(value)).collect())
            .collect::<Result<_, _>>()?,
    };
    procedure.validate()?;
    let mut context = Context::default();
    context.graph.procedures = Arc::new(BTreeMap::from([(spec.name.clone(), procedure)]));
    let Value::Map(bindings) = from_json(input, &mut context)? else {
        return Err("Expected procedure arguments".into());
    };
    let rows = crate::ir::runtime::context::procedure_call_op(
        &spec.name,
        &spec.args,
        &spec.yields,
        vec![KernelRow { bindings, bulk: 1 }],
        &context.graph,
    )
    .map_err(crate::ir::diagnostics::QueryExecutionError::from_error)?;
    Ok(serde_json::Value::Array(
        rows.into_iter()
            .map(|row| {
                context.wire(&Value::Map(
                    spec.yields
                        .iter()
                        .map(|name| (name.clone(), row.get(name)))
                        .collect(),
                ))
            })
            .collect(),
    ))
}
impl LoweringContext<'_> {
    pub(super) fn native_procedure(
        &mut self,
        name: &str,
        args: &[crate::ir::plan::ProcedureArg],
        yields: &[String],
        input: Option<&Node>,
    ) -> RelResult<LoweredNode> {
        let procedure = self
            .graph
            .procedures
            .get(name)
            .ok_or_else(|| RelError::Unsupported(format!("Unregistered host procedure {name}")))?
            .clone();
        let input = self.lower_node(input.unwrap_or(&Node::GraphOneRow))?;
        let mut packed = Vec::new();
        let mut call_args = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let name = format!("arg{i}");
            packed.extend([lit(name.clone()), self.lower_expr(&input.plan, &arg.value)?]);
            call_args.push(crate::ir::plan::ProcedureArg {
                name: arg.name.clone(),
                value: IrExpr::binding(name),
            });
        }
        if packed.is_empty() {
            packed.extend([lit("__unit"), lit(true)]);
        }
        let spec = ProcedureSpec {
            name: name.into(),
            signature: procedure.signature,
            rows: procedure
                .rows
                .iter()
                .map(|row| row.iter().map(encode).collect())
                .collect(),
            args: call_args,
            yields: yields.to_vec(),
        };
        let alias = format!("__orchid_procedure_{}", self.scan_counter);
        self.scan_counter += 1;
        let result = function(
            PROCEDURE,
            DataType::List(Arc::new(Field::new("item", value_type(), true))),
            vec![
                lit(serde_json::to_string(&spec)
                    .map_err(|e| RelError::Unsupported(e.to_string()))?),
                df_core::named_struct(packed),
            ],
        );
        let mut columns = existing_columns(&input.plan, &BTreeSet::new());
        columns.push(result.alias(&alias));
        let plan = LogicalPlanBuilder::from(input.plan.clone())
            .project(columns)?
            .build()?;
        let plan = collections::unnest_scope(
            plan,
            format!("__w_sql_cte_procedure_{}", self.scan_counter),
        )?;
        let plan = collections::unnest_input(plan)?;
        let plan = LogicalPlanBuilder::from(plan)
            .unnest_column(Column::new_unqualified(&alias))?
            .build()?;
        let plan = collections::unnest_scope(
            plan,
            format!("__w_sql_cte_procedure_output_{}", self.scan_counter),
        )?;
        let mut columns = existing_columns(&input.plan, &yields.iter().cloned().collect());
        for field in yields {
            let expression = IrExpr::property(
                &alias,
                field,
                crate::ir::policy::PropertyMissing::NullOnMissing,
            );
            columns.push(self.native_expr(&plan, &expression)?.alias(field));
        }
        Ok(input.with_plan(LogicalPlanBuilder::from(plan).project(columns)?.build()?))
    }
}

pub(crate) mod float_codec {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(value.to_bits())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        Ok(f64::from_bits(u64::deserialize(deserializer)?))
    }
}

pub(crate) mod scalar_codec {
    use super::*;
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &ScalarValue, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&encode(&Value::Scalar(value.clone())))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ScalarValue, D::Error> {
        let text = String::deserialize(deserializer)?;
        match decode(&text).map_err(serde::de::Error::custom)? {
            Value::Scalar(value) => Ok(value),
            _ => Err(serde::de::Error::custom("Expected scalar literal")),
        }
    }
}

impl LoweringContext<'_> {
    /// Route an expression as a scalar kernel only when every captured binding
    /// is an ordinary SQL column. Graph access remains in the relational plan.
    pub(super) fn native_bindings(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        fn captures(
            expr: &IrExpr,
            bound: &BTreeSet<String>,
            free: &mut BTreeMap<String, bool>,
        ) -> bool {
            match expr {
                IrExpr::Lit(_) => true,
                IrExpr::Binding(b)
                | IrExpr::IsBound(b)
                | IrExpr::Id(b)
                | IrExpr::Label(b)
                | IrExpr::SimplePath(b)
                | IrExpr::HasLabel { binding: b, .. } => {
                    if !bound.contains(b) {
                        free.insert(b.clone(), true);
                    }
                    true
                }
                IrExpr::Property { binding: b, .. } => {
                    if !bound.contains(b) {
                        free.entry(b.clone()).or_insert(false);
                    }
                    true
                }
                IrExpr::List(items) => items.iter().all(|e| captures(e, bound, free)),
                IrExpr::Call { name, args } => {
                    // Search scores use the existing typed UDF kernels. BM25
                    // additionally needs mapped corpus provenance at planning.
                    if crate::ir::functions::search::function(name).is_some() {
                        return false;
                    }
                    (constant_foldable_function(name)
                        || name.starts_with("path_")
                        || name.starts_with("select_history_")
                        || matches!(
                            name.as_str(),
                            "rand"
                                | "random"
                                | "uuid"
                                | "gen_random_uuid"
                                | "value_map"
                                | "value_map_tokens"
                                | "element_map"
                                | "property_map"
                                | "properties_list"
                                | "cypher_labels"
                                | "cypher_property_map"
                                | "labels"
                                | "element_id"
                                | "element_label"
                                | "requested_property_values"
                                | "gremlin_cast_string"
                                | "gremlin_id"
                        ))
                        && args.iter().all(|e| captures(e, bound, free))
                }
                IrExpr::Binary { lhs, rhs, .. }
                | IrExpr::StringPredicate {
                    target: lhs,
                    pattern: rhs,
                    ..
                } => captures(lhs, bound, free) && captures(rhs, bound, free),
                IrExpr::Not(e) | IrExpr::IsNull(e) | IrExpr::IsNotNull(e) => {
                    captures(e, bound, free)
                }
                IrExpr::Case { arms, otherwise } => {
                    arms.iter()
                        .all(|(a, b)| captures(a, bound, free) && captures(b, bound, free))
                        && otherwise
                            .as_deref()
                            .is_none_or(|e| captures(e, bound, free))
                }
                IrExpr::ListTransform { list, item, map }
                | IrExpr::ListFilter {
                    list,
                    item,
                    predicate: map,
                } => {
                    if !captures(list, bound, free) {
                        return false;
                    }
                    let mut bound = bound.clone();
                    bound.insert(item.clone());
                    captures(map, &bound, free)
                }
                IrExpr::ListReduce {
                    collection,
                    accumulator,
                    item,
                    map,
                } => {
                    if !captures(collection, bound, free) {
                        return false;
                    }
                    let mut bound = bound.clone();
                    bound.insert(item.clone());
                    bound.insert(accumulator.clone());
                    captures(map, &bound, free)
                }
            }
        }
        let mut free = BTreeMap::new();
        if !captures(expr, &BTreeSet::new(), &mut free) {
            return Err(RelError::Unsupported(
                "Expression is not a pure scalar kernel".into(),
            ));
        }
        let mut packed = Vec::new();
        for (binding, whole_value) in free {
            let value = if let Some(column) = resolve_column_name(plan, &binding) {
                col_exact(column)
            } else if whole_value && has_binding_shape(plan, &binding).is_some() {
                self.native_element(plan, &binding)?
            } else if !whole_value && has_binding_shape(plan, &binding).is_some() {
                // Property access needs the owner kind, not just a map of its
                // columns: absent map entries and absent element properties
                // have different productivity semantics.
                self.native_element(plan, &binding)?
            } else if self.language==Language::Gremlin || binding == "__path"
                || binding == "__path_labels"
                || binding.starts_with("__gremlin_select_history_")
            {
                lit(ScalarValue::Null)
            } else {
                return Err(RelError::Unsupported(format!(
                    "Native expression requires scalar binding {binding}"
                )));
            };
            packed.extend([lit(binding), value]);
        }
        if packed.is_empty() {
            packed.extend([lit("__unit"), lit(true)]);
        }
        Ok(df_core::named_struct(packed))
    }

    pub(super) fn native_expr(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        let packed = self.native_bindings(plan, expr)?;
        let boolean = matches!(expr,IrExpr::Call{name,..} if matches!(name.as_str(),"gremlin_compare"|"gremlin_within"|"path_simple"|"path_cyclic"))
            || matches!(
                expr,
                IrExpr::SimplePath(_)
                    | IrExpr::HasLabel { .. }
                    | IrExpr::IsNull(_)
                    | IrExpr::IsNotNull(_)
                    | IrExpr::IsBound(_)
                    | IrExpr::Not(_)
                    | IrExpr::StringPredicate { .. }
                    | IrExpr::Binary {
                        op: BinaryOp::Eq
                            | BinaryOp::Neq
                            | BinaryOp::Lt
                            | BinaryOp::Lte
                            | BinaryOp::Gt
                            | BinaryOp::Gte
                            | BinaryOp::And
                            | BinaryOp::Or,
                        ..
                    }
            );
        Ok(function(
            if boolean { PREDICATE } else { VALUE },
            if boolean {
                DataType::Boolean
            } else {
                value_type()
            },
            vec![
                lit(serde_json::to_string(expr)
                    .map_err(|e| RelError::Unsupported(e.to_string()))?),
                packed,
            ],
        ))
    }
}
#[derive(Debug, PartialEq, Eq, Hash)]
struct NativeFunction {
    name: String,
    result: DataType,
    signature: Signature,
}
impl ScalarUDFImpl for NativeFunction {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _: &[DataType]) -> datafusion::common::Result<DataType> {
        Ok(self.result.clone())
    }
    fn invoke_with_args(&self, _: ScalarFunctionArgs) -> datafusion::common::Result<ColumnarValue> {
        Err(DataFusionError::Execution(
            "Native value function requires the registered host implementation".into(),
        ))
    }
}

#[cfg(test)]
mod tests;
