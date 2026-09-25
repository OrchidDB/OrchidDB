//! Typed SPARQL algebra executed on DuckDB over a typed RDF quad table.
#![cfg(feature = "duckdb")]

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema};

use new_graph::ir::rel::mapping::schema_only_provider;
use new_graph::ir::rel::rdf::{IriQuadSource, RdfDatasetMapping, RdfTermColumns};
use new_graph::ir::rel::sql::DuckDbExecutor;
use new_graph::rdf_engine::{RdfGraphEngine, RdfTermValue, SparqlResults};

const EX: &str = "http://example.org/";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const PREFIX: &str =
    "PREFIX ex: <http://example.org/> PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> ";

fn engine() -> RdfGraphEngine {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE quads (
                g VARCHAR, s VARCHAR, s_kind VARCHAR, p VARCHAR, p_kind VARCHAR,
                o VARCHAR, o_kind VARCHAR, o_dt VARCHAR, o_lang VARCHAR);
            INSERT INTO quads VALUES
              (NULL, 'http://example.org/alice', 'IRI', 'http://example.org/name', 'IRI', 'Alice', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#string', NULL),
              (NULL, 'http://example.org/alice', 'IRI', 'http://example.org/nick', 'IRI', 'Alice', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#string', NULL),
              (NULL, 'http://example.org/alice', 'IRI', 'http://example.org/label', 'IRI', 'Alice', 'LITERAL', 'http://www.w3.org/1999/02/22-rdf-syntax-ns#langString', 'EN'),
              (NULL, 'http://example.org/alice', 'IRI', 'http://example.org/age', 'IRI', '30', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#integer', NULL),
              (NULL, 'http://example.org/alice', 'IRI', 'http://example.org/knows', 'IRI', 'http://example.org/bob', 'IRI', NULL, NULL),
              (NULL, 'http://example.org/bob', 'IRI', 'http://example.org/name', 'IRI', 'Bob', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#string', NULL),
              (NULL, 'http://example.org/bob', 'IRI', 'http://example.org/age', 'IRI', '25', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#integer', NULL),
              (NULL, 'http://example.org/bob', 'IRI', 'http://example.org/height', 'IRI', '1.80', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#decimal', NULL),
              (NULL, 'http://example.org/bob', 'IRI', 'http://example.org/knows', 'IRI', 'http://example.org/cara', 'IRI', NULL, NULL),
              (NULL, 'http://example.org/cara', 'IRI', 'http://example.org/name', 'IRI', 'Cara', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#string', NULL),
              (NULL, 'http://example.org/cara', 'IRI', 'http://example.org/age', 'IRI', '41', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#integer', NULL),
              (NULL, 'http://example.org/dan', 'IRI', 'http://example.org/name', 'IRI', 'Dan', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#string', NULL),
              ('http://example.org/g', 'http://example.org/cara', 'IRI', 'http://example.org/knows', 'IRI', 'http://example.org/alice', 'IRI', NULL, NULL),
              (NULL, 'b1', 'BLANK', 'http://example.org/name', 'IRI', 'Anon', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#string', NULL);
            "#,
        )
        .unwrap();
    let schema = Arc::new(Schema::new(
        [
            "g", "s", "s_kind", "p", "p_kind", "o", "o_kind", "o_dt", "o_lang",
        ]
        .into_iter()
        .map(|name| Field::new(name, DataType::Utf8, true))
        .collect::<Vec<_>>(),
    ));
    let mut mapping = RdfDatasetMapping::new();
    mapping
        .register_table("quads", schema_only_provider(schema))
        .map_typed_quads(
            "people",
            IriQuadSource::table("quads", "s", "p", "o")
                .graph_column("g")
                .typed_term_columns(
                    RdfTermColumns::new("s", "s_kind"),
                    RdfTermColumns::new("p", "p_kind"),
                    RdfTermColumns::new("o", "o_kind")
                        .datatype("o_dt")
                        .language("o_lang"),
                ),
        );
    RdfGraphEngine::new(
        DuckDbExecutor::from_connection(connection),
        Arc::new(mapping),
        "people",
    )
}

fn iri(local: &str) -> Option<RdfTermValue> {
    Some(RdfTermValue::iri(format!("{EX}{local}")))
}

fn int(value: &str) -> Option<RdfTermValue> {
    Some(RdfTermValue::typed(value, format!("{XSD}integer")))
}

fn string(value: &str) -> Option<RdfTermValue> {
    Some(RdfTermValue::string(value))
}

#[tokio::test]
async fn composed_alternatives_preserve_each_route() {
    let mut engine = engine();
    assert_eq!(rows(&mut engine, "SELECT ?t WHERE { ex:alice (ex:knows|ex:knows)/(ex:knows|ex:knows) ?t }").await,
        vec![vec![iri("cara")]; 4]);
}

#[tokio::test]
async fn minus_does_not_count_the_implicit_graph_scope_as_a_shared_binding() {
    let mut engine = engine();
    assert_eq!(rows(&mut engine, "SELECT ?a WHERE { GRAPH ?g { ?a ex:knows ex:alice MINUS { ?b ex:knows ex:alice } } }").await,
        vec![vec![iri("cara")]]);
}

async fn rows(engine: &mut RdfGraphEngine, body: &str) -> Vec<Vec<Option<RdfTermValue>>> {
    let query = format!("{PREFIX}{body}");
    if std::env::var_os("SPARQL_TEST_SQL").is_some() {
        eprintln!(
            "QUERY {body}\nSQL {} bytes\n{}",
            engine.sql(&query).await.map(|s| s.len()).unwrap_or(0),
            engine.sql(&query).await.unwrap_or_default()
        );
    }
    match engine.query(&query).await {
        Ok(SparqlResults::Solutions { rows, .. }) => rows,
        Ok(other) => panic!("expected solutions for {body}, got {other:?}"),
        Err(error) => panic!(
            "{body}\n{error}\n{}",
            engine.sql(&query).await.unwrap_or_default()
        ),
    }
}

async fn sorted(engine: &mut RdfGraphEngine, body: &str) -> Vec<Vec<Option<RdfTermValue>>> {
    let mut out = rows(engine, body).await;
    out.sort();
    out
}

#[tokio::test]
async fn composed_and_repeated_paths() {
    let mut e = engine();
    assert_eq!(
        sorted(&mut e, "SELECT ?o { ex:alice ex:knows+ ?o }").await,
        vec![vec![iri("bob")], vec![iri("cara")]]
    );
    assert_eq!(
        sorted(&mut e, "SELECT ?o { ex:alice ex:knows* ?o }").await,
        vec![vec![iri("alice")], vec![iri("bob")], vec![iri("cara")]]
    );
    assert_eq!(
        sorted(&mut e, "SELECT ?s { ex:cara ^ex:knows+ ?s }").await,
        vec![vec![iri("alice")], vec![iri("bob")]]
    );
    assert_eq!(
        rows(&mut e, "SELECT ?o { ex:alice ex:knows/ex:knows ?o }").await,
        vec![vec![iri("cara")]]
    );
    assert_eq!(
        rows(&mut e, "SELECT ?o { ex:alice (ex:name|ex:nick) ?o }").await,
        vec![vec![string("Alice")], vec![string("Alice")]]
    );
    assert_eq!(
        rows(&mut e, "SELECT ?s { ex:bob !^ex:name ?s }").await,
        vec![vec![iri("alice")]]
    );
    assert_eq!(
        rows(&mut e, "SELECT ?o { ex:bob !(ex:age|^ex:name) ?o }")
            .await
            .len(),
        4
    );
    assert_eq!(
        rows(
            &mut e,
            "SELECT ?o { ex:alice !(ex:name|ex:nick|ex:label|ex:age) ?o }"
        )
        .await,
        vec![vec![iri("bob")]]
    );
}

#[tokio::test]
async fn zero_length_keeps_term_identity_and_absent_constants() {
    let mut e = engine();
    assert_eq!(
        rows(&mut e, "SELECT ?o { ex:missing ex:knows* ?o }").await,
        vec![vec![iri("missing")]]
    );
    assert_eq!(
        rows(&mut e, "SELECT ?o { ex:missing ex:knows? ?o }").await,
        vec![vec![iri("missing")]]
    );
    assert!(
        rows(&mut e, "SELECT ?o { ex:missing ex:knows+ ?o }")
            .await
            .is_empty()
    );
    assert_eq!(
        rows(&mut e, "SELECT ?s { ?s ex:knows* 30 }").await,
        vec![vec![int("30")]]
    );
    assert_eq!(
        rows(
            &mut e,
            "SELECT ?s { ?s ex:knows* ?s FILTER(sameTerm(?s, 30)) }"
        )
        .await,
        vec![vec![int("30")]]
    );
    assert_eq!(
        rows(&mut e, "SELECT ?s { ?s ex:knows* ?s FILTER(isBlank(?s)) }").await,
        vec![vec![Some(RdfTermValue::BlankNode("b1".into()))]]
    );
    assert!(matches!(
        e.query(&format!(
            "{PREFIX}ASK {{ ex:missing ex:knows* ex:missing }}"
        ))
        .await
        .unwrap(),
        SparqlResults::Boolean(true)
    ));
}

#[tokio::test]
async fn graph_paths_stay_in_the_active_graph() {
    let mut e = engine();
    assert_eq!(
        rows(&mut e, "SELECT ?g ?o { GRAPH ?g { ex:cara ex:knows+ ?o } }").await,
        vec![vec![iri("g"), iri("alice")]]
    );
    assert_eq!(
        rows(&mut e, "SELECT ?o FROM ex:g { ex:cara ex:knows+ ?o }").await,
        vec![vec![iri("alice")]]
    );
    assert_eq!(
        rows(&mut e, "SELECT ?o { GRAPH ex:g { ex:cara ex:knows* ?o } }")
            .await
            .len(),
        2
    );
    assert!(
        rows(&mut e, "SELECT ?o { ex:cara ex:knows+ ?o }")
            .await
            .is_empty()
    );
}

#[tokio::test]
async fn nested_closures_terminate_and_preserve_outer_bags() {
    let mut e = engine();
    // Inverse edges create cycles without changing the source data.
    let actual = sorted(&mut e, "SELECT ?o { ex:alice (ex:knows|^ex:knows)+ ?o }").await;
    assert_eq!(
        actual,
        vec![vec![iri("alice")], vec![iri("bob")], vec![iri("cara")]]
    );
    assert_eq!(
        sorted(&mut e, "SELECT ?o { ex:alice (ex:knows*)+ ?o }").await,
        actual
    );
    assert_eq!(
        rows(
            &mut e,
            "SELECT ?o { VALUES ?x { 1 1 } ex:alice ex:knows+ ?o }"
        )
        .await
        .len(),
        4
    );
}

#[tokio::test]
async fn describe_outgoing_triples_and_construct_blank_nodes() {
    let mut e = engine();
    let SparqlResults::Graph(triples) = e
        .query(&format!("{PREFIX}DESCRIBE ex:alice"))
        .await
        .unwrap()
    else {
        panic!("graph expected")
    };
    assert_eq!(triples.len(), 5);
    let SparqlResults::Graph(triples) = e
        .query(&format!(
            "{PREFIX}DESCRIBE ?s FROM ex:g WHERE {{ ?s ex:knows ex:alice }}"
        ))
        .await
        .unwrap()
    else {
        panic!("graph expected")
    };
    assert_eq!(triples.len(), 1);
    let SparqlResults::Graph(triples) = e
        .query(&format!(
            "{PREFIX}CONSTRUCT {{ _:x ex:value ?x; ex:self _:x }} WHERE {{ VALUES ?x {{ 1 1 }} }}"
        ))
        .await
        .unwrap()
    else {
        panic!("graph expected")
    };
    assert_eq!(triples.len(), 4);
    let subjects: std::collections::BTreeSet<_> = triples.iter().map(|t| t[0].clone()).collect();
    assert_eq!(subjects.len(), 2);
    for t in &triples {
        assert!(matches!(t[0], RdfTermValue::BlankNode(_)));
        if t[1] == RdfTermValue::iri(format!("{EX}self")) {
            assert_eq!(t[0], t[2]);
        }
    }
}

#[tokio::test]
async fn negated_sets_deduplicate_and_missing_named_graphs_stay_empty() {
    let mut e = engine();
    // name and nick both point at the same literal, but a negated set
    // asks whether a qualifying edge exists, not how many predicates match.
    assert_eq!(
        rows(
            &mut e,
            "SELECT ?o { ex:alice !(ex:knows|ex:age|ex:label) ?o }"
        )
        .await,
        vec![vec![string("Alice")]]
    );
    assert!(
        rows(
            &mut e,
            "SELECT ?o { GRAPH ex:missing { ex:missing ex:knows* ?o } }"
        )
        .await
        .is_empty()
    );
    assert!(
        rows(
            &mut e,
            "SELECT ?o FROM NAMED ex:allowed { GRAPH ex:g { ex:missing ex:knows? ?o } }"
        )
        .await
        .is_empty()
    );
    assert_eq!(
        rows(
            &mut e,
            "SELECT ?o { GRAPH ex:g { ex:missing ex:knows* ?o } }"
        )
        .await,
        vec![vec![iri("missing")]]
    );
}

#[tokio::test]
async fn exists_substitutes_bound_endpoints_for_zero_length_paths() {
    let mut e = engine();
    assert_eq!(
        rows(
            &mut e,
            "SELECT ?s { VALUES ?s { ex:missing } FILTER EXISTS { ?s ex:knows* ?s } }"
        )
        .await,
        vec![vec![iri("missing")]]
    );
    assert!(
        rows(
            &mut e,
            "SELECT ?s { VALUES ?s { ex:missing } FILTER EXISTS { ?s ex:knows+ ?s } }"
        )
        .await
        .is_empty()
    );
}

#[tokio::test]
async fn correlated_zero_length_domains_do_not_leak_between_rows() {
    let mut e = engine();
    assert_eq!(rows(&mut e, "SELECT ?s { VALUES ?s { ex:missing UNDEF } FILTER EXISTS { ?s ex:knows* ?o FILTER(?o = ex:missing) } }").await,
        vec![vec![iri("missing")]]);
    assert_eq!(rows(&mut e, "SELECT ?s (EXISTS { ?s ex:knows* ?o FILTER(?o = ex:missing) } AS ?found) { VALUES ?s { ex:missing UNDEF } } ORDER BY DESC(BOUND(?s))").await,
        vec![vec![iri("missing"), Some(RdfTermValue::typed("true", format!("{XSD}boolean")))],
             vec![None, Some(RdfTermValue::typed("false", format!("{XSD}boolean")))]]);
}
