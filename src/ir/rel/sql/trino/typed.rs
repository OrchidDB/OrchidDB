//! Type-directed Trino implementations of shared compiler expressions.
use super::*;
use crate::ir::functions::logical::{LogicalFunction, SqlFunctionMapping};
use datafusion::{
    common::tree_node::{Transformed, TreeNode},
    logical_expr::{Expr, ExprSchemable, LogicalPlan, expr_rewriter::NamePreserver},
};

fn json_value(value: &str, ty: &DataType, depth: usize) -> SqlResult<String> {
    Ok(match ty {
        DataType::Struct(fields) => {
            let values = fields
                .iter()
                .map(|field| {
                    json_value(
                        &format!("({value}).{}", TRINO.quote_ident(field.name())),
                        field.data_type(),
                        depth + 1,
                    )
                })
                .collect::<SqlResult<Vec<_>>>()?
                .join(", ");
            format!(
                "CASE WHEN {value} IS NULL THEN CAST(NULL AS JSON) ELSE CAST(ARRAY[{values}] AS JSON) END"
            )
        }
        DataType::List(field) | DataType::LargeList(field) | DataType::FixedSizeList(field, _) => {
            let item = format!("__local{}", 300 + depth);
            let child = json_value(&item, field.data_type(), depth + 1)?;
            format!("CAST(transform({value}, {item} -> {child}) AS JSON)")
        }
        DataType::Binary | DataType::LargeBinary | DataType::BinaryView => {
            format!("CAST(concat('\\x', to_hex({value})) AS JSON)")
        }
        DataType::Timestamp(..) | DataType::Date32 => format!("CAST(to_iso8601({value}) AS JSON)"),
        DataType::Map(..) | DataType::Union(..) => {
            return Err(SqlError::Unsupported(format!(
                "Trino graph transport for {ty}"
            )));
        }
        _ => format!("CAST(CAST({value} AS VARCHAR) AS JSON)"),
    })
}

fn carrier(value: String) -> String {
    format!("CAST(ROW({value}) AS ROW(\"__orchiddb_value_v1\" VARCHAR))")
}

pub(in crate::ir::rel::sql) fn specialize(plan: LogicalPlan) -> SqlResult<LogicalPlan> {
    Ok(plan.transform_up_with_subqueries(|node| {
        let names = NamePreserver::new(&node);
        let mut schema = datafusion::common::DFSchema::empty();
        for input in node.inputs() { schema.merge(input.schema()); }
        schema.merge(node.schema());
        node.map_expressions(|expr| {
            let saved = names.save(&expr);
            Ok(expr.transform_up(|expr| {
                use crate::ir::rel::native_values as native;
                use crate::execution::graph_values::{ELEMENT_PREFIX, PATH_PREFIX};
                // The recursive seed is a semantic empty path, encoded using
                // the same carrier type as each subsequent step.
                if let Some(crate::ir::value::Value::Path(items)) = native::literal_value(&expr)
                    && items.is_empty() {
                    return Ok(Transformed::yes(datafusion::functions::core::expr_fn::named_struct(vec![
                        datafusion::logical_expr::lit(native::FIELD), datafusion::logical_expr::lit(format!("{PATH_PREFIX}[]"))
                    ])));
                }
                if let Expr::Literal(value, _) = &expr
                    && native::is_value(&value.data_type()) && !value.is_null() {
                    use base64::Engine;
                    let cell = crate::federation::scalar_json(value)?;
                    if let Some(encoded) = cell[native::FIELD].as_str()
                        && let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(encoded)
                        && matches!(crate::ir::catalog::snapshot::binary::decode_value_bytes(&bytes), Ok(crate::ir::value::Value::Path(items)) if items.is_empty()) {
                        let scalar = crate::federation::json_scalar(&serde_json::json!({native::FIELD:format!("{PATH_PREFIX}[]")}), &value.data_type())
                            .map_err(datafusion::common::DataFusionError::Plan)?;
                        return Ok(Transformed::yes(datafusion::logical_expr::lit(scalar)));
                    }
                }
                let Expr::ScalarFunction(call) = &expr else { return Ok(Transformed::no(expr)); };
                let template = match (call.name(), call.args.as_slice()) {
                    ("orchid_cypher_element", [arg]) => {
                        let ty = arg.get_type(&schema)?;
                        let encoded = json_value("__arg0", &ty, 0).map_err(|e| datafusion::common::DataFusionError::Plan(e.to_string()))?;
                        let type_name = crate::federation::type_name(&ty).map_err(datafusion::common::DataFusionError::Plan)?;
                        carrier(format!("concat({}, json_format(CAST(ARRAY[CAST(ARRAY[CAST({} AS JSON), {encoded}] AS JSON)] AS JSON)))",
                            super::string(ELEMENT_PREFIX), super::string(&type_name)))
                    }
                    ("orchid_cypher_path_append", [_, item]) if matches!(item, Expr::ScalarFunction(f)
                        if crate::ir::functions::logical::definition(&f.func).is_some_and(|d| d.logical_name().starts_with("trino.orchid_cypher_element."))) => {
                        let array = |i| format!("CAST(json_parse(substr((__arg{i}).\"__orchiddb_value_v1\", {})) AS ARRAY(JSON))", PATH_PREFIX.len() + 1);
                        carrier(format!("concat({}, json_format(CAST(concat({}, {}) AS JSON)))", super::string(PATH_PREFIX), array(0), array(1)))
                    }
                    ("named_struct", args) => {
                        let values = (1..args.len()).step_by(2).map(|i| format!("__arg{i}")).collect::<Vec<_>>().join(", ");
                        format!("CAST(ROW({values}) AS {})", TRINO.sql_type(&expr.get_type(&schema)?)
                            .map_err(|e| datafusion::common::DataFusionError::Plan(e.to_string()))?)
                    }
                    ("get_field", [arg, Expr::Literal(ScalarValue::Utf8(Some(field)), _)])
                        if matches!(arg.get_type(&schema)?, DataType::Struct(_)) => {
                        format!("(__arg0).{}", TRINO.quote_ident(field))
                    }
                    ("orchid_cypher_key", [arg]) if matches!(arg.get_type(&schema)?,
                        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) => {
                        // Exact shared Cypher equality encoding: null tag, or
                        // framed string tag + big-endian byte length + payload.
                        "to_base64(CASE WHEN __arg0 IS NULL THEN X'00' ELSE concat(X'ff', to_big_endian_64(length(to_utf8(__arg0)) + 1), X'04', to_utf8(__arg0)) END)".to_owned()
                    }
                    _ => return Ok(Transformed::no(expr)),
                };
                let function = LogicalFunction::new(format!("trino.{}.{template}", call.name()), call.func.clone(),
                    [("trino".into(), SqlFunctionMapping { value: template, ordering: None })].into());
                Ok(Transformed::yes(function.into_udf().call(call.args.clone())))
            })?.map_data(|expr| Ok(saved.restore(expr)))?)
        })
    })?.data)
}
