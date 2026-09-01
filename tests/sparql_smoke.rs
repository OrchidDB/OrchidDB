use std::sync::Arc;

use arrow::array::{ArrayRef, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::MemTable;

use new_graph::ir::PropertyGraph;
use new_graph::ir::df::{from_logical_plan, to_logical_plan};
use new_graph::ir::plan::Direction;
use new_graph::ir::rel::mapping::{EdgeMapping, GraphMapping, NodeMapping};
use new_graph::ir::rel::sql::{self, DuckDbExecutor, SqlDialect, SqlExecutor, SqlValue};
use new_graph::ir::rel::{RelBackend, RelBackendOptions};
use new_graph::language::sparql::{OntologyMapping, SparqlPlanner};

const EX: &str = "https://example.com/";

fn ontology() -> OntologyMapping {
    OntologyMapping::new()
        .class_with_identity(format!("{EX}Person"), "Person", "resource_id")
        .property(format!("{EX}name"), "Person", "name")
}

fn query() -> &'static str {
    r#"
        PREFIX ex: <https://example.com/>
        SELECT ?person ?name WHERE {
          ?person a ex:Person .
          ?person ex:name ?name .
        }
    "#
}

#[test]
fn ontology_resolves_sparql_to_property_graph_ir() {
    let plan = SparqlPlanner::default()
        .with_ontology(ontology())
        .plan_str(query())
        .expect("plan mapped SPARQL");
    let explained = new_graph::ir::explain(&plan);
    assert!(explained.contains("GraphNodeScan"));
    assert!(explained.contains("GraphProject"));
    assert!(!explained.contains("GraphSparqlTriplePattern"));
}

#[test]
fn unmapped_predicates_fail_when_an_ontology_is_configured() {
    let error = SparqlPlanner::default()
        .with_ontology(ontology())
        .plan_str("PREFIX ex: <https://example.com/> SELECT ?p WHERE { ?p ex:unknown ?x }")
        .unwrap_err();
    assert!(error.to_string().contains("ontology mapping"));
}

#[test]
fn mapped_sparql_lowers_to_the_users_relational_view() {
    let batch = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("person_id", DataType::Int64, false),
            Field::new("full_name", DataType::Utf8, true),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![1, 2])) as ArrayRef,
            Arc::new(StringArray::from(vec!["Alice", "Bob"])) as ArrayRef,
        ],
    )
    .unwrap();
    let mut mapping = GraphMapping::new();
    mapping.register_table(
        "people_view",
        Arc::new(MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap()),
    );
    mapping.map_node(
        NodeMapping::table("Person", "people_view", "person_id")
            .property("resource_id", "person_id")
            .property("name", "full_name"),
    );
    let backend = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        ..RelBackendOptions::default()
    });
    let plan = SparqlPlanner::default()
        .with_ontology(ontology())
        .plan_str(query())
        .unwrap();
    let lowered = backend
        .lower(&plan, &PropertyGraph::new())
        .expect("lower through schema mapping");
    let generated = sql::unparse(&lowered, SqlDialect::DuckDb).expect("generate DuckDB SQL");
    assert!(generated.contains("people_view"));
    assert!(generated.contains("full_name"));
    assert!(!generated.contains("https://example.com"));
    assert!(!generated.to_ascii_lowercase().contains("rdf"));

    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE people (person_id BIGINT, full_name VARCHAR);
            INSERT INTO people VALUES (1, 'Alice'), (2, 'Bob');
            CREATE VIEW people_view AS SELECT person_id, full_name FROM people;
            "#,
        )
        .unwrap();
    let mut statement = connection.prepare(&generated).unwrap();
    let rows: Vec<(i64, String)> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, vec![(1, "Alice".into()), (2, "Bob".into())]);
}

#[test]
fn graph_ir_datafusion_round_trip_is_preserved() {
    let plan = SparqlPlanner::default()
        .with_ontology(ontology())
        .plan_str(query())
        .unwrap();
    let logical = to_logical_plan(&plan).expect("to DataFusion logical plan");
    assert_eq!(
        from_logical_plan(&logical).expect("from DataFusion logical plan"),
        plan
    );
}

#[test]
fn mapped_sparql_relationship_executes_on_duckdb_views() {
    let people = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("person_id", DataType::Int64, false),
            Field::new("full_name", DataType::Utf8, true),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef,
            Arc::new(StringArray::from(vec!["Alice", "Bob", "Cara"])) as ArrayRef,
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
            Arc::new(Int64Array::from(vec![10, 11])) as ArrayRef,
            Arc::new(Int64Array::from(vec![1, 1])) as ArrayRef,
            Arc::new(Int64Array::from(vec![2, 3])) as ArrayRef,
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
                .property("name", "full_name"),
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
    let ontology = ontology().relationship_between(
        format!("{EX}knows"),
        "KNOWS",
        Direction::Out,
        "Person",
        "Person",
    );
    let query = r#"
        PREFIX ex: <https://example.com/>
        SELECT ?person ?friend ?friendName WHERE {
          ?person a ex:Person .
          ?friend a ex:Person .
          ?person ex:knows ?friend .
          ?friend ex:name ?friendName .
        }
        ORDER BY ?friendName
    "#;
    let plan = SparqlPlanner::default()
        .with_ontology(ontology)
        .plan_str(query)
        .unwrap();
    let backend = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        ..RelBackendOptions::default()
    });
    let lowered = backend.lower(&plan, &PropertyGraph::new()).unwrap();
    let generated = sql::unparse(&lowered, SqlDialect::DuckDb).unwrap();
    assert!(generated.contains("people_view"));
    assert!(generated.contains("knows_view"));
    assert!(!generated.to_ascii_lowercase().contains("rdf"));

    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE people (person_id BIGINT, full_name VARCHAR);
            INSERT INTO people VALUES (1, 'Alice'), (2, 'Bob'), (3, 'Cara');
            CREATE VIEW people_view AS SELECT * FROM people;
            CREATE TABLE knows (edge_id BIGINT, from_id BIGINT, to_id BIGINT);
            INSERT INTO knows VALUES (10, 1, 2), (11, 1, 3);
            CREATE VIEW knows_view AS SELECT * FROM knows;
            "#,
        )
        .unwrap();
    let rows = connection
        .prepare(&generated)
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows, vec![(1, 2, "Bob".into()), (1, 3, "Cara".into())]);
}

#[test]
fn broad_query_forms_reach_graph_ir() {
    let planner = SparqlPlanner::default();
    for query in [
        "SELECT ?s WHERE { ?s <https://example.com/p> ?o OPTIONAL { ?s <https://example.com/q> ?q } FILTER(BOUND(?o)) } ORDER BY ?s LIMIT 2",
        "ASK { VALUES ?s { <https://example.com/a> } ?s <https://example.com/p> ?o }",
        "CONSTRUCT { ?s <https://example.com/p> ?o } WHERE { ?s <https://example.com/p> ?o }",
        "DESCRIBE ?s WHERE { ?s <https://example.com/p> ?o }",
    ] {
        planner.plan_str(query).expect(query);
    }
}

#[test]
fn mapped_sparql_duckdb_execution_matrix() {
    let people = RecordBatch::try_new(
        Arc::new(Schema::new(vec![
            Field::new("person_id", DataType::Int64, false),
            Field::new("full_name", DataType::Utf8, true),
            Field::new("age_years", DataType::Int64, true),
        ])),
        vec![
            Arc::new(Int64Array::from(vec![1, 2, 3])) as ArrayRef,
            Arc::new(StringArray::from(vec!["Alice", "Bob", "Cara"])) as ArrayRef,
            Arc::new(Int64Array::from(vec![Some(29), None, Some(41)])) as ArrayRef,
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
            Arc::new(Int64Array::from(vec![10, 11])) as ArrayRef,
            Arc::new(Int64Array::from(vec![1, 1])) as ArrayRef,
            Arc::new(Int64Array::from(vec![2, 3])) as ArrayRef,
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
                .property("age", "age_years"),
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
    let backend = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        ..RelBackendOptions::default()
    });
    let ontology = ontology()
        .property(format!("{EX}age"), "Person", "age")
        .relationship_between(
            format!("{EX}knows"),
            "KNOWS",
            Direction::Out,
            "Person",
            "Person",
        );
    let planner = SparqlPlanner::default().with_ontology(ontology);
    let prefix = "PREFIX ex: <https://example.com/> ";
    let cases: Vec<(&str, Vec<Vec<SqlValue>>)> = vec![
        (
            "SELECT ?person ?name WHERE { ?person a ex:Person; ex:name ?name } ORDER BY ?person",
            vec![
                vec![SqlValue::Int(1), SqlValue::Text("Alice".into())],
                vec![SqlValue::Int(2), SqlValue::Text("Bob".into())],
                vec![SqlValue::Int(3), SqlValue::Text("Cara".into())],
            ],
        ),
        (
            "SELECT ?name WHERE { ?person a ex:Person; ex:name ?name FILTER(?name = \"Alice\") }",
            vec![vec![SqlValue::Text("Alice".into())]],
        ),
        (
            "SELECT ?name WHERE { ?person a ex:Person; ex:name ?name } ORDER BY DESC(?name) LIMIT 2",
            vec![
                vec![SqlValue::Text("Cara".into())],
                vec![SqlValue::Text("Bob".into())],
            ],
        ),
        (
            "SELECT DISTINCT ?name WHERE { ?person a ex:Person; ex:name ?name } ORDER BY ?name",
            vec![
                vec![SqlValue::Text("Alice".into())],
                vec![SqlValue::Text("Bob".into())],
                vec![SqlValue::Text("Cara".into())],
            ],
        ),
        (
            "SELECT ?person ?friend ?friendName WHERE { ?person a ex:Person; ex:knows ?friend . ?friend a ex:Person; ex:name ?friendName } ORDER BY ?friendName",
            vec![
                vec![
                    SqlValue::Int(1),
                    SqlValue::Int(2),
                    SqlValue::Text("Bob".into()),
                ],
                vec![
                    SqlValue::Int(1),
                    SqlValue::Int(3),
                    SqlValue::Text("Cara".into()),
                ],
            ],
        ),
        (
            "SELECT ?person ?age WHERE { ?person a ex:Person; ex:age ?age } ORDER BY ?person",
            vec![
                vec![SqlValue::Int(1), SqlValue::Int(29)],
                vec![SqlValue::Int(3), SqlValue::Int(41)],
            ],
        ),
    ];
    let setup = vec![
        "CREATE TABLE people (person_id BIGINT, full_name VARCHAR, age_years BIGINT)".into(),
        "INSERT INTO people VALUES (1, 'Alice', 29), (2, 'Bob', NULL), (3, 'Cara', 41)".into(),
        "CREATE VIEW people_view AS SELECT * FROM people".into(),
        "CREATE TABLE knows (edge_id BIGINT, from_id BIGINT, to_id BIGINT)".into(),
        "INSERT INTO knows VALUES (10, 1, 2), (11, 1, 3)".into(),
        "CREATE VIEW knows_view AS SELECT * FROM knows".into(),
    ];
    let mut executor = DuckDbExecutor::new();
    for (index, (body, expected)) in cases.iter().enumerate() {
        let plan = planner
            .plan_str(&format!("{prefix}{body}"))
            .unwrap_or_else(|error| {
                panic!("SPARQL matrix case {} did not plan: {error}", index + 1)
            });
        let lowered = backend
            .lower(&plan, &PropertyGraph::new())
            .unwrap_or_else(|error| {
                panic!("SPARQL matrix case {} did not lower: {error}", index + 1)
            });
        let generated = sql::unparse(&lowered, SqlDialect::DuckDb).unwrap_or_else(|error| {
            panic!("SPARQL matrix case {} did not unparse: {error}", index + 1)
        });
        let actual = executor
            .run(if index == 0 { &setup } else { &[] }, &generated)
            .unwrap_or_else(|error| {
                panic!(
                    "SPARQL matrix case {} did not execute: {error}\n{generated}",
                    index + 1
                )
            });
        assert_eq!(
            &actual,
            expected,
            "SPARQL matrix case {}: {body}",
            index + 1
        );
    }
    eprintln!(
        "mapped SPARQL DuckDB execution: {}/{} matched",
        cases.len(),
        cases.len()
    );
}
