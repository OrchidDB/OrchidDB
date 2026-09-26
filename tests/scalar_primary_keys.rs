//! Native scalar keys must survive scans and endpoint joins without conversion.
use arrow::array::*;
use arrow::datatypes::*;
use datafusion::datasource::MemTable;
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping};
use orchiddb::ir::rel::{RelBackend, RelBackendOptions, execute_lowered};
use orchiddb::language::cypher::parser::parse_query;
use orchiddb::language::cypher::planner::CypherPlanner;
use std::sync::Arc;

async fn check(keys: ArrayRef) {
    let ty = keys.data_type().clone();
    let mut mapping = GraphMapping::new();
    let nodes = RecordBatch::try_from_iter(vec![
        ("key", keys.clone()),
        (
            "name",
            Arc::new(StringArray::from(vec!["a", "b"])) as ArrayRef,
        ),
    ])
    .unwrap();
    let edges = RecordBatch::try_from_iter(vec![
        ("key", keys.slice(0, 1)),
        ("src", keys.slice(0, 1)),
        ("dst", keys.slice(1, 1)),
    ])
    .unwrap();
    let mut native = PropertyGraph::new();
    let native_keys = (0..keys.len())
        .map(|row| {
            orchiddb::ir::ElementId::new(
                datafusion::common::ScalarValue::try_from_array(&keys, row).unwrap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    native
        .add_keyed_nodes(
            orchiddb::ir::catalog::NodeTable {
                label: "N".into(),
                batch: nodes.project(&[1]).unwrap(),
            },
            native_keys.clone(),
        )
        .unwrap();
    native
        .add_keyed_edges(
            orchiddb::ir::catalog::EdgeTable {
                rel_type: "E".into(),
                src_label: "N".into(),
                dst_label: "N".into(),
                batch: RecordBatch::try_from_iter(vec![
                    ("__src_id", keys.slice(0, 1)),
                    ("__dst_id", keys.slice(1, 1)),
                ])
                .unwrap(),
            },
            vec![native_keys[0].clone()],
        )
        .unwrap();
    // Restore typed base identities and adjacency before exercising the runtime.
    let native =
        orchiddb::storage::decode_graph(&orchiddb::storage::encode_graph(&native).unwrap())
            .unwrap();
    let mut expected_keys = native_keys.clone();
    expected_keys.sort();
    assert_eq!(native.node_ids("N").unwrap(), expected_keys);
    for (name, batch) in [("nodes", nodes), ("edges", edges)] {
        mapping.register_table(
            name,
            Arc::new(MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap()),
        );
    }
    mapping.map_node(NodeMapping::table("N", "nodes", "key").property("name", "name"));
    mapping.map_edge(EdgeMapping::table("E", "edges", "src", "dst", "N", "N").with_id("key"));
    let backend = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        ..Default::default()
    });
    for query in [
        "MATCH (a:N)-[e:E]->(b:N) RETURN a.name, b.name",
        "MATCH (a:N)-[:E*1..2]->(b:N) RETURN a.name, b.name",
        "MATCH (a:N)-[:E*]->(b:N) RETURN a.name, b.name",
        "MATCH (a:N)-[:E]->(b:N) MATCH (c:N)-[:E]->(b) RETURN a.name, b.name",
    ] {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        let native_result = orchiddb::ir::rel::runtime::execute(&plan, &native, None)
            .await
            .unwrap_or_else(|e| panic!("native {ty:?}: {query}: {e}"));
        assert_eq!(
            native_result.0.batch.num_rows(),
            1,
            "native {ty:?}: {query}"
        );
        let lowered = backend
            .lower(&plan, &PropertyGraph::new())
            .unwrap_or_else(|e| panic!("{ty:?}: {query}: {e}"));
        let result = execute_lowered(lowered)
            .await
            .unwrap_or_else(|e| panic!("{ty:?}: {query}: {e}"));
        assert_eq!(result.batch.num_rows(), 1, "{ty:?}: {query}");
        for (i, expected) in ["a", "b"].iter().enumerate() {
            assert_eq!(
                result
                    .batch
                    .column(i)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .unwrap()
                    .value(0),
                *expected,
                "{ty:?}"
            );
        }
    }
}

#[tokio::test]
async fn scalar_primary_keys_join() {
    macro_rules! check_array {
        ($array:ty, $values:expr) => {
            check(Arc::new(<$array>::from($values))).await
        };
    }
    let mut dictionary = StringDictionaryBuilder::<Int8Type>::new();
    dictionary.append("a").unwrap();
    dictionary.append("b").unwrap();
    check(Arc::new(dictionary.finish())).await;
    check_array!(BooleanArray, vec![false, true]);
    check_array!(Int8Array, vec![-1, 2]);
    check_array!(Int16Array, vec![-1, 2]);
    check_array!(Int32Array, vec![-1, 2]);
    check_array!(Int64Array, vec![i64::MIN, i64::MAX]);
    check_array!(UInt8Array, vec![0, 255]);
    check_array!(UInt16Array, vec![0, 65535]);
    check_array!(UInt32Array, vec![0, u32::MAX]);
    check_array!(UInt64Array, vec![0, u64::MAX]);
    check(arrow::compute::cast(&Float32Array::from(vec![-0.0, 1.25]), &DataType::Float16).unwrap())
        .await;
    check_array!(Float32Array, vec![-0.0, 1.25]);
    check_array!(Float64Array, vec![-0.0, 1.25]);
    check_array!(Float64Array, vec![f64::NAN, f64::INFINITY]);
    check_array!(StringArray, vec!["a", "b"]);
    check_array!(LargeStringArray, vec!["a", "b"]);
    check_array!(StringViewArray, vec!["a", "b"]);
    check_array!(BinaryArray, vec![&b"\xff"[..], &b"\x00"[..]]);
    check_array!(LargeBinaryArray, vec![&b"\xff"[..], &b"\x00"[..]]);
    check_array!(BinaryViewArray, vec![&b"\xff"[..], &b"\x00"[..]]);
    check(Arc::new(
        FixedSizeBinaryArray::try_from_iter([b"a", b"b"].into_iter()).unwrap(),
    ))
    .await;
    check(Arc::new(
        Decimal32Array::from(vec![125, 250])
            .with_precision_and_scale(8, 2)
            .unwrap(),
    ))
    .await;
    check(Arc::new(
        Decimal64Array::from(vec![125, 250])
            .with_precision_and_scale(16, 2)
            .unwrap(),
    ))
    .await;
    check(Arc::new(
        Decimal128Array::from(vec![125, 250])
            .with_precision_and_scale(38, 2)
            .unwrap(),
    ))
    .await;
    check(Arc::new(
        Decimal256Array::from(vec![i256::from_i128(125), i256::from_i128(250)])
            .with_precision_and_scale(76, 2)
            .unwrap(),
    ))
    .await;
    check_array!(Date32Array, vec![1, 2]);
    check_array!(Date64Array, vec![0, 86400000]);
    check_array!(Time32SecondArray, vec![1, 2]);
    check_array!(Time32MillisecondArray, vec![1, 2]);
    check_array!(Time64MicrosecondArray, vec![1, 2]);
    check_array!(Time64NanosecondArray, vec![1, 2]);
    check_array!(TimestampSecondArray, vec![1, 2]);
    check_array!(TimestampMillisecondArray, vec![1, 2]);
    check_array!(TimestampMicrosecondArray, vec![1, 2]);
    check_array!(TimestampNanosecondArray, vec![1, 2]);
    check_array!(DurationSecondArray, vec![1, 2]);
    check_array!(DurationMillisecondArray, vec![1, 2]);
    check_array!(DurationMicrosecondArray, vec![1, 2]);
    check_array!(DurationNanosecondArray, vec![1, 2]);
    check_array!(IntervalYearMonthArray, vec![1, 2]);
    check_array!(
        IntervalDayTimeArray,
        vec![IntervalDayTime::new(1, 0), IntervalDayTime::new(2, 0)]
    );
    check_array!(
        IntervalMonthDayNanoArray,
        vec![
            IntervalMonthDayNano::new(1, 0, 0),
            IntervalMonthDayNano::new(2, 0, 0)
        ]
    );
}

#[test]
fn null_and_nested_columns_are_not_primary_keys() {
    for ty in [
        DataType::Null,
        DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
        DataType::Struct(vec![Field::new("value", DataType::Int64, false)].into()),
    ] {
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "nodes",
            Arc::new(Schema::new(vec![Field::new("key", ty.clone(), true)])),
        );
        mapping.map_node(NodeMapping::table("N", "nodes", "key"));
        let backend = RelBackend::with_options(RelBackendOptions {
            mapping: Some(Arc::new(mapping)),
            ..Default::default()
        });
        let plan = CypherPlanner::new()
            .plan(&parse_query("MATCH (n:N) RETURN count(*)").unwrap())
            .unwrap();
        let error = backend
            .lower(&plan, &PropertyGraph::new())
            .err()
            .expect("non-scalar key must fail")
            .to_string();
        assert!(
            error.contains("cannot be an element identity"),
            "{ty:?}: {error}"
        );
    }
}
