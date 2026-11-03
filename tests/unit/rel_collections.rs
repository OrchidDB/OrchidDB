//! Unit contracts for the constant collection lowering path used by Cypher.
use super::*;
fn int(v: i64) -> IrExpr {
    IrExpr::Lit(Lit::Int(v))
}
fn text(v: &str) -> IrExpr {
    IrExpr::Lit(Lit::String(v.into()))
}
fn null() -> IrExpr {
    IrExpr::Lit(Lit::Null)
}
fn list(xs: &[i64]) -> IrExpr {
    IrExpr::List(xs.iter().copied().map(int).collect())
}
fn values(xs: &[i64]) -> Value {
    Value::List(xs.iter().copied().map(Value::Int).collect())
}
fn check(name: &str, args: Vec<IrExpr>, expected: Value) {
    assert_eq!(
        constant_collection_function_value(name, &args).unwrap(),
        Some(expected),
        "{name}:{args:?}"
    );
}

#[test]
fn aliases_preserve_append_prepend_and_concat_order() {
    for name in ["array_append", "array_push_back", "list_append"] {
        check(name, vec![list(&[1, 2]), int(3)], values(&[1, 2, 3]));
    }
    for name in ["array_prepend", "array_push_front", "list_prepend"] {
        check(name, vec![list(&[1, 2]), int(3)], values(&[3, 1, 2]));
    }
    for name in ["list_cat", "list_concat", "array_cat", "array_concat"] {
        check(name, vec![list(&[1, 2]), list(&[3])], values(&[1, 2, 3]));
    }
    check("list_reverse", vec![list(&[1, 2, 3])], values(&[3, 2, 1]));
}
#[test]
fn null_and_dynamic_arguments_preserve_runtime_fallback() {
    let unary = [
        "list_distinct",
        "list_unique",
        "list_any_value",
        "list_reverse",
        "list_sum",
        "list_product",
        "map_keys",
    ];
    let binary = [
        "list_position",
        "list_contains",
        "list_has_all",
        "list_append",
        "list_prepend",
        "list_concat",
        "list_to_string",
        "list_extract",
    ];
    for name in unary.into_iter().chain(binary) {
        assert_eq!(
            constant_collection_function_value(name, &[]).unwrap(),
            None,
            "{name}"
        );
        let args = if unary.contains(&name) {
            vec![IrExpr::binding("x")]
        } else {
            vec![IrExpr::binding("x"), int(1)]
        };
        assert_eq!(
            constant_collection_function_value(name, &args).unwrap(),
            None,
            "{name}"
        );
        let args = if unary.contains(&name) {
            vec![null()]
        } else {
            vec![null(), int(1)]
        };
        assert_eq!(
            constant_collection_function_value(name, &args).unwrap(),
            Some(Value::Null),
            "{name}"
        );
    }
    for name in binary {
        assert_eq!(
            constant_collection_function_value(name, &[list(&[1]), IrExpr::binding("x")]).unwrap(),
            None,
            "{name}"
        );
    }
    for name in unary.into_iter().chain(binary) {
        let args = if unary.contains(&name) {
            vec![int(1)]
        } else {
            vec![int(1), int(2)]
        };
        let expected = if name == "map_keys" {
            Some(values(&[]))
        } else {
            None
        };
        assert_eq!(
            constant_collection_function_value(name, &args).unwrap(),
            expected,
            "{name}"
        );
    }
    assert_eq!(
        constant_collection_function_value("unknown", &[int(1)]).unwrap(),
        None
    );
}
#[test]
fn membership_and_position_skip_null_items_and_keep_one_based_indexes() {
    for name in [
        "array_indexof",
        "array_position",
        "list_indexof",
        "list_position",
    ] {
        check(name, vec![list(&[7, 2, 7]), int(7)], Value::Long(1));
        check(name, vec![list(&[7, 2, 7]), int(9)], Value::Long(0));
        check(name, vec![list(&[7, 2, 7]), null()], Value::Null);
    }
    for name in ["array_contains", "array_has", "list_contains", "list_has"] {
        check(name, vec![list(&[7, 2]), int(2)], Value::Bool(true));
        check(name, vec![list(&[7, 2]), int(3)], Value::Bool(false));
        check(name, vec![list(&[7, 2]), null()], Value::Null);
    }
    check(
        "list_has_all",
        vec![list(&[1, 2]), list(&[2, 2])],
        Value::Bool(true),
    );
    check(
        "list_has_all",
        vec![list(&[1, 2]), list(&[3])],
        Value::Bool(false),
    );
    check(
        "list_has_all",
        vec![list(&[1]), IrExpr::List(vec![null(), int(1)])],
        Value::Bool(true),
    );
    check("list_has_all", vec![list(&[1]), null()], Value::Null);
    assert_eq!(constant_list_has_all(&[list(&[1]), int(1)]).unwrap(), None);
}
#[test]
fn distinct_and_any_value_keep_first_occurrence_order() {
    let input = IrExpr::List(vec![null(), int(2), int(1), int(2), null()]);
    check("list_distinct", vec![input.clone()], values(&[2, 1]));
    check("list_unique", vec![input.clone()], Value::Int(2));
    check("list_any_value", vec![input], Value::Int(2));
    check(
        "list_any_value",
        vec![IrExpr::List(vec![null()])],
        Value::Null,
    );
    check("list_any_value", vec![list(&[])], Value::Null);
    let input = vec![
        Value::Null,
        Value::Int(2),
        Value::Float(2.0),
        Value::Bool(true),
        Value::Int(1),
        Value::Null,
    ];
    assert_eq!(
        list_distinct_values(&input, true),
        vec![Value::Null, Value::Int(2), Value::Bool(true), Value::Int(1)]
    );
    assert_eq!(
        list_distinct_values(&input, false),
        vec![Value::Int(2), Value::Bool(true), Value::Int(1)]
    );
}
#[test]
fn nullable_numeric_folds_use_correct_identity_and_promotion() {
    check("list_sum", vec![list(&[])], Value::Long(0));
    check("list_product", vec![list(&[])], Value::Long(1));
    let input = IrExpr::List(vec![int(2), null(), int(3)]);
    check("list_sum", vec![input.clone()], Value::Long(5));
    check("list_product", vec![input], Value::Long(6));
    let input = IrExpr::List(vec![IrExpr::Lit(Lit::Float(1.5)), null(), int(2)]);
    check("list_sum", vec![input.clone()], Value::Float(3.5));
    check("list_product", vec![input], Value::Float(3.0));
    let input = IrExpr::List(vec![
        IrExpr::Call {
            name: "cast".into(),
            args: vec![int(2), text("FLOAT")],
        },
        int(3),
    ]);
    check("list_product", vec![input], Value::Float32(6.0));
    check(
        "list_product",
        vec![list(&[i64::MAX, 2])],
        Value::BigInt((i64::MAX as i128 * 2).into()),
    );
}
#[test]
fn integer_sum_must_not_lose_precision_through_float_conversion() {
    check(
        "list_sum",
        vec![list(&[9_007_199_254_740_993])],
        Value::Long(9_007_199_254_740_993),
    );
}
#[test]
fn invalid_numeric_items_produce_a_binder_diagnostic() {
    for name in ["list_sum", "list_product"] {
        let value =
            constant_collection_function_value(name, &[IrExpr::List(vec![text("not a number")])])
                .unwrap()
                .unwrap();
        let Value::String(error) = value else {
            panic!("{name}: {value:?}")
        };
        assert!(error.starts_with("Binder exception:"), "{error}");
        assert!(error.contains("Unsupported inner data type"), "{error}");
    }
}
#[test]
fn list_join_skips_nulls_in_both_argument_orders() {
    let input = IrExpr::List(vec![text("a"), null(), text("é")]);
    for name in ["list_join", "list_to_string"] {
        check(
            name,
            vec![input.clone(), text("|")],
            Value::String("a|é".into()),
        );
        check(
            name,
            vec![text("|"), input.clone()],
            Value::String("a|é".into()),
        );
        check(name, vec![input.clone(), null()], Value::Null);
    }
}
#[test]
fn sorting_honors_direction_null_placement_and_duplicates() {
    let input = IrExpr::List(vec![int(2), null(), int(1), int(2)]);
    for (name, args, expected) in [
        (
            "list_sort",
            vec![input.clone()],
            vec![Value::Null, Value::Int(1), Value::Int(2), Value::Int(2)],
        ),
        (
            "list_sort",
            vec![input.clone(), text("DESC")],
            vec![Value::Null, Value::Int(2), Value::Int(2), Value::Int(1)],
        ),
        (
            "list_sort",
            vec![input.clone(), text("ASC"), text("NULLS LAST")],
            vec![Value::Int(1), Value::Int(2), Value::Int(2), Value::Null],
        ),
        (
            "list_reverse_sort",
            vec![input.clone()],
            vec![Value::Null, Value::Int(2), Value::Int(2), Value::Int(1)],
        ),
        (
            "list_reverse_sort",
            vec![input.clone(), text("NULLS LAST")],
            vec![Value::Int(2), Value::Int(2), Value::Int(1), Value::Null],
        ),
    ] {
        check(name, args, Value::List(expected));
    }
    for reverse in [false, true] {
        for args in [
            vec![],
            vec![int(1)],
            vec![input.clone(), int(1)],
            vec![input.clone(), text("ASC"), text("NULLS FIRST"), int(1)],
            vec![IrExpr::binding("x")],
        ] {
            assert_eq!(
                constant_list_sort(&args, reverse).unwrap(),
                None,
                "{args:?}"
            );
        }
        assert_eq!(
            constant_list_sort(&[null()], reverse).unwrap(),
            Some(Value::Null)
        );
    }
}
#[test]
fn one_based_indices_are_unicode_aware_and_bounds_checked() {
    let input = vec![
        Value::String("a".into()),
        Value::String("😀".into()),
        Value::String("é".into()),
    ];
    let expressions = vec![text("a"), text("😀"), text("é")];
    for (index, expected, expr) in [
        (0, Value::Null, None),
        (1, Value::String("a".into()), Some(&expressions[0])),
        (2, Value::String("😀".into()), Some(&expressions[1])),
        (-1, Value::String("é".into()), Some(&expressions[2])),
        (-3, Value::String("a".into()), Some(&expressions[0])),
        (-4, Value::Null, None),
        (4, Value::Null, None),
        (i64::MIN, Value::Null, None),
        (i64::MAX, Value::Null, None),
    ] {
        assert_eq!(list_element_1_based(&input, index), expected);
        assert_eq!(string_index_1_based("a😀é", index), expected);
        assert_eq!(list_element_1_based_expr(&expressions, index), expr);
    }
    assert_eq!(string_index_1_based("", 1), Value::Null);
    for name in ["element_at", "list_element", "list_extract"] {
        check(name, vec![list(&[1, 2]), int(-1)], Value::Int(2));
        check(name, vec![text("a😀é"), int(2)], Value::String("😀".into()));
        check(name, vec![list(&[1, 2]), null()], Value::Null);
    }
}
#[test]
fn inclusive_slices_clamp_and_count_unicode_characters() {
    for (start, end, expected) in [
        (Value::Null, Value::Null, "a😀é"),
        (Value::Int(2), Value::Int(2), "😀"),
        (Value::Int(-2), Value::Int(-1), "😀é"),
        (Value::Int(-99), Value::Int(99), "a😀é"),
        (Value::Int(3), Value::Int(1), ""),
        (Value::String("x".into()), Value::String("y".into()), "a😀é"),
    ] {
        assert_eq!(string_slice_range("a😀é", &start, &end), expected);
    }
    for name in ["array_slice", "list_slice"] {
        check(
            name,
            vec![list(&[1, 2, 3]), int(2), int(3)],
            values(&[2, 3]),
        );
        check(
            name,
            vec![text("a😀é"), int(2), int(3)],
            Value::String("😀é".into()),
        );
        check(name, vec![null(), int(1), int(3)], Value::Null);
    }
    for args in [
        vec![],
        vec![int(1), int(2), int(3)],
        vec![IrExpr::binding("x"), int(1), int(2)],
        vec![list(&[1]), IrExpr::binding("x"), int(2)],
        vec![list(&[1]), int(1), IrExpr::binding("x")],
    ] {
        assert_eq!(constant_array_slice(&args).unwrap(), None);
    }
}
#[test]
fn ranges_handle_direction_overflow_and_expansion_budget() {
    for (args, expected) in [
        (
            vec![int(1), int(3)],
            vec![Value::Int(1), Value::Int(2), Value::Int(3)],
        ),
        (vec![int(3), int(1)], vec![]),
        (
            vec![int(3), int(-1), int(-2)],
            vec![Value::Int(3), Value::Int(1), Value::Int(-1)],
        ),
        (
            vec![int(i64::MAX), int(i64::MAX)],
            vec![Value::Int(i64::MAX)],
        ),
        (
            vec![int(i64::MIN), int(i64::MIN), int(-1)],
            vec![Value::Int(i64::MIN)],
        ),
    ] {
        assert_eq!(constant_range_values(&args).unwrap(), expected);
    }
    for args in [
        vec![],
        vec![int(1)],
        vec![int(1), int(2), int(0)],
        vec![IrExpr::binding("x"), int(2)],
        vec![int(1), IrExpr::binding("x")],
        vec![int(1), int(2), IrExpr::binding("x")],
        vec![int(0), int(100_001)],
    ] {
        assert!(constant_range_values(&args).is_err(), "{args:?}");
    }
}
#[test]
fn unwind_distinguishes_empty_inner_and_empty_outer_rows() {
    for expr in [null(), list(&[])] {
        assert_eq!(constant_unwind_values(&expr, false).unwrap(), Some(vec![]));
        assert_eq!(
            constant_unwind_values(&expr, true).unwrap(),
            Some(vec![Value::Null])
        );
    }
    assert_eq!(
        constant_unwind_values(&int(7), false).unwrap(),
        Some(vec![Value::Int(7)])
    );
    assert_eq!(
        constant_unwind_values(&list(&[1, 2]), false).unwrap(),
        Some(vec![Value::Int(1), Value::Int(2)])
    );
    assert_eq!(
        constant_unwind_values(&IrExpr::binding("x"), false).unwrap(),
        None
    );
    assert_eq!(
        constant_unwind_values(&IrExpr::List(vec![IrExpr::binding("x")]), false).unwrap(),
        None
    );
    let range = IrExpr::Call {
        name: "cypher_range".into(),
        args: vec![int(1), int(2)],
    };
    assert_eq!(
        constant_unwind_values(&range, false).unwrap(),
        Some(vec![Value::Int(1), Value::Int(2)])
    );
}
#[test]
fn map_constructors_check_lengths_duplicates_and_dynamic_values() {
    assert!(constant_cypher_map(&[IrExpr::List(vec![text("a")]), list(&[])]).is_err());
    assert!(
        constant_cypher_map(&[IrExpr::List(vec![text("a"), text("a")]), list(&[1, 2])]).is_err()
    );
    for args in [
        vec![text("a")],
        vec![int(1), int(2)],
        vec![text("a"), IrExpr::binding("x")],
        vec![IrExpr::List(vec![IrExpr::binding("x")]), list(&[1])],
        vec![
            IrExpr::List(vec![text("a")]),
            IrExpr::List(vec![IrExpr::binding("x")]),
        ],
    ] {
        assert_eq!(constant_cypher_map(&args).unwrap(), None);
    }
    let expected = Value::Map(BTreeMap::from([
        ("a".into(), Value::Int(1)),
        ("b".into(), Value::Int(2)),
    ]));
    assert_eq!(
        constant_cypher_map(&[text("a"), int(1), text("b"), int(2)]).unwrap(),
        Some(expected.clone())
    );
    assert_eq!(
        constant_cypher_map(&[IrExpr::List(vec![text("a"), text("b")]), list(&[1, 2])]).unwrap(),
        Some(expected)
    );
}
#[test]
fn semantic_collection_equality_handles_nested_nulls_and_map_keys() {
    assert!(list_semantic_eq(&Value::Null, &Value::Null));
    assert!(!list_semantic_eq(&Value::Null, &Value::Int(1)));
    assert!(!list_semantic_eq(&Value::Int(1), &Value::Null));
    assert!(list_semantic_eq(
        &values(&[1, 2]),
        &Value::List(vec![Value::Float(1.0), Value::Int(2)])
    ));
    assert!(!list_semantic_eq(&values(&[1]), &values(&[1, 2])));
    let map = |key: &str, value| Value::Map(BTreeMap::from([(key.into(), value)]));
    assert!(list_semantic_eq(
        &map("a", Value::Int(1)),
        &map("a", Value::Float(1.0))
    ));
    assert!(!list_semantic_eq(
        &map("a", Value::Int(1)),
        &map("b", Value::Int(1))
    ));
    assert!(!list_semantic_eq(
        &map("a", Value::Int(1)),
        &Value::Map(BTreeMap::new())
    ));
}

#[test]
fn integer_sum_preserves_cancellation_and_widens_overflow() {
    for (input, expected) in [
        (vec![9007199254740993, -9007199254740992], Value::Long(1)),
        (vec![i64::MAX, 1, -1], Value::Long(i64::MAX)),
        (vec![i64::MAX, 1], Value::BigInt(BigInt::from(i64::MAX) + 1)),
        (
            vec![i64::MIN, -1],
            Value::BigInt(BigInt::from(i64::MIN) - 1),
        ),
        (vec![], Value::Long(0)),
    ] {
        check("list_sum", vec![list(&input)], expected);
    }
    check(
        "list_sum",
        vec![IrExpr::List(vec![
            int(1),
            IrExpr::Lit(Lit::Float(0.5)),
            null(),
        ])],
        Value::Float(1.5),
    );
}
