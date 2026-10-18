#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    Value,
    catalog::PropertyGraph,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, execute_lowered_sql},
    },
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};
use std::collections::BTreeMap;

#[tokio::test]
async fn temporal_properties_round_trip_through_portable_sql_with_nulls() {
    for (kind, text) in [
        ("date", "0001-01-01"),
        ("date", "9999-12-31"),
        ("localtime", "12:31:14.645876123"),
        ("time", "12:31:14.645876124+01:00"),
        ("localdatetime", "1984-10-11T12:31:14.645876123"),
        (
            "datetime",
            "1984-10-11T12:31:14.645876123+01:00[Europe/Stockholm]",
        ),
        ("duration", "P1M2DT3.000000004S"),
        ("duration", "-P1DT0.000000001S"),
    ] {
        let expected = orchiddb::ir::temporal::parse(kind, text).unwrap();
        let graph = PropertyGraph::new();
        graph.insert_node(
            "T",
            BTreeMap::from([
                ("v".into(), Value::Temporal(expected.clone())),
                ("k".into(), Value::Int(1)),
            ]),
        );
        graph.insert_node("T", BTreeMap::from([("k".into(), Value::Int(2))]));
        let plan = CypherPlanner::new()
            .plan(&parse_query("MATCH (n:T) WITH n.v AS v,n.k AS k RETURN v ORDER BY k").unwrap())
            .unwrap();
        let lowered = RelBackend::new()
            .with_language_functions(true)
            .lower(&plan, &graph)
            .unwrap();
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap();
        assert_eq!(result.batch.num_rows(), 2);
        let actual =
            datafusion::common::ScalarValue::try_from_array(result.batch.column(0), 0).unwrap();
        let storage = orchiddb::ir::functions::domain::storage(&actual).unwrap();
        if kind == "date" {
            assert!(matches!(storage, datafusion::common::ScalarValue::Date32(Some(_))));
            assert_eq!(arrow::util::display::array_value_to_string(storage.to_array().unwrap().as_ref(), 0).unwrap(), text);
        } else {
            assert_eq!(storage, datafusion::common::ScalarValue::Utf8(Some(expected.encode())), "{kind}: {text}");
        }
        assert!(result.batch.column(0).is_null(1));
    }
}

#[test]
fn temporal_property_operators_do_not_use_encoded_text_semantics() {
    let graph = PropertyGraph::new();
    graph.insert_node(
        "T",
        BTreeMap::from([(
            "v".into(),
            Value::Temporal(orchiddb::ir::temporal::parse("date", "1984-10-11").unwrap()),
        )]),
    );
    for query in [
        "MATCH (n:T) RETURN min(n.v)",
        "MATCH (n:T) RETURN DISTINCT n.v",
    ] {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        assert!(
            RelBackend::new()
                .with_language_functions(true)
                .lower(&plan, &graph)
                .is_err(),
            "{query}"
        );
    }
    // Adding another live type must be re-inferred, not coerced to text.
    graph.insert_node(
        "T",
        BTreeMap::from([("v".into(), Value::String("1984-10-11".into()))]),
    );
    let plan = CypherPlanner::new()
        .plan(&parse_query("MATCH (n:T) RETURN n.v").unwrap())
        .unwrap();
    assert!(RelBackend::new().lower(&plan, &graph).is_err());
}

#[tokio::test]
async fn temporal_property_updates_keep_typed_values_and_runtime_consumers() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    engine
        .cypher("CREATE (:T {v: date('1984-10-11')})")
        .await
        .unwrap();
    for (update, expected) in [
        ("MATCH (n:T) SET n.v = date('1999-12-31')", 1999),
        ("MATCH (n:T) SET n.v = date('2000-01-01')", 2000),
    ] {
        engine.cypher(update).await.unwrap();
        let result = engine
            .cypher("MATCH (n:T) WITH n.v AS d RETURN d.year")
            .await
            .unwrap();
        let typed: serde_json::Value = serde_json::from_str(
            &result.returned.batch.schema().metadata()["orchiddb.cypher.typed_rows.v1"],
        )
        .unwrap();
        assert_eq!(typed[0][0]["value"], expected);
    }
    engine.cypher("MATCH (n:T) REMOVE n.v").await.unwrap();
    let result = engine.cypher("MATCH (n:T) RETURN n.v").await.unwrap();
    assert!(result.returned.batch.column(0).is_null(0));
}

#[test]
fn duckdb_ingress_unwraps_temporal_envelopes_including_nested_lists() {
    use arrow::{
        array::RecordBatch,
        datatypes::{Field, Schema},
    };
    use datafusion::common::ScalarValue;
    use orchiddb::ir::{
        functions::domain,
        rel::sql::{SqlExecutor, SqlValue, TableData},
    };
    use std::sync::Arc;
    let temporal = domain::scalar(
        "cypher_temporal",
        ScalarValue::Utf8(Some("date|1984-10-11".into())),
    )
    .unwrap();
    let null = ScalarValue::try_from(&temporal.data_type()).unwrap();
    let list = ScalarValue::List(ScalarValue::new_list_nullable(
        &[temporal.clone(), null.clone()],
        &temporal.data_type(),
    ));
    let schema = Arc::new(Schema::new(vec![
        Field::new("value", temporal.data_type(), true),
        Field::new("items", list.data_type(), true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            ScalarValue::iter_to_array(vec![temporal, null]).unwrap(),
            ScalarValue::iter_to_array(vec![list.clone(), list]).unwrap(),
        ],
    )
    .unwrap();
    let table = TableData {
        name: "temporal_ingress".into(),
        schema,
        batches: vec![batch],
    };
    let rows = DuckDbExecutor::default()
        .run_with_tables(
            &[table],
            &[],
            "SELECT value, items FROM temporal_ingress ORDER BY value NULLS LAST",
        )
        .unwrap();
    let payload = SqlValue::Text("date|1984-10-11".into());
    assert_eq!(
        rows,
        vec![
            vec![
                payload.clone(),
                SqlValue::List(vec![payload.clone(), SqlValue::Null])
            ],
            vec![
                SqlValue::Null,
                SqlValue::List(vec![payload, SqlValue::Null])
            ],
        ]
    );
}

#[test]
fn duckdb_native_date_lists_keep_their_type_and_null_elements() {
    use orchiddb::ir::rel::sql::{SqlExecutor, SqlValue};
    let rows = DuckDbExecutor::default().run_with_tables(&[], &[],
        "SELECT typeof([DATE '1816-01-01', NULL]), [DATE '1816-01-01', NULL]").unwrap();
    assert_eq!(rows, vec![vec![SqlValue::Text("DATE[]".into()),
        SqlValue::List(vec![SqlValue::Text("1816-01-01".into()), SqlValue::Null])]]);
}
