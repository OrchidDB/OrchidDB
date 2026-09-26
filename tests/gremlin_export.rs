//! Supplemental native writer evidence; upstream Gherkin Write placeholders remain unchanged.
#[path = "common/execution.rs"]
mod datafusion_test;
use orchiddb::{
    ir::{
        catalog::{Cardinality, PropertyGraph},

        value::Value,
    },
    language::gremlin::{GremlinPlanner, parse_traversal},
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};
static SERIAL: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "orchiddb-writer-test-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn query(graph: &PropertyGraph, path: &Path, suffix: &str) -> Result<(), String> {
    let text = format!(
        "g.io({}){suffix}",
        serde_json::to_string(path.to_str().unwrap()).unwrap()
    );
    let ast = parse_traversal(&text).map_err(|e| e.to_string())?;
    let plan = GremlinPlanner::new()
        .plan(&ast)
        .map_err(|e| e.to_string())?;
    assert!(
        execute_rows(&plan, graph)
            .map_err(|e| e.to_string())?
            .is_empty()
    );
    Ok(())
}
fn fixture(rich: bool) -> PropertyGraph {
    let graph = PropertyGraph::new();
    let a = graph.insert_node("person", BTreeMap::new());
    let b = graph.insert_node("software", BTreeMap::new());
    graph
        .set_element_public_id(&a, Value::String("person-a".into()))
        .unwrap();
    graph
        .set_element_public_id(&b, Value::String("software-b".into()))
        .unwrap();
    let property = graph
        .set_vertex_property(
            &a,
            "age",
            Value::Long(27),
            Cardinality::Single,
            BTreeMap::new(),
        )
        .unwrap();
    graph
        .set_vertex_property_public_id(&property, Value::String("age-record".into()))
        .unwrap();
    let edge = graph
        .insert_edge(
            "created",
            &a,
            &b,
            BTreeMap::from([("weight".into(), Value::Float32(0.5))]),
        )
        .unwrap();
    graph
        .set_element_public_id(&edge, Value::String("edge-c".into()))
        .unwrap();
    if rich {
        for (i, value) in [Value::Int(1), Value::Long(1)].into_iter().enumerate() {
            let p = graph
                .set_vertex_property(
                    &a,
                    "tag",
                    value,
                    Cardinality::List,
                    BTreeMap::from([("since".into(), Value::Short(2020))]),
                )
                .unwrap();
            graph
                .set_vertex_property_public_id(&p, Value::String(format!("tag-{i}")))
                .unwrap();
        }
        let nested = Value::TypedMap(vec![(
            Value::Int(2),
            Value::List(vec![
                Value::BigInt("123456789012345678901234567890".parse().unwrap()),
                Value::BigDecimal("1.2300".parse().unwrap()),
                Value::Set(vec![Value::Int(3), Value::Long(3)]),
            ]),
        )]);
        graph
            .set_vertex_property(&b, "nested", nested, Cardinality::Single, BTreeMap::new())
            .unwrap();
    }
    graph
}
fn check(graph: &PropertyGraph, rich: bool, property_ids: bool) {
    assert_eq!(graph.node_ids("person").unwrap().len(), 1);
    let a = Value::Node {
        label: "person".into(),
        id: graph.node_ids("person").unwrap()[0],
    };
    let b = Value::Node {
        label: "software".into(),
        id: graph.node_ids("software").unwrap()[0],
    };
    assert_eq!(
        graph.element_public_id(&a),
        Value::String("person-a".into())
    );
    assert_eq!(
        graph.element_public_id(&b),
        Value::String("software-b".into())
    );
    let p = graph.properties(&a, &["age".into()]);
    assert_eq!(p.len(), 1);
    assert!(matches!(&p[0],Value::VertexProperty{value,..} if matches!(**value,Value::Long(27))));
    if property_ids {
        assert_eq!(
            graph.element_public_id(&p[0]),
            Value::String("age-record".into())
        );
    }
    let edges = graph.out_edges("person", graph.node_ids("person").unwrap()[0], &[]);
    assert_eq!(edges.len(), 1);
    assert_eq!(&edges[0].0, "created");
    assert!(
        matches!(graph.edge_property("created",edges[0].1,"weight"),Value::Float32(v) if v==0.5)
    );
    if rich {
        let tags = graph.properties(&a, &["tag".into()]);
        assert_eq!(tags.len(), 2);
        assert!(
            matches!(&tags[0],Value::VertexProperty{value,..} if matches!(**value,Value::Int(1)))
        );
        assert!(
            matches!(&tags[1],Value::VertexProperty{value,..} if matches!(**value,Value::Long(1)))
        );
        for (i, p) in tags.iter().enumerate() {
            assert_eq!(
                graph.element_public_id(p),
                Value::String(format!("tag-{i}"))
            );
            assert!(
                matches!(&graph.properties(p,&[])[0],Value::Property{value,..} if matches!(**value,Value::Short(2020)))
            );
        }
        let nested = graph.properties(&b, &["nested".into()]);
        assert!(
            matches!(&nested[0],Value::VertexProperty{value,..} if matches!(value.as_ref(),Value::TypedMap(items) if matches!(&items[0].1,Value::List(values) if matches!(&values[0],Value::BigInt(n) if n.to_string()=="123456789012345678901234567890") && matches!(&values[1],Value::BigDecimal(n) if n.to_string()=="1.2300") && matches!(&values[2],Value::Set(v) if v.len()==2))))
        );
    }
}
#[test]
fn native_graphson_exports_snapshot_records_and_roundtrips_exact_types() {
    let directory = Directory::new();
    let graph = fixture(true);
    for suffix in [
        ".write()",
        ".with(IO.writer, IO.graphson).write()",
        ".write().with('~tinkerpop.io.writer', 'graphson')",
    ] {
        let file = directory.path("snapshot.json");
        std::fs::write(&file, "old contents").unwrap();
        query(&graph, &file, suffix).unwrap();
        let restored = PropertyGraph::new();
        query(&restored, &file, ".read()").unwrap();
        check(&restored, true, true);
        let record: serde_json::Value = serde_json::from_str(
            std::fs::read_to_string(&file)
                .unwrap()
                .lines()
                .nth(1)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(record["inE"]["created"][0]["id"], "edge-c");
    }
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    check(&graph, true, true);
}
#[test]
fn writer_failures_preserve_destination_and_cleanup_staging() {
    let directory = Directory::new();
    let graph = fixture(true);
    let file = directory.path("existing.xml");
    for suffix in [".write()", ".with(IO.writer, 'unknown').write()"] {
        std::fs::write(&file, "untouched").unwrap();
        assert!(query(&graph, &file, suffix).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "untouched");
    }
    assert!(query(&graph, &directory.path("missing/file.json"), ".write()").is_err());
    assert!(query(&graph, &directory.path("extension.unknown"), ".write()").is_err());
    assert!(query(&graph, &directory.path("directory.json"), ".read().write()").is_err());
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
    let blocked = directory.path("blocked.json");
    std::fs::create_dir(&blocked).unwrap();
    assert!(query(&graph, &blocked, ".write()").is_err());
    assert!(blocked.is_dir());
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 2);
    let a = Value::Node {
        label: "person".into(),
        id: graph.node_ids("person").unwrap()[0],
    };
    graph
        .set_vertex_property(
            &a,
            "unsupported",
            Value::UInt64(u64::MAX),
            Cardinality::Single,
            BTreeMap::new(),
        )
        .unwrap();
    assert!(query(&graph, &file, ".with(IO.writer, IO.graphson).write()").is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "untouched");
}
#[test]
fn empty_graph_export_is_valid_and_read_only_strategy_allows_export() {
    let directory = Directory::new();
    let file = directory.path("empty.json");
    let graph = PropertyGraph::new();
    query(&graph, &file, ".write()").unwrap();
    assert_eq!(std::fs::read(&file).unwrap(), b"");
    query(&graph, &file, ".read()").unwrap();
    let text = format!(
        "g.withStrategies(ReadOnlyStrategy).io({}).write()",
        serde_json::to_string(file.to_str().unwrap()).unwrap()
    );
    let plan = GremlinPlanner::new()
        .plan(&parse_traversal(&text).unwrap())
        .unwrap();
    assert!(!orchiddb::ir::exec::contains_mutation(&plan.root));
    execute_rows(&plan, &graph).unwrap();
}
#[test]
#[ignore = "requires pinned Java ImportGraph/ExportGraph classpath; run explicitly with ORCHIDDB_GREMLIN_IO_CLASSPATH"]
fn pinned_independent_readers_validate_all_six_default_and_explicit_writers() {
    let classpath = std::env::var("ORCHIDDB_GREMLIN_IO_CLASSPATH").expect("codec classpath");
    let java = std::env::var("ORCHIDDB_GREMLIN_IO_JAVA").unwrap_or_else(|_| "java".into());
    let directory = Directory::new();
    for (format, extension) in [("graphson", "json"), ("gryo", "kryo"), ("graphml", "xml")] {
        for explicit in [false, true] {
            let graph = fixture(format != "graphml");
            let file = directory.path(&format!("{format}-{explicit}.{extension}"));
            let suffix = if explicit {
                format!(".write().with(IO.writer, IO.{format})")
            } else {
                ".write()".into()
            };
            query(&graph, &file, &suffix).unwrap();
            let output = Command::new(&java)
                .args([
                    "--add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED",
                    "--add-opens=java.base/java.lang=ALL-UNNAMED",
                    "--add-opens=java.base/java.util=ALL-UNNAMED",
                    "-cp",
                    &classpath,
                    "io.orchiddb.gremlin.codec.ImportGraph",
                    format,
                ])
                .arg(&file)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let independent = directory.path("independent.json");
            std::fs::write(&independent, &output.stdout).unwrap();
            let restored = PropertyGraph::new();
            query(&restored, &independent, ".read()").unwrap();
            check(&restored, format != "graphml", format != "graphml");
            let native = PropertyGraph::new();
            query(&native, &file, ".read()").unwrap();
            check(&native, format != "graphml", format != "graphml");
        }
    }
}

#[test]
fn graphson_preserves_enabled_null_vertex_edge_and_meta_properties() {
    let directory = Directory::new();
    let file = directory.path("nulls.json");
    let graph = PropertyGraph::new();
    graph.enable_null_property_values(true);
    let vertex = graph.insert_node("n", BTreeMap::new());
    graph
        .set_element_public_id(&vertex, Value::String("null-vertex".into()))
        .unwrap();
    let property = graph
        .set_vertex_property(
            &vertex,
            "null-value",
            Value::Null,
            Cardinality::Single,
            BTreeMap::from([("null-meta".into(), Value::Null)]),
        )
        .unwrap();
    graph
        .set_vertex_property_public_id(&property, Value::String("null-property".into()))
        .unwrap();
    let edge = graph
        .insert_edge("loop", &vertex, &vertex, BTreeMap::new())
        .unwrap();
    graph
        .set_element_public_id(&edge, Value::String("null-edge".into()))
        .unwrap();
    graph
        .set_gremlin_property(&edge, "null-edge-value", Value::Null)
        .unwrap();
    query(&graph, &file, ".write()").unwrap();
    let restored = PropertyGraph::new();
    restored.enable_null_property_values(true);
    query(&restored, &file, ".read()").unwrap();
    let vertex = Value::Node {
        label: "n".into(),
        id: restored.node_ids("n").unwrap()[0],
    };
    let properties = restored.properties(&vertex, &[]);
    assert_eq!(properties.len(), 1);
    assert!(
        matches!(&properties[0],Value::VertexProperty{value,..} if matches!(**value,Value::Null))
    );
    assert_eq!(restored.properties(&properties[0], &[]).len(), 1);
    let edge = Value::Edge {
        rel_type: "loop".into(),
        id: restored.edge_ids("loop")[0],
        src_label: "n".into(),
        src_id: 0,
        dst_label: "n".into(),
        dst_id: 0,
        projected_properties: None,
    };
    assert!(
        matches!(&restored.properties(&edge,&[])[0],Value::Property{value,..} if matches!(**value,Value::Null))
    );
}

#[test]
#[ignore = "requires pinned Java ExportGraph classpath"]
fn codec_failure_leaves_destination_and_removes_both_staging_files() {
    let directory = Directory::new();
    let file = directory.path("existing.xml");
    let graph = fixture(false);
    let vertex = Value::Node {
        label: "person".into(),
        id: graph.node_ids("person").unwrap()[0],
    };
    // A reserved GraphML label key is a real upstream writer error, after the
    // native snapshot has already been serialized and both files staged.
    graph
        .set_vertex_property(
            &vertex,
            "labelV",
            Value::String("conflict".into()),
            Cardinality::Single,
            BTreeMap::new(),
        )
        .unwrap();
    std::fs::write(&file, "existing destination").unwrap();
    let failure = query(&graph, &file, ".write()").unwrap_err();
    assert!(failure.contains("writer failed"), "{failure}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "existing destination"
    );
    assert_eq!(std::fs::read_dir(&directory.0).unwrap().count(), 1);
}

#[test]
#[ignore = "requires pinned Java ImportGraph/ExportGraph classpath"]
fn gryo_and_independent_graphson_reader_preserve_enabled_nulls() {
    let directory = Directory::new();
    let graph = PropertyGraph::new();
    graph.enable_null_property_values(true);
    let vertex = graph.insert_node("n", BTreeMap::new());
    let property = graph
        .set_vertex_property(
            &vertex,
            "n",
            Value::Null,
            Cardinality::Single,
            BTreeMap::from([("m".into(), Value::Null)]),
        )
        .unwrap();
    graph
        .set_vertex_property_public_id(&property, Value::String("p".into()))
        .unwrap();
    let edge = graph
        .insert_edge("loop", &vertex, &vertex, BTreeMap::new())
        .unwrap();
    graph.set_gremlin_property(&edge, "n", Value::Null).unwrap();
    let classpath = std::env::var("ORCHIDDB_GREMLIN_IO_CLASSPATH").expect("codec classpath");
    let java = std::env::var("ORCHIDDB_GREMLIN_IO_JAVA").unwrap_or_else(|_| "java".into());
    for (format, extension) in [("graphson", "json"), ("gryo", "kryo")] {
        let file = directory.path(&format!("nulls.{extension}"));
        query(&graph, &file, ".write()").unwrap();
        let output = Command::new(&java)
            .args([
                "--add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED",
                "--add-opens=java.base/java.lang=ALL-UNNAMED",
                "--add-opens=java.base/java.util=ALL-UNNAMED",
                "-cp",
                &classpath,
                "io.orchiddb.gremlin.codec.ImportGraph",
                format,
            ])
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let record: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(record["properties"]["n"][0]["value"].is_null());
        assert_eq!(
            record["properties"]["n"][0]["properties"]
                .as_object()
                .unwrap()
                .get("m"),
            Some(&serde_json::Value::Null)
        );
        assert_eq!(
            record["outE"]["loop"][0]["properties"]
                .as_object()
                .unwrap()
                .get("n"),
            Some(&serde_json::Value::Null)
        );
        let restored = PropertyGraph::new();
        restored.enable_null_property_values(true);
        query(&restored, &file, ".read()").unwrap();
        let vertex = Value::Node {
            label: "n".into(),
            id: restored.node_ids("n").unwrap()[0],
        };
        assert_eq!(restored.properties(&vertex, &[]).len(), 1);
        let edge = Value::Edge {
            rel_type: "loop".into(),
            id: restored.edge_ids("loop")[0],
            src_label: "n".into(),
            src_id: 0,
            dst_label: "n".into(),
            dst_id: 0,
            projected_properties: None,
        };
        assert_eq!(restored.properties(&edge, &[]).len(), 1);
    }
}

#[test]
fn graphml_rejects_mixed_scalar_types_for_one_key_without_replacing_output() {
    let directory = Directory::new();
    let file = directory.path("typed.xml");
    let graph = fixture(false);
    let vertex = Value::Node {
        label: "software".into(),
        id: graph.node_ids("software").unwrap()[0],
    };
    graph
        .set_vertex_property(
            &vertex,
            "age",
            Value::Int(27),
            Cardinality::Single,
            BTreeMap::new(),
        )
        .unwrap();
    std::fs::write(&file, "unchanged").unwrap();
    assert!(
        query(&graph, &file, ".write()")
            .unwrap_err()
            .contains("one scalar type")
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "unchanged");
}

#[test]
#[ignore = "requires pinned Java ImportGraph/ExportGraph classpath"]
fn typed_float_nonfinite_values_survive_native_and_independent_readers() {
    let directory = Directory::new();
    let graph = PropertyGraph::new();
    let vertex = graph.insert_node("n", BTreeMap::new());
    for (key, value) in [
        ("nan32", Value::Float32(f32::NAN)),
        ("nan64", Value::Float(f64::NAN)),
        ("inf32", Value::Float32(f32::INFINITY)),
        ("inf64", Value::Float(f64::INFINITY)),
        ("neg64", Value::Float(f64::NEG_INFINITY)),
        ("zero32", Value::Float32(-0.0)),
        ("ordinary", Value::Float(1.25)),
    ] {
        graph
            .set_vertex_property(&vertex, key, value, Cardinality::Single, BTreeMap::new())
            .unwrap();
    }
    let classpath = std::env::var("ORCHIDDB_GREMLIN_IO_CLASSPATH").expect("codec classpath");
    let java = std::env::var("ORCHIDDB_GREMLIN_IO_JAVA").unwrap_or_else(|_| "java".into());
    for (format, extension) in [("graphson", "json"), ("gryo", "kryo")] {
        let file = directory.path(&format!("floats.{extension}"));
        query(&graph, &file, ".write()").unwrap();
        let output = Command::new(&java)
            .args([
                "--add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED",
                "--add-opens=java.base/java.lang=ALL-UNNAMED",
                "--add-opens=java.base/java.util=ALL-UNNAMED",
                "-cp",
                &classpath,
                "io.orchiddb.gremlin.codec.ImportGraph",
                format,
            ])
            .arg(&file)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let independent = directory.path("independent.json");
        std::fs::write(&independent, &output.stdout).unwrap();
        for file in [&file, &independent] {
            let restored = PropertyGraph::new();
            query(&restored, file, ".read()").unwrap();
            let vertex = Value::Node {
                label: "n".into(),
                id: restored.node_ids("n").unwrap()[0],
            };
            let property = |key: &str| {
                let properties = restored.properties(&vertex, &[key.into()]);
                match &properties[0] {
                    Value::VertexProperty { value, .. } => value.as_ref().clone(),
                    _ => panic!("expected property"),
                }
            };
            assert!(matches!(property("nan32"),Value::Float32(v) if v.is_nan()));
            assert!(matches!(property("nan64"),Value::Float(v) if v.is_nan()));
            assert!(matches!(property("inf32"),Value::Float32(v) if v==f32::INFINITY));
            assert!(matches!(property("inf64"),Value::Float(v) if v==f64::INFINITY));
            assert!(matches!(property("neg64"),Value::Float(v) if v==f64::NEG_INFINITY));
            assert!(
                matches!(property("zero32"),Value::Float32(v) if v==0.0 && v.is_sign_negative())
            );
            assert!(matches!(property("ordinary"),Value::Float(v) if v==1.25));
        }
    }
}

use crate::datafusion_test::execute_rows;
