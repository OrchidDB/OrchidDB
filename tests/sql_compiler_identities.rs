//! Mapped identities keep their source types. Execute compiler-only SQL against
//! DuckDB tables keyed by text, integers and decimals, including queries whose
//! scans union labels with different identity types.
#![cfg(feature = "duckdb")]
use orchiddb::compiler::compile_json;
use serde_json::{Value, json};

const SETUP: &str = "
CREATE TABLE people(pid VARCHAR, name VARCHAR);
INSERT INTO people VALUES ('p-ada','Ada'),('p-bob','Bob'),('p-cy','Cy'),('1','One');
CREATE TABLE companies(cid INTEGER, name VARCHAR);
INSERT INTO companies VALUES (1,'Acme'),(2,'Globex');
CREATE TABLE sites(sid DECIMAL(12,2), name VARCHAR);
INSERT INTO sites VALUES (1.50,'North');
CREATE TABLE knows(kid VARCHAR, src VARCHAR, dst VARCHAR);
INSERT INTO knows VALUES ('k1','p-ada','p-bob'),('k2','p-bob','p-cy'),('k3','p-cy','p-ada');
CREATE TABLE works(wid BIGINT, person VARCHAR, company INTEGER);
INSERT INTO works VALUES (10,'p-ada',1),(11,'p-bob',1),(12,'1',2);
CREATE TABLE located(lid BIGINT, company INTEGER, site DECIMAL(12,2));
INSERT INTO located VALUES (20,1,1.50);
";

fn request(language: &str, query: &str) -> Value {
    json!({"version":1,"dialect":"duckdb","language":language,"query":query,
    "tables":[
        {"name":"people","columns":[{"name":"pid","data_type":"string"},{"name":"name","data_type":"string"}]},
        {"name":"companies","columns":[{"name":"cid","data_type":"int32"},{"name":"name","data_type":"string"}]},
        {"name":"sites","columns":[{"name":"sid","data_type":"decimal:12:2"},{"name":"name","data_type":"string"}]},
        {"name":"knows","columns":[{"name":"kid","data_type":"string"},{"name":"src","data_type":"string"},{"name":"dst","data_type":"string"}]},
        {"name":"works","columns":[{"name":"wid","data_type":"int64"},{"name":"person","data_type":"string"},{"name":"company","data_type":"int32"}]},
        {"name":"located","columns":[{"name":"lid","data_type":"int64"},{"name":"company","data_type":"int32"},{"name":"site","data_type":"decimal:12:2"}]}
    ],
    "nodes":[
        {"label":"Person","table":"people","id":"pid","properties":{"name":"name"}},
        {"label":"Company","table":"companies","id":"cid","properties":{"name":"name"}},
        {"label":"Site","table":"sites","id":"sid","properties":{"name":"name"}}
    ],
    "edges":[
        {"label":"KNOWS","table":"knows","id":"kid","source":"src","target":"dst","source_label":"Person","target_label":"Person"},
        {"label":"WORKS_AT","table":"works","id":"wid","source":"person","target":"company","source_label":"Person","target_label":"Company"},
        {"label":"LOCATED_IN","table":"located","id":"lid","source":"company","target":"site","source_label":"Company","target_label":"Site"}
    ],
    "ontology":{
        "classes":[{"iri":"http://ex.org/Person","label":"Person"}],
        "properties":[{"iri":"http://ex.org/name","label":"Person","property":"name"}],
        "relationships":[{"iri":"http://ex.org/knows","label":"KNOWS","source_label":"Person","target_label":"Person"}]
    }})
}

async fn compile(request: Value) -> Result<String, String> {
    let compiled: Value = serde_json::from_str(&compile_json(&request.to_string()).await?).unwrap();
    Ok(compiled["sql"].as_str().unwrap().to_owned())
}

/// Every output column rendered as text, rows sorted.
async fn run(language: &str, query: &str) -> Vec<Vec<String>> {
    let sql = compile(request(language, query))
        .await
        .unwrap_or_else(|e| panic!("{query}\n{e}"));
    let con = duckdb::Connection::open_in_memory().unwrap();
    con.execute_batch(SETUP).unwrap();
    let wrapped = format!("SELECT COLUMNS(*)::VARCHAR FROM ({sql}) AS q");
    let mut stmt = con
        .prepare(&wrapped)
        .unwrap_or_else(|e| panic!("{query}\n{e}\n{sql}"));
    let mut rows = stmt.query([]).unwrap_or_else(|e| panic!("{query}\n{e}\n{sql}"));
    let width = rows.as_ref().unwrap().column_count();
    let mut out = vec![];
    while let Some(row) = rows.next().unwrap() {
        out.push(
            (0..width)
                .map(|i| row.get::<_, Option<String>>(i).unwrap().unwrap_or_else(|| "null".into()))
                .collect(),
        );
    }
    out.sort();
    out
}

fn rows(expected: &[&[&str]]) -> Vec<Vec<String>> {
    let mut rows: Vec<Vec<String>> = expected
        .iter()
        .map(|row| row.iter().map(|value| value.to_string()).collect())
        .collect();
    rows.sort();
    rows
}

#[tokio::test]
async fn text_identities_join_and_expand() {
    assert_eq!(
        run("cypher", "MATCH (a:Person)-[:KNOWS]->(b:Person) RETURN a.name, b.name").await,
        rows(&[&["Ada", "Bob"], &["Bob", "Cy"], &["Cy", "Ada"]])
    );
    assert_eq!(
        run("cypher", "MATCH (a:Person {name:'Ada'})-[:KNOWS*1..3]->(b:Person) RETURN b.name").await,
        rows(&[&["Ada"], &["Bob"], &["Cy"]])
    );
    assert_eq!(
        run("cypher", "MATCH (p:Person) OPTIONAL MATCH (p)-[:WORKS_AT]->(c:Company) RETURN p.name, c.name").await,
        rows(&[&["Ada", "Acme"], &["Bob", "Acme"], &["Cy", "null"], &["One", "Globex"]])
    );
}

#[tokio::test]
async fn edges_join_endpoints_of_different_identity_types() {
    assert_eq!(
        run(
            "cypher",
            "MATCH (p:Person)-[:WORKS_AT]->(c:Company)-[:LOCATED_IN]->(s:Site) RETURN p.name, c.name, s.name"
        )
        .await,
        rows(&[&["Ada", "Acme", "North"], &["Bob", "Acme", "North"]])
    );
}

#[tokio::test]
async fn unlabeled_scans_union_mixed_identity_types_without_merging_labels() {
    // Person '1' and Company 1 share a textual id but remain distinct.
    assert_eq!(run("cypher", "MATCH (n) RETURN count(*)").await, rows(&[&["7"]]));
    assert_eq!(
        run("cypher", "MATCH (n)-[]->(m) RETURN n.name, m.name").await,
        rows(&[
            &["Acme", "North"],
            &["Ada", "Acme"],
            &["Ada", "Bob"],
            &["Bob", "Acme"],
            &["Bob", "Cy"],
            &["Cy", "Ada"],
            &["One", "Globex"],
        ])
    );
    assert_eq!(
        run("cypher", "MATCH (a:Person {name:'One'})-[*1..2]->(b) RETURN b.name").await,
        rows(&[&["Globex"]])
    );
}

#[tokio::test]
async fn gremlin_and_sparql_use_text_identities() {
    assert_eq!(
        run("gremlin", "g.V('p-ada').out('KNOWS').values('name')").await,
        rows(&[&["Bob"]])
    );
    assert_eq!(
        run("gremlin", "g.V().hasLabel('Person').out('WORKS_AT').values('name')").await,
        rows(&[&["Acme"], &["Acme"], &["Globex"]])
    );
    assert_eq!(
        run(
            "sparql",
            "PREFIX ex: <http://ex.org/> SELECT ?an ?bn WHERE { ?a a ex:Person; ex:knows ?b; ex:name ?an . ?b a ex:Person; ex:name ?bn }"
        )
        .await,
        rows(&[&["Ada", "Bob"], &["Bob", "Cy"], &["Cy", "Ada"]])
    );
}

#[tokio::test]
async fn floating_point_identities_are_accepted() {
    let mut r = request("cypher", "RETURN 1");
    r["tables"][1]["columns"][0]["data_type"] = json!("float64");
    compile(r).await.unwrap();
}

#[tokio::test]
async fn postgres_renders_text_identities() {
    let mut r = request("cypher", "MATCH (p:Person)-[:WORKS_AT]->(c:Company) RETURN p.name, c.name");
    r["dialect"] = json!("postgres");
    let sql = compile(r).await.unwrap();
    datafusion::sql::sqlparser::parser::Parser::parse_sql(
        &datafusion::sql::sqlparser::dialect::PostgreSqlDialect {},
        &sql,
    )
    .unwrap();
}

#[tokio::test]
async fn traversals_between_labels_with_different_identity_types() {
    // Undirected and variable-length traversals whose endpoints differ in type.
    assert_eq!(
        run("cypher", "MATCH (c:Company)-[:WORKS_AT]-(p) RETURN c.name, p.name").await,
        rows(&[&["Acme", "Ada"], &["Acme", "Bob"], &["Globex", "One"]])
    );
    assert_eq!(
        run("cypher", "MATCH (p:Person {name:'Ada'})-[:WORKS_AT|LOCATED_IN*1..2]->(x) RETURN x.name").await,
        rows(&[&["Acme"], &["North"]])
    );
    assert_eq!(
        run("cypher", "MATCH (p:Person {name:'Ada'})-[:WORKS_AT*1..2]->(x) RETURN x.name").await,
        rows(&[&["Acme"]])
    );
    assert_eq!(
        run("cypher", "MATCH (s:Site)<-[*1..3]-(x) RETURN x.name").await,
        // Ada is reached directly and through Bob: one row per path.
        rows(&[&["Acme"], &["Ada"], &["Ada"], &["Bob"], &["Cy"]])
    );
    assert_eq!(
        run("gremlin", "g.V('p-ada').out('WORKS_AT').out('LOCATED_IN').values('name')").await,
        rows(&[&["North"]])
    );
    assert_eq!(
        run("gremlin", "g.V('p-ada').repeat(out('KNOWS')).times(2).values('name')").await,
        rows(&[&["Cy"]])
    );
    assert_eq!(
        run("gremlin", "g.V('p-cy').repeat(out('KNOWS', 'WORKS_AT', 'LOCATED_IN')).times(3).values('name')").await,
        // Cy -> Ada -> {Bob, Acme} -> {Cy, Acme, North}
        rows(&[&["Acme"], &["Cy"], &["North"]])
    );
}

#[tokio::test]
async fn scalar_keys_execute_in_duckdb() {
    for (schema_type, sql_type, a, b) in [
        ("boolean", "BOOLEAN", "false", "true"),
        ("float32", "FLOAT", "-0.0", "1.25"),
        ("float64", "DOUBLE", "-0.0", "1.25"),
        ("float64", "DOUBLE", "'NaN'", "'Infinity'"),
        ("uint64", "UBIGINT", "0", "18446744073709551615"),
        ("binary", "BLOB", "'\\xFF'::BLOB", "'\\x00'::BLOB"),
        ("date", "DATE", "'2026-01-01'", "'2026-01-02'"),
        ("time", "TIME", "'01:02:03'", "'04:05:06'"),
        ("timestamp", "TIMESTAMP", "'2026-01-01 01:02:03'", "'2026-01-02 04:05:06'"),
        ("interval", "INTERVAL", "'1 month'", "'2 months'"),
        ("decimal:12:2", "DECIMAL(12,2)", "1.25", "2.50"),
    ] {
        // DuckDB cannot index INTERVAL, but it can use unique interval values
        // as mapped graph identities. Other types exercise real PRIMARY KEYs.
        let constraint = if schema_type == "interval" { "" } else { "PRIMARY KEY" };
        let con = duckdb::Connection::open_in_memory().unwrap();
        con.execute_batch(&format!(
            "CREATE TABLE n(k {sql_type} {constraint}, name VARCHAR);
             INSERT INTO n VALUES ({a}, 'a'), ({b}, 'b');
             CREATE TABLE e(k {sql_type} {constraint}, src {sql_type}, dst {sql_type});
             INSERT INTO e VALUES ({a}, {a}, {b});"
        )).unwrap();
        for query in [
            "MATCH (a:N)-[e:E]->(b:N) RETURN a.name, b.name",
            "MATCH (a:N)-[:E*1..2]->(b:N) RETURN a.name, b.name",
        ] {
            let r = json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
                "tables":[
                    {"name":"n","columns":[{"name":"k","data_type":schema_type},{"name":"name","data_type":"string"}]},
                    {"name":"e","columns":[{"name":"k","data_type":schema_type},{"name":"src","data_type":schema_type},{"name":"dst","data_type":schema_type}]}
                ],
                "nodes":[{"label":"N","table":"n","id":"k","properties":{"name":"name"}}],
                "edges":[{"label":"E","table":"e","id":"k","source":"src","target":"dst","source_label":"N","target_label":"N"}]
            });
            let sql = compile(r).await.unwrap_or_else(|e| panic!("{schema_type}: {e}"));
            let mut statement = con.prepare(&sql).unwrap_or_else(|e| panic!("{schema_type}: {e}\n{sql}"));
            let result = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .unwrap().collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(result, vec![("a".into(), "b".into())], "{schema_type}: {query}");
        }
    }
}

#[tokio::test]
async fn mixed_unsigned_primary_keys_keep_their_full_range() {
    let mut r = request("cypher", "MATCH (n) RETURN count(*)");
    r["tables"] = json!([
        {"name":"unsigned_nodes","columns":[{"name":"id","data_type":"uint64"}]},
        {"name":"signed_nodes","columns":[{"name":"id","data_type":"int64"}]},
        {"name":"links","columns":[{"name":"id","data_type":"uint64"},{"name":"src","data_type":"uint64"},{"name":"dst","data_type":"int64"}]}
    ]);
    r["nodes"] = json!([
        {"label":"U","table":"unsigned_nodes","id":"id"},
        {"label":"S","table":"signed_nodes","id":"id"}
    ]);
    r["edges"] = json!([{"label":"E","table":"links","id":"id","source":"src","target":"dst","source_label":"U","target_label":"S"}]);
    r.as_object_mut().unwrap().remove("ontology");
    let con = duckdb::Connection::open_in_memory().unwrap();
    con.execute_batch("CREATE TABLE unsigned_nodes(id UBIGINT PRIMARY KEY);
        INSERT INTO unsigned_nodes VALUES (18446744073709551615);
        CREATE TABLE signed_nodes(id BIGINT PRIMARY KEY);
        INSERT INTO signed_nodes VALUES (-1);
        CREATE TABLE links(id UBIGINT, src UBIGINT, dst BIGINT);
        INSERT INTO links VALUES (18446744073709551615,18446744073709551615,-1);").unwrap();
    for (query, expected) in [
        ("MATCH (n) RETURN count(*)", 2_i64),
        ("MATCH (n)-[]->(m) RETURN count(*)", 1),
        ("MATCH (n)-[:E*1..2]->(m) RETURN count(*)", 1),
    ] {
        r["query"] = json!(query);
        let sql = compile(r.clone()).await.unwrap();
        let count: i64 = con.query_row(&sql, [], |row| row.get(0)).unwrap_or_else(|e| panic!("{e}\n{sql}"));
        assert_eq!(count, expected, "{query}");
    }
}

#[tokio::test]
async fn relationship_history_distinguishes_label_and_key_boundaries() {
    let mut input = request("cypher", "MATCH (a:Person)-[:`A:B`]->()-[:A]->(b:Person) WHERE a.name='Ada' RETURN b.name");
    let mut second_table = input["tables"][3].clone();
    second_table["name"] = json!("knows2");
    input["tables"].as_array_mut().unwrap().push(second_table);
    let mut first = input["edges"][0].clone();
    first["label"] = json!("A:B");
    let mut second = first.clone();
    second["label"] = json!("A");
    second["table"] = json!("knows2");
    input["edges"] = json!([first, second]);
    let sql = compile(input).await.unwrap();
    let con = duckdb::Connection::open_in_memory().unwrap();
    con.execute_batch(SETUP).unwrap();
    con.execute_batch("DELETE FROM knows; INSERT INTO knows VALUES ('c','p-ada','p-bob'); CREATE TABLE knows2 AS SELECT 'B:c' AS kid,'p-bob' AS src,'p-cy' AS dst").unwrap();
    let name: String = con.query_row(&sql, [], |row|row.get(0)).unwrap();
    assert_eq!(name, "Cy");
}

#[tokio::test]
async fn mixed_identity_filters_preserve_scalar_types_and_negation() {
    assert_eq!(
        run("gremlin", "g.V(1).values('name')").await,
        rows(&[&["Acme"]])
    );
    assert_eq!(
        run("gremlin", "g.V('p-ada', 'p-bob').values('name')").await,
        rows(&[&["Ada"], &["Bob"]])
    );
    assert_eq!(
        run("gremlin", "g.V().hasId(neq('p-ada')).values('name')").await,
        rows(&[&["Acme"], &["Bob"], &["Cy"], &["Globex"], &["North"], &["One"]])
    );
}
