//! Live integration tests. Start the pinned services with
//! `docker compose -f tests/remote-engines.compose.yml up -d` and set both URLs.
//! These tests intentionally fail when their requested service is unavailable.
#![cfg(all(
    feature = "duckdb",
    any(feature = "quickwit", feature = "elasticsearch")
))]

use arrow::{array::RecordBatch, util::display::array_value_to_string};
use orchiddb::{
    compiler,
    federation::{self, Session},
    remote::transport::HttpOptions,
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    time::{SystemTime, UNIX_EPOCH},
};

struct Local(duckdb::Connection);
#[async_trait::async_trait(?Send)]
impl Session for Local {
    fn dialect(&self) -> &str {
        "duckdb"
    }
    async fn query(&mut self, sql: &str) -> Result<Vec<RecordBatch>, String> {
        let mut statement = self.0.prepare(sql).map_err(|e| e.to_string())?;
        Ok(statement
            .query_arrow([])
            .map_err(|e| e.to_string())?
            .collect())
    }
}

struct Fixture {
    kind: &'static str,
    endpoint: String,
    index: String,
    client: reqwest::Client,
}
impl Fixture {
    async fn new(kind: &'static str, variable: &str) -> Self {
        let endpoint = std::env::var(variable)
            .unwrap_or_else(|_| panic!("set {variable} to run the live {kind} test"));
        let index = format!(
            "orchiddb_test_{}_{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let fixture = Self {
            kind,
            endpoint: endpoint.trim_end_matches('/').into(),
            index,
            client: reqwest::Client::new(),
        };
        let properties = json!({"id":{"type":"long"},"body":{"type":"text"},"tenant":{"type":"keyword"},"title":{"type":"keyword"},"year":{"type":"long"},"payload":{"properties":{"tag":{"type":"keyword"},"items":{"type":"long"}}}});
        if kind == "quickwit" {
            fixture.json(reqwest::Method::POST,"/api/v1/indexes",json!({"version":"0.8","index_id":fixture.index,
                "doc_mapping":{"mode":"strict","field_mappings":[
                    {"name":"id","type":"i64","fast":true},
                    {"name":"body","type":"text","tokenizer":"default","record":"position","fieldnorms":true},
                    {"name":"tenant","type":"text","tokenizer":"raw","fast":true},
                    {"name":"title","type":"text","tokenizer":"raw"},
                    {"name":"year","type":"i64","fast":true},
                    {"name":"payload","type":"json"}
                ]}})).await;
        } else {
            fixture.json(reqwest::Method::PUT,&format!("/{}",fixture.index),json!({"settings":{"number_of_shards":1,"number_of_replicas":0},"mappings":{"properties":properties}})).await;
        }
        let documents = [
            json!({"id":1,"body":"graph storage database","tenant":"a","title":"One","year":2024,"payload":{"tag":"alpha","items":[1,null]}}),
            json!({"id":2,"body":"graph graph graph","tenant":"a","title":"Two","year":2025}),
            json!({"id":3,"body":"graph graph graph graph","tenant":"b","title":"Other tenant","year":2025}),
            json!({"id":4,"body":"database storage","tenant":"a","title":"Four","year":2023}),
            json!({"id":5,"body":"graph","title":"No tenant","year":2010}),
        ];
        let mut data = String::new();
        for document in documents {
            if kind == "elasticsearch" {
                data.push_str(&format!(
                    "{}\n",
                    json!({"index":{"_id":document["id"].to_string()}})
                ));
            }
            data.push_str(&format!("{document}\n"));
        }
        let path = if kind == "quickwit" {
            format!("/api/v1/{}/ingest?commit=force", fixture.index)
        } else {
            format!("/{}/_bulk?refresh=true", fixture.index)
        };
        let response = fixture
            .client
            .post(format!("{}{path}", fixture.endpoint))
            .header("content-type", "application/x-ndjson")
            .body(data)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let body = response.text().await.unwrap();
        assert!(status.is_success(), "{status}: {body}");
        let body: Value = serde_json::from_str(&body).unwrap();
        assert_ne!(body.get("errors"), Some(&Value::Bool(true)), "{body}");
        fixture
    }
    async fn json(&self, method: reqwest::Method, path: &str, body: Value) -> Value {
        let response = self
            .client
            .request(method, format!("{}{path}", self.endpoint))
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        assert!(status.is_success(), "{status}: {text}");
        serde_json::from_str(&text).unwrap()
    }
    async fn delete(&self) {
        let path = if self.kind == "quickwit" {
            format!("/api/v1/indexes/{}", self.index)
        } else {
            format!("/{}", self.index)
        };
        let response = self
            .client
            .delete(format!("{}{path}", self.endpoint))
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    }
    fn request(&self, query: &str) -> Value {
        json!({"version":1,"dialect":"duckdb","language":"cypher","query":query,
            "engines":{"local":{"dialect":"duckdb"},"text":{"dialect":self.kind}},"execution_engine":"local",
            "tables":[
                {"name":"queries","engine":"local","columns":[{"name":"id","data_type":"int64"},{"name":"body","data_type":"string"},{"name":"tenant","data_type":"string"}]},
                {"name":self.index,"engine":"text","columns":[{"name":"id","data_type":"int64"},{"name":"body","data_type":"string"},{"name":"tenant","data_type":"string"},{"name":"title","data_type":"string"},{"name":"year","data_type":"int64"},{"name":"payload","data_type":"json"}]},
                {"name":"authors","engine":"local","columns":[{"name":"id","data_type":"int64"},{"name":"name","data_type":"string"}]},
                {"name":"written_by","engine":"local","columns":[{"name":"id","data_type":"int64"},{"name":"document_id","data_type":"int64"},{"name":"author_id","data_type":"int64"}]}
            ],
            "nodes":[
                {"label":"Question","table":"queries","id":"id","properties":{"id":"id","body":"body","tenant":"tenant"}},
                {"label":"Document","table":self.index,"id":"id","properties":{"id":"id","body":"body","tenant":"tenant","title":"title","year":"year","payload":"payload"}},
                {"label":"Person","table":"authors","id":"id","properties":{"id":"id","name":"name"}}
            ],
            "edges":[{"label":"WRITTEN_BY","table":"written_by","id":"id","source":"document_id","target":"author_id","source_label":"Document","target_label":"Person"}],
            "source_metadata":[{"table":self.index,"format":self.kind,"options":{"index":self.index},"indexes":[{"column":"body","metric":"bm25"}]}],
            "computed_relationships":[{"name":"SIMILAR_TO","source":"Question","target":"Document","predicate":"source.tenant = target.tenant","properties":{"score":"text.bm25(source.body,target.body)"},"order_by":[{"expression":"score","direction":"desc"}],"limit_per_source":2}]
        })
    }
    fn sessions(&self) -> BTreeMap<String, Box<dyn Session>> {
        let connection = duckdb::Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE queries(id BIGINT, body VARCHAR, tenant VARCHAR); INSERT INTO queries VALUES (10,'graph','a'),(20,'database','a'),(30,NULL,'a'),(40,'graph',NULL); CREATE TABLE authors(id BIGINT,name VARCHAR); INSERT INTO authors VALUES (100,'Ada'),(200,'Grace'); CREATE TABLE written_by(id BIGINT,document_id BIGINT,author_id BIGINT); INSERT INTO written_by VALUES (1,1,100),(2,2,200),(4,4,100);").unwrap();
        connection.execute_batch(&format!("CREATE TABLE \"{}\"(id BIGINT,body VARCHAR,tenant VARCHAR,title VARCHAR,year BIGINT); INSERT INTO \"{}\" VALUES (1,'graph storage database','a','Authoritative one',2024),(2,'graph graph graph','a','Authoritative two',2025),(3,'graph graph graph graph','b','Authoritative other',2025),(4,'database storage','a','Authoritative four',2023),(5,'graph',NULL,'Authoritative null',2010);",self.index,self.index)).unwrap();
        connection
            .execute_batch(&format!(
                "ALTER TABLE \"{}\" ADD COLUMN payload JSON;",
                self.index
            ))
            .unwrap();
        let mut options = HttpOptions::new(&self.endpoint);
        options.page_size = 2;
        let remote: Box<dyn Session> = match self.kind {
            #[cfg(feature = "quickwit")]
            "quickwit" => Box::new(
                orchiddb::remote::transport::QuickwitSession::with_options(options).unwrap(),
            ),
            #[cfg(feature = "elasticsearch")]
            "elasticsearch" => Box::new(
                orchiddb::remote::transport::ElasticsearchSession::with_options(options).unwrap(),
            ),
            _ => unreachable!(),
        };
        BTreeMap::from([
            (
                "local".into(),
                Box::new(Local(connection)) as Box<dyn Session>,
            ),
            ("text".into(), remote),
        ])
    }
    async fn run(&self, request: Value) -> (compiler::CompiledSql, Vec<Vec<String>>) {
        let plan = compiler::compile(serde_json::from_value(request).unwrap())
            .await
            .unwrap();
        let batches = federation::execute(&plan, &mut self.sessions())
            .await
            .unwrap();
        let rows = batches
            .iter()
            .flat_map(|batch| {
                (0..batch.num_rows()).map(|row| {
                    (0..batch.num_columns())
                        .map(|col| array_value_to_string(batch.column(col).as_ref(), row).unwrap())
                        .collect()
                })
            })
            .collect();
        (plan, rows)
    }
}

async fn exercise(kind: &'static str, variable: &str) {
    let fixture = Fixture::new(kind, variable).await;
    let request=fixture.request("MATCH (q:Question)-[r:SIMILAR_TO]->(d:Document) MATCH (d)-[:WRITTEN_BY]->(a:Person) RETURN q.id AS query_id,d.id AS document_id,a.name AS author,r.score AS score ORDER BY query_id,score DESC");
    let (plan, rows) = fixture.run(request).await;
    assert_eq!(rows.len(), 4, "{rows:?}");
    assert_eq!(&rows[0][..3], &["10", "2", "Grace"]);
    assert_eq!(&rows[1][..3], &["10", "1", "Ada"]);
    assert_eq!(&rows[2][..3], &["20", "4", "Ada"]);
    assert_eq!(&rows[3][..3], &["20", "1", "Ada"]);
    for row in &rows {
        assert!(row[3].parse::<f64>().unwrap() > 0.0);
    }
    let path = if kind == "quickwit" {
        format!("/api/v1/_elastic/{}/_search", fixture.index)
    } else {
        format!("/{}/_search", fixture.index)
    };
    let direct=fixture.json(reqwest::Method::POST,&path,json!({"query":{"bool":{"must":[{"match":{"body":{"query":"graph","operator":if kind=="quickwit"{"OR"}else{"or"}}}}],"filter":[{"term":{"tenant":"a"}}]}},"sort":[{"_score":"desc"}],"size":2})).await;
    for (actual, expected) in rows
        .iter()
        .take(2)
        .zip(direct["hits"]["hits"].as_array().unwrap())
    {
        assert_eq!(actual[1], expected["_source"]["id"].to_string());
        assert!(
            (actual[3].parse::<f64>().unwrap()
                - (if kind == "quickwit" {
                    &expected["sort"][0]
                } else {
                    &expected["_score"]
                })
                .as_f64()
                .unwrap())
            .abs()
                < 1e-6
        );
    }
    assert!(
        plan.transfers
            .iter()
            .any(|transfer| transfer.request.is_some())
    );
    assert!(!plan.sql.contains("bm25"));

    // Stored remote nodes participate in ordinary graph queries, including
    // missing-field SQL nulls. A scan must not silently stop at one HTTP page.
    let (_, rows) = fixture
        .run(fixture.request("MATCH (d:Document) WHERE d.tenant IS NULL RETURN d.id"))
        .await;
    assert_eq!(rows, vec![vec!["5"]]);
    let (_, rows) = fixture
        .run(fixture.request(
            "MATCH (d:Document {id:1}) RETURN json.value(d.payload,'$.tag','string') AS tag",
        ))
        .await;
    assert_eq!(rows, vec![vec!["alpha"]]);
    let (_, rows) = fixture
        .run(fixture.request("MATCH (d:Document) RETURN count(d) AS n"))
        .await;
    assert_eq!(rows, vec![vec!["5"]]);
    let (_, rows) = fixture
        .run(fixture.request("MATCH (d:Document) WHERE NOT (d.tenant = 'a') RETURN d.id"))
        .await;
    assert_eq!(rows, vec![vec!["3"]]);
    let mut request = fixture.request(
        "MATCH (q:Question)-[r:SIMILAR_TO]->(d:Document) RETURN q.id,d.id ORDER BY q.id,d.id",
    );
    request["computed_relationships"][0]["predicate"] =
        json!("source.id = 10 AND source.tenant = target.tenant");
    let (_, rows) = fixture.run(request).await;
    assert_eq!(rows, vec![vec!["10", "1"], vec!["10", "2"]]);

    // A post-ranking predicate does not refill the relationship's top one.
    let mut request = fixture.request(
        "MATCH (q:Question {id:10})-[r:SIMILAR_TO]->(d:Document) WHERE d.id=1 RETURN d.id",
    );
    request["computed_relationships"][0]["limit_per_source"] = json!(1);
    let (_, rows) = fixture.run(request).await;
    assert!(
        rows.is_empty(),
        "post-ranking filter incorrectly refilled top-k: {rows:?}"
    );

    // An external index supplies only keys and scores; authoritative SQL
    // properties must not be substituted by stale indexed copies.
    let mut request=fixture.request("MATCH (q:Question {id:10})-[r:SIMILAR_TO]->(d:Document) RETURN d.id,d.title,r.score ORDER BY r.score DESC");
    request["tables"][1]["engine"] = json!("local");
    request["source_metadata"][0]["options"]["engine"] = json!("text");
    request["source_metadata"][0]["options"]["key_field"] = json!("id");
    let (plan, rows) = fixture.run(request).await;
    assert_eq!(&rows[0][..2], &["2", "Authoritative two"]);
    assert_eq!(&rows[1][..2], &["1", "Authoritative one"]);
    assert!(
        plan.transfers
            .iter()
            .any(|transfer| transfer.request.is_some())
    );
    // Native query responses remain JSON values, so aggregation buckets and
    // snippets can compose with the same JSON functions as document columns.
    {
        use arrow::datatypes::{Field, Schema};
        use datafusion::{
            common::{DFSchema, ScalarValue},
            logical_expr::lit,
        };
        use orchiddb::ir::{
            functions::domain,
            rel::dependent::{ArgumentBinding, TableFunction},
        };
        use std::sync::Arc;
        let native = kind == "quickwit";
        let body = if native {
            json!({"query":"body:graph","max_hits":2,"snippet_fields":"body","aggs":{"tenants":{"terms":{"field":"tenant"}}}})
        } else {
            json!({"query":{"match":{"body":"graph"}},"size":2,"highlight":{"fields":{"body":{}}},"aggs":{"tenants":{"terms":{"field":"tenant"}}}})
        };
        let function = TableFunction::new(
            vec![
                kind.into(),
                if native { "native_query" } else { "query" }.into(),
            ],
            vec![
                lit("text"),
                lit(fixture.index.clone()),
                lit(body.to_string()),
            ],
            None,
            ArgumentBinding::PrepareTime,
            Arc::new(
                DFSchema::try_from(Schema::new(vec![Field::new(
                    "response",
                    domain::json_type(),
                    false,
                )]))
                .unwrap(),
            ),
        )
        .unwrap()
        .into_plan();
        let adapter = orchiddb::operations::resolve(kind).unwrap().unwrap();
        let prepared = adapter.lower(&function).unwrap().unwrap();
        let request = prepared.template.bind(&[]).unwrap();
        let mut sessions = fixture.sessions();
        let batches = sessions
            .get_mut("text")
            .unwrap()
            .execute_request(
                &request,
                &[federation::TransferColumn {
                    name: "response".into(),
                    data_type: "json".into(),
                    nullable: false,
                }],
            )
            .await
            .unwrap();
        assert_eq!(batches.iter().map(RecordBatch::num_rows).sum::<usize>(), 1);
        let value = ScalarValue::try_from_array(batches[0].column(0), 0).unwrap();
        let response: Value =
            serde_json::from_str(&domain::json_text(&value).unwrap().unwrap()).unwrap();
        assert_eq!(
            response["aggregations"]["tenants"]["buckets"][0]["key"],
            json!("a"),
            "{response}"
        );
        assert_eq!(
            response["aggregations"]["tenants"]["buckets"][0]["doc_count"],
            json!(2),
            "{response}"
        );
    }
    fixture.delete().await;
}

#[cfg(feature = "quickwit")]
#[tokio::test]
#[ignore = "requires a running Quickwit service and QUICKWIT_URL"]
async fn live_quickwit_search_and_sql_graph_join() {
    exercise("quickwit", "QUICKWIT_URL").await;
}

#[cfg(feature = "elasticsearch")]
#[tokio::test]
#[ignore = "requires a running Elasticsearch service and ELASTICSEARCH_URL"]
async fn live_elasticsearch_search_and_sql_graph_join() {
    exercise("elasticsearch", "ELASTICSEARCH_URL").await;
}
