//! File imports use ordinary mutation plans, typed readers and transactions.
use orchiddb::{
    ir::{catalog::PropertyGraph, interpreter::execute_rows, value::Value},
    language::gremlin::{GremlinPlanner, parse_traversal},
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct File(PathBuf);
impl File {
    fn new(extension: &str, contents: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "crabgraph-import-{}-{}.{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed),
            extension
        ));
        std::fs::write(&path, contents).unwrap();
        Self(path)
    }
    fn query(&self, option: &str) -> String {
        format!(
            "g.io({}){option}.read()",
            serde_json::to_string(self.0.to_str().unwrap()).unwrap()
        )
    }
}
impl Drop for File {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}
const GRAPH: &str = r#"
{"id":{"@type":"g:Int32","@value":71},"label":"person","outE":{"knows":[{"id":{"@type":"g:Int64","@value":91},"inV":{"@type":"g:Int32","@value":72},"properties":{"weight":{"@type":"g:Float","@value":0.5}}}]},"properties":{"age":[{"id":{"@type":"g:Int64","@value":101},"value":{"@type":"g:Int64","@value":27}}],"names":[{"id":{"@type":"g:Int64","@value":102},"value":{"@type":"g:List","@value":["a","b"]}}]}}
{"id":{"@type":"g:Int32","@value":72},"label":"person","properties":{"age":[{"id":{"@type":"g:Int64","@value":103},"value":{"@type":"g:Int32","@value":29}}]}}
"#;
fn plan(query: &str) -> orchiddb::ir::plan::GraphPlan {
    GremlinPlanner::new()
        .plan(&parse_traversal(query).unwrap())
        .unwrap()
}
fn values(graph: &PropertyGraph, query: &str) -> Vec<Value> {
    execute_rows(&plan(query), graph)
        .unwrap()
        .into_iter()
        .map(|r| r.bindings["current"].clone())
        .collect()
}
#[test]
fn graphson_reader_imports_typed_properties_and_edges() {
    let file = File::new("json", GRAPH);
    for option in [
        "",
        ".with(IO.reader, IO.graphson)",
        ".with('~tinkerpop.io.reader', 'graphson')",
    ] {
        let graph = PropertyGraph::new();
        let p = plan(&file.query(option));
        assert!(orchiddb::ir::exec::contains_mutation(&p.root));
        assert!(execute_rows(&p, &graph).unwrap().is_empty());
        assert_eq!(values(&graph, "g.V().count()"), vec![Value::Long(2)]);
        assert_eq!(values(&graph, "g.E().count()"), vec![Value::Long(1)]);
        let ages = values(&graph, "g.V().values('age')");
        assert!(matches!(ages[0], Value::Long(27)));
        assert!(matches!(ages[1], Value::Int(29)));
        assert!(matches!(values(&graph,"g.E().values('weight')")[0],Value::Float32(v) if v==0.5));
        assert_eq!(
            values(&graph, "g.V().values('names')"),
            vec![Value::List(vec![
                Value::String("a".into()),
                Value::String("b".into())
            ])]
        );
    }
}
#[test]
fn imports_require_read_and_obey_read_only_strategy() {
    assert!(
        GremlinPlanner::new()
            .plan(&parse_traversal("g.io('x.json')").unwrap())
            .is_err()
    );
    assert!(parse_traversal("g.withStrategies(ReadOnlyStrategy).io('x.json').read()").is_err());
    assert!(parse_traversal("g.V().read()").is_err());
}
#[test]
fn malformed_graphson_does_not_mutate_existing_graph() {
    let file = File::new("json", &(GRAPH.to_string() + "{invalid"));
    let graph = PropertyGraph::new();
    graph.insert_node("existing", Default::default());
    assert!(execute_rows(&plan(&file.query("")), &graph).is_err());
    assert_eq!(values(&graph, "g.V().count()"), vec![Value::Long(1)]);
    let missing = File::new("missing", "");
    assert!(execute_rows(&plan(&missing.query("")), &graph).is_err());
}

#[cfg(feature = "duckdb")]
mod durable {
    use super::*;
    use arrow::array::Int64Array;
    use orchiddb::engine::GraphEngine;
    async fn count(engine: &mut GraphEngine) -> i64 {
        let result = engine.gremlin("g.V().count()").await.unwrap();
        result
            .returned
            .batch
            .column(0)
            .as_any()
            .downcast_ref::<Int64Array>()
            .unwrap()
            .value(0)
    }
    #[tokio::test]
    async fn committed_import_survives_reopen_and_transaction_rollback() {
        let source = File::new("json", GRAPH);
        let database = File::new("duckdb", "");
        std::fs::remove_file(&database.0).unwrap();
        {
            let mut engine = GraphEngine::open(&database.0).unwrap();
            assert_eq!(
                engine
                    .gremlin(&source.query(""))
                    .await
                    .unwrap()
                    .returned
                    .batch
                    .num_rows(),
                0
            );
            assert_eq!(count(&mut engine).await, 2);
        }
        {
            let mut engine = GraphEngine::open(&database.0).unwrap();
            assert_eq!(count(&mut engine).await, 2);
            // This fails after the importer has inserted its first vertex.
            // Statement rollback must remove that partial write.
            assert!(engine.gremlin(&source.query("")).await.is_err());
            assert_eq!(count(&mut engine).await, 2);
            engine.begin().unwrap();
            // A different graph proves rollback without requiring duplicate IDs.
            let second = File::new("json", r#"{"id":"new","label":"other"}"#);
            engine.gremlin(&second.query("")).await.unwrap();
            assert_eq!(count(&mut engine).await, 3);
            engine.rollback().unwrap();
            assert_eq!(count(&mut engine).await, 2);
            let invalid = File::new("json", &(GRAPH.to_string() + "{invalid"));
            assert!(engine.gremlin(&invalid.query("")).await.is_err());
            assert_eq!(count(&mut engine).await, 2);
        }
        let mut engine = GraphEngine::open(&database.0).unwrap();
        assert_eq!(count(&mut engine).await, 2);
    }
}

#[test]
fn native_import_preserves_public_ids_and_property_record_owners() {
    let file = File::new("json", r#"{"id":"account","label":"person","properties":{"tags":[{"id":"tag-1","value":"one","properties":{"since":{"@type":"g:Int32","@value":2020}}},{"id":"tag-2","value":"two"}],"items":[{"id":"items-1","value":{"@type":"g:List","@value":["a","b"]}}]}}"#);
    let graph = PropertyGraph::new();
    execute_rows(&plan(&file.query("")), &graph).unwrap();
    let vertices = values(&graph, "g.V()");
    assert_eq!(graph.element_public_id(&vertices[0]), Value::String("account".into()));
    let tags = graph.properties(&vertices[0], &["tags".into()]);
    assert_eq!(tags.len(), 2);
    assert_eq!(graph.element_public_id(&tags[0]), Value::String("tag-1".into()));
    assert_eq!(graph.element_public_id(&tags[1]), Value::String("tag-2".into()));
    let meta = graph.properties(&tags[0], &["since".into()]);
    assert!(matches!(&meta[0], Value::Property { owner, value, .. } if **owner == tags[0] && **value == Value::Int(2020)));
    let items = graph.properties(&vertices[0], &["items".into()]);
    assert_eq!(items.len(), 1);
    assert!(matches!(&items[0], Value::VertexProperty { value, .. } if matches!(value.as_ref(), Value::List(v) if v.len()==2)));
}
