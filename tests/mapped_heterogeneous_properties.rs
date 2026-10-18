#![cfg(feature = "duckdb")]
use arrow::{
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use datafusion::common::ScalarValue;
use orchiddb::ir::{
    Value,
    catalog::PropertyGraph,
    diagnostics::without_constant_folding,
    functions::scalar_union,
    rel::{
        RelBackend, RelBackendOptions,
        mapping::{GraphMapping, NodeMapping},
        sql::{self, DuckDbExecutor, SqlDialect, TableData},
    },
};
use std::{collections::BTreeMap, sync::Arc};

#[tokio::test]
async fn heterogeneous_properties_preserve_types_nulls_and_live_updates() {
    without_constant_folding(async {
        let values = vec![
            Value::Int(1),
            Value::String("1".into()),
            Value::String("xx".into()),
            Value::Null,
            Value::Int(i64::MAX),
        ];
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("v", scalar_union::data_type(), true),
        ]));
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                ScalarValue::iter_to_array((1..=5).map(|n| ScalarValue::Int64(Some(n)))).unwrap(),
                ScalarValue::iter_to_array(values.iter().map(|v| scalar_union::scalar(v).unwrap()))
                    .unwrap(),
            ],
        )
        .unwrap();
        let table = TableData {
            name: "items".into(),
            schema: schema.clone(),
            batches: vec![batch],
        };
        let conn = duckdb::Connection::open_in_memory().unwrap();
        for statement in sql::table_setup_sql(SqlDialect::DuckDb, &table).unwrap() {
            conn.execute_batch(&statement).unwrap();
        }
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema("items", schema);
        mapping.map_node(NodeMapping::table("Item", "items", "id").property("v", "v"));
        let mut executor = DuckDbExecutor::from_connection(conn);
        for (query, expected) in [
            ("MATCH (n) WHERE n.v = 1 RETURN n.v", vec![Value::Int(1)]),
            (
                "MATCH (n) WHERE n.v = '1' RETURN n.v",
                vec![Value::String("1".into())],
            ),
            (
                "MATCH (n) WHERE n.v > 'x' RETURN n.v",
                vec![Value::String("xx".into())],
            ),
            (
                "MATCH (n) WHERE n.v IS NULL OR n.v > 'x' RETURN n.v",
                vec![Value::String("xx".into()), Value::Null],
            ),
            (
                "MATCH (n) WHERE n.v = 9223372036854775807 RETURN n.v",
                vec![Value::Int(i64::MAX)],
            ),
        ] {
            let plan =
                orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None)
                    .unwrap();
            let lowered = RelBackend::with_options(RelBackendOptions {
                mapping: Some(Arc::new(mapping.clone())),
                language_functions: true,
                ..Default::default()
            })
            .lower(&plan, &PropertyGraph::new())
            .unwrap_or_else(|e| panic!("{query}: {e}"));
            let prepared = sql::prepare_with_external(
                &lowered,
                SqlDialect::DuckDb,
                &mapping.physical_table_names(),
            )
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
            let result = sql::execute_prepared(&mut executor, &prepared)
                .unwrap_or_else(|e| panic!("{query}: {e}\n{}", prepared.query));
            let actual = (0..result.batch.num_rows())
                .map(|row| {
                    scalar_union::decode(
                        &ScalarValue::try_from_array(result.batch.column(0), row).unwrap(),
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{query}\n{}", prepared.query);
        }
    })
    .await;
}

fn mutable_fixture() -> (GraphMapping, DuckDbExecutor) {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE items(id BIGINT PRIMARY KEY, v STRUCT(__cypher_scalar_tag TINYINT, integer BIGINT, string VARCHAR, boolean BOOLEAN), note VARCHAR); INSERT INTO items VALUES (1, {__cypher_scalar_tag:1, integer:9223372036854775807, string:NULL, boolean:NULL}, 'original'), (2, {__cypher_scalar_tag:2, integer:NULL, string:'1', boolean:NULL}, 'original'), (3, {__cypher_scalar_tag:3, integer:NULL, string:NULL, boolean:true}, 'original'), (4,NULL,'original')").unwrap();
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema(
        "items",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("v", scalar_union::data_type(), true),
            Field::new("note", DataType::Utf8, true),
        ])),
    );
    mapping.map_node(
        NodeMapping::table("Item", "items", "id")
            .property("id", "id")
            .property("v", "v")
            .property("note", "note"),
    );
    (mapping, DuckDbExecutor::from_connection(connection))
}
async fn compiled(mapping: &GraphMapping, query: &str) -> Result<sql::PreparedSql, String> {
    let plan = orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None)
        .map_err(|e| e.to_string())?;
    let lowered = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping.clone())),
        language_functions: true,
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new())
    .map_err(|e| e.to_string())?;
    sql::prepare_with_external(
        &lowered,
        SqlDialect::DuckDb,
        &mapping.physical_table_names(),
    )
    .await
    .map_err(|e| e.to_string())
}
fn display_rows(batch: &RecordBatch) -> Vec<String> {
    (0..batch.num_rows())
        .map(|row| {
            batch
                .columns()
                .iter()
                .map(|column| {
                    arrow::util::display::array_value_to_string(column.as_ref(), row).unwrap()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}
#[tokio::test]
async fn heterogeneous_order_comparisons_and_safe_operation_fences() {
    without_constant_folding(async {
        let (mapping, mut executor) = mutable_fixture();
        for (query, expected) in [
            (
                "MATCH (n) RETURN n.id ORDER BY n.v",
                vec!["2", "3", "1", "4"],
            ),
            (
                "MATCH (n) RETURN n.id ORDER BY n.v DESC",
                vec!["4", "1", "3", "2"],
            ),
            (
                "MATCH (n) WHERE n.v <> 1 RETURN n.id ORDER BY n.id",
                vec!["1", "2", "3"],
            ),
            ("MATCH (n) WHERE n.v = true RETURN n.id", vec!["3"]),
            (
                "MATCH (n) RETURN n.v = [1], n.v > [1] ORDER BY n.id",
                vec!["false|", "false|", "false|", "|"],
            ),
            (
                "MATCH (n) RETURN n.v = {a:1}, n.v < {a:1} ORDER BY n.id",
                vec!["false|", "false|", "false|", "|"],
            ),
        ] {
            let sql = compiled(&mapping, query)
                .await
                .unwrap_or_else(|e| panic!("{query}: {e}"));
            let result = sql::execute_prepared(&mut executor, &sql)
                .unwrap_or_else(|e| panic!("{query}: {e}\n{}", sql.query));
            assert_eq!(
                display_rows(&result.batch),
                expected,
                "{query}\n{}",
                sql.query
            );
        }
        for query in [
            "MATCH (n) RETURN n.v + 1",
            "MATCH (n) RETURN n.v = 1.0",
            "MATCH (n) RETURN min(n.v)",
            "MATCH (n) RETURN max(n.v)",
            "MATCH (n) RETURN size(n.v)",
            "MATCH (n) RETURN toString(n.v)",
            "MATCH (n) RETURN n.v STARTS WITH '1'",
        ] {
            assert!(
                compiled(&mapping, query).await.is_err(),
                "unsafe operation lowered: {query}"
            );
        }
    })
    .await;
}
#[tokio::test]
async fn heterogeneous_mapped_writes_preserve_nulls_kind_and_unrelated_updates() {
    let (mapping, executor) = mutable_fixture();
    let mut engine = orchiddb::mapped_engine::MappedGraphEngine::new(executor, Arc::new(mapping));
    engine
        .cypher("MATCH (n) SET n.note = 'updated' RETURN n.note")
        .await
        .unwrap();
    let intact:bool=engine.executor_mut().connection().unwrap().query_row("SELECT v.integer=9223372036854775807 AND v.__cypher_scalar_tag=1 FROM items WHERE id=1",[],|row|row.get(0)).unwrap();
    assert!(intact);
    engine
        .cypher("MATCH (n) WHERE n.id=2 SET n.v=7 RETURN n.v")
        .await
        .unwrap();
    engine
        .cypher("MATCH (n) WHERE n.id=1 SET n.v='1' RETURN n.v")
        .await
        .unwrap();
    engine
        .cypher("MATCH (n) WHERE n.id=3 REMOVE n.v RETURN n.v")
        .await
        .unwrap();
    engine
        .cypher("MATCH (n) WHERE n.id=4 SET n.v=false RETURN n.v")
        .await
        .unwrap();
    let preserved:bool=engine.executor_mut().connection().unwrap().query_row("SELECT bool_and(CASE id WHEN 1 THEN v.__cypher_scalar_tag=2 AND v.string='1' WHEN 2 THEN v.__cypher_scalar_tag=1 AND v.integer=7 WHEN 4 THEN v.__cypher_scalar_tag=3 AND v.boolean=false ELSE true END) FROM items",[],|row|row.get(0)).unwrap();
    assert!(preserved);
    let null: bool = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT v IS NULL FROM items WHERE id=3", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert!(null);
    // Return rendering and native read/modify/write must both recover values.
    let result = engine
        .cypher("MATCH (n) WHERE n.id=2 SET n.v=n.v+1 RETURN n.v")
        .await
        .unwrap();
    assert_eq!(display_rows(&result.batch), vec!["8"]);
    let invalid = engine
        .cypher("MATCH (n) WHERE n.id=2 SET n.v=[1] RETURN n.v")
        .await;
    let state: String = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row(
            "SELECT CAST(v AS VARCHAR) FROM items WHERE id=2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        invalid.is_err(),
        "invalid compound write unexpectedly succeeded: state={state}; result={:?}",
        invalid.as_ref().map(|result| display_rows(&result.batch))
    );
    let value: i64 = engine
        .executor_mut()
        .connection()
        .unwrap()
        .query_row("SELECT v.integer FROM items WHERE id=2", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(value, 8);
}
#[tokio::test]
async fn heterogeneous_properties_across_mapping_tables_promote_losslessly() {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE ints(id BIGINT, v BIGINT); INSERT INTO ints VALUES (1,9223372036854775807),(2,NULL); CREATE TABLE strings(id BIGINT, v VARCHAR); INSERT INTO strings VALUES (1,'1'),(2,NULL)").unwrap();
    let mut mapping = GraphMapping::new();
    for (label, table, ty) in [
        ("N", "ints", DataType::Int64),
        ("S", "strings", DataType::Utf8),
    ] {
        mapping.register_table_schema(
            table,
            Arc::new(Schema::new(vec![
                Field::new("id", DataType::Int64, false),
                Field::new("v", ty, true),
            ])),
        );
        mapping.map_node(NodeMapping::table(label, table, "id").property("v", "v"));
    }
    let mut executor = DuckDbExecutor::from_connection(connection);
    let sql = compiled(&mapping, "MATCH (n) RETURN n.v ORDER BY n.v")
        .await
        .unwrap();
    let result = sql::execute_prepared(&mut executor, &sql).unwrap();
    let values = (0..result.batch.num_rows())
        .map(|row| {
            scalar_union::decode(&ScalarValue::try_from_array(result.batch.column(0), row).unwrap())
                .unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        values,
        vec![
            Value::String("1".into()),
            Value::Int(i64::MAX),
            Value::Null,
            Value::Null
        ]
    );
}

#[test]
fn heterogeneous_transport_rejects_malformed_and_unrelated_structs() {
    use arrow::array::StructArray;
    for value in [
        Value::Int(i64::MIN),
        Value::Int(i64::MAX),
        Value::String("1".into()),
        Value::Bool(false),
        Value::Null,
    ] {
        assert_eq!(
            scalar_union::decode(&scalar_union::scalar(&value).unwrap()),
            Some(value)
        );
    }
    let ScalarValue::Struct(valid) = scalar_union::scalar(&Value::Int(1)).unwrap() else {
        panic!()
    };
    let mut columns = valid.columns().to_vec();
    columns[2] = ScalarValue::Utf8(Some("hidden payload".into()))
        .to_array()
        .unwrap();
    let ambiguous = ScalarValue::Struct(Arc::new(
        StructArray::try_new(valid.fields().clone(), columns, None).unwrap(),
    ));
    assert_eq!(scalar_union::decode(&ambiguous), None);
    let unrelated = ScalarValue::Struct(Arc::new(
        StructArray::try_new(
            vec![Field::new("integer", DataType::Int64, true)].into(),
            vec![ScalarValue::Int64(Some(1)).to_array().unwrap()],
            None,
        )
        .unwrap(),
    ));
    assert_eq!(scalar_union::decode(&unrelated), None);
    assert!(scalar_union::scalar(&Value::Float(1.0)).is_err());
    assert!(scalar_union::scalar(&Value::List(vec![Value::Int(1)])).is_err());
}

#[tokio::test]
async fn singleton_property_lists_keep_list_shape_in_mapped_writes() {
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema(
        "list_items",
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new(
                "v",
                DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
                true,
            ),
        ])),
    );
    mapping.map_node(
        NodeMapping::table("Item", "list_items", "id")
            .property("id", "id")
            .property("v", "v"),
    );
    let mut engine =
        orchiddb::mapped_engine::MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping));
    engine.execute_sql("CREATE TABLE list_items(id BIGINT PRIMARY KEY,v BIGINT[]); INSERT INTO list_items VALUES (1,[10,20])").unwrap();
    for (query, expected) in [
        ("MATCH(n) SET n.v=[1] RETURN n.v", "[1]"),
        ("MATCH(n) SET n.v=[] RETURN n.v", "[]"),
        ("MATCH(n) SET n.v=[1,null,2] RETURN n.v", "[1, NULL, 2]"),
    ] {
        engine.cypher(query).await.unwrap();
        let value: String = engine
            .executor_mut()
            .connection()
            .unwrap()
            .query_row("SELECT CAST(v AS VARCHAR) FROM list_items", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(value, expected);
    }
}
