#![cfg(feature = "duckdb")]
use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::MemTable;

use orchiddb::ir::PropertyGraph;
use orchiddb::ir::plan::Direction;
use orchiddb::ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping};
use orchiddb::ir::rel::sql::{self, DuckDbExecutor, SqlDialect, SqlExecutor, SqlValue};
use orchiddb::ir::rel::{RelBackend, RelBackendOptions};
use orchiddb::language::sparql::{OntologyMapping, SparqlPlanner};

const EX: &str = "https://example.com/";

fn fixture() -> (SparqlPlanner, RelBackend, DuckDbExecutor) {
    let people = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("person_id", DataType::Int64, false),
            Field::new("full_name", DataType::Utf8, false),
            Field::new("alias", DataType::Utf8, false),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef,
            Arc::new(StringArray::from(vec!["Alice", "Bob", "Cara"])) as ArrayRef,
            Arc::new(StringArray::from(vec!["Alice", "Bobby", "Cara"])) as ArrayRef,
        ],
    )
    .unwrap();
    let knows = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("edge_id", DataType::Int64, false),
            Field::new("from_id", DataType::Int64, false),
            Field::new("to_id", DataType::Int64, false),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![10, 11, 12])) as ArrayRef,
            Arc::new(Int64Array::from(vec![1, 1, 2])) as ArrayRef,
            Arc::new(Int64Array::from(vec![2, 3, 3])) as ArrayRef,
        ],
    )
    .unwrap();
    let mut mapping = GraphMapping::new();
    mapping
        .register_table(
            "people_view",
            Arc::new(MemTable::try_new(people.schema(), vec![vec![people]]).unwrap()),
        )
        .register_table(
            "knows_view",
            Arc::new(MemTable::try_new(knows.schema(), vec![vec![knows]]).unwrap()),
        )
        .map_node(
            NodeMapping::table("Person", "people_view", "person_id")
                .property("resource_id", "person_id")
                .property("name", "full_name")
                .property("alias", "alias"),
        )
        .map_edge(
            EdgeMapping::table(
                "KNOWS",
                "knows_view",
                "from_id",
                "to_id",
                "Person",
                "Person",
            )
            .with_id("edge_id"),
        );
    let ontology = OntologyMapping::new()
        .class_with_identity(format!("{EX}Person"), "Person", "resource_id")
        .property(format!("{EX}name"), "Person", "name")
        .property(format!("{EX}alias"), "Person", "alias")
        .relationship_between(
            format!("{EX}knows"),
            "KNOWS",
            Direction::Out,
            "Person",
            "Person",
        );
    let planner = SparqlPlanner::default().with_ontology(ontology);
    let backend = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        ..RelBackendOptions::default()
    });
    let setup = vec![
        "CREATE TABLE people (person_id BIGINT, full_name VARCHAR, alias VARCHAR)".into(),
        "INSERT INTO people VALUES (1, 'Alice', 'Alice'), (2, 'Bob', 'Bobby'), (3, 'Cara', 'Cara')"
            .into(),
        "CREATE VIEW people_view AS SELECT * FROM people".into(),
        "CREATE TABLE knows (edge_id BIGINT, from_id BIGINT, to_id BIGINT)".into(),
        "INSERT INTO knows VALUES (10, 1, 2), (11, 1, 3), (12, 2, 3)".into(),
        "CREATE VIEW knows_view AS SELECT * FROM knows".into(),
    ];
    let mut executor = DuckDbExecutor::new();
    executor.run(&setup, "SELECT 1").unwrap();
    (planner, backend, executor)
}

fn execute(
    planner: &SparqlPlanner,
    backend: &RelBackend,
    executor: &mut DuckDbExecutor,
    query: &str,
) -> Vec<Vec<SqlValue>> {
    let plan = planner.plan_str(query).unwrap();
    let lowered = backend.lower(&plan, &PropertyGraph::new()).unwrap();
    let generated = sql::unparse(&lowered, SqlDialect::DuckDb).unwrap();
    executor
        .run(&[], &generated)
        .unwrap_or_else(|error| panic!("DuckDB failed for {query}: {error}\n{generated}"))
}

#[test]
fn disconnected_typed_roots_and_object_constraints_execute_on_duckdb() {
    let (planner, backend, mut executor) = fixture();
    let prefix = "PREFIX ex: <https://example.com/> ";
    let cases = [
        (
            "SELECT ?a ?b WHERE { ?a a ex:Person; ex:name \"Alice\" . ?b a ex:Person; ex:name ?name } ORDER BY ?b",
            vec![
                vec![SqlValue::Int(1), SqlValue::Int(1)],
                vec![SqlValue::Int(1), SqlValue::Int(2)],
                vec![SqlValue::Int(1), SqlValue::Int(3)],
            ],
        ),
        (
            "SELECT ?p ?name WHERE { ?p a ex:Person; ex:name ?name; ex:alias ?name } ORDER BY ?p",
            vec![
                vec![SqlValue::Int(1), SqlValue::Text("Alice".into())],
                vec![SqlValue::Int(3), SqlValue::Text("Cara".into())],
            ],
        ),
        (
            "SELECT ?a ?b WHERE { ?a a ex:Person; ex:name ?shared . ?b a ex:Person; ex:alias ?shared } ORDER BY ?a",
            vec![
                vec![SqlValue::Int(1), SqlValue::Int(1)],
                vec![SqlValue::Int(3), SqlValue::Int(3)],
            ],
        ),
        (
            "SELECT ?a ?b WHERE { ?a a ex:Person . ?b a ex:Person . ?a ex:knows ?b . ?b ex:name \"Cara\" } ORDER BY ?a",
            vec![
                vec![SqlValue::Int(1), SqlValue::Int(3)],
                vec![SqlValue::Int(2), SqlValue::Int(3)],
            ],
        ),
        (
            "SELECT ?a ?b ?c WHERE { ?a a ex:Person . ?a ex:knows ?b, ?c . ?b ex:knows ?c }",
            vec![vec![SqlValue::Int(1), SqlValue::Int(2), SqlValue::Int(3)]],
        ),
    ];
    for (body, expected) in cases {
        let actual = execute(
            &planner,
            &backend,
            &mut executor,
            &format!("{prefix}{body}"),
        );
        assert_eq!(actual, expected, "{body}");
    }
}

#[test]
fn ontology_graph_scope_is_rejected_instead_of_ignored() {
    let planner = SparqlPlanner::default()
        .with_ontology(OntologyMapping::new().class(format!("{EX}Person"), "Person"));
    for query in [
        "PREFIX ex: <https://example.com/> SELECT ?p WHERE { GRAPH ex:g { ?p a ex:Person } }",
        "PREFIX ex: <https://example.com/> SELECT ?p WHERE { GRAPH ?g { ?p a ex:Person } }",
        "PREFIX ex: <https://example.com/> SELECT DISTINCT ?p WHERE { GRAPH ex:g { ?p a ex:Person } } LIMIT 1",
    ] {
        assert!(
            planner
                .plan_str(query)
                .unwrap_err()
                .to_string()
                .contains("named or active RDF graphs"),
            "{query}"
        );
    }
}

#[test]
fn multiple_mapped_classes_do_not_drop_a_type_constraint() {
    let planner = SparqlPlanner::default().with_ontology(
        OntologyMapping::new()
            .class(format!("{EX}Person"), "Person")
            .class(format!("{EX}Organization"), "Organization"),
    );
    let error = planner
        .plan_str(
            "PREFIX ex: <https://example.com/> SELECT ?x WHERE { ?x a ex:Person, ex:Organization }",
        )
        .unwrap_err();
    assert!(error.to_string().contains("require class intersection"));
}
