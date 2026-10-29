//! Compile with the actual adapter and execute against Trino when explicitly enabled.
use orchiddb::{
    compiler::compile_json,
    ir::rel::sql::{DialectAdapter, SqlDialect, trino::TRINO},
};
use serde_json::{Value, json};

async fn compile(query: &str) -> Value {
    let request = json!({"version":1,"dialect":"trino","language":"cypher","query":query,
        "tables":[{"name":"people","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"},{"name":"age","data_type":"int64"}]}],
        "nodes":[{"label":"Person","table":"people","id":"id","properties":{"id":"id","name":"name","age":"age"}}]});
    serde_json::from_str(
        &compile_json(&request.to_string())
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}")),
    )
    .unwrap()
}
fn execute(sql: &str) -> Vec<Value> {
    let result = std::process::Command::new("python3")
        .args(["tools/trino/query.py", sql])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{sql}\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .filter(|s| !s.is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
#[tokio::test]
async fn scalar_and_graph_queries() {
    for query in [
        "RETURN 1 + 2 AS n",
        "RETURN toUpper('orchid') AS name",
        "RETURN null IS NULL AS yes",
        "RETURN [1,2,3] AS items",
        "UNWIND [1,2,3] AS n RETURN sum(n) AS total",
        "MATCH (p:Person) WHERE p.age >= 18 RETURN p.name AS name ORDER BY name LIMIT 5",
        "MATCH (p:Person) RETURN count(*) AS n",
        "MATCH (p:Person) RETURN p.age AS age, count(*) AS n ORDER BY age",
        "MATCH (p:Person) WHERE p.name STARTS WITH 'A' RETURN p.name AS name",
    ] {
        let result = compile(query).await;
        let sql = result["sql"].as_str().unwrap();
        eprintln!("{query}\n{sql}");
        if std::env::var_os("ORCHIDDB_TEST_TRINO").is_some() {
            let sql = fixture(sql);
            let rows = execute(&sql);
            assert!(!rows.is_empty(), "{query}");
            if query == "RETURN 1 + 2 AS n" {
                assert_eq!(rows[0]["n"], json!(3));
            }
            if query.contains("sum(n)") {
                assert_eq!(rows[0]["total"], json!(6));
            }
        }
    }
}
#[test]
fn types_and_literal_codecs() {
    use arrow::datatypes::{DataType, Field, TimeUnit};
    use datafusion::common::ScalarValue;
    let dialect = SqlDialect::resolve("trino").unwrap();
    assert_eq!(
        dialect
            .sql_type(&DataType::Timestamp(TimeUnit::Nanosecond, None))
            .unwrap(),
        "TIMESTAMP(9)"
    );
    assert_eq!(
        dialect
            .sql_type(&DataType::List(std::sync::Arc::new(Field::new(
                "item",
                DataType::Float64,
                true
            ))))
            .unwrap(),
        "ARRAY(DOUBLE)"
    );
    assert_eq!(
        dialect
            .exchange_literal(
                ScalarValue::Utf8(Some("O'Reilly\\x".into())),
                DataType::Utf8
            )
            .unwrap(),
        "CAST('O''Reilly\\x' AS VARCHAR)"
    );
    assert!(SqlDialect::register(&TRINO).is_err());
}
#[test]
fn vector_execution() {
    if std::env::var_os("ORCHIDDB_TEST_TRINO").is_none() {
        return;
    }
    for (name, expected) in [
        ("vector.dot", 0.8),
        ("vector.cosine_similarity", 0.9701425001453318),
        ("vector.l2_distance", 0.282842712474619),
    ] {
        let mapping = TRINO.function_mapping(name).unwrap();
        let sql = format!(
            "SELECT {} AS score",
            mapping
                .value
                .replace("__arg0", "ARRAY[1.0,0.0]")
                .replace("__arg1", "ARRAY[0.8,0.2]")
        );
        let rows = execute(&sql);
        assert!((rows[0]["score"].as_f64().unwrap() - expected).abs() < 1e-12);
    }
}

async fn connector_query(options: Value, columns: Value, query: &str) -> Value {
    let properties = columns
        .as_array()
        .unwrap()
        .iter()
        .map(|c| (c["name"].as_str().unwrap().to_owned(), c["name"].clone()))
        .collect::<serde_json::Map<_, _>>();
    let request = json!({"version":1,"dialect":"trino","language":"cypher","query":query,
        "tables":[{"name":"source","columns":columns}],
        "nodes":[{"label":"Item","table":"source","id":if properties.contains_key("id") { "id" } else {properties.keys().next().unwrap().as_str()},"properties":properties}],
        "source_metadata":[{"table":"source","format":"trino","options":options}]});
    serde_json::from_str(
        &compile_json(&request.to_string())
            .await
            .unwrap_or_else(|e| panic!("{request}: {e}")),
    )
    .unwrap()
}
#[tokio::test]
async fn connector_lowering() {
    let cases = [
        (
            json!({"connector":"iceberg","catalog":"iceberg","schema":"orchid","table":"documents","snapshot":123}),
            "FOR VERSION AS OF 123",
        ),
        (
            json!({"connector":"iceberg","catalog":"iceberg","schema":"orchid","table":"documents","metadata":"snapshots"}),
            "documents$snapshots",
        ),
        (
            json!({"connector":"iceberg_changes","catalog":"iceberg","schema":"orchid","table":"documents","start_snapshot":1,"end_snapshot":2}),
            "start_snapshot_id => 1",
        ),
        (
            json!({"connector":"delta_changes","catalog":"delta","schema":"orchid","table":"documents","since_version":1}),
            "since_version => 1",
        ),
        (
            json!({"connector":"elasticsearch","catalog":"elastic","schema":"default","index":"documents","query":{"query":{"match":{"body":"O'Reilly"}}}}),
            "raw_query",
        ),
        (
            json!({"connector":"open_search","catalog":"search","schema":"default","index":"documents","query":{"query":{"match_all":{}}}}),
            "raw_query",
        ),
        (
            json!({"connector":"mongo_db","catalog":"mongo","database":"orchid","collection":"documents","filter":{"tenant":"acme"}}),
            "filter =>",
        ),
        (
            json!({"connector":"loki","catalog":"loki","query":"{job=\"orchid\"} |= \"ready\"","start":"2026-10-24T00:00:00Z","end":"2026-10-24T00:01:00Z"}),
            "query_range",
        ),
        (
            json!({"connector":"prometheus","catalog":"prometheus","metric":"up","start":"2026-10-24T00:00:00Z","end":"2026-10-24T00:01:00Z"}),
            "\"prometheus\".\"default\".\"up\"",
        ),
    ];
    for (options, expected) in cases {
        let result = connector_query(
            options,
            json!([{"name":"id","data_type":"string"}]),
            "MATCH (n:Item) RETURN n.id AS id",
        )
        .await;
        let sql = result["sql"].as_str().unwrap();
        assert!(sql.contains(expected), "{sql}");
    }
}
#[tokio::test]
async fn iceberg_vector_and_observability_execution() {
    if std::env::var_os("ORCHIDDB_TEST_TRINO_CONNECTORS").is_none() {
        return;
    }
    let result=connector_query(json!({"connector":"iceberg","catalog":"iceberg","schema":"orchid","table":"documents"}),json!([
        {"name":"id","data_type":"int64"},{"name":"title","data_type":"string"},{"name":"embedding","data_type":"list:float64"}
    ]),"MATCH (n:Item) WITH n, vector.cosine_similarity([1.0,0.0], n.embedding) AS score RETURN n.title AS title,score ORDER BY score DESC").await;
    let rows = execute(result["sql"].as_str().unwrap());
    assert_eq!(rows[0]["title"], json!("Orchid"));
    assert_eq!(rows[0]["score"], json!(1.0));
    let now = chrono::Utc::now();
    for kind in ["loki", "prometheus"] {
        let mut options = json!({"connector":kind,"catalog":kind,"start":(now-chrono::Duration::minutes(15)).to_rfc3339(),"end":(now+chrono::Duration::seconds(1)).to_rfc3339()});
        let (columns, cypher) = if kind == "loki" {
            options["query"] = json!("{job=\"orchid\"} |= \"ready\"");
            (
                json!([{"name":"line","data_type":"string"},{"name":"job","data_type":"string"}]),
                "MATCH (n:Item) WHERE n.job = 'orchid' RETURN n.line AS line",
            )
        } else {
            options["metric"] = json!("up");
            (
                json!([{"name":"value","data_type":"float64"},{"name":"job","data_type":"string"}]),
                "MATCH (n:Item) WHERE n.job = 'orchid' RETURN n.value AS value",
            )
        };
        let result = connector_query(options, columns, cypher).await;
        let rows = execute(result["sql"].as_str().unwrap());
        assert!(!rows.is_empty(), "{kind}: {result}");
    }
}

#[tokio::test]
async fn runtime_conformance() {
    let cases = [
        ("UNWIND range(1, 100001) AS n RETURN count(*) AS result", json!([100001])),
        ("UNWIND range(3, 1, -1) AS n RETURN n AS result ORDER BY result", json!([1, 2, 3])),
        ("UNWIND range(3, 1, 1) AS n RETURN count(*) AS result", json!([0])),
        ("MATCH (p:Person) RETURN sum(p.age) AS result", json!([47])),
        (
            "MATCH (p:Person) RETURN avg(p.age) AS result",
            json!([23.5]),
        ),
        ("MATCH (p:Person) RETURN min(p.age) AS result", json!([17])),
        ("MATCH (p:Person) RETURN max(p.age) AS result", json!([30])),
        (
            "MATCH (p:Person) RETURN count(DISTINCT p.age) AS result",
            json!([2]),
        ),
        (
            "MATCH (p:Person) RETURN toUpper(p.name) AS result ORDER BY result",
            json!(["ALICE", "BOB"]),
        ),
        (
            "MATCH (p:Person) RETURN toLower(p.name) AS result ORDER BY result",
            json!(["alice", "bob"]),
        ),
        (
            "MATCH (p:Person) RETURN size(p.name) AS result ORDER BY result",
            json!([3, 5]),
        ),
        (
            "MATCH (p:Person) RETURN substring(p.name,1,2) AS result ORDER BY result",
            json!(["li", "ob"]),
        ),
        (
            "MATCH (p:Person) RETURN replace(p.name,'o','x') AS result ORDER BY result",
            json!(["Alice", "Bxb"]),
        ),
        (
            "MATCH (p:Person) RETURN p.age / 2 AS result ORDER BY result",
            json!([8, 15]),
        ),
        (
            "MATCH (p:Person) RETURN p.age % 7 AS result ORDER BY result",
            json!([2, 3]),
        ),
        (
            "MATCH (p:Person) RETURN CASE WHEN p.age < 18 THEN 'child' ELSE 'adult' END AS result ORDER BY result",
            json!(["adult", "child"]),
        ),
        (
            "MATCH (p:Person) WHERE p.name CONTAINS 'li' RETURN p.name AS result",
            json!(["Alice"]),
        ),
        (
            "MATCH (p:Person) WHERE p.name ENDS WITH 'ob' RETURN p.name AS result",
            json!(["Bob"]),
        ),
        (
            "MATCH (p:Person) WHERE p.age IN [17,20] RETURN p.name AS result",
            json!(["Bob"]),
        ),
        (
            "MATCH (p:Person) RETURN abs(-p.age) AS result ORDER BY result",
            json!([17, 30]),
        ),
        (
            "MATCH (p:Person) RETURN toString(p.age) AS result ORDER BY result",
            json!(["17", "30"]),
        ),
        (
            "MATCH (p:Person) RETURN p.age IS NULL AS result",
            json!([false, false]),
        ),
        (
            "MATCH (p:Person) WITH p ORDER BY p.age SKIP 1 LIMIT 1 RETURN p.name AS result",
            json!(["Alice"]),
        ),
        (
            "MATCH (p:Person) WHERE p.age > 100 RETURN count(*) AS result",
            json!([0]),
        ),
        (
            "MATCH (p:Person) RETURN coalesce(p.name,'unknown') AS result ORDER BY result",
            json!(["Alice", "Bob"]),
        ),
        (
            "MATCH (p:Person) WITH p.age AS age,count(*) AS n WHERE n > 0 RETURN age AS result ORDER BY result",
            json!([17, 30]),
        ),
    ];
    for (query, expected) in cases {
        let result = compile(query).await;
        if std::env::var_os("ORCHIDDB_TEST_TRINO").is_some() {
            let sql = fixture(result["sql"].as_str().unwrap());
            let values = execute(&sql)
                .iter()
                .map(|r| r["result"].clone())
                .collect::<Vec<_>>();
            assert_eq!(json!(values), expected, "{query}\n{sql}");
        }
    }
}
#[tokio::test]
async fn custom_function_signature() {
    let request = json!({"version":1,"dialect":"trino","language":"cypher","query":"RETURN custom.length('orchid') AS n",
        "functions":[{"name":"custom.length","target":"length","parameters":["string"],"returns":"int64"}]});
    let result: Value =
        serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap();
    if std::env::var_os("ORCHIDDB_TEST_TRINO").is_some() {
        assert_eq!(execute(result["sql"].as_str().unwrap())[0]["n"], json!(6));
    }
}

fn fixture(sql: &str) -> String {
    let prefix = "WITH people(id,name,age) AS (VALUES (BIGINT '1','Alice',BIGINT '30'),(BIGINT '2','Bob',BIGINT '17'))";
    if let Some(body) = sql.strip_prefix("WITH ") {
        format!("{prefix}, {body}")
    } else {
        format!("{prefix} {sql}")
    }
}

#[test]
fn vector_edge_cases() {
    if std::env::var_os("ORCHIDDB_TEST_TRINO").is_none() {
        return;
    }
    let score = |name: &str, a: &str, b: &str| {
        format!(
            "SELECT {} AS score",
            TRINO
                .function_mapping(name)
                .unwrap()
                .value
                .replace("__arg0", a)
                .replace("__arg1", b)
        )
    };
    assert_eq!(
        execute(&score(
            "vector.cosine_similarity",
            "ARRAY[0.0,0.0]",
            "ARRAY[1.0,0.0]"
        ))[0]["score"],
        Value::Null
    );
    assert_eq!(
        execute(&score(
            "vector.dot",
            "CAST(NULL AS ARRAY(DOUBLE))",
            "ARRAY[1.0,0.0]"
        ))[0]["score"],
        Value::Null
    );
    assert_eq!(
        execute(&score(
            "vector.maxsim",
            "ARRAY[ARRAY[1.0,0.0],ARRAY[0.0,1.0]]",
            "ARRAY[ARRAY[1.0,0.0],ARRAY[0.0,1.0]]"
        ))[0]["score"],
        json!(2.0)
    );
    for (name, a, b) in [
        ("vector.dot", "ARRAY[1.0]", "ARRAY[1.0,2.0]"),
        (
            "vector.dot",
            "CAST(ARRAY[] AS ARRAY(DOUBLE))",
            "CAST(ARRAY[] AS ARRAY(DOUBLE))",
        ),
        ("vector.dot", "ARRAY[CAST(NULL AS DOUBLE)]", "ARRAY[1.0]"),
        ("vector.dot", "ARRAY[infinity()]", "ARRAY[1.0]"),
        (
            "vector.dot",
            "ARRAY[DOUBLE '1e308']",
            "ARRAY[DOUBLE '1e308']",
        ),
        (
            "vector.maxsim",
            "CAST(ARRAY[] AS ARRAY(ARRAY(DOUBLE)))",
            "ARRAY[ARRAY[1.0]]",
        ),
        (
            "vector.maxsim",
            "ARRAY[ARRAY[1.0]]",
            "ARRAY[CAST(NULL AS ARRAY(DOUBLE))]",
        ),
    ] {
        let sql = score(name, a, b);
        let output = std::process::Command::new("python3")
            .args(["tools/trino/query.py", &sql])
            .output()
            .unwrap();
        assert!(!output.status.success(), "{sql}");
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("invalid") || error.contains("overflow"),
            "{error}"
        );
    }
}

#[tokio::test]
async fn iceberg_history_execution() {
    if std::env::var_os("ORCHIDDB_TEST_TRINO_CONNECTORS").is_none() {
        return;
    }
    let snapshots = execute(
        "SELECT snapshot_id FROM iceberg.orchid.\"documents$snapshots\" WHERE CAST(element_at(summary, 'added-records') AS BIGINT) > 0 ORDER BY committed_at",
    );
    assert!(snapshots.len() >= 2);
    let columns = json!([{"name":"id","data_type":"int64"}]);
    let snapshot=connector_query(json!({"connector":"iceberg","catalog":"iceberg","schema":"orchid","table":"documents","snapshot":snapshots[0]["snapshot_id"]}),columns.clone(),"MATCH (n:Item) RETURN n.id AS id").await;
    assert_eq!(
        execute(snapshot["sql"].as_str().unwrap()),
        vec![json!({"id":1})]
    );
    let changes=connector_query(json!({"connector":"iceberg_changes","catalog":"iceberg","schema":"orchid","table":"documents","start_snapshot":snapshots[0]["snapshot_id"],"end_snapshot":snapshots[1]["snapshot_id"]}),columns,"MATCH (n:Item) RETURN n.id AS id").await;
    assert_eq!(
        execute(changes["sql"].as_str().unwrap()),
        vec![json!({"id":2})]
    );
}

#[test]
fn connector_literals_and_validation() {
    use orchiddb::ir::rel::sql::trino::connectors::ConnectorSource;
    let source:ConnectorSource=serde_json::from_value(json!({"connector":"loki","catalog":"logs\"archive","query":"{job=\"O'Reilly\"} |= \" FOR VERSION AS OF \"","start":"2026-10-24T02:00:00+02:00","end":"2026-10-24T00:01:00Z"})).unwrap();
    let sql = source.relation_sql().unwrap();
    assert!(sql.contains("\"logs\"\"archive\""));
    assert!(sql.contains("O''Reilly"));
    assert!(sql.contains("00:00:00"));
    assert!(sql.contains(" FOR VERSION AS OF "));
    for value in [
        json!({"connector":"loki","catalog":"loki","query":"{}","start":"invalid","end":"2026-10-24T00:01:00Z"}),
        json!({"connector":"prometheus","catalog":"prom","metric":"up","start":"2026-10-24T00:02:00Z","end":"2026-10-24T00:01:00Z"}),
        json!({"connector":"iceberg","catalog":"lake","schema":"x","table":"x","metadata":"unexpected"}),
        json!({"connector":"iceberg","catalog":"lake","schema":"x","table":"x","metadata":"files","snapshot":1}),
    ] {
        let source: ConnectorSource = serde_json::from_value(value).unwrap();
        assert!(source.relation_sql().is_err());
    }
}

#[tokio::test]
async fn computed_retrieval_edge_execution() {
    let request = json!({"version":1,"dialect":"trino","language":"cypher",
        "query":"MATCH (s:Document)-[r:SIMILAR_TO]->(d:Document) WHERE s.id=1 RETURN d.title AS title,r.score AS score",
        "tables":[{"name":"documents","columns":[{"name":"id","data_type":"int64"},{"name":"title","data_type":"string"},{"name":"embedding","data_type":"list:float64"}]}],
        "nodes":[{"label":"Document","table":"documents","id":"id","properties":{"id":"id","title":"title","embedding":"embedding"}}],
        "computed_relationships":[{"name":"SIMILAR_TO","source":"Document","target":"Document","predicate":"source.id <> target.id","properties":{"score":"vector.cosine_similarity(source.embedding,target.embedding)"},"order_by":[{"expression":"score","direction":"desc"}],"limit_per_source":1}],
        "source_metadata":[{"table":"documents","format":"trino","options":{"connector":"iceberg","catalog":"iceberg","schema":"orchid","table":"documents"}}]});
    let result: Value =
        serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap();
    if std::env::var_os("ORCHIDDB_TEST_TRINO_CONNECTORS").is_some() {
        assert_eq!(
            execute(result["sql"].as_str().unwrap()),
            vec![json!({"title":"Gardening","score":0.0})]
        );
    }
}

#[tokio::test]
async fn recursive_paths_preserve_declared_base_types() {
    let request = json!({"version":1,"dialect":"trino","language":"cypher",
        "query":"MATCH (a:Person {name:'A'})-[:R*1..3]->(b) RETURN b.name AS name ORDER BY name",
        "tables":[
            {"name":"nodes","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]},
            {"name":"edges","columns":[{"name":"id","data_type":"int64"},{"name":"src","data_type":"int64"},{"name":"dst","data_type":"int64"}]}
        ],
        "nodes":[{"label":"Person","table":"nodes","id":"id","properties":{"name":"name"}}],
        "edges":[{"label":"R","table":"edges","id":"id","source":"src","target":"dst","source_label":"Person","target_label":"Person"}]
    });
    let result: Value = serde_json::from_str(&compile_json(&request.to_string()).await.unwrap()).unwrap();
    if std::env::var_os("ORCHIDDB_TEST_TRINO").is_some() {
        let sql = result["sql"].as_str().unwrap();
        let body = sql.strip_prefix("WITH RECURSIVE ").expect("recursive CTE");
        let sql = format!("WITH RECURSIVE nodes(id,name) AS (VALUES (BIGINT '1','A'),(BIGINT '2','B'),(BIGINT '3','C')), edges(id,src,dst) AS (VALUES (BIGINT '1',BIGINT '1',BIGINT '2'),(BIGINT '2',BIGINT '2',BIGINT '3')), {body}");
        assert_eq!(execute(&sql), vec![json!({"name":"B"}),json!({"name":"C"})]);
    }
}
