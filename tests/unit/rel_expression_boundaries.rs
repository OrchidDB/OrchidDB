use super::*;
use crate::ir::policy::{PropertyMissing, OptionalMissing};
use crate::ir::plan::ProjectErrorPolicy;

#[test]
fn collection_type_unification_widens_only_compatible_integer_domains() {
    for left in [DataType::Int8, DataType::Int16, DataType::Int32, DataType::Int64] {
        for right in [DataType::Int8, DataType::Int16, DataType::Int32, DataType::Int64] {
            let expected = if left == right { left.clone() } else { DataType::Int64 };
            assert_eq!(nullable_collection_type(&left, &right), Some(expected));
        }
    }
    let metadata = std::collections::HashMap::from([("source".into(), "left".into())]);
    let left_field = Field::new("item", DataType::Int8, false).with_metadata(metadata.clone());
    let left = DataType::List(Arc::new(left_field));
    let right = DataType::List(Arc::new(Field::new("other", DataType::Int32, true)));
    assert_eq!(nullable_collection_type(&left, &right), Some(DataType::List(Arc::new(
        Field::new("item", DataType::Int64, true).with_metadata(metadata)))));
    for kind in [DataType::Boolean, DataType::Date32, left.clone(), DataType::Struct(vec![Field::new("x", DataType::Utf8, true)].into())] {
        assert_eq!(nullable_collection_type(&DataType::Null, &kind), Some(kind.clone()));
        assert_eq!(nullable_collection_type(&kind, &DataType::Null), Some(kind));
    }
    for (left, right) in [(DataType::Int64, DataType::Float64), (DataType::Int64, DataType::UInt64),
        (DataType::Utf8, DataType::Boolean),
        (left, DataType::List(Arc::new(Field::new("item", DataType::Boolean, true))))] {
        assert_eq!(nullable_collection_type(&left, &right), None);
    }
}

fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call { name: name.into(), args }
}

#[test]
fn literal_conditionals_prune_only_unreachable_arguments() {
    let poison = call("effectful", vec![]);
    let unknown = IrExpr::Binding("unknown".into());
    let null = IrExpr::Lit(Lit::Null);
    let value = IrExpr::Lit(Lit::Int(7));
    let typed_null = IrExpr::Lit(Lit::Scalar(ScalarValue::Int64(None)));
    for (name, input, expected) in [
        ("fn.nvl2", vec![typed_null.clone(), poison.clone(), value.clone()], vec![typed_null.clone(), null.clone(), value.clone()]),
        ("FN.NVL2", vec![value.clone(), value.clone(), poison.clone()], vec![value.clone(), value.clone(), null.clone()]),
        ("fn.coalesce", vec![typed_null, unknown.clone(), value.clone(), poison.clone()], vec![IrExpr::Lit(Lit::Scalar(ScalarValue::Int64(None))), unknown.clone(), value.clone(), null.clone()]),
        ("fn.nvl", vec![value.clone(), poison.clone()], vec![value.clone(), null.clone()]),
        ("fn.ifnull", vec![value.clone(), poison.clone()], vec![value.clone(), null.clone()]),
    ] {
        assert_eq!(prune_literal_conditional_branches(&call(name, input)), Some(call(name, expected)), "{name}");
    }
    for expr in [
        unknown.clone(),
        call("fn.coalesce", vec![]),
        call("fn.coalesce", vec![null.clone(), unknown.clone()]),
        call("fn.coalesce", vec![value.clone(), null.clone()]),
        call("fn.nvl2", vec![unknown.clone(), value.clone(), poison.clone()]),
        call("fn.nvl2", vec![null.clone(), poison.clone()]),
        call("fn.nvl", vec![unknown, poison.clone()]),
        call("fn.ifnull", vec![null.clone(), poison.clone()]),
        call("unrelated", vec![value, poison]),
    ] {
        assert!(prune_literal_conditional_branches(&expr).is_none(), "{expr:?}");
    }
}

fn cypher_context(graph: &PropertyGraph) -> LoweringContext<'_> {
    LoweringContext {
        island_memo: None, graph, options: RelBackendOptions::default(),
        policy: GraphPlanPolicy::cypher(), language: Language::Cypher, scan_counter: 0,
        correlate_plan: None, rdf_typed_terms_used: false, scan_logical_labels: false,
        gremlin_label_binds: Default::default(), unordered_collects: Default::default(),
        unordered_distincts: Default::default(),
    }
}
fn scalar_scope(fields: Vec<Field>) -> LogicalPlan {
    LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
        produce_one_row: false,
        schema: Arc::new(datafusion::common::DFSchema::try_from(Schema::new(fields)).unwrap()),
    })
}
fn binding(name: &str) -> IrExpr { IrExpr::binding(name) }

#[test]
fn cypher_temporal_parts_execute_for_text_and_preserve_native_runtime_boundary() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let days = chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap()
        .signed_duration_since(chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()).num_days();
    for input in [ScalarValue::Date32(Some(days as i32)),
        ScalarValue::TimestampNanosecond(Some(days * 86_400_000_000_000), None),
        ScalarValue::Utf8(Some("2024-02-29".into())),
        ScalarValue::LargeUtf8(Some("2024-02-29 12:34:56".into())),
        ScalarValue::Utf8View(Some("2024-02-29".into())),
        ScalarValue::Date32(None), ScalarValue::Utf8(Some("not a date".into()))] {
        for dynamic_unit in [false, true] {
            let plan = scalar_scope(vec![Field::new("value", input.data_type(), true), Field::new("unit", DataType::Utf8, true)]);
            let unit = if dynamic_unit { binding("unit") } else { IrExpr::Lit(Lit::String("year".into())) };
            let result = ctx.lower_sql_expr(&plan, &call("date_part", vec![unit, binding("value")]));
            if input.data_type() == DataType::Date32 {
                assert!(result.unwrap_err().to_string().contains("Temporal operand requires typed runtime evaluation"));
                continue;
            }
            let lowered = result.unwrap();
            let bound = lowered.transform_up(|expr| Ok(match &expr {
                Expr::Column(column) if column.name == "value" => Transformed::yes(lit(input.clone())),
                Expr::Column(column) if column.name == "unit" => Transformed::yes(lit("year")),
                _ => Transformed::no(expr),
            })).unwrap().data;
            let expected = if input.is_null() || input == ScalarValue::Utf8(Some("not a date".into())) { None } else { Some(2024) };
            assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]), bound), ScalarValue::Int32(expected), "{input:?}, dynamic={dynamic_unit}");
        }
    }
    for name in ["date_part", "date_trunc"] {
        for count in [0, 1, 3] {
            let error = ctx.lower_sql_expr(&scalar_scope(vec![]), &call(name, vec![binding("missing"); count])).unwrap_err();
            assert!(error.to_string().contains("arity"), "{error}");
        }
    }
}

#[test]
fn cypher_temporal_truncation_preserves_date_only_text_and_native_runtime_boundary() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let stamp = chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap().and_hms_opt(12, 34, 56).unwrap()
        .and_utc().timestamp_nanos_opt().unwrap();
    for (input, expected) in [
        (ScalarValue::Utf8(Some("2024-02-29".into())), Some("2024-02-01")),
        (ScalarValue::LargeUtf8(Some("2024-02-29".into())), Some("2024-02-01")),
        (ScalarValue::Utf8View(Some("2024-02-29".into())), Some("2024-02-01")),
        (ScalarValue::TimestampNanosecond(Some(stamp), None), Some("2024-02-01T00:00:00")),
        (ScalarValue::Utf8(Some("2024-02-29 12:34:56".into())), Some("2024-02-01T00:00:00")),
        (ScalarValue::TimestampNanosecond(None, None), None),
        (ScalarValue::Utf8(None), None),
        (ScalarValue::Utf8(Some("invalid".into())), None),
    ] {
        let plan = scalar_scope(vec![Field::new("value", input.data_type(), true)]);
        let result = ctx.lower_sql_expr(&plan, &call("date_trunc", vec![IrExpr::Lit(Lit::String("month".into())), binding("value")]));
        let lowered = result.unwrap();
        let bound = lowered.transform_up(|expr| Ok(match &expr {
            Expr::Column(column) if column.name == "value" => Transformed::yes(lit(input.clone())),
            _ => Transformed::no(expr),
        })).unwrap().data;
        assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]), bound).cast_to(&DataType::Utf8).unwrap(), ScalarValue::Utf8(expected.map(str::to_owned)), "{input:?}");
    }
}

fn fold_cypher_boundary_expression(plan: &LogicalPlan, expression: Expr) -> ScalarValue {
    use datafusion::{logical_expr::simplify::SimplifyContext,
        optimizer::simplify_expressions::ExprSimplifier};
    let simplifier = ExprSimplifier::new(SimplifyContext::default().with_schema(plan.schema().clone()));
    let expression = simplifier.simplify(simplifier.coerce(expression, plan.schema()).unwrap()).unwrap();
    let Expr::Literal(value, _) = expression else { panic!("not folded: {expression:?}") };
    value
}

#[test]
fn cypher_native_binary_lowering_executes_numeric_and_comparison_semantics() {
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![]);
    let left = IrExpr::Lit(Lit::Int(8));
    let right = IrExpr::Lit(Lit::Int(2));
    for (op, expected) in [(BinaryOp::Add,10), (BinaryOp::Sub,6), (BinaryOp::Mul,16), (BinaryOp::Div,4)] {
        for native_left in [false, true] {
            let (lhs, rhs) = if native_left {
                (native_values::literal(&Value::Int(8)).unwrap(), lit(2_i64))
            } else {
                (lit(8_i64), native_values::literal(&Value::Int(2)).unwrap())
            };
            let result = ctx.lower_bound_binary(&plan, &left, op, &right, lhs, rhs).unwrap();
            let equal = native_values::typed::call("cypher_eq", vec![result, lit(expected as i64)]);
            assert_eq!(fold_cypher_boundary_expression(&plan, equal), ScalarValue::Boolean(Some(true)), "{op:?}");
        }
    }
    for (op, expected) in [(BinaryOp::Eq,false), (BinaryOp::Neq,true), (BinaryOp::Lt,false),
        (BinaryOp::Lte,false), (BinaryOp::Gt,true), (BinaryOp::Gte,true)] {
        for null in [false, true] {
            let lhs = native_values::literal(&if null {Value::Null} else {Value::Int(8)}).unwrap();
            let result = ctx.lower_bound_binary(&plan, &left, op, &right, lhs, lit(2_i64)).unwrap();
            assert_eq!(fold_cypher_boundary_expression(&plan, result),
                ScalarValue::Boolean((!null).then_some(expected)), "{op:?}, null={null}");
        }
    }
}

#[test]
fn cypher_native_boolean_lowering_preserves_three_valued_logic() {
    let graph = PropertyGraph::new();
    let mut ctx = cypher_context(&graph);
    ctx.options.native_values = true;
    let plan = scalar_scope(vec![]);
    for (left, right, and, or) in [
        (Value::Bool(true), Value::Bool(false), Some(false), Some(true)),
        (Value::Null, Value::Bool(false), Some(false), None),
        (Value::Null, Value::Bool(true), None, Some(true)),
        (Value::Null, Value::Null, None, None),
    ] {
        for (lhs, rhs) in [(&left, &right), (&right, &left)] {
            for (op, expected) in [(BinaryOp::And,and), (BinaryOp::Or,or)] {
                let result = ctx.lower_bound_binary(&plan, &binding("a"), op, &binding("b"),
                    native_values::literal(lhs).unwrap(), native_values::literal(rhs).unwrap()).unwrap();
                assert_eq!(fold_cypher_boundary_expression(&plan, result), ScalarValue::Boolean(expected),
                    "{lhs:?} {op:?} {rhs:?}");
            }
        }
    }
}

#[test]
fn cypher_native_case_predicates_preserve_nulls_and_reject_truthy_coercions() {
    use datafusion::logical_expr::{ColumnarValue, ScalarFunctionArgs};
    let graph = PropertyGraph::new();
    let empty = scalar_scope(vec![]);
    let plan = scalar_scope(vec![Field::new("predicate", native_values::value_type(), true)]);
    let pack = |value: &Value| fold_cypher_boundary_expression(&empty, native_values::literal(value).unwrap());
    for native in [false, true] {
        let mut ctx = cypher_context(&graph);
        ctx.options.native_values = native;
        let expression = ctx.lower_case_condition(&plan, &binding("predicate")).unwrap();
        let Expr::ScalarFunction(call) = expression else { panic!("expected predicate validator") };
        let invoke = |values: Vec<ScalarValue>| {
            let rows = values.len();
            call.func.invoke_with_args(ScalarFunctionArgs {
                args: vec![ColumnarValue::Array(ScalarValue::iter_to_array(values).unwrap())],
                arg_fields: vec![Arc::new(Field::new("predicate", native_values::value_type(), true))],
                number_rows: rows, return_field: Arc::new(Field::new("result", DataType::Boolean, true)),
                config_options: Default::default(),
            })
        };
        let output = invoke(vec![pack(&Value::Bool(true)), pack(&Value::Null), pack(&Value::Bool(false))])
            .unwrap().into_array(3).unwrap();
        for (row, expected) in [Some(true), None, Some(false)].into_iter().enumerate() {
            assert_eq!(ScalarValue::try_from_array(&output, row).unwrap(), ScalarValue::Boolean(expected));
        }
        for invalid in [Value::Int(1), Value::String("true".into()), Value::List(vec![])] {
            assert!(invoke(vec![pack(&invalid)]).is_err(), "must not coerce {invalid:?} to a predicate");
        }
    }
}

#[test]
fn cypher_case_preserves_selected_native_and_plain_values_without_common_type_casts() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("condition", DataType::Boolean, true),
        Field::new("native", native_values::value_type(), true), Field::new("plain", DataType::Utf8, true)]);
    let empty = scalar_scope(vec![]);
    for native_then in [false, true] {
        let expression = IrExpr::Case {
            arms: vec![(binding("condition"), binding(if native_then {"native"} else {"plain"}))],
            otherwise: Some(Box::new(binding(if native_then {"plain"} else {"native"}))),
        };
        let lowered = ctx.lower_sql_expr(&plan, &expression).unwrap();
        assert_eq!(lowered.get_type(plan.schema()).unwrap(), native_values::value_type());
        for condition in [Some(true), Some(false), None] {
            let bound = lowered.clone().transform_up(|expr| {
                Ok(match &expr {
                    Expr::Column(column) => Transformed::yes(match column.name.as_str() {
                        "condition" => lit(ScalarValue::Boolean(condition)),
                        "native" => native_values::literal(&Value::Long(i64::MAX)).unwrap(),
                        "plain" => lit("other"),
                        _ => panic!("unexpected column {column}"),
                    }),
                    _ => Transformed::no(expr),
                })
            }).unwrap().data;
            let expected = if (condition == Some(true)) == native_then {
                Value::Long(i64::MAX)
            } else {Value::String("other".into())};
            let equal = native_values::typed::call("cypher_eq", vec![bound,
                native_values::literal(&expected).unwrap()]);
            assert_eq!(fold_cypher_boundary_expression(&empty, equal), ScalarValue::Boolean(Some(true)),
                "native_then={native_then}, condition={condition:?}");
        }
    }
}

#[test]
fn cypher_native_case_without_else_retains_a_typed_null() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("condition", DataType::Boolean, true),
        Field::new("native", native_values::value_type(), true)]);
    let expression = IrExpr::Case { arms: vec![(binding("condition"), binding("native"))], otherwise: None };
    let lowered = ctx.lower_sql_expr(&plan, &expression).unwrap();
    for condition in [Some(false), None] {
        let bound = lowered.clone().transform_up(|expr| Ok(match &expr {
            Expr::Column(column) if column.name == "condition" => Transformed::yes(lit(ScalarValue::Boolean(condition))),
            Expr::Column(column) if column.name == "native" => Transformed::yes(native_values::literal(&Value::Int(7)).unwrap()),
            _ => Transformed::no(expr),
        })).unwrap().data;
        let result = fold_cypher_boundary_expression(&scalar_scope(vec![]), bound);
        assert_eq!(result.data_type(), native_values::value_type());
        assert!(result.is_null());
    }
}

#[test]
fn cypher_boolean_integer_casts_preserve_nulls_without_direct_boolean_sql_casts() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("value", DataType::Boolean, true)]);
    for (target, kind) in [("INT8", DataType::Int8), ("INT16", DataType::Int16),
        ("INT32", DataType::Int32), ("INT64", DataType::Int64)] {
        let expression = call("cast", vec![binding("value"), IrExpr::Lit(Lit::String(target.into()))]);
        let lowered = ctx.lower_sql_expr(&plan, &expression).unwrap();
        assert_eq!(lowered.get_type(plan.schema()).unwrap(), kind);
        if kind == DataType::Int64 {
            assert!(matches!(lowered, Expr::Case(_)), "Int64 CASE needs no extra cast: {lowered:?}");
        }
        for value in [Some(true), Some(false), None] {
            let bound = lowered.clone().transform_up(|expr| Ok(match &expr {
                Expr::Column(column) if column.name == "value" => Transformed::yes(lit(ScalarValue::Boolean(value))),
                _ => Transformed::no(expr),
            })).unwrap().data;
            let actual = fold_cypher_boundary_expression(&scalar_scope(vec![]), bound);
            let expected = ScalarValue::Int64(value.map(i64::from)).cast_to(&kind).unwrap();
            assert_eq!(actual, expected, "{target}, {value:?}");
        }
    }
}

#[test]
fn cypher_to_integer_truncates_fractional_values_toward_zero_and_keeps_nulls() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    for (input, expected) in [
        (ScalarValue::Float64(Some(1.9)), Some(1)),
        (ScalarValue::Float64(Some(-1.9)), Some(-1)),
        (ScalarValue::Float64(Some(-0.0)), Some(0)),
        (ScalarValue::Float64(None), None),
        (ScalarValue::Float32(Some(-1.9)), Some(-1)),
        (ScalarValue::Decimal128(Some(199), 8, 2), Some(1)),
        (ScalarValue::Decimal128(Some(-199), 8, 2), Some(-1)),
    ] {
        let plan = scalar_scope(vec![Field::new("value", input.data_type(), true)]);
        let lowered = ctx.lower_sql_expr(&plan, &call("tointeger", vec![binding("value")])).unwrap();
        let bound = lowered.transform_up(|expr| Ok(match &expr {
            Expr::Column(column) if column.name == "value" => Transformed::yes(lit(input.clone())),
            _ => Transformed::no(expr),
        })).unwrap().data;
        assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]), bound),
            ScalarValue::Int64(expected), "{input:?}");
    }
}

#[test]
fn cypher_canonical_integer_conversion_matches_runtime_float_boundaries() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    for (input, expected) in [
        (ScalarValue::Float64(Some(9_223_372_036_854_775_808.0)), Some(i64::MAX)),
        (ScalarValue::Float64(Some(-9_223_372_036_854_775_808.0)), Some(i64::MIN)),
        (ScalarValue::Float64(Some(f64::MAX)), Some(i64::MAX)),
        (ScalarValue::Float64(Some(-f64::MAX)), Some(i64::MIN)),
        (ScalarValue::Float64(Some(f64::INFINITY)), None),
        (ScalarValue::Float64(Some(f64::NEG_INFINITY)), None),
        (ScalarValue::Float64(Some(f64::NAN)), None),
        (ScalarValue::Float64(Some(-1.9)), Some(-1)),
        (ScalarValue::Float64(None), None),
        (ScalarValue::Float32(Some(f32::MAX)), Some(i64::MAX)),
        (ScalarValue::Float32(Some(-1.9)), Some(-1)),
    ] {
        let plan = scalar_scope(vec![Field::new("value", input.data_type(), true)]);
        // This is the canonical operation emitted by the parsed Cypher planner.
        let lowered = ctx.lower_expr(&plan, &call("cypher_convert.tointeger", vec![binding("value")])).unwrap();
        assert_eq!(lowered.get_type(plan.schema()).unwrap(), DataType::Int64);
        let bound = lowered.transform_up(|expr| Ok(match &expr {
            Expr::Column(column) if column.name == "value" => Transformed::yes(lit(input.clone())),
            _ => Transformed::no(expr),
        })).unwrap().data;
        let empty = scalar_scope(vec![]);
        let actual = fold_cypher_boundary_expression(&empty, bound);
        assert_eq!(actual, ScalarValue::Int64(expected), "{input:?}");
        let converted = native_values::typed::call("cypher_convert.tointeger", vec![lit(input.clone())]);
        let text = native_values::typed::call("scalar_text", vec![converted]);
        let runtime = fold_cypher_boundary_expression(&empty, text);
        assert_eq!(runtime, ScalarValue::Utf8(expected.map(|v| v.to_string())), "runtime {input:?}");
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn cypher_wide_integer_float_comparisons_preserve_exactness_and_nulls() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    for (integer, exact) in [("18446744073709551616", true), ("18446744073709551617", false)] {
        let original = call("integer_literal", vec![IrExpr::Lit(Lit::String(integer.into()))]);
        for value in [None, Some(18_446_744_073_709_551_616.0), Some(0.0)] {
            let plan = LogicalPlanBuilder::empty(true).project(vec![
                lit(ScalarValue::Float64(value)).alias("x")]).unwrap().build().unwrap();
            for reverse in [false, true] {
                for op in [BinaryOp::Eq, BinaryOp::Neq] {
                    let (left, right, lhs, rhs) = if reverse {
                        (binding("x"), original.clone(), col_exact("x"), lit(integer))
                    } else {
                        (original.clone(), binding("x"), lit(integer), col_exact("x"))
                    };
                    let result = ctx.lower_bound_binary(&plan, &left, op, &right, lhs, rhs).unwrap();
                    let output = LogicalPlanBuilder::from(plan.clone()).project(vec![result.alias("result")]).unwrap().build().unwrap();
                    let batches = crate::ir::rel::execute_test_plan(output).await.unwrap();
                    let actual = ScalarValue::try_from_array(batches[0].column(0), 0).unwrap();
                    let expected = value.map(|v| {
                        let equal = exact && v == 18_446_744_073_709_551_616.0;
                        if op == BinaryOp::Eq { equal } else { !equal }
                    });
                    assert_eq!(actual, ScalarValue::Boolean(expected), "{integer}, {value:?}, {op:?}, reversed={reverse}");
                }
            }
            for op in [BinaryOp::Lt, BinaryOp::Lte, BinaryOp::Gt, BinaryOp::Gte] {
                let error = ctx.lower_bound_binary(&plan, &original, op, &binding("x"),
                    lit(integer), col_exact("x")).unwrap_err();
                assert!(error.to_string().contains("exact ordering"), "{error}");
            }
        }
    }
}

#[test]
fn cypher_scalar_columns_preserve_physical_types_without_redundant_casts() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Boolean, DataType::Int8, DataType::Int16, DataType::Int32,
        DataType::Int64, DataType::UInt64, DataType::Float32, DataType::Float64,
        DataType::Utf8, DataType::Decimal128(38, 8)] {
        let plan = scalar_scope(vec![Field::new("x", kind.clone(), true)]);
        for expr in [binding("x"), call("cypher_property_value", vec![binding("x")]),
            call("cypher_order_key", vec![binding("x")])] {
            let result = ctx.lower_sql_expr(&plan, &expr).unwrap();
            assert_eq!(result, col_exact("x"), "{kind:?}: {expr:?}");
            assert_eq!(result.get_type(plan.schema()).unwrap(), kind);
        }
    }
}

#[test]
fn cypher_case_eliminates_missing_bindings_before_lowering_dead_values() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("x", DataType::Int64, true)]);
    for otherwise in [None, Some(Box::new(binding("x")))] {
        let expr = IrExpr::Case {
            arms: vec![(IrExpr::IsBound("missing".into()), binding("also_missing"))],
            otherwise: otherwise.clone(),
        };
        let lowered = ctx.lower_sql_expr(&plan, &expr).unwrap();
        assert_eq!(lowered, if otherwise.is_some() { col_exact("x") } else { lit(ScalarValue::Null) });
    }
    assert_eq!(ctx.lower_sql_expr(&plan, &IrExpr::IsBound("missing".into())).unwrap(), lit(false));
    assert_eq!(ctx.lower_sql_expr(&plan, &IrExpr::IsBound("x".into())).unwrap(), col_exact("x").is_not_null());
}

#[test]
fn cypher_case_aligns_null_branches_and_rejects_lossy_common_types() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Int64, DataType::Float64, DataType::Utf8, DataType::Boolean] {
        let plan = scalar_scope(vec![Field::new("x",kind.clone(),true),Field::new("p",DataType::Boolean,true)]);
        for null in [ScalarValue::Null, ScalarValue::Utf8(None), ScalarValue::Int64(None)] {
            let expr = IrExpr::Case { arms: vec![(binding("p"), IrExpr::Lit(Lit::Scalar(null)))], otherwise: Some(Box::new(binding("x"))) };
            let result = ctx.lower_sql_expr(&plan, &expr).unwrap();
            let Expr::Case(case) = result else { panic!("expected CASE") };
            assert_eq!(case.when_then_expr[0].1.get_type(plan.schema()).unwrap(), kind);
            assert_eq!(*case.else_expr.unwrap(), col_exact("x"));
        }
    }
    let plan = scalar_scope(vec![Field::new("n",DataType::Int64,true),Field::new("s",DataType::Utf8,true),Field::new("p",DataType::Boolean,true)]);
    let expr = IrExpr::Case { arms: vec![(binding("p"),binding("n"))], otherwise: Some(Box::new(binding("s"))) };
    assert!(ctx.lower_sql_expr(&plan,&expr).unwrap_err().to_string().contains("Heterogeneous Cypher CASE"));
}

#[test]
fn cypher_predicate_lowering_retains_boolean_and_types_null_conditions() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("p",DataType::Boolean,true)]);
    assert_eq!(ctx.lower_case_condition(&plan,&binding("p")).unwrap(),col_exact("p"));
    for null in [ScalarValue::Null, ScalarValue::Int64(None), ScalarValue::Utf8(None)] {
        assert_eq!(ctx.lower_case_condition(&plan,&IrExpr::Lit(Lit::Scalar(null))).unwrap(),lit(ScalarValue::Boolean(None)));
    }
    for expr in [IrExpr::Not(Box::new(binding("p"))),IrExpr::IsNull(Box::new(binding("p"))),IrExpr::IsNotNull(Box::new(binding("p")))] {
        assert_eq!(ctx.lower_sql_expr(&plan,&expr).unwrap().get_type(plan.schema()).unwrap(),DataType::Boolean);
    }
}

#[test]
fn cypher_nested_property_guards_require_runtime_validation() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let scalar = Arc::new(Field::new("item",DataType::Int64,true));
    for kind in [DataType::List(scalar.clone()),DataType::LargeList(scalar.clone()),DataType::FixedSizeList(scalar,2)] {
        let plan = scalar_scope(vec![Field::new("x",kind,true)]);
        assert_eq!(ctx.lower_sql_expr(&plan,&call("cypher_property_value",vec![binding("x")])).unwrap(),col_exact("x"));
    }
    let nested = DataType::Struct(vec![Field::new("k",DataType::Int64,true)].into());
    for kind in [nested.clone(),DataType::List(Arc::new(Field::new("item",nested,true)))] {
        let plan = scalar_scope(vec![Field::new("x",kind,true)]);
        assert!(ctx.lower_sql_expr(&plan,&call("cypher_property_value",vec![binding("x")])).unwrap_err().to_string().contains("property type requires runtime validation"));
    }
}

#[test]
fn cypher_unavailable_identity_and_label_inputs_have_specific_errors() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![]);
    for (expr,message) in [
        (binding("missing"),"unavailable binding"),
        (IrExpr::Id("missing".into()),"id(missing)"),
        (IrExpr::Label("missing".into()),"label(missing)"),
        (IrExpr::HasLabel {binding:"missing".into(),label:"Person".into()},"has_label"),
        (call("cypher_id",vec![binding("missing")]),"Mapped identity requires"),
        (call("cypher_labels",vec![binding("missing")]),"logical node label metadata"),
        (call("cypher_has_label",vec![binding("missing"),binding("dynamic")]),"bound element and label"),
    ] {
        assert!(ctx.lower_sql_expr(&plan,&expr).unwrap_err().to_string().contains(message),"{expr:?}");
    }
}

#[test]
fn cypher_lists_widen_integer_columns_but_never_stringify_heterogeneous_values() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Int8,DataType::Int16,DataType::Int32,DataType::Int64] {
        let plan = scalar_scope(vec![Field::new("x",kind,true),Field::new("y",DataType::Int64,true)]);
        let expr = IrExpr::List(vec![binding("x"),IrExpr::Lit(Lit::Null),binding("y")]);
        let result = ctx.lower_sql_expr(&plan,&expr).unwrap();
        let DataType::List(field) = result.get_type(plan.schema()).unwrap() else {panic!("expected typed list")};
        assert_eq!(field.data_type(),&DataType::Int64);
    }
    for kind in [DataType::Utf8,DataType::Float64,DataType::Boolean] {
        let plan = scalar_scope(vec![Field::new("x",kind,true),Field::new("y",DataType::Int64,true)]);
        let expr = IrExpr::List(vec![binding("x"),binding("y")]);
        assert!(ctx.lower_sql_expr(&plan,&expr).unwrap_err().to_string().contains("heterogeneous Cypher collection"));
    }
}

#[test]
fn cypher_dynamic_collection_dispatch_checks_arity_before_lowering_operands() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![]);
    for (name,arity) in [("list_append",2),("array_append",2),("array_push_back",2),
        ("list_prepend",2),("array_prepend",2),("array_push_front",2),
        ("list_extract",2),("list_element",2),("element_at",2),("list_unique",1),
        ("list_contains",2),("list_has",2),("array_contains",2),("array_has",2),("list_has_all",2)] {
        for count in 0..4 {
            if count == arity {continue;}
            let args = vec![binding("missing");count];
            assert!(ctx.lower_dynamic_collection_function(&plan,name,&args).unwrap().is_none(),"{name}/{count}");
        }
    }
    assert!(ctx.lower_dynamic_collection_function(&plan,"unknown",&[]).unwrap().is_none());
}

#[test]
fn cypher_constant_list_index_fast_path_preserves_selected_column() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("x",DataType::Int64,true),Field::new("y",DataType::Int64,true)]);
    let items = IrExpr::List(vec![binding("x"),binding("y")]);
    for name in ["list_extract","list_element","element_at"] {
        for (index,expected) in [(1,"x"),(2,"y"),(-1,"y"),(-2,"x")] {
            let result = ctx.lower_dynamic_collection_function(&plan,name,&[items.clone(),IrExpr::Lit(Lit::Int(index))]).unwrap().unwrap();
            assert_eq!(result,col_exact(expected),"{name}/{index}");
        }
    }
    for name in ["list_element","element_at"] {
        for index in [0,3,-3,i64::MIN,i64::MAX] {
            let result = ctx.lower_dynamic_collection_function(&plan,name,&[items.clone(),IrExpr::Lit(Lit::Int(index))]).unwrap().unwrap();
            assert_eq!(result,lit(ScalarValue::Utf8(None)));
        }
        assert!(ctx.lower_dynamic_collection_function(&plan,name,&[items.clone(),binding("x")]).unwrap().is_none());
        assert!(ctx.lower_dynamic_collection_function(&plan,name,&[binding("x"),IrExpr::Lit(Lit::Int(1))]).unwrap().is_none());
    }
}

#[test]
fn cypher_list_dispatch_preserves_native_list_types_and_predicates() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let kind = DataType::List(Arc::new(Field::new("item",DataType::Int64,true)));
    let plan = scalar_scope(vec![Field::new("xs",kind.clone(),true),Field::new("x",DataType::Int64,true)]);
    for name in ["list_append","array_append","array_push_back","list_prepend","array_prepend","array_push_front"] {
        let result = ctx.lower_dynamic_collection_function(&plan,name,&[binding("xs"),binding("x")]).unwrap().unwrap();
        assert_eq!(result.get_type(plan.schema()).unwrap(),kind,"{name}");
    }
    for name in ["list_contains","list_has","array_contains","array_has"] {
        let result = ctx.lower_dynamic_collection_function(&plan,name,&[binding("xs"),binding("x")]).unwrap().unwrap();
        assert_eq!(result.get_type(plan.schema()).unwrap(),DataType::Boolean,"{name}");
    }
    let result = ctx.lower_dynamic_collection_function(&plan,"list_has_all",&[binding("xs"),binding("xs")]).unwrap().unwrap();
    assert_eq!(result.get_type(plan.schema()).unwrap(),DataType::Boolean);
    let result = ctx.lower_dynamic_collection_function(&plan,"list_unique",&[binding("xs")]).unwrap().unwrap();
    assert_eq!(result.get_type(plan.schema()).unwrap(),DataType::Int64);
    assert_eq!(ctx.lower_native_list(&plan,&binding("xs")).unwrap(),col_exact("xs"));
    assert!(ctx.lower_native_list(&plan,&binding("x")).is_err());
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn cypher_case_lowered_sql_preserves_nulls_and_exact_large_integers() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for condition in [Some(true),Some(false),None] {
        let plan = LogicalPlanBuilder::empty(true).project(vec![
            lit(ScalarValue::Boolean(condition)).alias("p"),
            lit(9_007_199_254_740_993_i64).alias("x"),
        ]).unwrap().build().unwrap();
        for null_else in [false,true] {
            let expr = IrExpr::Case {arms:vec![(binding("p"),binding("x"))],otherwise:Some(Box::new(
                if null_else {IrExpr::Lit(Lit::Null)} else {IrExpr::Lit(Lit::Int(-7))}
            ))};
            let lowered = ctx.lower_sql_expr(&plan,&expr).unwrap();
            let output = LogicalPlanBuilder::from(plan.clone()).project(vec![lowered.alias("result")]).unwrap().build().unwrap();
            let batches = crate::ir::rel::execute_test_plan(output).await.unwrap();
            assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(),1);
            let actual = ScalarValue::try_from_array(batches[0].column(0),0).unwrap();
            let expected = if condition == Some(true) {Some(9_007_199_254_740_993)} else if null_else {None} else {Some(-7)};
            assert_eq!(actual,ScalarValue::Int64(expected));
        }
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn cypher_collection_aliases_execute_with_order_duplicates_and_null_elements() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let items = ScalarValue::List(ScalarValue::new_list(&[
        ScalarValue::Int64(Some(7)),ScalarValue::Int64(None),ScalarValue::Int64(Some(7))
    ],&DataType::Int64,true));
    let plan = LogicalPlanBuilder::empty(true).project(vec![lit(items).alias("xs"),lit(9_i64).alias("x")]).unwrap().build().unwrap();
    for name in ["list_append","array_append","array_push_back","list_prepend","array_prepend","array_push_front"] {
        let lowered = ctx.lower_dynamic_collection_function(&plan,name,&[binding("xs"),binding("x")]).unwrap().unwrap();
        let output = LogicalPlanBuilder::from(plan.clone()).project(vec![lowered.alias("result")]).unwrap().build().unwrap();
        let batches = crate::ir::rel::execute_test_plan(output).await.unwrap();
        let ScalarValue::List(array) = ScalarValue::try_from_array(batches[0].column(0),0).unwrap() else {panic!("list expected")};
        let actual = array.value(0);
        let expected = if name.contains("prepend") || name.ends_with("front") {vec![Some(9),Some(7),None,Some(7)]} else {vec![Some(7),None,Some(7),Some(9)]};
        assert_eq!(actual.len(),expected.len());
        for (index,value) in expected.into_iter().enumerate() {
            assert_eq!(ScalarValue::try_from_array(&actual,index).unwrap(),ScalarValue::Int64(value),"{name}/{index}");
        }
    }
}

#[test]
fn cypher_graph_identity_and_distinct_count_preserve_source_key_types() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Int32,DataType::Int64,DataType::UInt64,DataType::Utf8] {
        let plan = scalar_scope(vec![Field::new(id_col("n"),kind.clone(),true),Field::new(label_col("n"),DataType::Utf8,true)]);
        assert_eq!(ctx.lower_sql_expr(&plan,&IrExpr::Id("n".into())).unwrap(),col_exact(id_col("n")));
        assert_eq!(ctx.lower_count_operand(&plan,&binding("n")).unwrap(),col_exact(id_col("n")));
        assert_eq!(ctx.lower_sql_expr(&plan,&IrExpr::IsBound("n".into())).unwrap(),col_exact(id_col("n")).is_not_null());
        assert_eq!(ctx.lower_sql_expr(&plan,&IrExpr::Label("n".into())).unwrap(),col_exact(label_col("n")));
        let distinct = ctx.lower_distinct_count_operand(&plan,&binding("n")).unwrap();
        let Expr::Case(case) = distinct else {panic!("count distinct must exclude absent optional nodes")};
        assert_eq!(*case.when_then_expr[0].0,col_exact(id_col("n")).is_not_null());
        let DataType::Struct(fields) = case.when_then_expr[0].1.get_type(plan.schema()).unwrap() else {panic!("identity must include label and source key")};
        assert_eq!(fields[1].data_type(),&kind);
    }
    let plan = scalar_scope(vec![Field::new(id_col("n"),DataType::Struct(vec![Field::new("part",DataType::Int64,true)].into()),true),Field::new(label_col("n"),DataType::Utf8,true)]);
    assert!(ctx.lower_sql_expr(&plan,&IrExpr::Id("n".into())).unwrap_err().to_string().contains("composite or heterogeneous source IDs"));
}

#[test]
fn cypher_same_type_casts_preserve_columns_without_sql_cast_nodes() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    for (kind,target) in [(DataType::Boolean,"BOOLEAN"),(DataType::Int8,"INT8"),(DataType::Int16,"INT16"),
        (DataType::Int32,"INT32"),(DataType::Int64,"INT64"),(DataType::UInt64,"UINT64"),
        (DataType::Float32,"FLOAT32"),(DataType::Float64,"FLOAT64"),(DataType::Utf8,"VARCHAR"),
        (DataType::Decimal128(18,3),"DECIMAL(18,3)")] {
        let plan=scalar_scope(vec![Field::new("x",kind,true)]);
        let expr=call("cast",vec![binding("x"),IrExpr::Lit(Lit::String(target.into()))]);
        assert_eq!(ctx.lower_sql_expr(&plan,&expr).unwrap(),col_exact("x"),"{target}");
    }
    for (kind,function) in [(DataType::Int64,"tointeger"),(DataType::Float64,"tofloat"),(DataType::Boolean,"toboolean"),(DataType::Utf8,"tostring")] {
        let plan=scalar_scope(vec![Field::new("x",kind,true)]);
        assert_eq!(ctx.lower_sql_expr(&plan,&call(function,vec![binding("x")])).unwrap(),col_exact("x"),"{function}");
    }
}

#[test]
fn property_star_without_catalog_properties_retains_one_typed_null_cell() {
    let graph=PropertyGraph::new(); let ctx=cypher_context(&graph);
    let plan=scalar_scope(vec![Field::new(id_col("n"),DataType::Int64,false),Field::new(label_col("n"),DataType::Utf8,false)]);
    let result=ctx.project_item_exprs(&plan,"out",&call("cypher_property_star",vec![binding("n")])).unwrap();
    assert_eq!(result,vec![lit(ScalarValue::Utf8(None)).alias(format!("out{STAR_SEP}"))]);
    assert_eq!(result[0].get_type(plan.schema()).unwrap(),DataType::Utf8);
}

#[test]
fn property_star_uses_struct_shadow_field_order_and_types_without_neighbor_fields() {
    let graph=PropertyGraph::new(); let ctx=cypher_context(&graph);
    let fields=[("z",DataType::Int64),("a",DataType::Boolean),("text",DataType::Utf8)];
    let mut schema=fields.iter().map(|(name,kind)|Field::new(struct_field_col("n","payload",name),kind.clone(),true)).collect::<Vec<_>>();
    schema.push(Field::new(struct_field_col("n","other","hidden"),DataType::Int64,true));
    let plan=scalar_scope(schema);
    let property=IrExpr::property("n","payload",PropertyMissing::NullOnMissing);
    let result=ctx.project_item_exprs(&plan,"out",&call("cypher_property_star",vec![property])).unwrap();
    assert_eq!(result.len(),3);
    for (expr,(name,kind)) in result.iter().zip(fields) {
        assert_eq!(expr,&col_exact(struct_field_col("n","payload",name)).alias(format!("out{STAR_SEP}{name}")));
        assert_eq!(expr.get_type(plan.schema()).unwrap(),kind);
    }
}

#[test]
fn property_equality_crosses_identity_projection_and_one_row_inner_apply_only() {
    let graph=PropertyGraph::new(); let mut ctx=cypher_context(&graph);
    ctx.options.mapping=Some(Arc::new(mapping::GraphMapping::new()));
    let source=Node::GraphNodeScan { graph:"g".into(),binding:"a".into(),labels:LabelExpr::Any };
    let predicate=IrExpr::eq(IrExpr::property("a","age",PropertyMissing::NullOnMissing),IrExpr::Lit(Lit::Int(7)));
    let projection=|expr|Node::GraphProject { mode:ProjectMode::ReplaceScope,items:vec![ProjectionItem { alias:"a".into(),expr }],error_policy:ProjectErrorPolicy::PropagateError,input:Box::new(source.clone()) };
    let identity=projection(binding("a"));
    let result=ctx.push_expand_equality(&predicate,&identity).unwrap().unwrap();
    let Node::GraphProject { input,.. }=result else { panic!("identity projection must remain") };
    assert_eq!(*input,Node::GraphFilter { condition:predicate.clone(),input:Box::new(source.clone()) });
    assert!(ctx.push_expand_equality(&predicate,&projection(IrExpr::Lit(Lit::Int(3)))).unwrap().is_none());
    let apply=|kind,left|Node::GraphApply { kind,correlation:vec![],outputs:vec!["a".into()],optional_missing:OptionalMissing::Null,left:Box::new(left),right:Box::new(identity.clone()) };
    let result=ctx.push_expand_equality(&predicate,&apply(ApplyKind::Inner,Node::GraphOneRow)).unwrap().unwrap();
    let Node::GraphApply { right,.. }=result else { panic!("apply must remain") };
    let Node::GraphProject { input,.. }=*right else { panic!("identity projection must remain") };
    assert_eq!(*input,Node::GraphFilter { condition:predicate.clone(),input:Box::new(source.clone()) });
    for plan in [apply(ApplyKind::Optional,Node::GraphOneRow),apply(ApplyKind::Inner,source)] {
        assert!(ctx.push_expand_equality(&predicate,&plan).unwrap().is_none());
    }
}
#[test]
fn cypher_list_subscripts_execute_zero_based_negative_null_and_extreme_indices() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let list = ScalarValue::List(ScalarValue::new_list_nullable(&[
        ScalarValue::Int64(Some(11)), ScalarValue::Int64(None), ScalarValue::Int64(Some(33)),
    ], &DataType::Int64));
    let item = Arc::new(Field::new("item", DataType::Int64, true));
    for kind in [list.data_type(), DataType::LargeList(item.clone()), DataType::FixedSizeList(item, 3)] {
    let list = list.cast_to(&kind).unwrap();
    for (index, expected) in [
        (ScalarValue::Int8(Some(0)), Some(11)),
        (ScalarValue::UInt8(Some(2)), Some(33)),
        (ScalarValue::Int16(Some(-1)), Some(33)),
        (ScalarValue::Int32(Some(-3)), Some(11)),
        (ScalarValue::Int64(Some(1)), None),
        (ScalarValue::Int64(Some(3)), None),
        (ScalarValue::Int64(Some(-4)), None),
        (ScalarValue::Int64(Some(i64::MIN)), None),
        (ScalarValue::Int64(Some(i64::MAX)), None),
        (ScalarValue::Int64(Some(i64::from(i32::MAX))), None),
        (ScalarValue::Int64(Some(i64::from(i32::MIN) - 1)), None),
        (ScalarValue::UInt64(Some(u64::MAX)), None),
        (ScalarValue::UInt64(Some(1_u64 << 63)), None),
        (ScalarValue::Int64(None), None),
    ] {
        let plan = scalar_scope(vec![Field::new("items", list.data_type(), true), Field::new("index", index.data_type(), true)]);
        let lowered = ctx.lower_sql_expr(&plan, &call("cypher_subscript", vec![binding("items"), binding("index")])).unwrap();
        if index.data_type() == DataType::Int64 {
            lowered.apply(|expr| {
                assert!(!matches!(expr, Expr::Cast(cast) if matches!(cast.expr.as_ref(), Expr::Column(column) if column.name == "index")), "redundant index cast: {expr}");
                Ok(datafusion::common::tree_node::TreeNodeRecursion::Continue)
            }).unwrap();
        }
        let bound = lowered.transform_up(|expr| Ok(match &expr {
            Expr::Column(column) if column.name == "items" => Transformed::yes(lit(list.clone())),
            Expr::Column(column) if column.name == "index" => Transformed::yes(lit(index.clone())),
            _ => Transformed::no(expr),
        })).unwrap().data;
        assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]), bound), ScalarValue::Int64(expected), "{index:?}");
    }
    }
}
#[tokio::test]
async fn cypher_parsed_extreme_subscripts_return_null_without_constant_folding() {
    let graph = PropertyGraph::new();
    let query = "UNWIND [-9223372036854775808,-2147483649,2147483647,9223372036854775807] AS i RETURN [11,22][i] AS value";
    let plan = crate::language::cypher::preparation::prepare(query, &Default::default(), None).unwrap();
    for native in [false, true] {
        let returned = crate::ir::diagnostics::without_constant_folding(async {
            RelBackend::with_options(RelBackendOptions { native_values: native, ..Default::default() })
                .execute(&plan, &graph).await
        }).await.unwrap();
        let rows = native_values::decode_rows(&returned.batch, &returned.fields, &graph).unwrap();
        assert_eq!(rows.len(), 4);
        for row in rows { assert_eq!(row.get("value"), Value::Null, "native={native}"); }
    }
}

#[test]
fn cypher_contains_alias_preserves_unicode_case_empty_strings_and_nulls_across_arrow_layouts() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for left_type in [DataType::Utf8, DataType::LargeUtf8, DataType::Utf8View] {
        for right_type in [DataType::Utf8, DataType::LargeUtf8, DataType::Utf8View] {
            let plan = scalar_scope(vec![Field::new("left", left_type.clone(), true), Field::new("right", right_type.clone(), true)]);
            let lowered = ctx.lower_sql_expr(&plan, &call("contains_fn", vec![binding("left"), binding("right")])).unwrap();
            for (left, right, expected) in [
                (Some("café猫"), Some("fé猫"), Some(true)),
                (Some("café"), Some("FÉ"), Some(false)),
                (Some(""), Some(""), Some(true)),
                (Some(""), Some("x"), Some(false)),
                (None, Some("x"), None), (Some("x"), None, None), (None, None, None),
            ] {
                let lhs = ScalarValue::Utf8(left.map(str::to_owned)).cast_to(&left_type).unwrap();
                let rhs = ScalarValue::Utf8(right.map(str::to_owned)).cast_to(&right_type).unwrap();
                let bound = lowered.clone().transform_up(|expr| Ok(match &expr {
                    Expr::Column(column) if column.name == "left" => Transformed::yes(lit(lhs.clone())),
                    Expr::Column(column) if column.name == "right" => Transformed::yes(lit(rhs.clone())),
                    _ => Transformed::no(expr),
                })).unwrap().data;
                assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]), bound), ScalarValue::Boolean(expected), "{left_type:?}/{right_type:?}: {left:?}, {right:?}");
            }
        }
    }
}

#[test]
fn cypher_contains_alias_rejects_non_string_operands_and_invalid_arity() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Int64, DataType::Boolean, DataType::List(Arc::new(Field::new("item", DataType::Utf8, true)))] {
        let plan = scalar_scope(vec![Field::new("bad", kind, true), Field::new("text", DataType::Utf8, true)]);
        for args in [vec![binding("bad"), binding("text")], vec![binding("text"), binding("bad")]] {
            let error = ctx.lower_sql_expr(&plan, &call("contains_fn", args)).unwrap_err();
            assert!(error.to_string().contains("requires string operands"), "{error}");
        }
    }
    for count in [0,1,3] {
        let error = ctx.lower_sql_expr(&scalar_scope(vec![]), &call("contains_fn", vec![binding("missing");count])).unwrap_err();
        assert!(error.to_string().contains("contains_fn arity"), "{error}");
    }
    let plan = scalar_scope(vec![Field::new("missing", DataType::Null, true)]);
    assert!(ctx.lower_sql_expr(&plan, &call("contains_fn", vec![binding("missing"), IrExpr::Lit(Lit::String("x".into()))])).is_ok());
}

#[tokio::test]
async fn cypher_contains_function_and_operator_agree_without_constant_folding() {
    let graph = PropertyGraph::new();
    for expression in ["contains(text,'fé')", "text CONTAINS 'fé'"] {
        let query = format!("UNWIND ['café','CAFÉ',null] AS text RETURN {expression} AS value");
        let plan = crate::language::cypher::preparation::prepare(&query, &Default::default(), None).unwrap();
        for native in [false, true] {
            let returned = crate::ir::diagnostics::without_constant_folding(async {
                RelBackend::with_options(RelBackendOptions { native_values: native, ..Default::default() }).execute(&plan, &graph).await
            }).await.unwrap();
            let rows = native_values::decode_rows(&returned.batch, &returned.fields, &graph).unwrap();
            let actual = rows.into_iter().map(|row| row.get("value")).collect::<Vec<_>>();
            assert_eq!(actual, vec![Value::Bool(true), Value::Bool(false), Value::Null], "{expression}, native={native}");
        }
    }
}

#[test]
fn cypher_native_list_casts_preserve_nulls_and_elide_matching_types() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let list = ScalarValue::List(ScalarValue::new_list_nullable(&[
        ScalarValue::Int64(Some(11)), ScalarValue::Int64(None), ScalarValue::Int64(Some(-3)),
    ], &DataType::Int64));
    let item = Arc::new(Field::new("item", DataType::Int64, true));
    for kind in [list.data_type(), DataType::LargeList(item.clone()), DataType::FixedSizeList(item, 3)] {
        for shadow in [false, true] {
            let column = if shadow { collections::list_shadow_col("n", "items") } else { prop_col("n", "items") };
            let input = list.cast_to(&kind).unwrap();
            let plan = scalar_scope(vec![Field::new(&column, kind.clone(), true)]);
            let property = IrExpr::property("n", "items", PropertyMissing::NullOnMissing);
            for target in ["INT64[]", "DOUBLE[]"] {
                let args = vec![property.clone(), IrExpr::Lit(Lit::String(target.into()))];
                let lowered = ctx.lower_list_cast(&plan, "cast", &args, target).unwrap();
                if target == "INT64[]" && kind == list.data_type() {
                    assert_eq!(lowered, col_exact(&column), "matching list type must not add a cast");
                }
                let bound = lowered.transform_up(|expr| Ok(match &expr {
                    Expr::Column(c) if c.name == column => Transformed::yes(lit(input.clone())),
                    _ => Transformed::no(expr),
                })).unwrap().data;
                let actual = fold_cypher_boundary_expression(&scalar_scope(vec![]), bound);
                let element = if target == "INT64[]" { DataType::Int64 } else { DataType::Float64 };
                let expected = list.cast_to(&DataType::List(Arc::new(Field::new("item", element, true)))).unwrap();
                assert_eq!(actual, expected, "{kind:?}, shadow={shadow}, {target}");
            }
            let args = vec![property, IrExpr::Lit(Lit::String("UNKNOWN[]".into()))];
            assert!(ctx.lower_list_cast(&plan, "cast", &args, "UNKNOWN[]").is_err());
        }
    }
}

#[test]
fn cypher_projected_union_tag_subscripts_preserve_nullable_tags() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let tags = ScalarValue::List(ScalarValue::new_list_nullable(&[
        ScalarValue::Utf8(Some("number".into())), ScalarValue::Utf8(None), ScalarValue::Utf8(Some("text".into())),
    ], &DataType::Utf8));
    let item = Arc::new(Field::new("item", DataType::Utf8, true));
    let column = casts::value_union_tag_col("u");
    for kind in [tags.data_type(), DataType::LargeList(item.clone()), DataType::FixedSizeList(item, 3)] {
        let input = tags.cast_to(&kind).unwrap();
        let plan = scalar_scope(vec![Field::new(&column, kind, true)]);
        for alias in ["cypher_subscript", "list_extract", "list_element", "element_at"] {
            for (index, expected) in [(0, Some("number")), (1, None), (-1, Some("text")), (3, None), (i64::MIN, None), (i64::MAX, None)] {
                let expression = call(alias, vec![binding("u"), IrExpr::Lit(Lit::Int(index))]);
                let lowered = ctx.lower_value_union_tag(&plan, &expression).unwrap().expect("tag list should lower");
                let bound = lowered.transform_up(|expr| Ok(match &expr {
                    Expr::Column(c) if c.name == column => Transformed::yes(lit(input.clone())),
                    _ => Transformed::no(expr),
                })).unwrap().data;
                assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]), bound), ScalarValue::Utf8(expected.map(str::to_owned)), "{alias} index={index}");
            }
        }
    }
}

fn bind_boundary_columns(expr: Expr, values: &[(&str, ScalarValue)]) -> Expr {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    expr.transform_up(|expr| {
        if let Expr::Column(column) = &expr {
            if let Some((_, value)) = values.iter().find(|(name, _)| *name == column.name) {
                return Ok(Transformed::yes(lit(value.clone())));
            }
        }
        Ok(Transformed::no(expr))
    }).unwrap().data
}

#[test]
fn cypher_slice_clamps_signed_bounds_and_preserves_null_elements() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let bounds = [i64::MIN, -5, -3, -1, 0, 1, 2, 3, 5, i64::MAX];
    for items in [vec![], vec![Some(11)], vec![Some(11), None, Some(33)]] {
        let values = items.iter().map(|v| ScalarValue::Int64(*v)).collect::<Vec<_>>();
        let list = ScalarValue::List(ScalarValue::new_list_nullable(&values, &DataType::Int64));
        let plan = scalar_scope(vec![Field::new("items", list.data_type(), true), Field::new("start", DataType::Int64, true), Field::new("end", DataType::Int64, true)]);
        let lowered = ctx.lower_sql_expr(&plan, &call("cypher_slice", vec![binding("items"), binding("start"), binding("end")])).unwrap();
        for start in bounds { for end in bounds {
            let bound = bind_boundary_columns(lowered.clone(), &[("items",list.clone()),("start",ScalarValue::Int64(Some(start))),("end",ScalarValue::Int64(Some(end)))]);
            let output = fold_cypher_boundary_expression(&scalar_scope(vec![]), bound);
            let ScalarValue::List(array) = output else {panic!("expected list: {output:?}")};
            let actual = array.value(0);
            let actual = (0..actual.len()).map(|i| match ScalarValue::try_from_array(&actual,i).unwrap() {ScalarValue::Int64(v)=>v,other=>panic!("{other:?}")}).collect::<Vec<_>>();
            let relative = |n: i64| if n < 0 {(items.len() as i64).saturating_add(n)} else {n};
            let from = relative(start).clamp(0, items.len() as i64) as usize;
            let until = relative(end).clamp(0, items.len() as i64) as usize;
            assert_eq!(actual,items[from.min(until)..until],"items={items:?}, start={start}, end={end}");
        }}
    }
}

#[test]
fn cypher_slice_null_inputs_remain_null_and_small_integer_bounds_are_exact() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let list=ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int64(Some(11)),ScalarValue::Int64(None),ScalarValue::Int64(Some(33))],&DataType::Int64));
    for kind in [DataType::Int8,DataType::Int16,DataType::Int32,DataType::Int64,DataType::UInt8,DataType::UInt16,DataType::UInt32] {
        let plan=scalar_scope(vec![Field::new("items",list.data_type(),true),Field::new("start",kind.clone(),true),Field::new("end",kind.clone(),true)]);
        let lowered=ctx.lower_sql_expr(&plan,&call("cypher_slice",vec![binding("items"),binding("start"),binding("end")])).unwrap();
        for null_at in [None,Some(0),Some(1),Some(2)] {
            let mut args=[list.clone(),ScalarValue::Int64(Some(1)).cast_to(&kind).unwrap(),ScalarValue::Int64(Some(3)).cast_to(&kind).unwrap()];
            if let Some(index)=null_at {args[index]=ScalarValue::try_from(args[index].data_type()).unwrap();}
            let result=fold_cypher_boundary_expression(&scalar_scope(vec![]),bind_boundary_columns(lowered.clone(),&[("items",args[0].clone()),("start",args[1].clone()),("end",args[2].clone())]));
            if null_at.is_some(){assert!(result.is_null(),"{kind:?}, {null_at:?}: {result:?}");} else {
                assert_eq!(result,ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int64(None),ScalarValue::Int64(Some(33))],&DataType::Int64)));
            }
        }
    }
}

#[test]
fn cypher_slice_invalid_domains_decline_without_coercing_to_integers_or_lists() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    let list=DataType::List(Arc::new(Field::new("item",DataType::Int64,true)));
    for kind in [DataType::Boolean,DataType::Float64,DataType::Utf8] {
        for bad_start in [true,false] {
            let plan=scalar_scope(vec![Field::new("items",list.clone(),true),Field::new("start",if bad_start {kind.clone()} else {DataType::Int64},true),Field::new("end",if bad_start {DataType::Int64} else {kind.clone()},true)]);
            let error=ctx.lower_sql_expr(&plan,&call("cypher_slice",vec![binding("items"),binding("start"),binding("end")])).unwrap_err().to_string();
            assert!(error.contains("slice bounds require runtime type validation"),"{error}");
        }
    }
    for kind in [DataType::Boolean,DataType::Int64,DataType::Utf8] {
        let plan=scalar_scope(vec![Field::new("items",kind,true)]);
        assert!(ctx.lower_sql_expr(&plan,&call("cypher_slice",vec![binding("items"),IrExpr::Lit(Lit::Int(0)),IrExpr::Lit(Lit::Int(1))])).is_err());
    }
}

#[test]
fn cypher_slice_unsigned_64_bit_bounds_require_runtime_range_validation() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    for unsigned_start in [true,false] {
        let plan=scalar_scope(vec![Field::new("items",DataType::List(Arc::new(Field::new("item",DataType::Int64,true))),true),Field::new("start",if unsigned_start {DataType::UInt64} else {DataType::Int64},true),Field::new("end",if unsigned_start {DataType::Int64} else {DataType::UInt64},true)]);
        assert!(ctx.lower_sql_expr(&plan,&call("cypher_slice",vec![binding("items"),binding("start"),binding("end")])).is_err(),"UInt64 may exceed the runtime slice index domain");
    }
}

#[test]
fn cypher_integer_literal_helpers_preserve_wide_values_and_reject_malformed_input() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    for text in [i64::MIN.to_string(),i64::MAX.to_string(),"0".into(),"+7".into()] {
        let expr=ctx.lower_integer_literal(&IrExpr::Lit(Lit::String(text.clone()))).unwrap();
        assert_eq!(expr,lit(text.parse::<i64>().unwrap()));
    }
    for (text,expected) in [("9_223_372_036_854_775_808","9223372036854775808"),("-9223372036854775809","-9223372036854775809")] {
        let expr=ctx.lower_integer_literal(&IrExpr::Lit(Lit::String(text.into()))).unwrap();
        assert_eq!(expr,lit(expected));
    }
    for arg in [binding("dynamic"),IrExpr::Lit(Lit::String("1.5".into())),IrExpr::Lit(Lit::String("bad".into()))] {
        assert!(ctx.lower_integer_literal(&arg).is_err());
    }
    assert_eq!(integer_literal_text(&IrExpr::Lit(Lit::Int(-8))),Some("-8".into()));
    assert_eq!(literal_i64(&call("integer_literal",vec![IrExpr::Lit(Lit::String("1_000".into()))])),Some(1000));
    assert_eq!(literal_i64(&call("integer_literal",vec![IrExpr::Lit(Lit::String("9223372036854775808".into()))])),None);
}

#[test]
fn cypher_string_predicates_preserve_unicode_null_and_empty_pattern_semantics() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    for kind in [DataType::Utf8,DataType::LargeUtf8,DataType::Utf8View] {
        let plan=scalar_scope(vec![Field::new("text",kind.clone(),true),Field::new("pattern",kind.clone(),true)]);
        for op in [StringOp::StartsWith,StringOp::EndsWith,StringOp::Contains] {
            let lowered=ctx.lower_sql_expr(&plan,&IrExpr::StringPredicate{op,target:Box::new(binding("text")),pattern:Box::new(binding("pattern"))}).unwrap();
            for (text,pattern) in [(Some("猫café雪"),Some("猫")),(Some("猫café雪"),Some("雪")),(Some("猫café雪"),Some("CAFÉ")),(Some(""),Some("")),(Some("x"),Some("")),(None,Some("")),(Some("x"),None)] {
                let expected=text.zip(pattern).map(|(s,p)|match op {StringOp::StartsWith=>s.starts_with(p),StringOp::EndsWith=>s.ends_with(p),StringOp::Contains=>s.contains(p)});
                let expression=bind_boundary_columns(lowered.clone(),&[("text",ScalarValue::Utf8(text.map(str::to_owned)).cast_to(&kind).unwrap()),("pattern",ScalarValue::Utf8(pattern.map(str::to_owned)).cast_to(&kind).unwrap())]);
                assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]),expression),ScalarValue::Boolean(expected));
            }
        }
    }
}

#[test]
fn cypher_slice_null_list_cannot_overflow_one_based_start_conversion() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    let list_type=DataType::List(Arc::new(Field::new("item",DataType::Int64,true)));
    let plan=scalar_scope(vec![Field::new("items",list_type.clone(),true),Field::new("start",DataType::Int64,true),Field::new("end",DataType::Int64,true)]);
    let lowered=ctx.lower_sql_expr(&plan,&call("cypher_slice",vec![binding("items"),binding("start"),binding("end")])).unwrap();
    for end in [i64::MIN,0,i64::MAX] {
        let result=fold_cypher_boundary_expression(&scalar_scope(vec![]),bind_boundary_columns(lowered.clone(),&[("items",ScalarValue::try_from(&list_type).unwrap()),("start",ScalarValue::Int64(Some(i64::MAX))),("end",ScalarValue::Int64(Some(end)))]));
        assert!(result.is_null(),"{result:?}");
    }
}

#[test]
fn cypher_bitwise_lowering_preserves_signed_widths_nulls_and_exact_bits() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    let kinds=[DataType::Int8,DataType::Int16,DataType::Int32,DataType::Int64,DataType::Null];
    for left_type in &kinds {
        for right_type in &kinds {
            let plan=scalar_scope(vec![Field::new("left",left_type.clone(),true),Field::new("right",right_type.clone(),true)]);
            for name in ["bitwise_and","bitwise_or"] {
                let lowered=ctx.lower_sql_expr(&plan,&call(name,vec![binding("left"),binding("right")])).unwrap();
                let Expr::BinaryExpr(binary)=&lowered else {panic!("expected SQL bitwise operator")};
                if *left_type==DataType::Int64 {assert!(matches!(binary.left.as_ref(),Expr::Column(_)));}
                if *right_type==DataType::Int64 {assert!(matches!(binary.right.as_ref(),Expr::Column(_)));}
                for (left,right) in [(Some(-128),Some(127)),(Some(-1),Some(5)),(None,Some(1)),(Some(1),None),(None,None)] {
                    let left=if *left_type==DataType::Null {None}else{left};
                    let right=if *right_type==DataType::Null {None}else{right};
                    let scalar=|value,kind:&DataType|if *kind==DataType::Null {ScalarValue::Null}else{ScalarValue::Int64(value).cast_to(kind).unwrap()};
                    let bound=bind_boundary_columns(lowered.clone(),&[("left",scalar(left,left_type)),("right",scalar(right,right_type))]);
                    let expected=left.zip(right).map(|(a,b)|if name=="bitwise_and" {a&b}else{a|b});
                    assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]),bound),ScalarValue::Int64(expected),"{name} {left_type:?} {right_type:?}");
                }
            }
        }
    }
    for invalid in [DataType::UInt8,DataType::UInt64,DataType::Float64,DataType::Utf8,DataType::Boolean] {
        for swapped in [false,true] {
            let plan=scalar_scope(vec![Field::new("invalid",invalid.clone(),true),Field::new("valid",DataType::Int64,true)]);
            let args=if swapped {vec![binding("valid"),binding("invalid")]}else{vec![binding("invalid"),binding("valid")]};
            for name in ["bitwise_and","bitwise_or"] {
                assert!(ctx.lower_sql_expr(&plan,&call(name,args.clone())).unwrap_err().to_string().contains("integral SQL types"));
            }
        }
    }
}

#[test]
fn cypher_literal_modulo_preserves_signed_remainders_and_nulls() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    let plan=scalar_scope(vec![Field::new("value",DataType::Int64,true)]);
    for name in ["mod","MOD"] {
        for divisor in [-7_i64,-2,2,7] {
            let lowered=ctx.lower_sql_expr(&plan,&call(name,vec![binding("value"),IrExpr::Lit(Lit::Int(divisor))])).unwrap();
            assert!(matches!(&lowered,Expr::BinaryExpr(binary) if binary.op==Operator::Modulo));
            for value in [None,Some(i64::MIN),Some(-9),Some(0),Some(9),Some(i64::MAX)] {
                let bound=bind_boundary_columns(lowered.clone(),&[("value",ScalarValue::Int64(value))]);
                assert_eq!(fold_cypher_boundary_expression(&scalar_scope(vec![]),bound),ScalarValue::Int64(value.map(|v|v%divisor)),"{name} {value:?} divisor={divisor}");
            }
        }
    }
}

#[test]
fn cypher_json_query_lowering_restores_domains_for_dynamic_and_plural_paths() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let empty = scalar_scope(vec![]);
    let document = crate::ir::functions::domain::json_scalar(r#"{"items":[1,null,3],"nothing":null}"#).unwrap();
    for packed in [false, true] {
        let document = if packed {
            fold_cypher_boundary_expression(&empty, native_values::literal(&Value::Scalar(document.clone())).unwrap())
        } else { document.clone() };
        let plan = scalar_scope(vec![Field::new("doc", document.data_type(), true), Field::new("path", DataType::Utf8, true)]);
        for (path, expected) in [
            (Some("$.items[0]"), Some("1")),
            (Some("$.items[*]"), Some("[1,null,3]")),
            (Some("$.nothing"), Some("null")),
            (Some("$.absent"), None),
            (None, None),
        ] {
            for dynamic in [false, true] {
                let path_value = ScalarValue::Utf8(path.map(str::to_owned));
                let path_expr = if dynamic { binding("path") } else { IrExpr::Lit(Lit::Scalar(path_value.clone())) };
                let expr = ctx.lower_sql_expr(&plan, &call("json.query", vec![binding("doc"), path_expr])).unwrap();
                assert!(crate::ir::functions::domain::is_json(&expr.get_type(plan.schema()).unwrap()));
                let bound = expr.transform_up(|expr| Ok(match &expr {
                    Expr::Column(column) if column.name == "doc" => Transformed::yes(lit(document.clone())),
                    Expr::Column(column) if column.name == "path" => Transformed::yes(lit(path_value.clone())),
                    _ => Transformed::no(expr),
                })).unwrap().data;
                let rendered = crate::ir::functions::json::function("json.stringify").unwrap().call(vec![bound]);
                assert_eq!(fold_cypher_boundary_expression(&empty, rendered), ScalarValue::Utf8(expected.map(str::to_owned)), "packed={packed} dynamic={dynamic} path={path:?}");
            }
        }
    }
}

#[test]
fn cypher_json_binary_lowering_restores_both_document_operands() {
    use datafusion::common::tree_node::{TreeNode, Transformed};
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let empty = scalar_scope(vec![]);
    for (operation, left, right, expected) in [
        ("json.equals", r#"{"a":1,"b":2}"#, r#"{"b":2,"a":1}"#, ScalarValue::Boolean(Some(true))),
        ("json.contains", r#"{"a":1,"b":2}"#, r#"{"a":1}"#, ScalarValue::Boolean(Some(true))),
        ("json.contains", r#"{"a":1}"#, r#"{"a":2}"#, ScalarValue::Boolean(Some(false))),
        ("json.merge_patch", r#"{"a":1,"b":2}"#, r#"{"a":null,"c":3}"#, ScalarValue::Utf8(Some(r#"{"b":2,"c":3}"#.into()))),
    ] {
        for (pack_left, pack_right) in [(false, false), (true, false), (false, true), (true, true)] {
            let pack = |text: &str, packed| {
                let value = crate::ir::functions::domain::json_scalar(text).unwrap();
                if packed { fold_cypher_boundary_expression(&empty, native_values::literal(&Value::Scalar(value)).unwrap()) } else { value }
            };
            let left = pack(left, pack_left);
            let right = pack(right, pack_right);
            let plan = scalar_scope(vec![Field::new("left", left.data_type(), true), Field::new("right", right.data_type(), true)]);
            let expr = ctx.lower_sql_expr(&plan, &call(operation, vec![binding("left"), binding("right")])).unwrap();
            let bound = expr.transform_up(|expr| Ok(match &expr {
                Expr::Column(column) if column.name == "left" => Transformed::yes(lit(left.clone())),
                Expr::Column(column) if column.name == "right" => Transformed::yes(lit(right.clone())),
                _ => Transformed::no(expr),
            })).unwrap().data;
            let bound = if operation == "json.merge_patch" {
                crate::ir::functions::json::function("json.stringify").unwrap().call(vec![bound])
            } else { bound };
            assert_eq!(fold_cypher_boundary_expression(&empty, bound), expected, "{operation}: {pack_left}/{pack_right}");
        }
    }
}

#[test]
fn cypher_json_lowering_rejects_untyped_documents_and_invalid_static_paths() {
    use datafusion::logical_expr::execution_props::ExecutionProps;
    let graph = PropertyGraph::new();
    let ctx = cypher_context(&graph);
    let empty = scalar_scope(vec![]);
    let plan = scalar_scope(vec![Field::new("doc", native_values::value_type(), true)]);
    for value in [Value::String(r#"{"a":1}"#.into()), Value::Int(7), Value::Bool(true)] {
        let packed = fold_cypher_boundary_expression(&empty, native_values::literal(&value).unwrap());
        let expr = ctx.lower_sql_expr(&plan, &call("json.query", vec![binding("doc"), IrExpr::Lit(Lit::String("$.a".into()))])).unwrap();
        let schema = Arc::new(Schema::new(vec![Field::new("doc", packed.data_type(), true)]));
        let batch = RecordBatch::try_new(schema, vec![packed.to_array().unwrap()]).unwrap();
        let physical = datafusion_physical_expr::create_physical_expr(&expr, plan.schema(), &ExecutionProps::new()).unwrap();
        let error = physical.evaluate(&batch).unwrap_err();
        assert!(error.to_string().contains("document must have JSON type"), "{value:?}: {error}");
    }
    let doc = crate::ir::functions::domain::json_scalar("{}").unwrap();
    let plan = scalar_scope(vec![Field::new("doc", doc.data_type(), true)]);
    for path in ["$[", "$..", "$['unterminated"] {
        assert!(ctx.lower_sql_expr(&plan, &call("json.query", vec![binding("doc"), IrExpr::Lit(Lit::String(path.into()))])).is_err(), "{path}");
    }
}

fn projection_boundary_scan() -> LogicalPlan {
    use datafusion::datasource::{MemTable, provider_as_source};
    let schema = Arc::new(Schema::new(vec![Field::new("a",DataType::Int64,false),Field::new("b",DataType::Int64,false)]));
    let batch = RecordBatch::try_new(schema.clone(), vec![
        Arc::new(arrow::array::Int64Array::from(vec![1,2,3])) as ArrayRef,
        Arc::new(arrow::array::Int64Array::from(vec![9,8,7])) as ArrayRef,
    ]).unwrap();
    let provider = MemTable::try_new(schema,vec![vec![batch]]).unwrap();
    LogicalPlanBuilder::scan("source",provider_as_source(Arc::new(provider)),None).unwrap().build().unwrap()
}

#[tokio::test]
async fn scan_projection_filter_resolves_shadowed_aliases_by_output_position() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let scan = projection_boundary_scan();
    let plan = LogicalPlanBuilder::from(scan).project(vec![
        col_exact("a").alias("b"), col_exact("b").alias("a"), lit(5_i64).alias("constant"),
    ]).unwrap().build().unwrap();
    let predicate = col_exact("a").gt(lit(7_i64)).and(col_exact("constant").eq(lit(5_i64)));
    let rewritten = ctx.filter_through_scan_projection(&plan,&predicate).unwrap().unwrap();
    assert_eq!(rewritten.schema(),plan.schema());
    let reference = LogicalPlanBuilder::from(plan).filter(predicate).unwrap().build().unwrap();
    let expected = crate::ir::rel::execute_test_plan(reference).await.unwrap();
    let actual = crate::ir::rel::execute_test_plan(rewritten).await.unwrap();
    assert_eq!(actual,expected);
    assert_eq!(actual.iter().map(RecordBatch::num_rows).sum::<usize>(),2);
    let a = actual[0].column(1).as_any().downcast_ref::<arrow::array::Int64Array>().unwrap();
    assert_eq!(a.values().as_ref(), &[9,8]);
}

#[test]
fn scan_projection_filter_fences_computation_correlation_and_disabled_rewrites() {
    let graph = PropertyGraph::new(); let mut ctx = cypher_context(&graph);
    let scan = projection_boundary_scan();
    let naming = LogicalPlanBuilder::from(scan.clone()).project(vec![col_exact("a").alias("renamed")]).unwrap().build().unwrap();
    let predicate = col_exact("renamed").gt(lit(1_i64));
    assert!(ctx.filter_through_scan_projection(&scan,&lit(true)).unwrap().is_none());
    let computed = LogicalPlanBuilder::from(scan).project(vec![(col_exact("a")+lit(1_i64)).alias("renamed")]).unwrap().build().unwrap();
    assert!(ctx.filter_through_scan_projection(&computed,&predicate).unwrap().is_none());
    let nested = LogicalPlanBuilder::from(naming.clone()).project(vec![col_exact("renamed")]).unwrap().build().unwrap();
    assert!(ctx.filter_through_scan_projection(&nested,&predicate).unwrap().is_none());
    let outer = Expr::OuterReferenceColumn(Arc::new(Field::new("outer",DataType::Int64,false)),datafusion::common::Column::from_name("outer"));
    assert!(ctx.filter_through_scan_projection(&naming,&col_exact("renamed").eq(outer)).unwrap().is_none());
    assert!(ctx.filter_through_scan_projection(&naming,&col_exact("missing").eq(lit(1_i64))).is_err());
    ctx.options.rule_context.optimize = false;
    assert!(ctx.filter_through_scan_projection(&naming,&predicate).unwrap().is_none());
}
