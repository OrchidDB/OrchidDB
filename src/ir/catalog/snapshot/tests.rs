use crate::ir::catalog::{NodeTable, PropertyGraph, nodes_from_columns_with_count};
use crate::ir::value::Value;
use crate::ir::{edges_from_columns, nodes_from_columns};
use arrow::array::{ArrayRef, Float64Array, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;
use std::sync::Arc;

fn roundtrip(g: &PropertyGraph) -> PropertyGraph {
    let bytes = g.snapshot_encode().unwrap();
    PropertyGraph::snapshot_decode(&bytes).unwrap()
}

#[test]
fn roundtrip_full_graph() {
    let mut g = PropertyGraph::new();
    g.add_nodes(nodes_from_columns(
        "Person",
        vec![
            (
                "name",
                Arc::new(StringArray::from(vec!["alice", "bob", "carol"])) as ArrayRef,
            ),
            (
                "age",
                Arc::new(Int64Array::from(vec![30, 40, 50])) as ArrayRef,
            ),
        ],
    ));
    g.add_nodes(nodes_from_columns_with_count("Lonely", vec![], 2));
    g.add_edges(edges_from_columns(
        "LIKES",
        "Person",
        "Person",
        vec![0, 1],
        vec![1, 2],
        vec![],
    ))
    .unwrap();
    g.add_edges(edges_from_columns(
        "LIKES",
        "Person",
        "Lonely",
        vec![0],
        vec![0],
        vec![("since", Arc::new(Int64Array::from(vec![1999])) as ArrayRef)],
    ))
    .unwrap();
    g.add_edges(edges_from_columns(
        "KNOWS",
        "Person",
        "Person",
        vec![2],
        vec![0],
        vec![],
    ))
    .unwrap();

    let n = g.insert_node(
        "Person",
        BTreeMap::from([("name".to_string(), Value::String("dave".into()))]),
    );
    assert_eq!(
        n,
        Value::Node {
            label: "Person".into(),
            id: 3
        }
    );
    let e = g
        .insert_edge(
            "LIKES",
            &Value::Node {
                label: "Person".into(),
                id: 3,
            },
            &Value::Node {
                label: "Person".into(),
                id: 0,
            },
            BTreeMap::from([("since".to_string(), Value::Int(2020))]),
        )
        .unwrap();
    assert!(matches!(e, Value::Edge { id: 3, .. }));
    g.set_property(
        &Value::Node {
            label: "Person".into(),
            id: 3,
        },
        "age",
        Value::Int(27),
    )
    .unwrap();
    g.set_properties(
        &Value::Node {
            label: "Person".into(),
            id: 0,
        },
        BTreeMap::from([("extra".to_string(), Value::Bool(true))]),
        false,
    )
    .unwrap();
    g.set_properties(
        &Value::Node {
            label: "Person".into(),
            id: 1,
        },
        BTreeMap::from([("name".to_string(), Value::String("bobby".into()))]),
        true,
    )
    .unwrap();
    g.delete_value(
        &Value::Node {
            label: "Person".into(),
            id: 2,
        },
        true,
    )
    .unwrap();
    g.delete_value(
        &Value::Edge {
            rel_type: "KNOWS".into(),
            id: 0,
            src_label: "Person".into(),
            src_id: 2,
            dst_label: "Person".into(),
            dst_id: 0,
            projected_properties: None,
        },
        false,
    )
    .unwrap();

    let g2 = roundtrip(&g);
    assert!(g.graph_deep_eq(&g2));

    let bytes = g.snapshot_encode().unwrap();
    let bytes2 = g2.snapshot_encode().unwrap();
    assert_eq!(bytes, bytes2);

    assert_eq!(
        g2.node_label_order().to_vec(),
        vec!["Person".to_string(), "Lonely".to_string()]
    );
    assert_eq!(
        g2.edge_rel_order().to_vec(),
        vec!["LIKES".to_string(), "KNOWS".to_string()]
    );
    assert_eq!(g2.node_property("Person", 3, "age"), Value::Int(27));
    assert_eq!(
        g2.node_property("Person", 1, "name"),
        Value::String("bobby".into())
    );
    assert_eq!(g2.node_ids("Person").unwrap(), vec![0i64, 1, 3]);

    // Allocation counters continue without reusing deleted IDs.
    let n2 = g2.insert_node(
        "Person",
        BTreeMap::from([("name".to_string(), Value::String("eve".into()))]),
    );
    assert_eq!(
        n2,
        Value::Node {
            label: "Person".into(),
            id: 4
        }
    );
    let e2 = g2
        .insert_edge(
            "LIKES",
            &Value::Node {
                label: "Person".into(),
                id: 4,
            },
            &Value::Node {
                label: "Person".into(),
                id: 0,
            },
            BTreeMap::new(),
        )
        .unwrap();
    assert!(matches!(e2, Value::Edge { id: 4, .. }));
}

#[test]
fn roundtrip_preserves_field_metadata() {
    let field = Field::new("born", DataType::Utf8, true).with_metadata(HashMap::from([(
        "orchiddb.value_type".to_string(),
        "datetime".to_string(),
    )]));
    let schema = Arc::new(Schema::new(vec![field]));
    let batch = RecordBatch::try_new(
        schema,
        vec![Arc::new(StringArray::from(vec!["1990-01-01"])) as ArrayRef],
    )
    .unwrap();
    let mut g = PropertyGraph::new();
    g.add_nodes(NodeTable {
        label: "Person".to_string(),
        batch,
    });

    let g2 = roundtrip(&g);
    assert!(g.graph_deep_eq(&g2));
    // The metadata is what makes `array_value` read this as a DateTime.
    assert_eq!(
        g2.node_property("Person", 0, "born"),
        Value::DateTime("1990-01-01".into())
    );
}

#[test]
fn roundtrip_all_value_variants() {
    let mut props = BTreeMap::new();
    props.insert("null".to_string(), Value::Null);
    props.insert("bool".to_string(), Value::Bool(true));
    props.insert("byte".to_string(), Value::Byte(-5));
    props.insert("uint8".to_string(), Value::UInt8(200));
    props.insert("short".to_string(), Value::Short(-1234));
    props.insert("uint16".to_string(), Value::UInt16(60_000));
    props.insert("int".to_string(), Value::Int(42));
    props.insert("uint32".to_string(), Value::UInt32(4_000_000_000));
    props.insert("long".to_string(), Value::Long(9_000_000_000_i64));
    props.insert(
        "uint64".to_string(),
        Value::UInt64(18_000_000_000_000_000_000),
    );
    props.insert("float32".to_string(), Value::Float32(1.5));
    props.insert("float".to_string(), Value::Float(2.25));
    props.insert(
        "bigint".to_string(),
        Value::BigInt(BigInt::from(123_456_789_012_345_678_901_234_567_890u128)),
    );
    props.insert(
        "uint128".to_string(),
        Value::UInt128(BigInt::from(
            340_282_366_920_938_463_463_374_607_431_768_211_455u128,
        )),
    );
    props.insert(
        "bigdecimal".to_string(),
        Value::BigDecimal(BigDecimal::from_str("3.14159265358979323846").unwrap()),
    );
    props.insert(
        "datetime".to_string(),
        Value::DateTime("2020-01-01T00:00:00".to_string()),
    );
    props.insert(
        "internalid".to_string(),
        Value::InternalId {
            table: 7,
            offset: 9,
        },
    );
    props.insert("string".to_string(), Value::String("hello".to_string()));
    props.insert(
        "node".to_string(),
        Value::Node {
            label: "X".to_string(),
            id: 3,
        },
    );
    props.insert(
        "edge".to_string(),
        Value::Edge {
            rel_type: "E".to_string(),
            id: 1,
            src_label: "X".to_string(),
            src_id: 0,
            dst_label: "X".to_string(),
            dst_id: 3,
            projected_properties: Some(vec!["p".to_string()]),
        },
    );
    props.insert(
        "list".to_string(),
        Value::List(vec![Value::Int(1), Value::String("a".to_string())]),
    );
    props.insert(
        "map".to_string(),
        Value::Map(BTreeMap::from([("k".to_string(), Value::Int(9))])),
    );
    props.insert(
        "path".to_string(),
        Value::Path(vec![
            Value::Node {
                label: "X".to_string(),
                id: 0,
            },
            Value::String("step".to_string()),
        ]),
    );

    let mut g = PropertyGraph::new();
    g.insert_node("Rich", props);

    let g2 = roundtrip(&g);
    assert!(g.graph_deep_eq(&g2));

    let original = g.node_property("Rich", 0, "bigdecimal");
    let restored = g2.node_property("Rich", 0, "bigdecimal");
    assert_eq!(original, restored);
    assert_eq!(
        restored,
        Value::BigDecimal(BigDecimal::from_str("3.14159265358979323846").unwrap())
    );
}

#[test]
fn roundtrip_multitype_edge_groups_preserve_endpoints() {
    let mut g = PropertyGraph::new();
    g.add_nodes(nodes_from_columns(
        "A",
        vec![("x", Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef)],
    ));
    g.add_nodes(nodes_from_columns(
        "B",
        vec![("y", Arc::new(Float64Array::from(vec![1.0])) as ArrayRef)],
    ));
    g.add_edges(edges_from_columns("T", "A", "A", vec![0], vec![1], vec![]))
        .unwrap();
    g.add_edges(edges_from_columns("T", "A", "B", vec![1], vec![0], vec![]))
        .unwrap();

    let g2 = roundtrip(&g);
    assert!(g.graph_deep_eq(&g2));
    assert_eq!(
        g2.edge_endpoints("T", 0),
        Some(("A".to_string(), 0, "A".to_string(), 1))
    );
    assert_eq!(
        g2.edge_endpoints("T", 1),
        Some(("A".to_string(), 1, "B".to_string(), 0))
    );
    assert_eq!(g2.edge_ids("T"), vec![0i64, 1]);
}

#[test]
fn corrupt_data_is_rejected() {
    let mut g = PropertyGraph::new();
    g.add_nodes(nodes_from_columns(
        "P",
        vec![("x", Arc::new(Int64Array::from(vec![1])) as ArrayRef)],
    ));
    g.insert_node("P", BTreeMap::from([("k".to_string(), Value::Int(1))]));
    let bytes = g.snapshot_encode().unwrap();

    assert!(PropertyGraph::snapshot_decode(&[]).is_err());
    assert!(PropertyGraph::snapshot_decode(&[0u8; 32]).is_err());

    let mut bad_magic = bytes.clone();
    bad_magic[0] ^= 0xFF;
    assert!(PropertyGraph::snapshot_decode(&bad_magic).is_err());

    let mut bad_version = bytes.clone();
    bad_version[4] = 99;
    assert!(PropertyGraph::snapshot_decode(&bad_version).is_err());

    // Truncation within the trailing (overlay) section framing.
    for cut in 1..=8 {
        assert!(
            PropertyGraph::snapshot_decode(&bytes[..bytes.len() - cut]).is_err(),
            "cut {cut} bytes should fail"
        );
    }

    // A corrupt length prefix in a section should be rejected.
    let mut bad_len = bytes.clone();
    // The first section length starts after the 12-byte header and tag.
    // Flipping arbitrary payload bytes could simply produce a valid value.
    bad_len[13..21].fill(0xFF);
    assert!(PropertyGraph::snapshot_decode(&bad_len).is_err());
    assert!(PropertyGraph::snapshot_decode(&bytes[..12]).is_err());
}
