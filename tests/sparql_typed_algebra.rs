//! Typed SPARQL algebra executed on DuckDB over a typed RDF quad table.
#![cfg(feature = "duckdb")]

use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema};

use orchiddb::ir::rel::mapping::schema_only_provider;
use orchiddb::ir::rel::rdf::{IriQuadSource, RdfDatasetMapping, RdfTermColumns};
use orchiddb::ir::rel::sql::DuckDbExecutor;
use orchiddb::rdf_engine::{RdfGraphEngine, RdfTermValue, SparqlResults};

const EX: &str = "http://example.org/";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const PREFIX: &str =
    "PREFIX ex: <http://example.org/> PREFIX xsd: <http://www.w3.org/2001/XMLSchema#> ";

fn engine() -> RdfGraphEngine {
    engine_with("")
}

fn engine_with(extra: &str) -> RdfGraphEngine {
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection
        .execute_batch(
            r#"
            CREATE TABLE quads (
                g VARCHAR, s VARCHAR, s_kind VARCHAR, p VARCHAR, p_kind VARCHAR,
                o VARCHAR, o_kind VARCHAR, o_dt VARCHAR, o_lang VARCHAR);
            CREATE TABLE graph_names (iri VARCHAR);
            INSERT INTO graph_names VALUES ('http://example.org/empty');
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
              (NULL, 'b1', 'BLANK', 'http://example.org/name', 'IRI', 'Anon', 'LITERAL', 'http://www.w3.org/2001/XMLSchema#string', NULL);
            "#,
        )
        .unwrap();
    connection.execute_batch(extra).unwrap();
    let schema = Arc::new(Schema::new(
        [
            "g", "s", "s_kind", "p", "p_kind", "o", "o_kind", "o_dt", "o_lang",
        ]
        .into_iter()
        .map(|name| Field::new(name, DataType::Utf8, true))
        .collect::<Vec<_>>(),
    ));
    let mut mapping = RdfDatasetMapping::new();
    mapping.register_table("graph_names", schema_only_provider(Arc::new(Schema::new(vec![
        Field::new("iri", DataType::Utf8, false)
    ])))).map_named_graphs("people", "graph_names", "iri");
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

fn typed(value: &str, local: &str) -> Option<RdfTermValue> {
    Some(RdfTermValue::typed(value, format!("{XSD}{local}")))
}

fn string(value: &str) -> Option<RdfTermValue> {
    Some(RdfTermValue::string(value))
}

#[tokio::test]
async fn sparql_scalar_contracts_execute_over_mapped_rows() {
    let mut engine = engine();
    assert_eq!(rows(&mut engine, r#"SELECT (ENCODE_FOR_URI(?name) AS ?uri)
        (REGEX(?name, "a l i c e", "ix") AS ?matches)
        (REPLACE(?name, "(Ali)", "[$1]") AS ?replacement)
        WHERE { ex:alice ex:name ?name }"#).await,
        vec![vec![string("Alice"), typed("true", "boolean"), string("[Ali]ce")]]);
    assert_eq!(rows(&mut engine, r#"SELECT
        (TIMEZONE("2020-01-01T00:00:00-05:30"^^xsd:dateTime) AS ?zone)
        (STRLEN(SHA384("abc")) AS ?sha384) (STRLEN(SHA512("abc")) AS ?sha512)
        (NOW() = NOW() AS ?stable) (ENCODE_FOR_URI("a /é") AS ?encoded) WHERE {}"#).await,
        vec![vec![typed("-PT5H30M", "dayTimeDuration"), int("96"), int("128"), typed("true", "boolean"), string("a%20%2F%C3%A9")]]);
    assert_eq!(rows(&mut engine, r#"SELECT (REGEX("x", "[") AS ?invalid) WHERE {}"#).await,
        vec![vec![None]]);
    assert_eq!(rows(&mut engine, r#"BASE <https://example.org/a/b/>
        SELECT (IRI("../c") AS ?iri) (ENCODE_FOR_URI("café"@fr) AS ?encoded) WHERE {}"#).await,
        vec![vec![Some(RdfTermValue::iri("https://example.org/a/c")), string("caf%C3%A9")]]);
    assert_eq!(rows(&mut engine, "SELECT * WHERE {}" ).await, vec![vec![]]);
    assert!(rows(&mut engine, "SELECT * WHERE { FILTER(false) }" ).await.is_empty());
}

#[tokio::test]
async fn mapped_empty_named_graphs_preserve_the_graph_domain() {
    let mut engine = engine();
    assert_eq!(rows(&mut engine, "SELECT * WHERE { GRAPH ?g {} }").await, vec![vec![iri("empty")]]);
    assert_eq!(rows(&mut engine, "SELECT * WHERE { GRAPH ex:empty {} }").await, vec![vec![]]);
    assert!(rows(&mut engine, "SELECT * WHERE { GRAPH ex:absent {} }").await.is_empty());
    assert!(rows(&mut engine, "SELECT * WHERE { GRAPH ex:empty { ?s ?p ?o } }").await.is_empty());
    assert_eq!(rows(&mut engine, "SELECT * FROM NAMED ex:empty WHERE { GRAPH ?g {} }").await, vec![vec![iri("empty")]]);
    assert!(rows(&mut engine, "SELECT * FROM NAMED ex:absent WHERE { GRAPH ?g {} }").await.is_empty());
    assert!(rows(&mut engine, "SELECT * WHERE { GRAPH ?g { FILTER(BOUND(?g)) } }").await.is_empty());
    assert_eq!(rows(&mut engine, r#"SELECT ?g ?v WHERE { GRAPH ?g { VALUES (?g ?v) { (UNDEF "x") (ex:absent "y") } } }"#).await,
        vec![vec![iri("empty"), string("x")]]);
    assert_eq!(rows(&mut engine, "SELECT ?g ?c WHERE { GRAPH ?g { SELECT (COUNT(*) AS ?c) WHERE { ?s ?p ?o } } }").await,
        vec![vec![iri("empty"), int("0")]]);
}

#[tokio::test]
async fn graph_variables_bind_after_optional_and_subquery_scope() {
    let mut engine = engine_with("INSERT INTO quads VALUES
        ('http://example.org/g1','http://example.org/a','IRI','http://example.org/p','IRI','http://example.org/other','IRI',NULL,NULL),
        ('http://example.org/g2','http://example.org/b','IRI','http://example.org/p','IRI','http://example.org/g2','IRI',NULL,NULL);");
    assert_eq!(rows(&mut engine, "SELECT ?s WHERE { GRAPH ?g { ?s ex:p ?o OPTIONAL { ?s ex:p ?g } } }").await,
        vec![vec![iri("b")]]);
    assert_eq!(sorted(&mut engine, "SELECT ?s WHERE { GRAPH ?g { SELECT ?s WHERE { ?s ex:p ?g } } }").await,
        vec![vec![iri("a")], vec![iri("b")]]);
    assert_eq!(sorted(&mut engine, "SELECT ?g ?c WHERE { GRAPH ?g { SELECT (COUNT(*) AS ?c) WHERE { ?s ex:p ?v FILTER(?v = ex:other) } } }").await,
        vec![vec![iri("empty"), int("0")], vec![iri("g1"), int("1")], vec![iri("g2"), int("0")]]);
}

#[tokio::test]
async fn parser_preserves_nested_filter_scope_and_boolean_tokens() {
    let mut engine = engine();
    assert_eq!(rows(&mut engine, r#"SELECT (TRUE AS ?a) (FaLsE AS ?b) ("TRUE" AS ?text) WHERE {}"#).await,
        vec![vec![typed("true", "boolean"), typed("false", "boolean"), string("TRUE")]]);
    let nested = rows(&mut engine, r#"SELECT ?name ?age WHERE {
        ex:alice ex:name ?name OPTIONAL { { ex:alice ex:age ?age FILTER(?name = "Alice") } } }"#).await;
    assert_eq!(nested, vec![vec![string("Alice"), None]]);
    let direct = rows(&mut engine, r#"SELECT ?name ?age WHERE {
        ex:alice ex:name ?name OPTIONAL { ex:alice ex:age ?age FILTER(?name = "Alice") } }"#).await;
    assert_eq!(direct, vec![vec![string("Alice"), int("30")]]);
    use orchiddb::language::sparql::parse_query_with_base;
    assert!(parse_query_with_base("SELECT * WHERE { FILTER (?x<?a&&?b>?y) }", EX).is_err());
    assert!(parse_query_with_base("SELECT * WHERE { FILTER (?x < ?a && ?b > ?y) }", EX).is_ok());
}

#[tokio::test]
async fn bnode_allocation_is_local_to_each_solution() {
    let mut engine = engine();
    let result = rows(&mut engine, r#"SELECT (BNODE(?x) AS ?a) (BNODE(?x) AS ?b)
        (BNODE("other") AS ?c) WHERE { VALUES ?x { "same" "same" } }"#).await;
    assert_eq!(result.len(), 2);
    for row in &result {
        assert!(matches!(&row[0], Some(RdfTermValue::BlankNode(_))));
        assert_eq!(row[0], row[1]);
        assert_ne!(row[0], row[2]);
    }
    assert_ne!(result[0][0], result[1][0]);
    let fresh = rows(&mut engine, "SELECT (BNODE() AS ?a) (BNODE() AS ?b) WHERE {}").await;
    assert_ne!(fresh[0][0], fresh[0][1]);
    assert_eq!(rows(&mut engine, r#"SELECT DISTINCT ?x WHERE {
        VALUES ?x { "same" "same" } BIND(BNODE(?x) AS ?hidden) }"#).await,
        vec![vec![string("same")]]);
}

#[tokio::test]
async fn rdf_datetime_values_and_decimal_division() {
    let mut engine = engine();
    assert_eq!(rows(&mut engine, r#"SELECT
      ("2002-04-02T23:00:00-04:00"^^xsd:dateTime = "2002-04-03T02:00:00-01:00"^^xsd:dateTime AS ?offset)
      ("1999-12-31T24:00:00"^^xsd:dateTime = "2000-01-01T00:00:00"^^xsd:dateTime AS ?midnight)
      ("2008-10-01T00:00:00Z"^^xsd:dateTime < "2008-10-03T00:00:00"^^xsd:dateTime AS ?mixed)
      (11.1 / 5 AS ?decimal) WHERE {}"#).await,
      vec![vec![typed("true", "boolean"), typed("true", "boolean"), typed("true", "boolean"), typed("2.22", "decimal")]]);
    assert_eq!(rows(&mut engine, "SELECT (AVG(?v) AS ?mean) WHERE { VALUES ?v { 1.11 3.33 } }").await,
        vec![vec![typed("2.22", "decimal")]]);
    assert_eq!(rows(&mut engine, r#"SELECT (xsd:string("0"^^xsd:boolean) AS ?boolean)
        (xsd:string("1.00"^^xsd:decimal) AS ?decimal) (STR("1.00"^^xsd:decimal) AS ?lexical) WHERE {}"#).await,
        vec![vec![string("false"), string("1"), string("1.00")]]);
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
async fn select_returns_typed_terms_in_sparql_order() {
    let mut engine = engine();
    let query = format!("{PREFIX}SELECT ?s ?age WHERE {{ ?s ex:age ?age }} ORDER BY DESC(?age)");
    let SparqlResults::Solutions { variables, rows } = engine.query(&query).await.unwrap() else {
        panic!("solutions expected");
    };
    assert_eq!(variables, vec!["?s", "?age"]);
    assert_eq!(
        rows,
        vec![
            vec![iri("cara"), int("41")],
            vec![iri("alice"), int("30")],
            vec![iri("bob"), int("25")],
        ]
    );
}

#[tokio::test]
async fn literal_identity_distinguishes_language_tags_and_blank_nodes() {
    let mut engine = engine();
    assert_eq!(
        sorted(&mut engine, "SELECT ?p WHERE { ex:alice ?p \"Alice\" }").await,
        vec![vec![iri("name")], vec![iri("nick")]]
    );
    assert_eq!(
        rows(&mut engine, "SELECT ?p WHERE { ex:alice ?p \"Alice\"@en }").await,
        vec![vec![iri("label")]]
    );
    assert_eq!(
        rows(&mut engine, "SELECT ?s WHERE { ?s ex:name \"Anon\" }").await,
        vec![vec![Some(RdfTermValue::BlankNode("b1".into()))]]
    );
    assert_eq!(
        rows(&mut engine, "SELECT ?l WHERE { ex:alice ex:label ?l FILTER(lang(?l) = \"en\" && langMatches(lang(?l), \"EN\")) }").await,
        vec![vec![Some(RdfTermValue::lang("Alice", "en"))]]
    );
    // The same lexical form with a different datatype is a different term.
    assert!(
        rows(&mut engine, "SELECT ?s WHERE { ?s ex:age \"30\" }")
            .await
            .is_empty()
    );
    assert_eq!(
        rows(&mut engine, "SELECT ?s WHERE { ?s ex:age 30 }").await,
        vec![vec![iri("alice")]]
    );
}

#[tokio::test]
async fn filters_use_typed_comparison_and_error_rules() {
    let mut engine = engine();
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?s WHERE { ?s ex:age ?age FILTER(?age > 26) }"
        )
        .await,
        vec![vec![iri("alice")], vec![iri("cara")]]
    );
    // A decimal compares numerically with an integer.
    assert_eq!(
        rows(
            &mut engine,
            "SELECT ?s WHERE { ?s ex:height ?h FILTER(?h = 1.8 && ?h < 2) }"
        )
        .await,
        vec![vec![iri("bob")]]
    );
    assert!(
        rows(
            &mut engine,
            "SELECT ?s WHERE { ?s ex:name ?n FILTER(?n = 3) }"
        )
        .await
        .is_empty()
    );
    // Literals of different known datatypes are unequal, not an error.
    assert_eq!(
        rows(
            &mut engine,
            "SELECT ?s WHERE { ?s ex:name ?n FILTER(?n != 3 && !bound(?nope)) }"
        )
        .await
        .len(),
        5
    );
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?n WHERE { ?s ex:name ?n FILTER(isBlank(?s) || regex(?n, \"^c\", \"i\")) }"
        )
        .await,
        vec![vec![string("Anon")], vec![string("Cara")]]
    );
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?n WHERE { ?s ex:name ?n FILTER(STRSTARTS(STR(?s), \"http://example.org/b\")) }"
        )
        .await,
        vec![vec![string("Bob")]]
    );
}

#[tokio::test]
async fn optional_keeps_unbound_and_applies_its_filter_to_the_merged_solution() {
    let mut engine = engine();
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?n ?age WHERE { ?s ex:name ?n OPTIONAL { ?s ex:age ?age } }"
        )
        .await,
        vec![
            vec![string("Alice"), int("30")],
            vec![string("Anon"), None],
            vec![string("Bob"), int("25")],
            vec![string("Cara"), int("41")],
            vec![string("Dan"), None],
        ]
    );
    // The OPTIONAL filter reads ?n from the left side.
    assert_eq!(
        sorted(&mut engine, "SELECT ?n ?age WHERE { ?s ex:name ?n OPTIONAL { ?s ex:age ?age FILTER(?n = \"Bob\") } }").await,
        vec![
            vec![string("Alice"), None],
            vec![string("Anon"), None],
            vec![string("Bob"), int("25")],
            vec![string("Cara"), None],
            vec![string("Dan"), None],
        ]
    );
    // An unbound OPTIONAL variable joins compatibly with a later pattern.
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?n ?f WHERE { ?s ex:name ?n OPTIONAL { ?s ex:knows ?f } ?s ex:age ?a }"
        )
        .await,
        vec![
            vec![string("Alice"), iri("bob")],
            vec![string("Bob"), iri("cara")],
            vec![string("Cara"), None],
        ]
    );
}

#[tokio::test]
async fn minus_with_disjoint_domains_differs_from_not_exists() {
    let mut engine = engine();
    // No shared variable: MINUS removes nothing.
    assert_eq!(
        rows(
            &mut engine,
            "SELECT ?n WHERE { ?s ex:name ?n MINUS { ?x ex:age ?a } }"
        )
        .await
        .len(),
        5
    );
    // NOT EXISTS with no shared variable removes everything.
    assert!(
        rows(
            &mut engine,
            "SELECT ?n WHERE { ?s ex:name ?n FILTER NOT EXISTS { ?x ex:age ?a } }"
        )
        .await
        .is_empty()
    );
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?n WHERE { ?s ex:name ?n MINUS { ?s ex:age ?a } }"
        )
        .await,
        vec![vec![string("Anon")], vec![string("Dan")]]
    );
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?n WHERE { ?s ex:name ?n FILTER NOT EXISTS { ?s ex:knows ?o } }"
        )
        .await,
        vec![
            vec![string("Anon")],
            vec![string("Cara")],
            vec![string("Dan")]
        ]
    );
    // EXISTS sees outer bindings inside its own filter.
    assert_eq!(
        rows(&mut engine, "SELECT ?n WHERE { ?s ex:name ?n FILTER EXISTS { ?s ex:age ?a FILTER(?a < 30 && ?n != \"x\") } }").await,
        vec![vec![string("Bob")]]
    );
}

#[tokio::test]
async fn aggregates_follow_sparql_typing_and_empty_group_rules() {
    let mut engine = engine();
    assert_eq!(
        rows(
            &mut engine,
            "SELECT (COUNT(*) AS ?c) (SUM(?age) AS ?sum) (AVG(?age) AS ?avg) (MIN(?age) AS ?min) (MAX(?n) AS ?max) WHERE { ?s ex:age ?age ; ex:name ?n }",
        )
        .await,
        vec![vec![int("3"), int("96"), typed("32.0", "decimal"), int("25"), string("Cara")]]
    );
    // Without GROUP BY an empty input still yields one group.
    assert_eq!(
        rows(
            &mut engine,
            "SELECT (COUNT(*) AS ?c) (SUM(?x) AS ?s) (MIN(?x) AS ?m) WHERE { ?x ex:missing ?y }"
        )
        .await,
        vec![vec![int("0"), int("0"), None]]
    );
    // With GROUP BY an empty input has no groups.
    assert!(
        rows(
            &mut engine,
            "SELECT ?x (COUNT(*) AS ?c) WHERE { ?x ex:missing ?y } GROUP BY ?x"
        )
        .await
        .is_empty()
    );
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?s (COUNT(?o) AS ?c) WHERE { ?s ?p ?o } GROUP BY ?s HAVING (COUNT(?o) > 3)"
        )
        .await,
        vec![vec![iri("alice"), int("5")], vec![iri("bob"), int("4")]]
    );
    // SUM over a non-numeric value is an error: the aggregate is unbound.
    assert_eq!(
        rows(
            &mut engine,
            "SELECT (SUM(?n) AS ?s) WHERE { ?x ex:name ?n }"
        )
        .await,
        vec![vec![None]]
    );
    assert_eq!(
        rows(&mut engine, "SELECT (GROUP_CONCAT(?n; SEPARATOR=\",\") AS ?names) WHERE { SELECT ?n WHERE { ?x ex:age ?a ; ex:name ?n } ORDER BY ?n }").await.len(),
        1
    );
    assert_eq!(
        rows(
            &mut engine,
            "SELECT (COUNT(DISTINCT ?o) AS ?c) WHERE { ex:alice ?p ?o }"
        )
        .await,
        // "Alice", "Alice"@en, 30, ex:bob
        vec![vec![int("4")]]
    );
}

#[tokio::test]
async fn values_bind_and_arithmetic_keep_numeric_types() {
    let mut engine = engine();
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?s WHERE { VALUES ?age { 30 41 \"25\" } ?s ex:age ?age }"
        )
        .await,
        vec![vec![iri("alice")], vec![iri("cara")]]
    );
    assert_eq!(
        rows(
            &mut engine,
            "SELECT ?next ?half ?scaled ?d WHERE { ex:bob ex:age ?age BIND(?age + 1 AS ?next) BIND(?age / 2 AS ?half) BIND(?age * 1.5 AS ?scaled) BIND(xsd:double(?age) AS ?d) }",
        )
        .await,
        vec![vec![int("26"), typed("12.5", "decimal"), typed("37.5", "decimal"), typed("25.0", "double")]]
    );
    // A BIND error leaves the variable unbound rather than dropping the row.
    assert_eq!(
        rows(
            &mut engine,
            "SELECT ?x WHERE { ex:dan ex:name ?n BIND(?n + 1 AS ?x) }"
        )
        .await,
        vec![vec![None]]
    );
    assert_eq!(
        rows(
            &mut engine,
            "SELECT ?x WHERE { VALUES (?x ?y) { (UNDEF 1) } }"
        )
        .await,
        vec![vec![None]]
    );
}

#[tokio::test]
async fn modifiers_union_subqueries_and_ask() {
    let mut engine = engine();
    assert_eq!(
        rows(&mut engine, "SELECT DISTINCT ?n WHERE { { ?s ex:name ?n } UNION { ?s ex:nick ?n } } ORDER BY ?n LIMIT 2 OFFSET 1").await,
        vec![vec![string("Anon")], vec![string("Bob")]]
    );
    // UNION branches bind different variables; the other side is unbound.
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?a ?h WHERE { { ex:bob ex:age ?a } UNION { ex:bob ex:height ?h } }"
        )
        .await,
        vec![vec![None, typed("1.80", "decimal")], vec![int("25"), None]]
    );
    assert_eq!(
        rows(&mut engine, "SELECT ?n ?max WHERE { ?s ex:name ?n { SELECT (MAX(?a) AS ?max) WHERE { ?x ex:age ?a } } ?s ex:age ?max }").await,
        vec![vec![string("Cara"), int("41")]]
    );
    assert_eq!(
        rows(
            &mut engine,
            "SELECT REDUCED ?s WHERE { ?s ex:age ?a } ORDER BY ?a"
        )
        .await,
        vec![vec![iri("bob")], vec![iri("alice")], vec![iri("cara")]]
    );
    let ask = format!("{PREFIX}ASK {{ ex:alice ex:knows ?x . ?x ex:age ?a FILTER(?a < 30) }}");
    assert_eq!(
        engine.query(&ask).await.unwrap(),
        SparqlResults::Boolean(true)
    );
    let ask = format!("{PREFIX}ASK {{ ex:cara ex:knows ?x }}");
    assert_eq!(
        engine.query(&ask).await.unwrap(),
        SparqlResults::Boolean(false)
    );
}

#[tokio::test]
async fn construct_builds_a_set_of_typed_triples() {
    let mut engine = engine();
    let query = format!(
        "{PREFIX}CONSTRUCT {{ ?s ex:years ?a . ?s ex:missing ?nope }} WHERE {{ ?s ex:age ?a FILTER(?a > 26) }}"
    );
    let SparqlResults::Graph(mut triples) = engine.query(&query).await.unwrap() else {
        panic!("graph expected");
    };
    triples.sort();
    assert_eq!(
        triples,
        vec![
            [
                iri("alice").unwrap(),
                iri("years").unwrap(),
                int("30").unwrap()
            ],
            [
                iri("cara").unwrap(),
                iri("years").unwrap(),
                int("41").unwrap()
            ],
        ]
    );
}

fn boolean(value: bool) -> Option<RdfTermValue> {
    typed(if value { "true" } else { "false" }, "boolean")
}

#[tokio::test]
async fn exists_inside_expressions_is_a_per_solution_mark() {
    let mut engine = engine();
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?n ?knows WHERE { ?s ex:name ?n BIND(EXISTS { ?s ex:knows ?x } AS ?knows) }"
        )
        .await,
        vec![
            vec![string("Alice"), boolean(true)],
            vec![string("Anon"), boolean(false)],
            vec![string("Bob"), boolean(true)],
            vec![string("Cara"), boolean(false)],
            vec![string("Dan"), boolean(false)],
        ]
    );
    assert_eq!(
        sorted(
            &mut engine,
            "SELECT ?n WHERE { ?s ex:name ?n FILTER(!EXISTS { ?s ex:age ?a } || ?n = \"Alice\") }"
        )
        .await,
        vec![
            vec![string("Alice")],
            vec![string("Anon")],
            vec![string("Dan")]
        ]
    );
    // The inner filter reads ?n, which only the outer solution binds.
    assert_eq!(
        sorted(&mut engine, "SELECT ?n ?old WHERE { ?s ex:age ?a0 ; ex:name ?n BIND(EXISTS { ?s ex:age ?a FILTER(?a > 26 && ?n != \"Cara\") } AS ?old) }").await,
        vec![
            vec![string("Alice"), boolean(true)],
            vec![string("Bob"), boolean(false)],
            vec![string("Cara"), boolean(false)],
        ]
    );
}

#[tokio::test]
async fn string_and_conditional_functions_preserve_term_types() {
    let mut engine = engine();
    assert_eq!(
        rows(
            &mut engine,
            "SELECT ?c ?u ?b ?a ?sub ?len ?dt ?lang WHERE { ex:alice ex:label ?l ; ex:name ?n BIND(CONCAT(?l, \"!\"@en) AS ?c) BIND(UCASE(?l) AS ?u) BIND(STRBEFORE(?n, \"ic\") AS ?b) BIND(STRAFTER(?n, \"zz\") AS ?a) BIND(SUBSTR(?n, 2, 3) AS ?sub) BIND(STRLEN(?n) AS ?len) BIND(DATATYPE(?n) AS ?dt) BIND(LANG(?l) AS ?lang) }",
        )
        .await,
        vec![vec![
            Some(RdfTermValue::lang("Alice!", "en")),
            Some(RdfTermValue::lang("ALICE", "en")),
            string("Al"),
            string(""),
            string("lic"),
            int("5"),
            Some(RdfTermValue::iri(format!("{XSD}string"))),
            string("en"),
        ]]
    );
    assert_eq!(
        sorted(&mut engine, "SELECT ?s ?v WHERE { ?s ex:name ?n OPTIONAL { ?s ex:age ?age } BIND(COALESCE(?age, \"none\") AS ?v) FILTER(?s IN (ex:alice, ex:dan)) }").await,
        vec![vec![iri("alice"), int("30")], vec![iri("dan"), string("none")]]
    );
    assert_eq!(
        rows(&mut engine, "SELECT ?x ?y ?z WHERE { BIND(IF(1 < 2, \"yes\", 1/0) AS ?x) BIND(xsd:integer(\" 12 \") AS ?y) BIND(STRDT(\"7\", xsd:byte) AS ?z) }").await,
        vec![vec![string("yes"), int("12"), typed("7", "byte")]]
    );
    assert_eq!(
        rows(&mut engine, "SELECT ?s WHERE { ?s ex:age ?a FILTER(sameTerm(?a, 30) && isNumeric(?a) && isLiteral(?a) && isIRI(?s)) }").await,
        vec![vec![iri("alice")]]
    );
}

#[tokio::test]
async fn order_by_ranks_term_kinds_and_numbers() {
    let mut engine = engine();
    let ordered = rows(
        &mut engine,
        "SELECT ?o WHERE { ex:alice ?p ?o } ORDER BY ?o",
    )
    .await;
    // IRIs sort before literals; numerics sort by value.
    assert_eq!(ordered[0], vec![iri("bob")]);
    assert_eq!(ordered[1], vec![int("30")]);
    let limited = rows(&mut engine, "SELECT ?n WHERE { { SELECT ?n WHERE { ?s ex:name ?n } ORDER BY DESC(?n) LIMIT 2 } } ORDER BY ?n").await;
    assert_eq!(limited, vec![vec![string("Cara")], vec![string("Dan")]]);
}

#[test]
fn typed_algebra_is_standard_ir_without_extensions() {
    use orchiddb::ir::df::{from_logical_plan, to_logical_plan};
    use orchiddb::language::sparql::SparqlPlanner;
    for body in [
        "SELECT * WHERE { GRAPH ?g {} }",
        "SELECT ?s (COUNT(?o) AS ?c) (GROUP_CONCAT(?o) AS ?g) (SAMPLE(?o) AS ?x) WHERE { ?s ?p ?o } GROUP BY ?s HAVING (COUNT(?o) > 1)",
        "SELECT REDUCED ?n WHERE { VALUES (?s ?n) { (ex:a \"x\"@en) (UNDEF 3) } OPTIONAL { ?s ex:age ?a FILTER(?a > ?n) } }",
        "SELECT ?n WHERE { ?s ex:name ?n FILTER(EXISTS { ?s ex:age ?a } || NOT EXISTS { ?s ex:knows ?k }) MINUS { ?s ex:nick ?n } }",
    ] {
        let plan = SparqlPlanner::new("people")
            .plan_str(&format!("{PREFIX}{body}"))
            .unwrap();
        let text = orchiddb::ir::explain(&plan);
        assert!(!text.contains("GraphExtension"), "{text}");
        let logical = to_logical_plan(&plan).unwrap();
        assert_eq!(from_logical_plan(&logical).unwrap(), plan, "{body}");
    }
}

#[tokio::test]
async fn correlated_negation_and_nested_optionals() {
    let mut engine = engine();
    // Substitution semantics: ?n comes from the outer solution only.
    assert_eq!(
        sorted(&mut engine, "SELECT ?n WHERE { ?s ex:name ?n FILTER NOT EXISTS { ?s ?p ?v FILTER(?v = ?n && ?p != ex:name) } }").await,
        vec![vec![string("Anon")], vec![string("Bob")], vec![string("Cara")], vec![string("Dan")]]
    );
    assert_eq!(
        sorted(&mut engine, "SELECT ?n ?f ?fa WHERE { ?s ex:name ?n OPTIONAL { ?s ex:knows ?f OPTIONAL { ?f ex:age ?fa FILTER(?fa > 30) } } FILTER(?n IN (\"Alice\", \"Bob\", \"Dan\")) }").await,
        vec![
            vec![string("Alice"), iri("bob"), None],
            vec![string("Bob"), iri("cara"), int("41")],
            vec![string("Dan"), None, None],
        ]
    );
    // An OPTIONAL filter on a variable the left side may leave unbound.
    assert_eq!(
        sorted(&mut engine, "SELECT ?n ?a ?h WHERE { ?s ex:name ?n OPTIONAL { ?s ex:age ?a } OPTIONAL { ?s ex:height ?h FILTER(!bound(?a) || ?a < 30) } }").await,
        vec![
            vec![string("Alice"), int("30"), None],
            vec![string("Anon"), None, None],
            vec![string("Bob"), int("25"), typed("1.80", "decimal")],
            vec![string("Cara"), int("41"), None],
            vec![string("Dan"), None, None],
        ]
    );
}

#[tokio::test]
async fn variable_free_patterns_and_asks() {
    let mut engine = engine();
    for (query, expected) in [
        ("ASK { FILTER NOT EXISTS { ?s ex:missing ?o } }", true),
        ("ASK { FILTER EXISTS { ?s ex:missing ?o } }", false),
        ("ASK { ex:alice ex:knows ex:bob }", true),
        ("ASK { BIND(1 AS ?x) FILTER(?x = 1.0) }", true),
        (
            "ASK { SELECT DISTINCT * WHERE { ex:bob ex:knows ex:alice } }",
            false,
        ),
    ] {
        let query = format!("{PREFIX}{query}");
        assert_eq!(
            engine.query(&query).await.unwrap(),
            SparqlResults::Boolean(expected),
            "{query}"
        );
    }
    assert_eq!(
        rows(&mut engine, "SELECT (COUNT(*) AS ?c) WHERE { { ex:alice ex:knows ex:bob } UNION { ex:bob ex:knows ex:cara } }").await,
        vec![vec![int("2")]]
    );
}
