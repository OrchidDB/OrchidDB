use orchiddb::compiler::compile_json;
use serde_json::{Value, json};
fn stats(table: &str, column: &str, partitions: Vec<(&str, &str, u64)>) -> Value {
    json!({"table":table,"specs":[{"spec_id":0,"fields":[]}],"partitions":partitions.into_iter().map(|(min,max,bytes)|json!({"spec_id":0,"bytes":bytes,"files":1,"rows":10,"bounds":{column:{"min":min,"max":max}}})).collect::<Vec<_>>()})
}
fn request(query: &str) -> Value {
    let mut request: Value =
        serde_json::from_str(include_str!("../examples/data/representation_sources.json")).unwrap();
    request["query"] = json!(query);
    request
}
async fn compile(r: Value) -> Value {
    serde_json::from_str(&compile_json(&r.to_string()).await.unwrap()).unwrap()
}
#[tokio::test]
async fn chooses_nested_or_flat_for_different_predicates() {
    let nested = compile(request("MATCH (i:Item) WHERE i.order_id = 1 RETURN i.sku")).await;
    assert_eq!(
        nested["representation_selections"][0]["representation"], "nested",
        "{nested}"
    );
    assert_eq!(
        nested["representation_selections"][0]["estimated_bytes"],
        1000
    );
    assert_eq!(
        nested["representation_selections"][0]["estimated_expanded_rows"],
        20
    );
    assert!(nested["logical_plan"].as_str().unwrap().contains("Unnest"));
    let flat = compile(request(
        "MATCH (i:Item) WHERE i.sku = 'a' RETURN i.order_id",
    ))
    .await;
    assert_eq!(
        flat["representation_selections"][0]["representation"], "flat_sku",
        "{flat}"
    );
    assert_eq!(
        flat["representation_selections"][0]["estimated_bytes"],
        10000
    );
    assert!(
        flat["logical_plan"]
            .as_str()
            .unwrap()
            .contains("TableScan: flat")
    );
    assert!(!flat["logical_plan"].as_str().unwrap().contains("Unnest"));
}
#[tokio::test]
async fn self_joins_choose_per_occurrence_and_do_not_limit_before_filtering() {
    let result=compile(request("MATCH (a:Item), (b:Item) WHERE a.order_id = 1 AND b.sku = 'a' RETURN a.sku,b.order_id LIMIT 2")).await;
    let choices = result["representation_selections"].as_array().unwrap();
    assert_eq!(choices.len(), 2, "{result}");
    assert!(choices.iter().any(|c| c["representation"] == "nested"));
    assert!(choices.iter().any(|c| c["representation"] == "flat_sku"));
}
#[tokio::test]
async fn unknown_costs_and_stale_generations_keep_default() {
    let mut r = request("MATCH (i:Item) WHERE i.sku = 'a' RETURN i.sku");
    r["representation_sources"][0]["representations"][1]["generation"] = json!("old");
    let result = compile(r).await;
    assert_eq!(
        result["representation_selections"][0]["representation"],
        "nested"
    );
    assert_eq!(
        result["representation_selections"][0]["candidates"][1]["reason"],
        "stale generation"
    );
    let mut r = request("MATCH (i:Item) WHERE i.sku = 'a' RETURN i.sku");
    r["representation_sources"][0]["representations"][0]["statistics"] = json!([]);
    let result = compile(r).await;
    assert_eq!(
        result["representation_selections"][0]["representation"],
        "nested"
    );
    assert!(result["representation_selections"][0]["estimated_cost"].is_null());
}
#[tokio::test]
async fn rdf_and_gremlin_share_representation_selection() {
    let mut r = request("");
    r["language"] = json!("sparql");
    r["query"] = json!("SELECT ?sku WHERE { ?item <urn:sku> ?sku FILTER(?sku = \"a\") }");
    r["rdf"] = json!([{"table":"items","subject":{"kind":"template","prefix":"urn:item:","columns":["order_id","item_id"]},"predicate":{"kind":"constant","value":"urn:sku"},"object":{"kind":"literal","column":"sku"}}]);
    assert_eq!(
        compile(r).await["representation_selections"][0]["representation"],
        "flat_sku"
    );
    let mut r = request("");
    r["language"] = json!("gremlin");
    r["query"] = json!("g.V().hasLabel('Item').has('sku','a').values('sku')");
    assert_eq!(
        compile(r).await["representation_selections"][0]["representation"],
        "flat_sku"
    );
}
#[tokio::test]
async fn schema_and_catalog_errors_are_explicit() {
    for r in [
        {
            let mut r = request("MATCH (i:Item) RETURN i.sku");
            r["representation_sources"][0]["representations"][1]["columns"]["sku"] = json!("qty");
            r
        },
        {
            let mut r = request("MATCH (i:Item) RETURN i.sku");
            r["representation_sources"][0]["default_representation"] = json!("missing");
            r
        },
        {
            let mut r = request("MATCH (i:Item) RETURN i.sku");
            r["representation_sources"][0]["representations"][1]["name"] = json!("nested");
            r
        },
        {
            let mut r = request("MATCH (i:Item) RETURN i.sku");
            r["representation_sources"][0]["representations"][0]["source"]["name"] = json!("items");
            r
        },
    ] {
        assert!(compile_json(&r.to_string()).await.is_err(), "{r}");
    }
}
#[cfg(feature = "duckdb")]
fn database() -> duckdb::Connection {
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE orders(id BIGINT,customer_id BIGINT,items STRUCT(item_id BIGINT,sku VARCHAR,quantity BIGINT)[]); INSERT INTO orders VALUES (1,10,[{'item_id':1,'sku':'a','quantity':2},{'item_id':2,'sku':'n','quantity':3}]),(50,20,[{'item_id':1,'sku':'a','quantity':4}]),(2,10,[]),(3,20,NULL); CREATE TABLE flat AS SELECT id AS oid, i.item_id AS line,i.sku AS product,i.quantity AS qty FROM (SELECT id,unnest(items) AS i FROM orders);").unwrap();
    db
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn both_representations_execute_with_equal_answers_and_residual_filters() {
    let db = database();
    for query in [
        "MATCH (i:Item) WHERE i.order_id = 1 RETURN i.sku ORDER BY i.sku",
        "MATCH (i:Item) WHERE i.sku = 'a' RETURN i.order_id ORDER BY i.order_id",
        "MATCH (i:Item) WHERE i.quantity > 2 RETURN i.sku ORDER BY i.sku LIMIT 1",
    ] {
        let r = request(query);
        let compiled = compile(r.clone()).await;
        let execute = |sql: &str| {
            let mut s = db.prepare(sql).unwrap();
            let reader = s.query_arrow([]).unwrap();
            reader
                .flat_map(|b| {
                    (0..b.num_rows())
                        .map(|i| {
                            arrow::util::display::array_value_to_string(b.column(0), i).unwrap()
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        let mut forced = r;
        forced["representation_sources"][0]["representations"][1]["generation"] = json!("stale");
        let fallback = compile(forced).await;
        assert_eq!(
            execute(compiled["sql"].as_str().unwrap()),
            execute(fallback["sql"].as_str().unwrap())
        );
    }
}
#[cfg(feature = "duckdb")]
#[tokio::test]
async fn query_defined_joins_and_aggregates_use_materializations() {
    let db = database();
    db.execute_batch("CREATE TABLE totals AS SELECT oid AS order_id,sum(qty)::BIGINT AS total FROM flat GROUP BY oid; CREATE TABLE purchases AS SELECT o.customer_id,f.product AS sku FROM orders o JOIN flat f ON o.id=f.oid").unwrap();
    for (table, definition, columns, id, prop) in [
        (
            "totals",
            "SELECT oid AS order_id,sum(qty) AS total FROM flat GROUP BY oid",
            json!([{"name":"order_id","data_type":"int64"},{"name":"total","data_type":"int64"}]),
            "order_id",
            "total",
        ),
        (
            "purchases",
            "SELECT o.customer_id,f.product AS sku FROM orders o JOIN flat f ON o.id=f.oid",
            json!([{"name":"customer_id","data_type":"int64"},{"name":"sku","data_type":"string"}]),
            "customer_id",
            "sku",
        ),
    ] {
        let mut r = request(&format!("MATCH (x:Summary) RETURN x.{prop}"));
        r["tables"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":table,"columns":columns}));
        let mut input_stats = vec![stats("flat", "product", vec![("a", "z", 100000)])];
        if table == "purchases" {
            input_stats.push(stats("orders", "id", vec![("1", "99", 100000)]));
        }
        r["representation_sources"] = json!([{"name":"summary","default_representation":"definition","representations":[
            {"name":"definition","source":{"kind":"query","sql":definition},"statistics":input_stats},
            {"name":"materialized","source":{"kind":"table","name":table},"statistics":[stats(table,id,vec![("0","100",100)])]}
        ]}]);
        r["nodes"] =
            json!([{"label":"Summary","table":"summary","id":id,"properties":{prop:prop}}]);
        let result = compile(r.clone()).await;
        assert_eq!(
            result["representation_selections"][0]["representation"], "materialized",
            "{result}"
        );
        let mut s = db.prepare(result["sql"].as_str().unwrap()).unwrap();
        let rows = s
            .query_arrow([])
            .unwrap()
            .map(|b| b.num_rows())
            .sum::<usize>();
        assert_eq!(rows, if table == "totals" { 2 } else { 3 });
        r["representation_sources"][0]["representations"][1]["generation"] = json!("stale");
        let fallback = compile(r).await;
        let values = |sql: &str| {
            let mut s = db.prepare(sql).unwrap();
            let mut values = s
                .query_arrow([])
                .unwrap()
                .flat_map(|b| {
                    (0..b.num_rows())
                        .map(|i| {
                            arrow::util::display::array_value_to_string(b.column(0), i).unwrap()
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            values.sort();
            values
        };
        assert_eq!(
            values(result["sql"].as_str().unwrap()),
            values(fallback["sql"].as_str().unwrap())
        );
    }
}

fn mapping() -> orchiddb::ir::rel::mapping::GraphMapping {
    use arrow::datatypes::{Field, Schema};
    use orchiddb::{
        compiler::data_type,
        ir::rel::mapping::{GraphMapping, NodeMapping},
    };
    use std::sync::Arc;
    let r = request("");
    let mut m = GraphMapping::new();
    for table in r["tables"].as_array().unwrap() {
        let fields = table["columns"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                Field::new(
                    c["name"].as_str().unwrap(),
                    data_type(c["data_type"].as_str().unwrap()).unwrap(),
                    true,
                )
            })
            .collect::<Vec<_>>();
        m.register_table_schema(
            table["name"].as_str().unwrap(),
            Arc::new(Schema::new(fields)),
        );
    }
    m.register_collection_source(
        serde_json::from_value(r["collection_sources"][0].clone()).unwrap(),
    )
    .unwrap();
    m.register_representation_source(
        serde_json::from_value(r["representation_sources"][0].clone()).unwrap(),
    )
    .unwrap();
    m.map_node(
        NodeMapping::table("Item", "items", ["order_id", "item_id"])
            .property("order_id", "order_id")
            .property("sku", "sku")
            .property("quantity", "quantity"),
    );
    m
}

#[tokio::test]
async fn catalog_round_trip_rebinding_and_dependency_cycles() {
    use orchiddb::ir::rel::{mapping::GraphMapping, representation::select};
    let mut m = mapping();
    let mut restored = GraphMapping::from_toml(&m.to_toml()).unwrap();
    for name in ["flat", "orders"] {
        restored.register_table_schema(name, m.table_schema(name).unwrap());
    }
    assert_eq!(m.table_schema("items"), restored.table_schema("items"));
    let chosen = select(
        restored
            .relational_plan("SELECT sku FROM items WHERE sku = 'a'")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        chosen.representation_selections[0].representation,
        "flat_sku"
    );
    assert_eq!(chosen.layout_selections[0].source, "flat");
    let wrapper = json!({"name":"wrapper","default_representation":"items","representations":[{"name":"items","source":{"kind":"table","name":"items"}}]});
    m.register_representation_source(serde_json::from_value(wrapper).unwrap())
        .unwrap();
    let nested = select(
        m.relational_plan("SELECT sku FROM wrapper WHERE sku = 'a'")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(nested.representation_selections.len(), 2);
    let mut cyclic = request("")["representation_sources"][0].clone();
    cyclic["representations"][0]["source"]["name"] = json!("wrapper");
    assert!(
        m.register_representation_source(serde_json::from_value(cyclic).unwrap())
            .unwrap_err()
            .to_string()
            .contains("cyclic")
    );
    // Replacing a physical provider must rebuild all downstream definitions.
    let schema = m.table_schema("flat").unwrap();
    m.register_table_schema("flat", schema);
    assert!(m.table_schema("wrapper").is_some());
    assert_eq!(
        select(
            m.relational_plan("SELECT sku FROM wrapper WHERE sku='a'")
                .unwrap()
        )
        .unwrap()
        .representation_selections[1]
            .representation,
        "flat_sku"
    );
}

#[tokio::test]
async fn in_process_preserves_duplicates_nulls_and_column_order() {
    use arrow::{
        array::{Int64Array, RecordBatch, StringArray},
        datatypes::{DataType, Field, Schema},
    };
    use datafusion::{datasource::MemTable, prelude::SessionContext};
    use orchiddb::ir::rel::{mapping::GraphMapping, representation::select};
    use std::sync::Arc;
    let mut m = GraphMapping::new();
    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int64, false),
        Field::new("value", DataType::Utf8, true),
    ]));
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(Int64Array::from(vec![1, 1, 2])),
            Arc::new(StringArray::from(vec![Some("a"), Some("a"), None])),
        ],
    )
    .unwrap();
    m.register_table(
        "base",
        Arc::new(MemTable::try_new(schema, vec![vec![batch]]).unwrap()),
    );
    let source = json!({"name":"logical","default_representation":"raw","representations":[
        {"name":"raw","source":{"kind":"table","name":"base"},"statistics":[stats("base","id",vec![("1","2",1000)])]},
        {"name":"reordered","source":{"kind":"query","sql":"SELECT value,id FROM base"},"statistics":[stats("base","id",vec![("1","2",100)])]}
    ]});
    m.register_representation_source(serde_json::from_value(source).unwrap())
        .unwrap();
    let selected = select(
        m.relational_plan("SELECT id,value FROM logical ORDER BY id")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        selected.representation_selections[0].representation,
        "reordered"
    );
    let batches = SessionContext::new()
        .execute_logical_plan(selected.plan)
        .await
        .unwrap()
        .collect()
        .await
        .unwrap();
    assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 3);
    assert_eq!(
        batches
            .iter()
            .map(|b| b.column(1).null_count())
            .sum::<usize>(),
        1
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn shared_engine_execution_statistics_lazy_elements_and_writes() {
    use orchiddb::engine::GraphEngine;
    use std::sync::Arc;
    let mut engine = GraphEngine::mapped(database(), Arc::new(mapping())).unwrap();
    for (predicate, chosen, count) in [("i.order_id=1", "nested", 2), ("i.sku='a'", "flat_sku", 2)]
    {
        let result = engine
            .cypher(&format!("MATCH (i:Item) WHERE {predicate} RETURN i.sku"))
            .await
            .unwrap();
        assert_eq!(result.returned.batch.num_rows(), count);
        assert_eq!(
            result.stats.representation_selections[0].representation,
            chosen
        );
        assert_eq!(
            engine
                .cypher(&format!("MATCH (i:Item) WHERE {predicate} RETURN i"))
                .await
                .unwrap()
                .returned
                .batch
                .num_rows(),
            count
        );
    }
    let error = engine
        .cypher("MATCH (i:Item) WHERE i.order_id=1 SET i.sku='updated'")
        .await
        .unwrap_err();
    assert!(error.to_string().contains("read-only"), "{error}");
    // Query-backed mappings may also refer to the logical representation.
    let mut m = mapping();
    m.map_node(
        orchiddb::ir::rel::mapping::NodeMapping::query(
            "Alias",
            "SELECT * FROM items",
            ["order_id", "item_id"],
        )
        .property("sku", "sku"),
    );
    let mut engine = GraphEngine::mapped(database(), Arc::new(m)).unwrap();
    assert_eq!(
        engine
            .cypher("MATCH (i:Alias) WHERE i.sku='a' RETURN i")
            .await
            .unwrap()
            .returned
            .batch
            .num_rows(),
        2
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn unified_rdf_reads_selected_representation_and_rejects_writes() {
    use orchiddb::engine::GraphEngine;
    use std::sync::Arc;
    let mut m = mapping();
    m.map_rdf(serde_json::from_value(json!({"table":"items","subject":{"kind":"template","prefix":"urn:item:","columns":["order_id","item_id"]},"predicate":{"kind":"constant","value":"urn:sku"},"object":{"kind":"literal","column":"sku"},"writable":true,"key":["order_id","item_id"]})).unwrap());
    let mut engine = GraphEngine::mapped(database(), Arc::new(m)).unwrap();
    let result = engine
        .sparql_dataset(
            "SELECT ?s WHERE {?s <urn:sku> ?sku FILTER(?sku = \"a\")}",
            "default",
        )
        .await
        .unwrap();
    assert_eq!(result.returned.batch.num_rows(), 2);
    assert_eq!(
        result.stats.representation_selections[0].representation,
        "flat_sku"
    );
    let error = engine
        .sparql_update("DELETE WHERE {?s <urn:sku> ?sku}", "default", None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("read-only"), "{error}");
}

#[tokio::test]
async fn definitions_reject_statements_and_forward_references_bind() {
    for sql in [
        "DELETE FROM flat",
        "CREATE TABLE bad AS SELECT * FROM flat",
        "EXPLAIN SELECT * FROM flat",
    ] {
        let mut m = mapping();
        let source = json!({"name":"invalid","default_representation":"bad","representations":[{"name":"bad","source":{"kind":"query","sql":sql}}]});
        assert!(
            m.register_representation_source(serde_json::from_value(source).unwrap())
                .is_err(),
            "{sql}"
        );
    }
    let mut r = request("MATCH (i:Item) WHERE i.sku='a' RETURN i.sku");
    r["representation_sources"].as_array_mut().unwrap().insert(0,json!({"name":"front","default_representation":"items","representations":[{"name":"items","source":{"kind":"table","name":"items"}}]}));
    r["nodes"][0]["table"] = json!("front");
    let result = compile(r).await;
    assert_eq!(
        result["representation_selections"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        result["representation_selections"][1]["representation"],
        "flat_sku"
    );
}

#[tokio::test]
async fn representation_costs_compose_with_partition_layouts() {
    let mut r = request("MATCH (i:Item) WHERE i.order_id=1 RETURN i.sku");
    let columns = r["tables"][0]["columns"].clone();
    r["tables"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"orders_by_id","columns":columns}));
    r["logical_sources"] = json!([{"name":"order_layouts","default_table":"orders","layouts":[
        stats("orders","id",vec![("1","99",100000)]),
        stats("orders_by_id","id",vec![("1","49",1000),("50","99",99000)])
    ]}]);
    r["collection_sources"][0]["table"] = json!("order_layouts");
    r["representation_sources"][0]["representations"][0]["statistics"] = json!([]);
    let result = compile(r).await;
    assert_eq!(
        result["representation_selections"][0]["representation"],
        "nested"
    );
    assert_eq!(
        result["representation_selections"][0]["scans"][0]["table"],
        "orders_by_id"
    );
    assert_eq!(result["layout_selections"].as_array().unwrap().len(), 1);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn quad_mapping_cannot_write_through_representation() {
    use orchiddb::{engine::GraphEngine, ir::rel::rdf::IriQuadSource};
    use std::sync::Arc;
    let mut m = mapping();
    m.register_representation_source(
        serde_json::from_value(json!({
            "name":"quads", "default_representation":"definition",
            "representations":[{"name":"definition","source":{"kind":"query",
                "sql":"SELECT sku AS s, sku AS p, sku AS o FROM items"}}]
        }))
        .unwrap(),
    )
    .unwrap();
    let mut rdf = m.rdf_mapping();
    rdf.map_iri_quads(
        "quads",
        IriQuadSource::table("quads", "s", "p", "o").writable(),
    );
    let mut engine = GraphEngine::mapped(database(), Arc::new(m.with_rdf_mapping(rdf))).unwrap();
    let error = engine
        .sparql_update("INSERT DATA { <urn:s> <urn:p> <urn:o> }", "quads", None)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("representation source `quads` is read-only"),
        "{error}"
    );
}

#[tokio::test]
async fn relationship_mapping_uses_canonical_endpoint_keys() {
    let mut r = request("MATCH (o:Order)-[e:CONTAINS]->(i:Item) WHERE e.sku='a' RETURN o.id,i.sku");
    r["nodes"].as_array_mut().unwrap().push(json!({
        "label":"Order","table":"orders","id":"id","properties":{"id":"id"}
    }));
    r["edges"] = json!([{
        "label":"CONTAINS","table":"items","id":["order_id","item_id"],
        "source":"order_id","target":["order_id","item_id"],
        "source_label":"Order","target_label":"Item","properties":{"sku":"sku"}
    }]);
    let result = compile(r).await;
    assert!(
        result["representation_selections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| {
                s["representation"] == "flat_sku"
                    && s["predicates"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|p| p.as_str().unwrap().contains("sku"))
            }),
        "{result}"
    );
    #[cfg(feature = "duckdb")]
    {
        let db = database();
        let mut statement = db.prepare(result["sql"].as_str().unwrap()).unwrap();
        let mut rows = statement
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows.sort();
        assert_eq!(rows, vec![(1, "a".into()), (50, "a".into())]);
    }
}

#[tokio::test]
async fn representation_dependencies_can_exceed_sixty_four_levels() {
    use orchiddb::ir::rel::{mapping::GraphMapping, representation::select};
    use arrow::datatypes::{DataType, Field, Schema};
    use std::sync::Arc;
    let mut mapping = GraphMapping::new();
    mapping.register_table_schema("base", Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false)])));
    let mut previous = "base".to_owned();
    for index in 0..80 {
        let name = format!("level_{index}");
        mapping.register_representation_source(serde_json::from_value(json!({
            "name": name,
            "default_representation": "source",
            "representations": [{"name": "source", "source": {"kind": "table", "name": previous}}]
        })).unwrap()).unwrap();
        previous = name;
    }
    let selected = select(mapping.relational_plan(&format!("SELECT id FROM {previous}")).unwrap()).unwrap();
    assert_eq!(selected.representation_selections.len(), 80);
    assert_eq!(selected.plan.schema().field(0).data_type(), &DataType::Int64);
}
