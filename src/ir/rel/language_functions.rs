//! Optional language kernels shared by DataFusion and registered DuckDB UDFs.
//! Only pure, explicitly supported operations are eligible for SQL placement.
use super::*;
use arrow::array::{Array, BinaryBuilder, StructArray};
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use std::any::Any;

pub const KEY: &str = "__orchiddb_lang_cypher_key";
pub const SPARQL: &str = "__orchiddb_lang_sparql";

pub(crate) fn supported(name: &str) -> bool {
    matches!(
        name,
        KEY | SPARQL
            | "__orchiddb_lang_text"
            | "__orchiddb_lang_texts"
            | "__orchiddb_lang_int"
            | "__orchiddb_lang_ints"
    )
}

/// Decode only types for which the semantic representation is lossless.
fn value(s: ScalarValue) -> datafusion::common::Result<Value> {
    if let Some(v) = crate::ir::value::scalar_semantic_value(&s) {
        return Ok(v);
    }
    Ok(match s {
        ScalarValue::List(a) => Value::List(values(a.value(0).as_ref())?),
        ScalarValue::LargeList(a) => Value::List(values(a.value(0).as_ref())?),
        ScalarValue::FixedSizeList(a) => Value::List(values(a.value(0).as_ref())?),
        ScalarValue::Struct(a) => Value::Map(
            a.fields()
                .iter()
                .zip(a.columns())
                .map(|(f, a)| Ok((f.name().clone(), value(ScalarValue::try_from_array(a, 0)?)?)))
                .collect::<datafusion::common::Result<_>>()?,
        ),
        s => {
            return Err(DataFusionError::Execution(format!(
                "unsupported language value {}",
                s.data_type()
            )));
        }
    })
}
fn values(a: &dyn Array) -> datafusion::common::Result<Vec<Value>> {
    (0..a.len())
        .map(|i| value(ScalarValue::try_from_array(a, i)?))
        .collect()
}
pub(crate) fn key_type(t: &DataType) -> bool {
    match t {
        DataType::Null
        | DataType::Boolean
        | DataType::Int8
        | DataType::Int16
        | DataType::Int32
        | DataType::Int64
        | DataType::UInt8
        | DataType::UInt16
        | DataType::UInt32
        | DataType::UInt64
        | DataType::Float32
        | DataType::Float64
        | DataType::Utf8
        | DataType::LargeUtf8
        | DataType::Utf8View
        | DataType::Decimal128(..) => true,
        DataType::List(f) | DataType::LargeList(f) => key_type(f.data_type()),
        // Domain structs carry their own equality contract and are excluded.
        DataType::Struct(fs) => {
            crate::ir::functions::domain::descriptor(t).is_none()
                && fs
                    .iter()
                    .all(|f| !f.name().starts_with("__") && key_type(f.data_type()))
        }
        _ => false,
    }
}
pub(super) fn key(expr: Expr, plan: &LogicalPlan) -> RelResult<Expr> {
    if !key_type(&expr.get_type(plan.schema())?) {
        return Err(RelError::Unsupported(
            "Cypher key type requires native values".into(),
        ));
    }
    Ok(udf(KEY, DataType::Binary, 1).call(vec![expr]))
}
fn list_type(t: DataType) -> DataType {
    DataType::List(Arc::new(Field::new("item", t, true)))
}

/// Typed transport keeps null arguments inside a non-null struct: DuckDB's
/// default null propagation must not erase Gremlin concat/split semantics.
pub(super) fn gremlin(name: &str, args: Vec<Expr>, plan: &LogicalPlan) -> RelResult<Expr> {
    let op = name
        .strip_prefix("gremlin_string_")
        .ok_or_else(|| RelError::Unsupported(name.into()))?;
    let scalar = op.strip_prefix("local_").unwrap_or(op);
    let expected = match scalar {
        "substring" => 2..=3,
        "replace" => 3..=3,
        "split" | "conjoin" => 2..=2,
        "concat" => 1..=usize::MAX,
        "length" | "lcase" | "ucase" | "trim" | "ltrim" | "rtrim" | "reverse" | "split_ws" => 1..=1,
        _ => return Err(RelError::Unsupported(name.into())),
    };
    if !expected.contains(&args.len()) {
        return Err(RelError::Unsupported(format!("invalid arity for {name}")));
    }
    let types = args
        .iter()
        .map(|e| e.get_type(plan.schema()))
        .collect::<datafusion::common::Result<Vec<_>>>()?;
    let text = |t: &DataType| {
        matches!(
            t,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null
        )
    };
    let local_list =
        op.starts_with("local_") && matches!(&types[0], DataType::List(f) if text(f.data_type()));
    let conjoin = scalar == "conjoin";
    if !(text(&types[0])
        || local_list
        || conjoin && matches!(&types[0], DataType::List(f) if text(f.data_type())))
    {
        return Err(RelError::Unsupported(format!("{name} argument type")));
    }
    if local_list && matches!(scalar, "split" | "split_ws" | "conjoin") {
        return Err(RelError::Unsupported(
            "nested Gremlin output requires native values".into(),
        ));
    }
    for t in types.iter().skip(1) {
        if if scalar == "substring" {
            !matches!(t, DataType::Int32 | DataType::Int64 | DataType::Null)
        } else {
            !text(t)
        } {
            return Err(RelError::Unsupported(format!("{name} argument type")));
        }
    }
    let (function, result) = match (
        scalar == "length",
        local_list || matches!(scalar, "split" | "split_ws"),
    ) {
        (true, false) => ("__orchiddb_lang_int", DataType::Int32),
        (true, true) => ("__orchiddb_lang_ints", list_type(DataType::Int32)),
        (false, false) => ("__orchiddb_lang_text", DataType::Utf8),
        (false, true) => ("__orchiddb_lang_texts", list_type(DataType::Utf8)),
    };
    let packed = df_core::named_struct(
        args.into_iter()
            .enumerate()
            .flat_map(|(i, e)| [lit(format!("a{i}")), e])
            .collect(),
    );
    Ok(udf(function, result, 2).call(vec![lit(op.to_string()), packed]))
}
fn udf(name: &str, result: DataType, arity: usize) -> ScalarUDF {
    ScalarUDF::from(LanguageFunction {
        name: name.into(),
        result,
        signature: Signature::any(arity, Volatility::Immutable),
    })
}
#[derive(Debug, PartialEq, Eq, Hash)]
struct LanguageFunction {
    name: String,
    result: DataType,
    signature: Signature,
}
impl ScalarUDFImpl for LanguageFunction {
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
    fn invoke_with_args(
        &self,
        args: ScalarFunctionArgs,
    ) -> datafusion::common::Result<ColumnarValue> {
        let arrays = args
            .args
            .iter()
            .map(|a| a.to_array(args.number_rows))
            .collect::<datafusion::common::Result<Vec<_>>>()?;
        Ok(ColumnarValue::Array(evaluate(
            &self.name,
            &arrays,
            args.number_rows,
        )?))
    }
}
fn scalar(v: Value, ty: &DataType) -> datafusion::common::Result<ScalarValue> {
    Ok(match v {
        Value::Null => ScalarValue::try_from(ty)?,
        Value::String(s) if *ty == DataType::Utf8 => ScalarValue::Utf8(Some(s)),
        Value::Int(n) if *ty == DataType::Int32 => ScalarValue::Int32(Some(
            i32::try_from(n).map_err(|e| DataFusionError::Execution(e.to_string()))?,
        )),
        Value::List(items) if matches!(ty, DataType::List(_)) => {
            let DataType::List(f) = ty else {
                unreachable!()
            };
            let items = items
                .into_iter()
                .map(|v| scalar(v, f.data_type()))
                .collect::<datafusion::common::Result<Vec<_>>>()?;
            ScalarValue::List(ScalarValue::new_list(&items, f.data_type(), true))
        }
        _ => {
            return Err(DataFusionError::Execution(
                "invalid language function result".into(),
            ));
        }
    })
}
pub(crate) fn evaluate(
    name: &str,
    arrays: &[ArrayRef],
    rows: usize,
) -> datafusion::common::Result<ArrayRef> {
    if name == KEY {
        if arrays.len() != 1 || !key_type(arrays[0].data_type()) {
            return Err(DataFusionError::Execution(
                "unsupported Cypher key input".into(),
            ));
        }
        let mut keys = BinaryBuilder::new();
        for row in 0..rows {
            // SQL NULL groups together and COUNT(DISTINCT) excludes it.
            if arrays[0].is_null(row) {
                keys.append_null();
            } else {
                keys.append_value(
                    crate::ir::runtime::ops::distinct::encode_cypher_equivalence(&value(
                        ScalarValue::try_from_array(&arrays[0], row)?,
                    )?),
                );
            }
        }
        return Ok(Arc::new(keys.finish()));
    }
    let ty = match name {
        "__orchiddb_lang_text" => DataType::Utf8,
        "__orchiddb_lang_texts" => list_type(DataType::Utf8),
        "__orchiddb_lang_int" => DataType::Int32,
        "__orchiddb_lang_ints" => list_type(DataType::Int32),
        _ => {
            return Err(DataFusionError::Execution(format!(
                "unknown language function {name}"
            )));
        }
    };
    if arrays.len() != 2 {
        return Err(DataFusionError::Execution(
            "language function expects operation and arguments".into(),
        ));
    }
    let packed = arrays[1]
        .as_any()
        .downcast_ref::<StructArray>()
        .ok_or_else(|| {
            DataFusionError::Execution("language function expects struct arguments".into())
        })?;
    let mut out = Vec::with_capacity(rows);
    for row in 0..rows {
        let op = ScalarValue::try_from_array(&arrays[0], row)?;
        let ScalarValue::Utf8(Some(op)) = op else {
            return Err(DataFusionError::Execution(
                "invalid language operation".into(),
            ));
        };
        let args = packed
            .columns()
            .iter()
            .map(|a| value(ScalarValue::try_from_array(a, row)?))
            .collect::<datafusion::common::Result<Vec<_>>>()?;
        let result = crate::ir::runtime::scalar::gremlin_string_call(&op, &args)
            .map_err(|e| DataFusionError::Execution(e.to_string()))?;
        out.push(scalar(result, &ty)?);
    }
    if out.is_empty() {
        Ok(arrow::array::new_empty_array(&ty))
    } else {
        ScalarValue::iter_to_array(out)
    }
}

/// Only terminal scalar pipelines may discard maintenance-only traverser state.
/// A path/label consumer or a branch keeps the existing state-preserving path.
pub(crate) fn terminal_gremlin(root: &Node) -> bool {
    fn expr(e: &IrExpr, uses_function: &mut bool) -> bool {
        match e {
            IrExpr::Lit(_) => true,
            IrExpr::Binding(b)
            | IrExpr::Property { binding: b, .. }
            | IrExpr::Id(b)
            | IrExpr::Label(b)
            | IrExpr::HasLabel { binding: b, .. }
            | IrExpr::IsBound(b) => {
                !b.starts_with("__path") && !b.starts_with("__gremlin_select_history")
            }
            IrExpr::Binary { lhs, rhs, .. } => expr(lhs, uses_function) && expr(rhs, uses_function),
            IrExpr::Not(e) | IrExpr::IsNull(e) | IrExpr::IsNotNull(e) => expr(e, uses_function),
            IrExpr::StringPredicate {
                target, pattern, ..
            } => expr(target, uses_function) && expr(pattern, uses_function),
            IrExpr::List(items) => items.iter().all(|e| expr(e, uses_function)),
            IrExpr::Case { arms, otherwise } => {
                arms.iter()
                    .all(|(a, b)| expr(a, uses_function) && expr(b, uses_function))
                    && otherwise.as_deref().is_none_or(|e| expr(e, uses_function))
            }
            IrExpr::Call { name, args } if name == "requested_property_values" => {
                args.iter().all(|e| expr(e, uses_function))
            }
            IrExpr::Call { name, args } if name.starts_with("gremlin_string_") => {
                *uses_function = true;
                args.iter().all(|e| expr(e, uses_function))
            }
            _ => false,
        }
    }
    let mut uses_function = false;
    let mut pending = vec![root];
    while let Some(node) = pending.pop() {
        let safe = match node {
            Node::GraphReturn { fields, .. } => fields.iter().all(|s| s == "current"),
            Node::GraphNodeScan { .. } | Node::GraphOneRow | Node::GraphEmpty => true,
            Node::GraphBind {
                bind, expr: None, ..
            } => bind == "current",
            Node::GraphProject { items, .. } => items.iter().all(|item| {
                // These assignments only maintain a path that this allowlisted
                // pipeline never consumes. Any other state expression fences it.
                if item.alias == "__path"
                    && matches!(&item.expr, IrExpr::Call { name, .. } if name.starts_with("path_"))
                {
                    true
                } else {
                    expr(&item.expr, &mut uses_function)
                }
            }),
            Node::GraphUnwind { input_expr, .. } => expr(input_expr, &mut uses_function),
            Node::GraphFilter { condition, .. } => expr(condition, &mut uses_function),
            Node::GraphSlice { .. } => true,
            Node::GraphSort { keys, .. } => keys.iter().all(|k| {
                matches!(&k.expr, IrExpr::Call { name, .. } if name == "gremlin_scan_order")
                    || expr(&k.expr, &mut uses_function)
            }),
            _ => false,
        };
        if !safe {
            return false;
        }
        pending.extend(crate::ir::analysis::children(node));
    }
    uses_function
}
