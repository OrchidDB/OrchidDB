//! JSON row expansion uses the common graph mapping and relational operators.
use arrow::{
    array::{Int64Array, RecordBatch},
    datatypes::{DataType, Field, Schema},
};
use datafusion::{
    common::ScalarValue, datasource::MemTable, logical_expr::LogicalPlan, prelude::SessionContext,
};
use orchiddb::ir::{
    functions::domain,
    rel::{dependent, mapping::GraphMapping},
};
use serde_json::json;
use std::sync::Arc;

fn mapping(documents: &[Option<&str>]) -> GraphMapping {
    let values = documents
        .iter()
        .map(|s| {
            s.map(domain::json_scalar)
                .unwrap_or_else(|| ScalarValue::try_from(&domain::json_type()))
        })
        .collect::<datafusion::common::Result<Vec<_>>>()
        .unwrap();
    let docs = ScalarValue::iter_to_array(values).unwrap();
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("payload", domain::json_type(), true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(
                (1..=documents.len() as i64).collect::<Vec<_>>(),
            )),
            docs,
        ],
    )
    .unwrap();
    let mut mapping = GraphMapping::new();
    mapping.register_table(
        "documents",
        Arc::new(MemTable::try_new(schema, vec![vec![batch]]).unwrap()),
    );
    mapping
}
fn register(mapping: &mut GraphMapping, value: serde_json::Value) {
    mapping
        .register_collection_source(serde_json::from_value(value).unwrap())
        .unwrap();
}
async fn native(plan: LogicalPlan) -> Vec<Vec<String>> {
    let batches = SessionContext::new()
        .execute_logical_plan(dependent::native(plan).unwrap())
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    let mut rows = Vec::new();
    for batch in batches {
        for row in 0..batch.num_rows() {
            rows.push(
                batch
                    .columns()
                    .iter()
                    .map(|column| {
                        ScalarValue::try_from_array(column.as_ref(), row)
                            .unwrap()
                            .to_string()
                    })
                    .collect(),
            );
        }
    }
    rows.sort();
    rows
}
fn sql(plan: LogicalPlan, dialect: orchiddb::ir::rel::sql::SqlDialect) -> String {
    let fields = plan
        .schema()
        .fields()
        .iter()
        .map(|f| f.name().clone())
        .collect();
    orchiddb::ir::rel::sql::unparse(
        &orchiddb::ir::rel::LoweredPlan {
            plan,
            fields,
            result_form: orchiddb::ir::policy::ResultForm::RowSet,
            islands: Default::default(),
        },
        dialect,
    )
    .unwrap()
}
#[tokio::test]
async fn json_elements_preserve_parent_null_kind_index_and_outer_ordinality() {
    let mut mapping = mapping(&[Some("[10,null,20]"), Some("[]"), None]);
    register(
        &mut mapping,
        json!({"name":"items","table":"documents","expand":"json.elements(payload)","as":"item","outer":true,"ordinality":"position","parent_columns":{"owner":"id"},"fields":{"number":"json.value(item.value, '$', 'BIGINT')","kind":"json.type(item.value)","index":"item.index","path":"item.path"}}),
    );
    let rows = native(
        mapping
            .relational_plan("SELECT owner, number, kind, index, path, position FROM items")
            .unwrap(),
    )
    .await;
    assert_eq!(
        rows,
        vec![
            vec!["1", "10", "number", "0", "$[0]", "1"],
            vec!["1", "20", "number", "2", "$[2]", "3"],
            vec!["1", "NULL", "null", "1", "$[1]", "2"],
            vec!["2", "NULL", "NULL", "NULL", "NULL", "NULL"],
            vec!["3", "NULL", "NULL", "NULL", "NULL", "NULL"]
        ]
    );
}
#[tokio::test]
async fn nested_json_sources_can_back_edges_and_roundtrip_config() {
    let mut mapping = mapping(&[Some("[[1,2],[],[3]]")]);
    register(
        &mut mapping,
        json!({"name":"groups","table":"documents","expand":"json.elements(payload)","as":"group","parent_columns":{"owner":"id"},"fields":{"payload":"group.value","group_index":"group.index"}}),
    );
    register(
        &mut mapping,
        json!({"name":"members","table":"groups","expand":"json.elements(payload)","as":"member","parent_columns":{"owner":"owner","group_index":"group_index"},"fields":{"number":"json.value(member.value, '$', 'BIGINT')","member_index":"member.index"}}),
    );
    let rows = native(
        mapping
            .relational_plan("SELECT owner,group_index,member_index,number FROM members")
            .unwrap(),
    )
    .await;
    assert_eq!(
        rows,
        vec![
            vec!["1", "0", "0", "1"],
            vec!["1", "0", "1", "2"],
            vec!["1", "2", "0", "3"]
        ]
    );
    let mut restored = GraphMapping::from_toml(&mapping.to_toml()).unwrap();
    restored.register_table_schema("documents", mapping.table_schema("documents").unwrap());
    assert_eq!(
        restored.table_schema("members"),
        mapping.table_schema("members")
    );
}
#[tokio::test]
async fn json_tree_and_entries_have_canonical_paths_and_depth() {
    let mut mapping = mapping(&[Some(r#"{"a.b":[null,{"z":2}],"x":3}"#)]);
    register(
        &mut mapping,
        json!({"name":"tree","table":"documents","expand":"json.tree(payload)","as":"node","ordinality":"position","fields":{"path":"node.path","parent":"node.parent_path","depth":"node.depth","kind":"json.type(node.value)"}}),
    );
    let rows = native(
        mapping
            .relational_plan("SELECT path,parent,depth,kind,position FROM tree")
            .unwrap(),
    )
    .await;
    assert_eq!(rows.len(), 6);
    assert!(rows.contains(&vec![
        "$[\"a.b\"][1][\"z\"]".into(),
        "$[\"a.b\"][1]".into(),
        "3".into(),
        "number".into(),
        "5".into()
    ]));
    register(
        &mut mapping,
        json!({"name":"entries","table":"documents","expand":"json.entries(payload)","as":"entry","fields":{"key":"entry.key","path":"entry.path"}}),
    );
    assert_eq!(
        native(
            mapping
                .relational_plan("SELECT key,path FROM entries")
                .unwrap()
        )
        .await,
        vec![vec!["a.b", "$[\"a.b\"]"], vec!["x", "$[\"x\"]"]]
    );
}
#[test]
fn mapping_expressions_accept_typed_json_and_reject_statement_injection() {
    let mut mapping = mapping(&[Some("[1]")]);
    register(
        &mut mapping,
        json!({"name":"literal_items","table":"documents","expand":"json.elements(JSON '[1,2]')","as":"item","fields":{"number":"json.value(item.value, '$', 'BIGINT')"}}),
    );
    for expand in [
        "json.elements(payload); DROP TABLE documents",
        "json.elements(JSON '[invalid')",
        "(SELECT payload FROM documents)",
    ] {
        assert!(mapping.register_collection_source(serde_json::from_value(json!({"name":"invalid","table":"documents","expand":expand,"fields":{"number":"1"}})).unwrap()).is_err());
    }
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn duckdb_uses_direct_json_row_functions_for_elements_entries_and_tree() {
    use orchiddb::ir::rel::sql::{DuckDbExecutor, SqlDialect, SqlExecutor, SqlValue};
    for (operation, document) in [
        ("elements", "[10,null,20]"),
        ("entries", r#"{"z":2,"a.b":1}"#),
        ("tree", r#"{"a.b":[null,{"z":2}],"x":3}"#),
        ("entries", r#"{"k":[1],"k":[2]}"#),
        ("tree", r#"{"k":{"old":1},"k":{"new":2}}"#),
    ] {
        let mut mapping = mapping(&[Some(document), None]);
        register(
            &mut mapping,
            json!({"name":"rows","table":"documents","expand":format!("json.{operation}(payload)"),"as":"row","outer":true,"ordinality":"position","parent_columns":{"owner":"id"},"fields":{"value":"json.stringify(row.value)","index":"row.index","key":"row.key","path":"row.path","parent":"row.parent_path","depth":"row.depth"}}),
        );
        let plan = mapping
            .relational_plan("SELECT owner,value,index,key,path,parent,depth,position FROM rows")
            .unwrap();
        let expected = native(plan.clone()).await;
        let query = sql(plan, SqlDialect::DuckDb);
        assert!(query.contains("json_each"), "{query}");
        assert!(!query.to_lowercase().contains("unnest("), "{query}");
        let setup = vec![
            "CREATE TABLE documents(id BIGINT,payload JSON)".into(),
            format!(
                "INSERT INTO documents VALUES (1,'{}'),(2,NULL)",
                document.replace('\'', "''")
            ),
        ];
        let result = DuckDbExecutor::default()
            .run(&setup, &query)
            .unwrap_or_else(|e| panic!("{e}\n{query}"));
        let mut actual = result
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| match value {
                        SqlValue::Null => "NULL".into(),
                        SqlValue::Int(i) => i.to_string(),
                        SqlValue::Text(s) => s,
                        other => panic!("unexpected {other:?}"),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        actual.sort();
        assert_eq!(actual, expected, "{operation}\n{query}");
    }
}

#[tokio::test]
async fn json_collection_rows_back_ordinary_graph_relationships() {
    use orchiddb::{
        ir::{
            catalog::PropertyGraph,
            rel::{
                RelBackend, RelBackendOptions, execute_lowered,
                mapping::{EdgeMapping, NodeMapping},
            },
        },
        language::cypher::{parse_query, planner::CypherPlanner},
    };
    let mut mapping = mapping(&[Some("[11,12]"), Some("[21]")]);
    register(
        &mut mapping,
        json!({"name":"items","table":"documents","expand":"json.elements(payload)","as":"item","parent_columns":{"owner":"id"},"fields":{"number":"json.value(item.value,'$','BIGINT')","index":"item.index"}}),
    );
    mapping.map_node(NodeMapping::table("Document", "documents", "id").property("id", "id"));
    mapping.map_node(
        NodeMapping::table("Item", "items", ["owner", "index"]).property("number", "number"),
    );
    mapping.map_edge(EdgeMapping::table(
        "HAS_ITEM",
        "items",
        "owner",
        ["owner", "index"],
        "Document",
        "Item",
    ));
    let plan=CypherPlanner::new().plan(&parse_query("MATCH (d:Document)-[:HAS_ITEM]->(i:Item) WHERE d.id = 1 RETURN i.number ORDER BY i.number").unwrap()).unwrap();
    let lowered = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new())
    .unwrap();
    let returned = execute_lowered(lowered).await.unwrap();
    assert_eq!(returned.batch.num_rows(), 2);
    assert_eq!(
        ScalarValue::try_from_array(returned.batch.column(0).as_ref(), 0)
            .unwrap()
            .to_string(),
        "11"
    );
    assert_eq!(
        ScalarValue::try_from_array(returned.batch.column(0).as_ref(), 1)
            .unwrap()
            .to_string(),
        "12"
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn duckdb_row_paths_distinguish_object_fields_and_array_indices_and_keep_last_key() {
    use orchiddb::ir::rel::sql::{DuckDbExecutor, SqlDialect, SqlExecutor, SqlValue};
    let document = r#"{"0":[7],"k":[1],"k":[2],"a":[[8],[9]]}"#;
    for (path, expected) in [
        ("$.k", vec!["2"]),
        ("$[\"0\"]", vec!["7"]),
        ("$[0]", vec![]),
        ("/0", vec!["7"]),
        ("/a/1", vec!["9"]),
        ("$.a[-1]", vec!["9"]),
        ("/a/01", vec![]),
        ("/a/-1", vec![]),
    ] {
        let mut mapping = mapping(&[Some(document)]);
        register(
            &mut mapping,
            json!({"name":"items","table":"documents","expand":format!("json.elements(payload, '{}')",path),"as":"item","fields":{"value":"json.value(item.value,'$','BIGINT')"}}),
        );
        let plan = mapping.relational_plan("SELECT value FROM items").unwrap();
        let expected_native = native(plan.clone()).await;
        assert_eq!(
            expected_native,
            expected
                .iter()
                .map(|v| vec![v.to_string()])
                .collect::<Vec<_>>()
        );
        let query = sql(plan, SqlDialect::DuckDb);
        let rows = DuckDbExecutor::default()
            .run(
                &[
                    "CREATE TABLE documents(id BIGINT,payload JSON)".into(),
                    format!("INSERT INTO documents VALUES(1,'{document}')"),
                ],
                &query,
            )
            .unwrap_or_else(|e| panic!("{e}\n{query}"));
        assert_eq!(
            rows,
            expected
                .iter()
                .map(|v| vec![SqlValue::Int(v.parse().unwrap())])
                .collect::<Vec<_>>()
        );
    }
}

#[tokio::test]
async fn nested_outer_expansions_can_reuse_ordinality_names() {
    let mut mapping = mapping(&[Some("[[1],[]]")]);
    register(
        &mut mapping,
        json!({"name":"groups","table":"documents","expand":"json.elements(payload)","ordinality":"position","fields":{"payload":"item.value"}}),
    );
    register(
        &mut mapping,
        json!({"name":"members","table":"groups","expand":"json.elements(payload)","outer":true,"ordinality":"position","parent_columns":{"group_position":"position"},"fields":{"number":"json.value(item.value,'$','BIGINT')"}}),
    );
    let rows = native(
        mapping
            .relational_plan("SELECT group_position,position,number FROM members")
            .unwrap(),
    )
    .await;
    assert_eq!(rows, vec![vec!["1", "1", "1"], vec!["2", "NULL", "NULL"]]);
}
