//! Constant lowering contracts; included by the production constants module.
use super::*;
use datafusion::prelude::col;

fn integer(value: i64) -> IrExpr {
    IrExpr::Lit(Lit::Int(value))
}
fn string(value: &str) -> IrExpr {
    IrExpr::Lit(Lit::String(value.into()))
}
fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        name: name.into(),
        args,
    }
}
fn scalar(expr: Expr) -> ScalarValue {
    match expr {
        Expr::Literal(value, _) => value,
        other => panic!("expected literal, got {other:?}"),
    }
}

#[test]
fn literal_lowering_preserves_native_scalar_type_and_null() {
    for (input, expected) in [
        (Lit::Null, ScalarValue::Null),
        (Lit::Bool(true), ScalarValue::Boolean(Some(true))),
        (Lit::Bool(false), ScalarValue::Boolean(Some(false))),
        (Lit::Int(-42), ScalarValue::Int64(Some(-42))),
        (Lit::Float(1.25), ScalarValue::Float64(Some(1.25))),
        (
            Lit::String("é'\\".into()),
            ScalarValue::Utf8(Some("é'\\".into())),
        ),
        (
            Lit::Scalar(ScalarValue::UInt64(Some(u64::MAX))),
            ScalarValue::UInt64(Some(u64::MAX)),
        ),
    ] {
        assert_eq!(scalar(lit_to_expr(&input)), expected);
    }
    assert_eq!(lit_to_value(&Lit::Null), Value::Null);
    assert_eq!(lit_to_value(&Lit::Bool(true)), Value::Bool(true));
    assert_eq!(lit_to_value(&Lit::Int(42)), Value::Int(42));
    assert_eq!(lit_to_value(&Lit::Float(1.25)), Value::Float(1.25));
    assert_eq!(
        lit_to_value(&Lit::String("x".into())),
        Value::String("x".into())
    );
    assert_eq!(
        lit_to_value(&Lit::Scalar(ScalarValue::Int8(Some(-128)))),
        Value::Scalar(ScalarValue::Int8(Some(-128)))
    );
}

#[test]
fn scalar_literal_construction_checks_unsigned_overflow() {
    for (input, expected) in [
        (Value::Null, ScalarValue::Null),
        (Value::Bool(false), ScalarValue::Boolean(Some(false))),
        (Value::Byte(-128), ScalarValue::Int8(Some(-128))),
        (Value::Short(-32768), ScalarValue::Int16(Some(-32768))),
        (Value::UInt8(255), ScalarValue::Int64(Some(255))),
        (Value::UInt16(65535), ScalarValue::Int64(Some(65535))),
        (
            Value::UInt32(u32::MAX),
            ScalarValue::Int64(Some(u32::MAX.into())),
        ),
        (
            Value::UInt64(i64::MAX as u64),
            ScalarValue::Int64(Some(i64::MAX)),
        ),
        (Value::Int(-42), ScalarValue::Int64(Some(-42))),
        (Value::Long(42), ScalarValue::Int64(Some(42))),
        (Value::Float32(1.25), ScalarValue::Float32(Some(1.25))),
        (Value::Float(1.25), ScalarValue::Float64(Some(1.25))),
        (
            Value::String("x".into()),
            ScalarValue::Utf8(Some("x".into())),
        ),
        (
            Value::DateTime("2024-01-01".into()),
            ScalarValue::Utf8(Some("2024-01-01".into())),
        ),
        (
            Value::Scalar(ScalarValue::UInt64(Some(u64::MAX))),
            ScalarValue::UInt64(Some(u64::MAX)),
        ),
    ] {
        assert_eq!(
            scalar(value_literal_expr(&input).unwrap()),
            expected,
            "{input:?}"
        );
    }
    assert!(value_literal_expr(&Value::UInt64(u64::MAX)).is_err());
    assert!(value_literal_expr(&Value::List(vec![])).is_err());
    assert!(value_literal_expr(&Value::Map(Default::default())).is_err());
}

#[test]
fn constant_detection_respects_local_lambda_bindings() {
    let list = Box::new(IrExpr::List(vec![integer(1), integer(2)]));
    for body in [IrExpr::binding("item"), IrExpr::binding("outer")] {
        let local = matches!(&body,IrExpr::Binding(name) if name=="item");
        for expression in [
            IrExpr::ListTransform {
                list: list.clone(),
                item: "item".into(),
                map: Box::new(body.clone()),
            },
            IrExpr::ListFilter {
                list: list.clone(),
                item: "item".into(),
                predicate: Box::new(body.clone()),
            },
            IrExpr::Quantifier {
                kind: crate::ir::plan::QuantifierKind::Any,
                list: list.clone(),
                item: "item".into(),
                predicate: Box::new(body.clone()),
            },
        ] {
            assert_eq!(expr_is_constant(&expression, &[]), local);
            assert!(expr_is_constant(&expression, &["outer"]));
        }
    }
    let reduce = IrExpr::ListReduce {
        collection: list,
        accumulator: "sum".into(),
        item: "item".into(),
        map: Box::new(IrExpr::Binary {
            op: BinaryOp::Add,
            lhs: Box::new(IrExpr::binding("sum")),
            rhs: Box::new(IrExpr::binding("item")),
        }),
    };
    assert!(expr_is_constant(&reduce, &[]));
    assert!(!expr_is_constant(&IrExpr::binding("item"), &[]));
    for expression in [
        IrExpr::ListTransform {
            list: Box::new(IrExpr::binding("item")),
            item: "item".into(),
            map: Box::new(integer(1)),
        },
        IrExpr::ListFilter {
            list: Box::new(IrExpr::binding("item")),
            item: "item".into(),
            predicate: Box::new(integer(1)),
        },
        IrExpr::ListReduce {
            collection: Box::new(IrExpr::binding("sum")),
            accumulator: "sum".into(),
            item: "item".into(),
            map: Box::new(integer(1)),
        },
    ] {
        assert!(!expr_is_constant(&expression, &[]));
    }
}

#[test]
fn every_expression_container_propagates_free_dependencies() {
    for input in [integer(1), IrExpr::binding("free")] {
        let expected = matches!(input, IrExpr::Lit(_));
        for expression in [
            IrExpr::List(vec![integer(0), input.clone()]),
            IrExpr::Not(Box::new(input.clone())),
            IrExpr::IsNull(Box::new(input.clone())),
            IrExpr::IsNotNull(Box::new(input.clone())),
            IrExpr::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(input.clone()),
                rhs: Box::new(integer(1)),
            },
            IrExpr::Binary {
                op: BinaryOp::Add,
                lhs: Box::new(integer(1)),
                rhs: Box::new(input.clone()),
            },
            IrExpr::StringPredicate {
                op: crate::ir::expr::StringOp::Contains,
                target: Box::new(input.clone()),
                pattern: Box::new(string("x")),
            },
            IrExpr::StringPredicate {
                op: crate::ir::expr::StringOp::StartsWith,
                target: Box::new(string("x")),
                pattern: Box::new(input.clone()),
            },
            IrExpr::Case {
                arms: vec![(input.clone(), integer(1))],
                otherwise: None,
            },
            IrExpr::Case {
                arms: vec![(integer(1), input.clone())],
                otherwise: None,
            },
            IrExpr::Case {
                arms: vec![(integer(1), integer(1))],
                otherwise: Some(Box::new(input.clone())),
            },
            call("abs", vec![input.clone()]),
        ] {
            assert_eq!(
                expr_is_constant(&expression, &[]),
                expected,
                "{expression:?}"
            );
        }
    }
    for expression in [
        IrExpr::Id("n".into()),
        IrExpr::Label("n".into()),
        IrExpr::HasLabel {
            binding: "n".into(),
            label: "P".into(),
        },
        IrExpr::property("n", "x", crate::ir::policy::PropertyMissing::NullOnMissing),
    ] {
        assert!(!expr_is_constant(&expression, &["n"]));
    }
    assert!(!expr_is_constant(&call("cypher_temporal.date", vec![]), &[]));
    assert!(!expr_is_constant(&call("rand", vec![]), &[]));
    assert!(!expr_is_constant(&call("unknown_function", vec![]), &[]));
}

#[test]
fn deterministic_function_policy_rejects_clocks_randomness_and_unknown_names() {
    for name in ["abs", "range", "size", "coalesce", "toInteger"] {
        assert!(constant_foldable_function(name), "{name}");
    }
    for name in [
        "rand",
        "random",
        "uuid",
        "gen_random_uuid",
        "now",
        "nextval",
        "currval",
        "__private",
        "unknown_function",
    ] {
        assert!(!constant_foldable_function(name), "{name}");
    }
}

#[test]
fn engine_errors_are_distinguished_from_internal_failures() {
    for message in [
        "Conversion exception: bad cast",
        "Overflow exception: too large",
        "Runtime exception: invalid index",
        "Binder exception: invalid type",
        "Parser exception: unexpected input",
        "Catalog exception: unknown name",
        "RuntimeError: bad value",
        "SyntaxError: invalid",
        "engine exception: message",
    ] {
        assert!(looks_like_engine_error(message), "{message}");
    }
    for message in [
        "",
        "connection refused",
        "conversion failed",
        "internal type mismatch",
    ] {
        assert!(!looks_like_engine_error(message), "{message}");
    }
}

#[test]
fn integer_and_float_sources_preserve_all_numeric_widths() {
    for value in [
        Value::Byte(42),
        Value::Short(42),
        Value::Int(42),
        Value::Long(42),
        Value::UInt8(42),
        Value::UInt16(42),
        Value::UInt32(42),
        Value::UInt64(42),
        Value::BigInt(42.into()),
        Value::UInt128(42.into()),
        Value::BigDecimal("42.0".parse().unwrap()),
        Value::Float32(42.0),
        Value::Float(42.0),
        Value::String("42".into()),
    ] {
        assert_eq!(value_to_bigint(&value), Some(42.into()), "{value:?}");
        assert_eq!(value_to_f64(&value), Some(42.0), "{value:?}");
    }
    assert_eq!(value_to_bigint(&Value::Bool(true)), Some(1.into()));
    assert_eq!(value_to_bigint(&Value::Bool(false)), Some(0.into()));
    for value in [
        Value::Null,
        Value::List(vec![]),
        Value::Map(Default::default()),
        Value::String("bad".into()),
    ] {
        assert_eq!(value_to_bigint(&value), None);
        assert_eq!(value_to_f64(&value), None);
    }
    assert_eq!(value_to_f64(&Value::Bool(true)), None);
}

#[test]
fn bounded_integer_casts_reject_both_sides_of_every_range() {
    for (name, min, max) in [
        ("INT8", -128, 127),
        ("INT16", -32768, 32767),
        ("INT32", i32::MIN as i128, i32::MAX as i128),
        ("INT64", i64::MIN as i128, i64::MAX as i128),
        ("UINT8", 0, 255),
        ("UINT16", 0, 65535),
        ("UINT32", 0, u32::MAX as i128),
        ("UINT64", 0, u64::MAX as i128),
    ] {
        for value in [min, max] {
            assert!(
                constant_cast_value(&Value::String(value.to_string()), name)
                    .unwrap()
                    .is_some(),
                "{name}:{value}"
            );
        }
        for value in [min - 1, max + 1] {
            assert!(
                constant_cast_value(&Value::String(value.to_string()), name).is_err(),
                "{name}:{value}"
            );
        }
        assert!(constant_cast_value(&Value::String("bad".into()), name).is_err());
    }
}

#[test]
fn cast_targets_and_unsupported_sources_preserve_fallback_contract() {
    assert_eq!(
        constant_cast_value(&Value::String("TrUe".into()), " BOOLEAN ").unwrap(),
        Some(Value::Bool(true))
    );
    assert_eq!(
        constant_cast_value(&Value::String("FaLsE".into()), "\"BOOL\"").unwrap(),
        Some(Value::Bool(false))
    );
    assert_eq!(
        constant_cast_value(&Value::Bool(true), "BOOL").unwrap(),
        Some(Value::Bool(true))
    );
    assert_eq!(constant_cast_value(&Value::Int(1), "BOOL").unwrap(), None);
    assert_eq!(
        constant_cast_value(&Value::String("bad".into()), "BOOL").unwrap(),
        None
    );
    for target in ["INT64", "BOOL", "DOUBLE", "DATE", "UNKNOWN"] {
        assert_eq!(
            constant_cast_value(&Value::Null, target).unwrap(),
            Some(Value::Null)
        );
    }
    assert_eq!(
        constant_cast_value(&Value::List(vec![Value::Int(1)]), "INT64[]").unwrap(),
        Some(Value::List(vec![Value::Int(1)]))
    );
    assert_eq!(
        constant_cast_value(&Value::Int(1), "unknown").unwrap(),
        None
    );
    assert_eq!(
        constant_cast_value(&Value::Int(42), "SERIAL").unwrap(),
        Some(Value::Long(42))
    );
    for target in ["FLOAT", "DOUBLE", "FLOAT64", "INT128", "UINT128"] {
        assert!(constant_cast_value(&Value::String("bad".into()), target).is_err());
    }
    assert_eq!(
        constant_cast_value(&Value::Int(-1), "UINT128").unwrap(),
        None
    );
    for target in [
        "DATE",
        "TIMESTAMP",
        "TIMESTAMP_NS",
        "TIMESTAMP_MS",
        "TIMESTAMP_SEC",
        "TIMESTAMP_S",
        "TIMESTAMP_TZ",
    ] {
        assert_eq!(
            constant_cast_value(&Value::String("2024-01-01".into()), target).unwrap(),
            Some(Value::DateTime("2024-01-01".into()))
        );
        assert_eq!(
            constant_cast_value(&Value::DateTime("2024-01-01".into()), target).unwrap(),
            Some(Value::DateTime("2024-01-01".into()))
        );
        assert_eq!(constant_cast_value(&Value::Int(1), target).unwrap(), None);
    }
    assert_eq!(
        constant_cast_target("cast", &[integer(1), string("INT8")]).unwrap(),
        Some("INT8".into())
    );
    assert!(constant_cast_target("cast", &[integer(1)]).is_err());
    assert_eq!(
        constant_cast_target("cast", &[integer(1), IrExpr::binding("type")]).unwrap(),
        None
    );
    assert_eq!(
        constant_cast_target("cast", &[integer(1), integer(2)]).unwrap(),
        None
    );
}

#[test]
fn malformed_union_constructors_are_rejected() {
    for args in [
        vec![],
        vec![integer(1)],
        vec![call("other", vec![])],
        vec![call("map", vec![])],
        vec![call("map", vec![integer(1), integer(2)])],
        vec![call(
            "map",
            vec![string("a"), integer(1), string("b"), integer(2)],
        )],
    ] {
        assert!(union_constructor_field(&args).is_err(), "{args:?}");
    }
    let args = vec![call("map", vec![string("tag"), integer(42)])];
    assert_eq!(
        union_constructor_field(&args).unwrap(),
        ("tag", &integer(42))
    );
}

#[test]
fn constant_values_keep_dynamic_lists_unfolded_and_invalid_literals_diagnostic() {
    assert_eq!(
        constant_value_expr(&IrExpr::List(vec![integer(1), IrExpr::binding("x")])).unwrap(),
        None
    );
    assert_eq!(
        constant_value_expr(&call(
            "integer_literal",
            vec![string("9_223_372_036_854_775_808")]
        ))
        .unwrap(),
        Some(Value::BigInt("9223372036854775808".parse().unwrap()))
    );
    assert_eq!(
        constant_value_expr(&call("integer_literal", vec![string("4_2")])).unwrap(),
        Some(Value::Int(42))
    );
    assert!(constant_value_expr(&call("integer_literal", vec![string("bad")])).is_err());
    assert_eq!(
        constant_value_expr(&call("integer_literal", vec![IrExpr::binding("x")])).unwrap(),
        None
    );
    assert_eq!(
        constant_value_expr(&call("cast", vec![IrExpr::binding("x"), string("INT8")])).unwrap(),
        None
    );
    assert_eq!(
        constant_value_expr(&call("cast", vec![integer(1), IrExpr::binding("target")])).unwrap(),
        None
    );
    assert_eq!(
        constant_value_expr(&call("unknown", vec![integer(1)])).unwrap(),
        None
    );
}

#[test]
fn folded_unary_negation_keeps_width_and_rejects_signed_overflow() {
    let negative = |rhs| IrExpr::Binary {
        op: BinaryOp::Sub,
        lhs: Box::new(integer(0)),
        rhs: Box::new(rhs),
    };
    for (kind, expected) in [
        ("INT8", Value::Byte(-7)),
        ("INT16", Value::Short(-7)),
        ("INT64", Value::Long(-7)),
        ("INT128", Value::BigInt((-7).into())),
        ("FLOAT", Value::Float32(-7.0)),
        ("DOUBLE", Value::Float(-7.0)),
    ] {
        assert_eq!(
            constant_value_expr(&negative(call("cast", vec![integer(7), string(kind)]))).unwrap(),
            Some(expected)
        );
    }
    assert_eq!(
        constant_value_expr(&negative(integer(7))).unwrap(),
        Some(Value::Int(-7))
    );
    for expression in [
        integer(i64::MIN),
        string("x"),
        IrExpr::Lit(Lit::Bool(true)),
        IrExpr::binding("x"),
    ] {
        assert_eq!(constant_value_expr(&negative(expression)).unwrap(), None);
    }
}
#[test]
fn scalar_fold_results_preserve_values_and_numeric_text_boundaries() {
    for (value, expected) in [
        (Value::Null, ScalarValue::Utf8(None)),
        (Value::Bool(true), ScalarValue::Boolean(Some(true))),
        (Value::Byte(-8), ScalarValue::Int8(Some(-8))),
        (Value::Short(-16), ScalarValue::Int16(Some(-16))),
        (Value::UInt8(8), ScalarValue::Int64(Some(8))),
        (Value::UInt16(16), ScalarValue::Int64(Some(16))),
        (Value::UInt32(32), ScalarValue::Int64(Some(32))),
        (Value::UInt64(64), ScalarValue::UInt64(Some(64))),
        (
            Value::UInt64(u64::MAX),
            ScalarValue::UInt64(Some(u64::MAX)),
        ),
        (Value::Int(42), ScalarValue::Int64(Some(42))),
        (Value::Long(42), ScalarValue::Int64(Some(42))),
        (Value::Float32(1.25), ScalarValue::Float32(Some(1.25))),
        (Value::Float(1.25), ScalarValue::Float64(Some(1.25))),
        (
            Value::String("literal".into()),
            ScalarValue::Utf8(Some("literal".into())),
        ),
        (
            Value::DateTime("2024-01-01".into()),
            ScalarValue::Utf8(Some("2024-01-01".into())),
        ),
        (Value::BigInt(42.into()), ScalarValue::Int64(Some(42))),
        (
            Value::BigInt(i128::MAX.into()),
            ScalarValue::Utf8(Some(i128::MAX.to_string())),
        ),
        (
            Value::List(vec![Value::Int(1), Value::Null, Value::Int(2)]),
            ScalarValue::Utf8(Some("[1,,2]".into())),
        ),
    ] {
        assert_eq!(
            scalar(constant_fold_result_expr(&value, Language::Cypher)),
            expected,
            "{value:?}"
        );
    }
}
#[test]
fn cypher_text_display_preserves_unicode_order_and_special_floats() {
    assert_eq!(cypher_float_text(1.25), "1.250000");
    assert_eq!(cypher_float_text(-0.0), "-0.000000");
    assert_eq!(cypher_float_text(f64::NAN), "NaN");
    assert_eq!(cypher_float_text(f64::INFINITY), "inf");
    assert_eq!(cypher_float_text(f64::NEG_INFINITY), "-inf");
    assert_eq!(escape_debug_string("é\\\""), "é\\\\\\\"");
    let map = BTreeMap::from([
        ("visible".into(), Value::Int(1)),
        ("__private".into(), Value::Int(2)),
        (STRUCT_ORDER_KEY.into(), Value::List(vec![])),
        (STRUCT_TYPES_KEY.into(), Value::Null),
    ]);
    assert_eq!(visible_map_keys(&map), vec!["visible"]);
    assert_eq!(
        cypher_plain_value(&Value::Map(BTreeMap::from([
            ("__tag".into(), Value::String("chosen".into())),
            ("__value".into(), Value::Int(42))
        ]))),
        "42"
    );
    assert_eq!(
        scalar(constant_result_expr(
            &Value::Null,
            Language::Cypher,
            DisplayContext::Scalar
        )),
        ScalarValue::Utf8(None)
    );
    assert_eq!(
        scalar(constant_result_expr(
            &Value::String("é".into()),
            Language::Cypher,
            literal_collection_context(Language::Cypher)
        )),
        ScalarValue::Utf8(Some("é".into()))
    );
}

#[test]
fn temporal_detection_visits_each_expression_position() {
    let temporal = call("cypher_temporal.date", vec![string("2024-02-29")]);
    for (leaf, expected) in [(temporal, true), (integer(1), false)] {
        let b = || Box::new(leaf.clone());
        for expr in [
            leaf.clone(), call("coalesce", vec![integer(0), leaf.clone()]),
            IrExpr::List(vec![integer(0), leaf.clone()]),
            IrExpr::Not(b()), IrExpr::IsNull(b()), IrExpr::IsNotNull(b()),
            IrExpr::Binary { op: BinaryOp::Add, lhs: b(), rhs: Box::new(integer(0)) },
            IrExpr::Binary { op: BinaryOp::Add, lhs: Box::new(integer(0)), rhs: b() },
            IrExpr::StringPredicate { op: crate::ir::expr::StringOp::Contains, target: b(), pattern: Box::new(string("x")) },
            IrExpr::StringPredicate { op: crate::ir::expr::StringOp::Contains, target: Box::new(string("x")), pattern: b() },
            IrExpr::Case { arms: vec![(leaf.clone(), integer(0))], otherwise: None },
            IrExpr::Case { arms: vec![(integer(0), leaf.clone())], otherwise: None },
            IrExpr::Case { arms: vec![], otherwise: Some(b()) },
            IrExpr::ListTransform { list: b(), item: "x".into(), map: Box::new(integer(0)) },
            IrExpr::ListTransform { list: Box::new(integer(0)), item: "x".into(), map: b() },
            IrExpr::ListFilter { list: b(), item: "x".into(), predicate: Box::new(integer(0)) },
            IrExpr::ListFilter { list: Box::new(integer(0)), item: "x".into(), predicate: b() },
            IrExpr::Quantifier { kind: crate::ir::plan::QuantifierKind::Any, list: b(), item: "x".into(), predicate: Box::new(integer(0)) },
            IrExpr::Quantifier { kind: crate::ir::plan::QuantifierKind::Any, list: Box::new(integer(0)), item: "x".into(), predicate: b() },
            IrExpr::ListReduce { collection: b(), accumulator: "a".into(), item: "x".into(), map: Box::new(integer(0)) },
            IrExpr::ListReduce { collection: Box::new(integer(0)), accumulator: "a".into(), item: "x".into(), map: b() },
        ] { assert_eq!(has_temporal_constructor(&expr), expected, "{expr:?}"); }
    }
    assert!(!has_temporal_constructor(&IrExpr::binding("date")));
    assert!(!has_temporal_constructor(&IrExpr::Lit(Lit::Scalar(ScalarValue::Int64(Some(0))))));
    let date = crate::ir::temporal::TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
    let encoded = crate::ir::temporal::transport::scalar(&date).unwrap();
    assert!(has_temporal_constructor(&IrExpr::Lit(Lit::Scalar(encoded.clone()))));
    assert_eq!(scalar(value_literal_expr(&Value::Temporal(date)).unwrap()), encoded);
}

#[test]
fn temporal_constants_cross_only_transparent_relational_operators() {
    let date = crate::ir::temporal::TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
    let encoded = crate::ir::temporal::transport::scalar(&date).unwrap();
    let plan = LogicalPlanBuilder::empty(true).project(vec![lit(encoded).alias("d"), lit(7_i64).alias("n")]).unwrap().build().unwrap();
    let expected = BTreeMap::from([("d".into(), Value::Temporal(date)), ("n".into(), Value::Int(7))]);
    assert_eq!(temporal_constant_bindings(&plan), expected);
    let plan = LogicalPlanBuilder::from(plan).alias("t").unwrap().filter(lit(true)).unwrap().sort(vec![col("n").sort(true, false)]).unwrap().limit(0, Some(1)).unwrap().build().unwrap();
    assert_eq!(temporal_constant_bindings(&plan), expected);
    let renamed = LogicalPlanBuilder::from(plan.clone()).project(vec![col("d").alias("renamed"), (col("n") + lit(1_i64)).alias("computed")]).unwrap().build().unwrap();
    let constants = temporal_constant_bindings(&renamed);
    assert_eq!(constants.get("renamed"), expected.get("d"));
    assert!(!constants.contains_key("computed"));
    let aggregate = LogicalPlanBuilder::from(plan).aggregate(vec![col("d")], Vec::<Expr>::new()).unwrap().build().unwrap();
    assert!(temporal_constant_bindings(&aggregate).is_empty());
}

#[test]
fn cypher_value_rendering_preserves_widths_nulls_and_nested_maps() {
    let date = crate::ir::temporal::TemporalValue::Date(chrono::NaiveDate::from_ymd_opt(2024, 2, 29).unwrap());
    for (value, plain, joined) in [
        (Value::Null, "", ""), (Value::Bool(true), "True", "True"), (Value::Bool(false), "False", "False"),
        (Value::Byte(-8), "-8", "-8"), (Value::UInt8(8), "8", "8"), (Value::Short(-16), "-16", "-16"),
        (Value::UInt16(16), "16", "16"), (Value::Int(-32), "-32", "-32"), (Value::UInt32(32), "32", "32"),
        (Value::Long(-64), "-64", "-64"), (Value::UInt64(64), "64", "64"),
        (Value::BigInt(BigInt::from(128)), "128", "128"), (Value::UInt128(BigInt::from(256)), "256", "256"),
        (Value::BigDecimal(bigdecimal::BigDecimal::from_str("1.25").unwrap()), "1.25", "1.25"),
        (Value::Float32(1.5), "1.500000", "1.5"), (Value::Float(2.5), "2.500000", "2.5"),
        (Value::String("é".into()), "é", "é"), (Value::DateTime("2024-02-29".into()), "2024-02-29", "2024-02-29"),
        (Value::Temporal(date), "2024-02-29", "2024-02-29"),
        (Value::InternalId { table: 2, offset: 3 }, "2:3", "2:3"),
        (Value::List(vec![Value::Int(1), Value::Null]), "[1,]", "[1,]"),
        (Value::Path(vec![Value::Int(1), Value::Int(2)]), "[1,2]", "[1,2]"),
        (Value::Map(BTreeMap::from([("x".into(), Value::Int(1)), ("__hidden".into(), Value::Int(9))])), "{x: 1}", "{x: 1}"),
    ] {
        assert_eq!(cypher_plain_value(&value), plain, "{value:?}");
        assert_eq!(display_for_list_to_string(&value), joined, "{value:?}");
        assert_eq!(rel_display_value(&value, Language::Cypher, DisplayContext::Scalar), plain);
    }
    assert_eq!(scalar(constant_result_expr(&Value::Null, Language::Cypher, DisplayContext::Scalar)), ScalarValue::Utf8(None));
    assert_eq!(scalar(constant_result_expr(&Value::Int(42), Language::Cypher, literal_collection_context(Language::Cypher))), ScalarValue::Int64(Some(42)));
}

#[test]
fn struct_display_uses_declared_order_and_union_payload() {
    let mut map = BTreeMap::from([("a".into(), Value::Int(1)), ("z".into(), Value::Int(2))]);
    map.insert(STRUCT_ORDER_KEY.into(), Value::List(vec![Value::String("z".into()), Value::Null, Value::String("missing".into()), Value::String("a".into())]));
    assert_eq!(cypher_plain_value(&Value::Map(map)), "{z: 2, a: 1}");
    let union = Value::Map(BTreeMap::from([("__tag".into(), Value::String("i".into())), ("__value".into(), Value::Int(42))]));
    assert_eq!(cypher_plain_value(&union), "42");
    let map = Value::Map(BTreeMap::from([("\0kuzu_map_entries".into(), Value::List(vec![Value::Null, Value::List(vec![]), Value::List(vec![Value::Int(1), Value::String("one".into())])]))]));
    assert_eq!(cypher_plain_value(&map), "{1=one}");
}

#[test]
fn label_indices_preserve_catalog_order_base_and_stride() {
    assert_eq!(scalar(label_index_case(col("label"), &[], 7, 2)), ScalarValue::Utf8(Some("7".into())));
    let expression = label_index_case(col("label"), &["Z".into(), "A".into()], 3, 2);
    let Expr::Case(case) = expression else { panic!("expected CASE") };
    assert_eq!(case.expr.as_deref(), Some(&col("label")));
    assert_eq!(case.when_then_expr, vec![(Box::new(lit("Z")), Box::new(lit("3"))), (Box::new(lit("A")), Box::new(lit("5")))]);
    assert_eq!(case.else_expr.as_deref(), Some(&lit("?")));
}

#[test]
fn literal_range_validation_preserves_boundary_and_diagnostic() {
    assert_eq!(range::literal_range_bounds(&[integer(i64::MIN), integer(i64::MAX)]).unwrap(), (i64::MIN, i64::MAX, 1));
    assert_eq!(range::literal_range_bounds(&[integer(3), integer(1), integer(-1)]).unwrap(), (3, 1, -1));
    for (args, diagnostic) in [
        (vec![], "range arity"), (vec![integer(1)], "range arity"),
        (vec![integer(1); 4], "range arity"),
        (vec![string("1"), integer(2)], "range start must be a literal integer"),
        (vec![integer(1), string("2")], "range stop must be a literal integer"),
        (vec![integer(1), integer(2), string("1")], "range step must be a literal integer"),
        (vec![integer(1), integer(2), integer(0)], "range step cannot be zero"),
    ] { assert!(range::literal_range_bounds(&args).unwrap_err().to_string().contains(diagnostic)); }
    use datafusion::catalog::TableProvider;
    let provider = range::IntegerRange::new(-1, 3, 2, "x".into());
    assert!(provider.as_any().downcast_ref::<range::IntegerRange>().is_some());
    assert_eq!(provider.table_type(), datafusion::logical_expr::TableType::Temporary);
    assert_eq!(provider.schema().field(0).name(), "x");
    assert!(!provider.schema().field(0).is_nullable());
}

#[test]
fn dynamic_slice_accepts_only_proven_nonnegative_finite_bounds() {
    let convert = |expr| call("cypher_convert.tointeger", vec![expr]);
    let float = |value| IrExpr::Lit(Lit::Float(value));
    let binary = |op, lhs, rhs| IrExpr::Binary { op, lhs: Box::new(lhs), rhs: Box::new(rhs) };
    for expr in [integer(0), integer(i64::MAX), float(0.5), call("random", vec![]), call("rand", vec![]),
        binary(BinaryOp::Add, call("rand", vec![]), float(2.0)),
        binary(BinaryOp::Sub, float(2.0), call("rand", vec![])),
        binary(BinaryOp::Mul, float(2.0), call("rand", vec![])),
        binary(BinaryOp::Mul, float(-2.0), float(-3.0)),
    ] { assert!(dynamic_slice::bound(&convert(expr.clone())).is_some(), "{expr:?}"); }
    for expr in [integer(-1), float(-0.5), float(f64::NAN), float(f64::INFINITY), float(9223372036854775808.0),
        call("rand", vec![integer(1)]), IrExpr::binding("n"),
        binary(BinaryOp::Add, integer(1), integer(2)),
        binary(BinaryOp::Add, integer(9007199254740993), float(0.0)),
        binary(BinaryOp::Add, float(f64::MAX), float(f64::MAX)),
        binary(BinaryOp::Div, float(2.0), float(1.0)),
        binary(BinaryOp::Sub, call("rand", vec![]), float(1.0)),
    ] { assert!(dynamic_slice::bound(&convert(expr.clone())).is_none(), "{expr:?}"); }
    for expr in [integer(1), call("tointeger", vec![integer(1)]), call("cypher_convert.tointeger", vec![]), call("cypher_convert.tointeger", vec![integer(1), integer(2)])] {
        assert!(dynamic_slice::bound(&expr).is_none());
    }
}

#[test]
fn constant_result_scalars_keep_narrow_unsigned_and_float_domains() {
    for (value,expected) in [
        (Value::Byte(i8::MIN),ScalarValue::Int8(Some(i8::MIN))),
        (Value::UInt8(u8::MAX),ScalarValue::UInt8(Some(u8::MAX))),
        (Value::Short(i16::MIN),ScalarValue::Int16(Some(i16::MIN))),
        (Value::UInt16(u16::MAX),ScalarValue::UInt16(Some(u16::MAX))),
        (Value::UInt32(u32::MAX),ScalarValue::UInt32(Some(u32::MAX))),
        (Value::UInt64(u64::MAX),ScalarValue::UInt64(Some(u64::MAX))),
        (Value::Float32(1.25),ScalarValue::Float32(Some(1.25))),
        (Value::Float(f64::INFINITY),ScalarValue::Float64(Some(f64::INFINITY))),
        (Value::Scalar(ScalarValue::Decimal128(Some(125),12,2)),ScalarValue::Decimal128(Some(125),12,2)),
        (Value::Scalar(ScalarValue::Int16(None)),ScalarValue::Int16(None)),
    ] {
        let result=scalar(constant_result_expr(&value,Language::Cypher,DisplayContext::Scalar));
        assert_eq!(result.data_type(),expected.data_type(),"{value:?}");
        assert_eq!(result,expected,"{value:?}");
    }
}

#[test]
fn constant_range_dispatch_preserves_direction_and_rejects_zero_step() {
    for name in ["range","RANGE","cypher_range"] {
        for (start,end,step,expected) in [(1,5,2,vec![1,3,5]),(5,1,-2,vec![5,3,1]),(5,1,1,vec![]),(1,5,-1,vec![])] {
            let result=constant_value_expr(&call(name,vec![integer(start),integer(end),integer(step)])).unwrap();
            assert_eq!(result,Some(Value::List(expected.into_iter().map(Value::Int).collect())),"{name}");
        }
        assert!(constant_value_expr(&call(name,vec![integer(1),integer(5),integer(0)])).is_err());
    }
}

#[test]
fn constant_temporal_dispatch_keeps_nulls_and_defers_row_dependent_inputs() {
    for name in ["date","to_date","timestamp"] {
        assert_eq!(constant_value_expr(&call(name,vec![string("2026-01-02")])).unwrap(),Some(Value::DateTime("2026-01-02".into())));
        assert_eq!(constant_value_expr(&call(name,vec![IrExpr::Lit(Lit::Null)])).unwrap(),Some(Value::Null));
        assert_eq!(constant_value_expr(&call(name,vec![IrExpr::binding("input")])).unwrap(),None);
        assert_eq!(constant_value_expr(&call(name,vec![])).unwrap(),None);
        assert_eq!(constant_value_expr(&call(name,vec![integer(1)])).unwrap(),None);
    }
}

#[test]
fn constant_unwind_preserves_null_empty_and_dynamic_distinctions() {
    for outer in [false,true] {
        let empty=if outer {vec![Value::Null]} else {vec![]};
        for input in [IrExpr::Lit(Lit::Null),IrExpr::List(vec![]),call("cypher_range",vec![integer(2),integer(1)])] {
            assert_eq!(constant_unwind_values(&input,outer).unwrap(),Some(empty.clone()));
        }
        assert_eq!(constant_unwind_values(&integer(7),outer).unwrap(),Some(vec![Value::Int(7)]));
        assert_eq!(constant_unwind_values(&IrExpr::List(vec![integer(7),IrExpr::Lit(Lit::Null),integer(7)]),outer).unwrap(),Some(vec![Value::Int(7),Value::Null,Value::Int(7)]));
        for input in [IrExpr::binding("items"),IrExpr::List(vec![integer(1),IrExpr::binding("item")]),call("unknown",vec![])] {
            assert_eq!(constant_unwind_values(&input,outer).unwrap(),None);
        }
    }
    for (start,stop,step,expected) in [
        (i64::MAX-1,i64::MAX,2,vec![i64::MAX-1]),
        (i64::MIN+1,i64::MIN,-2,vec![i64::MIN+1]),
        (i64::MIN,i64::MAX,i64::MAX,vec![i64::MIN,-1,i64::MAX-1]),
    ] {assert_eq!(constant_range_values(&[integer(start),integer(stop),integer(step)]).unwrap(),expected.into_iter().map(Value::Int).collect::<Vec<_>>());}
    assert!(constant_range_values(&[integer(0),integer(100_002)]).unwrap_err().to_string().contains("too large"));
    for args in [vec![],vec![integer(1)],vec![integer(1);4],vec![string("1"),integer(2)],vec![integer(1),string("2")],vec![integer(1),integer(2),string("1")]] {
        assert!(constant_range_values(&args).is_err());
    }
}

#[test]
fn cypher_constant_maps_preserve_duplicate_policy_and_dynamic_fallbacks() {
    let pairs=vec![string("a"),integer(1),string("a"),integer(2),string("雪"),IrExpr::Lit(Lit::Null)];
    assert_eq!(constant_cypher_map(&pairs).unwrap(),Some(Value::Map(BTreeMap::from([("a".into(),Value::Int(2)),("雪".into(),Value::Null)]))));
    assert_eq!(constant_cypher_map(&[]).unwrap(),Some(Value::Map(BTreeMap::new())));
    let lists=|keys,values|vec![IrExpr::List(keys),IrExpr::List(values)];
    assert_eq!(constant_cypher_map(&lists(vec![string("a"),string("b")],vec![integer(1),integer(2)])).unwrap(),Some(Value::Map(BTreeMap::from([("a".into(),Value::Int(1)),("b".into(),Value::Int(2))]))));
    let error=constant_cypher_map(&lists(vec![string("a"),string("a")],vec![integer(1),integer(2)])).unwrap_err().to_string();
    assert!(error.contains("duplicate key: a"),"{error}");
    assert!(constant_cypher_map(&lists(vec![string("a")],vec![])).unwrap_err().to_string().contains("length mismatch"));
    for args in [vec![string("a")],vec![integer(1),integer(2)],vec![string("a"),IrExpr::binding("value")],lists(vec![IrExpr::binding("key")],vec![integer(1)]),lists(vec![string("a")],vec![IrExpr::binding("value")])] {
        assert_eq!(constant_cypher_map(&args).unwrap(),None);
    }
}

#[test]
fn constant_slice_and_index_helpers_preserve_unicode_and_boundary_semantics() {
    let text="a雪🙂z";
    let items=vec![Value::String("a".into()),Value::String("雪".into()),Value::String("🙂".into()),Value::String("z".into())];
    let expressions=vec![string("a"),string("雪"),string("🙂"),string("z")];
    for (index,position) in [(i64::MIN,None),(-5,None),(-4,Some(0)),(-1,Some(3)),(0,None),(1,Some(0)),(3,Some(2)),(4,Some(3)),(5,None),(i64::MAX,None)] {
        let expected=position.map(|i|items[i].clone()).unwrap_or(Value::Null);
        assert_eq!(list_element_1_based(&items,index),expected);
        assert_eq!(string_index_1_based(text,index),expected);
        assert_eq!(list_element_1_based_expr(&expressions,index),position.map(|i|&expressions[i]));
        assert_eq!(string_index_1_based("",index),Value::Null);
    }
    for (start,end,expected) in [(Value::Null,Value::Null,"a雪🙂z"),(Value::Int(2),Value::Int(3),"雪🙂"),(Value::Int(-2),Value::Int(-1),"🙂z"),(Value::Int(i64::MIN),Value::Int(i64::MAX),"a雪🙂z"),(Value::Int(4),Value::Int(1),""),(Value::String("bad".into()),Value::String("bad".into()),"a雪🙂z")] {
        assert_eq!(string_slice_range(text,&start,&end),expected);
        let actual=list_slice_range(&items,&start,&end);
        assert_eq!(actual,expected.chars().map(|c|Value::String(c.to_string())).collect::<Vec<_>>());
    }
    for (target,expected) in [(string(text),Some(Value::String("雪🙂".into()))),(IrExpr::List(expressions),Some(Value::List(items[1..3].to_vec()))),(IrExpr::Lit(Lit::Null),Some(Value::Null)),(integer(1),None)] {
        assert_eq!(constant_array_slice(&[target,integer(2),integer(3)]).unwrap(),expected);
    }
    for args in [vec![],vec![string(text)],vec![IrExpr::binding("text"),integer(1),integer(2)],vec![string(text),IrExpr::binding("start"),integer(2)],vec![string(text),integer(1),IrExpr::binding("end")]] {
        assert_eq!(constant_array_slice(&args).unwrap(),None);
    }
}

#[test]
fn property_text_rendering_preserves_heterogeneous_native_payloads_and_nulls() {
    use datafusion::logical_expr::execution_props::ExecutionProps;
    use arrow::array::Array;
    let union = crate::ir::functions::scalar_union::scalar;
    let cases = vec![
        (vec![union(&Value::Int(i64::MIN)).unwrap(),union(&Value::String("雪🙂'".into())).unwrap(),union(&Value::Bool(true)).unwrap(),union(&Value::Bool(false)).unwrap(),union(&Value::Null).unwrap()],vec![Some(i64::MIN.to_string()),Some("雪🙂'".into()),Some("True".into()),Some("False".into()),None]),
        (vec![ScalarValue::Float64(Some(1.25)),ScalarValue::Float64(Some(-2.5)),ScalarValue::Float64(None)],vec![Some("1.250000".into()),Some("-2.500000".into()),None]),
        (vec![ScalarValue::Float32(Some(1.25)),ScalarValue::Float32(None)],vec![Some("1.250000".into()),None]),
        (vec![ScalarValue::Int64(Some(i64::MAX)),ScalarValue::Int64(None)],vec![Some(i64::MAX.to_string()),None]),
        (vec![ScalarValue::LargeUtf8(Some("雪🙂'".into())),ScalarValue::LargeUtf8(None)],vec![Some("雪🙂'".into()),None]),
        (vec![ScalarValue::Utf8View(Some("雪🙂'".into())),ScalarValue::Utf8View(None)],vec![Some("雪🙂'".into()),None]),
    ];
    for (values,expected) in cases {
        let ty=values[0].data_type();
        let schema=Arc::new(Schema::new(vec![Field::new("v",ty.clone(),true)]));
        let batch=RecordBatch::try_new(schema.clone(),vec![ScalarValue::iter_to_array(values).unwrap()]).unwrap();
        let df_schema=datafusion::common::DFSchema::try_from(schema.as_ref().clone()).unwrap();
        let expr=render_property_text_expr(col("v"),&ty);
        let physical=datafusion_physical_expr::create_physical_expr(&expr,&df_schema,&ExecutionProps::new()).unwrap();
        let output=physical.evaluate(&batch).unwrap().into_array(batch.num_rows()).unwrap();
        let actual=(0..output.len()).map(|i|if output.is_null(i){None}else{Some(arrow::util::display::array_value_to_string(output.as_ref(),i).unwrap())}).collect::<Vec<_>>();
        assert_eq!(actual,expected,"{ty:?}");
        let empty=RecordBatch::new_empty(schema);
        assert_eq!(physical.evaluate(&empty).unwrap().into_array(0).unwrap().len(),0);
    }
}
