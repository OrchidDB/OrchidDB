use std::collections::BTreeMap;
use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array};

use crate::ir::catalog::{edges_from_columns, nodes_from_columns};

use super::*;

fn call(name: &str, args: &[Value]) -> Value {
    let graph = PropertyGraph::new();
    eval_call(name, args.to_vec(), &graph).unwrap_or(Value::Null)
}

fn call_error(name: &str, args: &[Value]) -> String {
    let graph = PropertyGraph::new();
    eval_call(name, args.to_vec(), &graph)
        .expect_err("runtime call should fail")
        .to_string()
}

fn call_with_graph(name: &str, args: &[Value], graph: &PropertyGraph) -> Value {
    eval_call(name, args.to_vec(), graph).unwrap_or(Value::Null)
}

fn id_projection_graph() -> PropertyGraph {
    let ids: ArrayRef = Arc::new(Int64Array::from(vec![0, 1]));
    let person = nodes_from_columns("person", vec![("ID", ids)]);
    let edge_weight: ArrayRef = Arc::new(Int64Array::from(vec![99]));
    let knows = edges_from_columns(
        "knows",
        "person",
        "person",
        vec![0],
        vec![1],
        vec![("weight", edge_weight)],
    );
    let mut graph = PropertyGraph::new();
    graph.add_nodes(person);
    graph.add_edges(knows).unwrap();
    graph
}

#[test]
fn path_predicates_detect_repeated_nodes_and_edges() {
    let edge = Value::Edge {
        rel_type: "knows".into(),
        id: 12,
        src_label: "person".into(),
        src_id: 7,
        dst_label: "person".into(),
        dst_id: 6,
        projected_properties: None,
    };
    let repeated_node = Value::Path(vec![
        Value::Node {
            label: "person".into(),
            id: 7,
        },
        edge.clone(),
        Value::Node {
            label: "person".into(),
            id: 6,
        },
        Value::Edge {
            rel_type: "knows".into(),
            id: 13,
            src_label: "person".into(),
            src_id: 6,
            dst_label: "person".into(),
            dst_id: 7,
            projected_properties: None,
        },
        Value::Node {
            label: "person".into(),
            id: 7,
        },
    ]);
    let repeated_edge = Value::Path(vec![
        Value::Node {
            label: "person".into(),
            id: 7,
        },
        edge.clone(),
        Value::Node {
            label: "person".into(),
            id: 6,
        },
        edge,
        Value::Node {
            label: "person".into(),
            id: 7,
        },
    ]);

    assert_eq!(call("is_acyclic", &[repeated_node]), Value::Bool(false));
    assert_eq!(call("is_trail", &[repeated_edge]), Value::Bool(false));
}

#[test]
fn properties_projects_internal_element_ids() {
    let graph = id_projection_graph();
    let edge = Value::Edge {
        rel_type: "knows".into(),
        id: 0,
        src_label: "person".into(),
        src_id: 0,
        dst_label: "person".into(),
        dst_id: 1,
        projected_properties: None,
    };

    assert_eq!(
        call_with_graph(
            "properties",
            &[Value::List(vec![edge]), Value::String("_id".into())],
            &graph,
        ),
        Value::List(vec![Value::InternalId {
            table: 1,
            offset: 0,
        }])
    );
}

#[test]
fn list_unique_ignores_nulls() {
    let observed = call(
        "list_unique",
        &[Value::List(vec![
            Value::Null,
            Value::Int(1),
            Value::Long(1),
            Value::Null,
            Value::Int(2),
        ])],
    );

    assert_eq!(observed, Value::Int(2));
}

#[test]
fn list_any_value_skips_nulls() {
    let observed = call(
        "list_any_value",
        &[Value::List(vec![
            Value::Null,
            Value::Null,
            Value::String("first".into()),
            Value::String("second".into()),
        ])],
    );
    let all_null = call(
        "list_any_value",
        &[Value::List(vec![Value::Null, Value::Null])],
    );

    assert_eq!(observed, Value::String("first".into()));
    assert_eq!(all_null, Value::Null);
}

#[test]
fn arithmetic_functions_cover_kuzu_scalar_math() {
    assert_eq!(
        call("factorial", &[Value::Int(14)]),
        Value::Long(87178291200)
    );
    assert!(matches!(call("factorial", &[Value::Int(-1)]), Value::Null));
    assert_eq!(call("even", &[Value::Float(4.1)]), Value::Float(6.0));
    assert_eq!(
        call("bitwise_and", &[Value::Int(640), Value::Int(935)]),
        Value::Long(640)
    );
    assert_eq!(
        call("bitshift_left", &[Value::Int(5), Value::Int(7)]),
        Value::Long(640)
    );

    for (name, expected) in [
        ("cbrt", 1.546680),
        ("ln", 1.308333),
        ("log", 0.568202),
        ("log2", 1.887525),
        ("gamma", 4.170652),
        ("lgamma", 1.428072),
    ] {
        let Value::Float(observed) = call(name, &[Value::Float(3.7)]) else {
            panic!("{name} should return a float");
        };
        assert!(
            (observed - expected).abs() < 0.000001,
            "{name}: observed {observed}, expected {expected}"
        );
    }
}

#[test]
fn list_distinct_uses_nested_semantic_equality() {
    let mut int_map = BTreeMap::new();
    int_map.insert(
        "grades".into(),
        Value::List(vec![Value::Int(80), Value::Long(78)]),
    );
    let mut long_map = BTreeMap::new();
    long_map.insert(
        "grades".into(),
        Value::List(vec![Value::Long(80), Value::Int(78)]),
    );

    let observed = call(
        "list_distinct",
        &[Value::List(vec![
            Value::List(vec![Value::Int(1)]),
            Value::List(vec![Value::Long(1)]),
            Value::Map(int_map.clone()),
            Value::Map(long_map),
            Value::Null,
            Value::Null,
        ])],
    );

    assert_eq!(
        observed,
        Value::List(vec![Value::List(vec![Value::Int(1)]), Value::Map(int_map),])
    );
}

#[test]
fn list_sort_keeps_nulls_first_by_default() {
    let desc = call(
        "list_sort",
        &[
            Value::List(vec![
                Value::Int(2),
                Value::Int(3),
                Value::Int(1),
                Value::Null,
            ]),
            Value::String("DESC".into()),
        ],
    );
    let nulls_last = call(
        "list_sort",
        &[
            Value::List(vec![
                Value::String("sss".into()),
                Value::String("abs".into()),
                Value::Null,
            ]),
            Value::String("ASC".into()),
            Value::String("NULLS LAST".into()),
        ],
    );

    assert_eq!(
        desc,
        Value::List(vec![
            Value::Null,
            Value::Int(3),
            Value::Int(2),
            Value::Int(1),
        ])
    );
    assert_eq!(
        nulls_last,
        Value::List(vec![
            Value::String("abs".into()),
            Value::String("sss".into()),
            Value::Null,
        ])
    );
}

#[test]
fn list_extract_uses_one_based_positions() {
    let observed = call(
        "list_extract",
        &[
            Value::List(vec![Value::Int(5), Value::Int(2), Value::Int(8)]),
            Value::Int(1),
        ],
    );
    let from_text = call(
        "list_extract",
        &[Value::String("[10,5]".into()), Value::Int(2)],
    );

    assert_eq!(observed, Value::Int(5));
    assert_eq!(from_text, Value::Long(5));
}

#[test]
fn list_extract_rejects_non_integer_index() {
    let err = call_error(
        "list_extract",
        &[
            Value::List(vec![Value::Int(5), Value::Int(2), Value::Int(8)]),
            Value::Bool(true),
        ],
    );

    assert!(err.contains(
        "Binder exception: Function LIST_EXTRACT did not receive correct arguments:"
    ));
}

#[test]
fn interval_constructor_normalizes_fractional_and_large_units() {
    assert_eq!(
        call("interval", &[Value::String("1.5 microsecond".into())]),
        Value::String("00:00:00.000002".into())
    );
    assert_eq!(
        call("interval", &[Value::String("1.5 quarter".into())]),
        Value::String("4 months 15 days".into())
    );
    assert_eq!(
        call("duration", &[Value::String("3 millennium".into())]),
        Value::String("3000 years".into())
    );
}

#[test]
fn interval_constructor_reports_strict_parse_errors() {
    assert_eq!(
        call_error("interval", &[Value::String(String::new())]),
        "Conversion exception: Error occurred during parsing interval. Given empty string."
    );
    assert_eq!(
        call_error("interval", &[Value::String("12".into())]),
        "Conversion exception: Error occurred during parsing interval. Field name is missing."
    );
    assert_eq!(
        call_error("interval", &[Value::String("12 13".into())]),
        "Conversion exception: Unrecognized interval specifier string: 13."
    );
    assert_eq!(
        call_error(
            "interval",
            &[Value::String("9999999999:54:32.101234".into())],
        ),
        "Conversion exception: Error occurred during parsing time. Given: \"9999999999:54:32.101234\"."
    );
}

#[test]
fn unsigned_casts_report_kuzu_range_errors() {
    assert_eq!(
        call_error("to_uint64", &[Value::Int(-500)]),
        "Overflow exception: Value -500 is not within UINT64 range"
    );
    assert_eq!(
        call_error("to_uint64", &[Value::BigInt((-15).into())]),
        "Overflow exception: Cast failed. Cannot cast -15 to unsigned type."
    );
    assert_eq!(
        call_error(
            "to_int32",
            &[Value::BigInt(18446744073709551615_u128.into())]
        ),
        "Overflow exception: Value 18446744073709551615 is not within INT32 range"
    );
}

#[test]
fn utility_null_helpers_follow_kuzu_semantics() {
    assert_eq!(
        call("ifnull", &[Value::Null, Value::String("a".into())]),
        Value::String("a".into())
    );
    assert_eq!(
        call(
            "nullif",
            &[Value::String("hello".into()), Value::String("hello".into())]
        ),
        Value::Null
    );
    assert_eq!(
        call("constant_or_null", &[Value::Int(1), Value::Int(10)]),
        Value::Int(1)
    );
    assert_eq!(
        call("constant_or_null", &[Value::Int(1), Value::Null]),
        Value::Null
    );
    assert!(
        call_error("constant_or_null", &[Value::Int(1)])
            .contains("Function CONSTANT_OR_NULL did not receive correct arguments")
    );
}

#[test]
fn abs_preserves_unsigned_and_reports_signed_boundary_overflow() {
    assert_eq!(
        call("abs", &[Value::UInt64(202474672468)]),
        Value::UInt64(202474672468)
    );
    assert_eq!(
        call(
            "abs",
            &[Value::UInt128(
                340282366920938463463374607431768211455_u128.into()
            )]
        ),
        Value::UInt128(340282366920938463463374607431768211455_u128.into())
    );
    assert_eq!(
        call_error("abs", &[Value::Byte(i8::MIN)]),
        "Overflow exception: Cannot take the absolute value of -128 within INT8 range."
    );
    assert_eq!(
        call_error("abs", &[Value::Int(i32::MIN as i64)]),
        "Overflow exception: Cannot take the absolute value of -2147483648 within INT32 range."
    );
    assert_eq!(
        call_error("abs", &[Value::Long(i64::MIN)]),
        "Overflow exception: Cannot take the absolute value of -9223372036854775808 within INT64 range."
    );
}

#[test]
fn utility_error_surfaces_match_binder_cases() {
    assert_eq!(
        call_error(
            "array_concat",
            &[Value::List(vec![Value::Int(1)]), Value::Int(1)]
        ),
        "Binder exception: Function ARRAY_CONCAT did not receive correct arguments:\nActual:   (INT64[],INT64)\nExpected: (LIST,LIST) -> LIST"
    );
    assert_eq!(
        call_error(
            "LIST_TRANSFORM",
            &[Value::List(vec![Value::Int(1)]), Value::Int(1)]
        ),
        "Binder exception: The second argument of LIST_TRANSFORM should be a lambda expression but got LITERAL."
    );
    assert_eq!(
        call_error("date", &[]),
        "Binder exception: Function DATE did not receive correct arguments:\nActual:   ()\nExpected: (STRING) -> DATE"
    );
    assert_eq!(
        call_error("add5", &[Value::Int(1), Value::Int(2)]),
        "Binder exception: Invalid number of arguments for macro ADD5."
    );
    assert_eq!(
        call_error("uuid", &[Value::String("0".into())]),
        "Conversion exception: Invalid UUID: 0"
    );
    assert_eq!(
        call_error(
            "timestamp",
            &[Value::String("2112-08-04 08:23.005612".into())]
        ),
        "Conversion exception: Error occurred during parsing TIMESTAMP. Given: \"2112-08-04 08:23.005612\". Expected format: (YYYY-MM-DD hh:mm:ss[.zzzzzz][+-TT[:tt]])"
    );
}

#[test]
fn map_rejects_null_and_duplicate_keys() {
    assert_eq!(
        call_error(
            "map",
            &[
                Value::List(vec![Value::Null, Value::Null]),
                Value::List(vec![Value::Int(1), Value::Int(2)])
            ]
        ),
        "Runtime exception: Null value key is not allowed in map."
    );
    assert_eq!(
        call_error(
            "map",
            &[
                Value::List(vec![
                    Value::Float(2.75),
                    Value::Float(3.2),
                    Value::Float(3.2)
                ]),
                Value::List(vec![Value::Int(20), Value::Int(34), Value::Int(50)])
            ]
        ),
        "Runtime exception: Found duplicate key: 3.200000 in map."
    );
    assert_eq!(
        call_error(
            "map",
            &[
                Value::List(vec![
                    Value::List(vec![Value::Int(7), Value::Int(8)]),
                    Value::List(vec![Value::Int(7), Value::Int(8)])
                ]),
                Value::List(vec![Value::Int(20), Value::Int(34)])
            ]
        ),
        "Runtime exception: Found duplicate key: [7,8] in map."
    );
}

#[test]
fn list_slice_uses_one_based_inclusive_bounds() {
    let observed = call(
        "list_slice",
        &[
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]),
            Value::Int(1),
            Value::Int(-1),
        ],
    );
    let string_slice = call(
        "list_slice",
        &[Value::String("abcdef".into()), Value::Int(1), Value::Int(4)],
    );

    assert_eq!(
        observed,
        Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    );
    assert_eq!(string_slice, Value::String("abcd".into()));
}

#[test]
fn gremlin_substring_uses_zero_based_end_exclusive_bounds() {
    assert_eq!(
        call(
            "gremlin_substring",
            &[
                Value::String("hello world".into()),
                Value::Int(1),
                Value::Int(8)
            ]
        ),
        Value::String("ello wo".into())
    );
    assert_eq!(
        call(
            "gremlin_substring",
            &[Value::String("ripple".into()), Value::Int(2)]
        ),
        Value::String("pple".into())
    );
    assert_eq!(
        call(
            "gremlin_substring",
            &[
                Value::String("ripple".into()),
                Value::Int(-3),
                Value::Int(-1)
            ]
        ),
        Value::String("pl".into())
    );
}

#[test]
fn list_functions_parse_string_list_literals() {
    let size = call("size", &[Value::String("[10,5]".into())]);
    let contains = call("in", &[Value::Int(5), Value::String("[10,5]".into())]);
    let joined = call(
        "list_to_string",
        &[Value::String(",".into()), Value::String("[10,5]".into())],
    );

    assert_eq!(size, Value::Int(2));
    assert_eq!(contains, Value::Bool(true));
    assert_eq!(joined, Value::String("10,5".into()));
}

#[test]
fn named_casts_support_list_timestamp_and_uuid_types() {
    let timestamp_ms = call(
        "cast",
        &[
            Value::String("1993-05-03 11:13:25.43225".into()),
            Value::String("TIMESTAMP_MS".into()),
        ],
    );
    let timestamp_tz = call(
        "cast",
        &[
            Value::String("1993-05-03 11:13:25.012343".into()),
            Value::String("TIMESTAMP_TZ".into()),
        ],
    );
    let uuid = call(
        "cast",
        &[
            Value::String("a0ee-bc99-9c0b-4ef8-bb6d-6bb9-bd38-0a14".into()),
            Value::String("UUID".into()),
        ],
    );

    assert_eq!(
        timestamp_ms,
        Value::DateTime("1993-05-03 11:13:25.432".into())
    );
    assert_eq!(
        timestamp_tz,
        Value::DateTime("1993-05-03 11:13:25.012343+00".into())
    );
    assert_eq!(
        uuid,
        Value::String("a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a14".into())
    );
    assert_eq!(
        call(
            "timestamp",
            &[Value::String("1970-01-01 00:00:00.004666-10".into())]
        ),
        Value::DateTime("1970-01-01 10:00:00.004666".into())
    );
    assert_eq!(
        call(
            "cast",
            &[
                Value::DateTime("2024-04-05 23:59:59.999".into()),
                Value::String("date".into())
            ]
        ),
        Value::DateTime("2024-04-05".into())
    );
}

#[test]
fn list_has_all_ignores_null_needles() {
    let observed = call(
        "list_has_all",
        &[
            Value::List(vec![Value::Int(5), Value::Int(6)]),
            Value::List(vec![Value::Null]),
        ],
    );

    assert_eq!(observed, Value::Bool(true));
}
