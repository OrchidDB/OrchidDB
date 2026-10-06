use arrow::{
    array::{ArrayRef, Float64Array, Int64Array, ListArray, RecordBatch, StringArray},
    datatypes::Float64Type,
};
use datafusion::datasource::MemTable;
use orchiddb::{
    ir::{
        catalog::PropertyGraph,
        rel::{
            LoweredPlan, RelBackend, RelBackendOptions, execute_lowered,
            mapping::GraphMapping,
            sql::{self, SqlDialect},
        },
    },
    language::cypher::{parse_query, planner::CypherPlanner},
};
use std::sync::Arc;

fn mapping(rule: &str) -> GraphMapping {
    let mut mapping = GraphMapping::from_toml(&format!(
        r#"
[node.Document]
table = "documents"
id = "id"
[node.Document.properties]
id = "id"
embedding = "embedding"
body = "body"
tokens = "tokens"
{rule}
"#
    ))
    .unwrap();
    let vectors = ListArray::from_iter_primitive::<Float64Type, _, _>([
        Some(vec![Some(1.), Some(0.)]),
        Some(vec![Some(0.8), Some(0.6)]),
        Some(vec![Some(0.), Some(1.)]),
        Some(vec![Some(0.8), Some(0.6)]),
    ]);
    let mut token_builder = arrow::array::ListBuilder::new(arrow::array::ListBuilder::new(
        arrow::array::Float64Builder::new(),
    ));
    for matrix in [
        vec![vec![1., 0.], vec![0., 1.]],
        vec![vec![0.8, 0.6], vec![0., 1.]],
        vec![vec![0., 1.]],
        vec![vec![0.8, 0.6], vec![0., 1.]],
    ] {
        for vector in matrix {
            for value in vector {
                token_builder.values().values().append_value(value);
            }
            token_builder.values().append(true);
        }
        token_builder.append(true);
    }
    let batch = RecordBatch::try_from_iter(vec![
        ("tokens", Arc::new(token_builder.finish()) as ArrayRef),
        (
            "id",
            Arc::new(Int64Array::from(vec![1, 2, 3, 4])) as ArrayRef,
        ),
        ("embedding", Arc::new(vectors) as ArrayRef),
        (
            "body",
            Arc::new(StringArray::from(vec!["cat cat dog", "cat", "dog", "cat"])) as ArrayRef,
        ),
    ])
    .unwrap();
    mapping.register_table(
        "documents",
        Arc::new(MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap()),
    );
    mapping
}
fn lower(mapping: GraphMapping, query: &str) -> LoweredPlan {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping)),
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new())
    .unwrap()
}
const COSINE: &str = r#"
[edge.SIMILAR_TO]
source = "Document"
target = "Document"
predicate = "source.id <> target.id"
order_by = [{ expression = "score", direction = "desc", nulls = "last" }]
limit_per_source = 1
[edge.SIMILAR_TO.properties]
score = "vector.cosine_similarity(source.embedding, target.embedding)"
"#;
fn rows(batch: &RecordBatch) -> Vec<String> {
    (0..batch.num_rows())
        .map(|r| {
            (0..batch.num_columns())
                .map(|c| {
                    arrow::util::display::array_value_to_string(batch.column(c).as_ref(), r)
                        .unwrap()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}
#[tokio::test]
async fn native_edges_rank_per_source_and_reverse_the_same_edges() {
    let query = "MATCH (s:Document)-[e:SIMILAR_TO]->(t:Document) RETURN s.id, t.id ORDER BY s.id";
    let result = execute_lowered(lower(mapping(COSINE), query))
        .await
        .unwrap();
    assert_eq!(rows(&result.batch), vec!["1|2", "2|4", "3|2", "4|2"]);
    let reverse =
        "MATCH (t:Document)<-[e:SIMILAR_TO]-(s:Document) WHERE t.id = 2 RETURN s.id ORDER BY s.id";
    let result = execute_lowered(lower(mapping(COSINE), reverse))
        .await
        .unwrap();
    assert_eq!(rows(&result.batch), vec!["1", "3", "4"]);
}
#[tokio::test]
async fn bm25_uses_target_corpus_before_pair_filtering() {
    let rule = r#"
[edge.MATCHES]
source = "Document"
target = "Document"
predicate = "source.id = 2 AND target.id <> source.id"
order_by = [{ expression = "score", direction = "desc" }]
limit_per_source = 2
[edge.MATCHES.properties]
score = "text.bm25(source.body, target.body)"
"#;
    let lowered = lower(
        mapping(rule),
        "MATCH (s:Document)-[e:MATCHES]->(t:Document) RETURN t.id, e.score ORDER BY e.score DESC",
    );
    let result = execute_lowered(lowered).await.unwrap();
    assert_eq!(result.batch.num_rows(), 2);
    let scores = result
        .batch
        .column(1)
        .as_any()
        .downcast_ref::<Float64Array>()
        .unwrap();
    let expected = orchiddb::ir::functions::search::bm25(
        "cat",
        "cat",
        &[
            Some("cat cat dog".into()),
            Some("cat".into()),
            Some("dog".into()),
            Some("cat".into()),
        ],
    );
    assert!((scores.value(0) - expected).abs() < 1e-12);
}
#[test]
fn relationship_config_roundtrips_and_rejects_statements() {
    let map = mapping(COSINE);
    let serialized = map.to_toml();
    assert_eq!(
        GraphMapping::from_toml(&serialized)
            .unwrap()
            .edge("SIMILAR_TO"),
        map.edge("SIMILAR_TO")
    );
    assert!(
        GraphMapping::from_toml(&serialized.replace(
            "source.id <> target.id",
            "source.id <> target.id; DROP TABLE documents"
        ))
        .is_err()
    );
}
#[tokio::test]
async fn sql_uses_backend_mappings() {
    for (dialect, expected) in [
        (SqlDialect::DuckDb, "list_cosine_similarity"),
        (SqlDialect::Postgres, "<=>"),
    ] {
        let lowered = lower(
            mapping(COSINE),
            "MATCH (s:Document)-[e:SIMILAR_TO]->(t:Document) RETURN s.id, t.id, e.score",
        );
        let sql = sql::unparse(&lowered, dialect).unwrap();
        assert!(sql.contains(expected), "{sql}");
        assert!(!sql.contains("__orchiddb_logical_"), "{sql}");
        assert!(sql.contains("PARTITION BY"), "{sql}");
    }
}

#[tokio::test]
async fn gremlin_uses_the_same_ranked_relationships() {
    use orchiddb::language::gremlin::{GremlinPlanner, parse_traversal};
    let plan = GremlinPlanner::new()
        .plan(
            &parse_traversal(
                "g.V().hasLabel('Document').has('id',1).out('SIMILAR_TO').values('id')",
            )
            .unwrap(),
        )
        .unwrap();
    let lowered = RelBackend::with_options(RelBackendOptions {
        mapping: Some(Arc::new(mapping(COSINE))),
        ..Default::default()
    })
    .lower(&plan, &PropertyGraph::new())
    .unwrap();
    assert_eq!(
        rows(&execute_lowered(lowered).await.unwrap().batch),
        vec!["2"]
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn duckdb_executes_vector_and_text_relationships() {
    for function in [
        "vector.cosine_similarity(source.embedding, target.embedding)",
        "vector.dot(source.embedding, target.embedding)",
        "vector.l2_distance(source.embedding, target.embedding)",
        "vector.maxsim(source.tokens, target.tokens)",
        "text.bm25(source.body, target.body)",
    ] {
        let rule = COSINE.replace(
            "vector.cosine_similarity(source.embedding, target.embedding)",
            function,
        );
        let query = "MATCH (s:Document)-[e:SIMILAR_TO]->(t:Document) RETURN s.id, t.id, e.score ORDER BY s.id";
        let lowered = lower(mapping(&rule), query);
        let native = execute_lowered(lowered.clone()).await.unwrap();
        let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap();
        let actual = sql::execute_prepared(
            &mut sql::DuckDbExecutor::from_connection(
                duckdb::Connection::open_in_memory().unwrap(),
            ),
            &prepared,
        )
        .unwrap_or_else(|e| panic!("{e}\n{}", prepared.query));
        assert_eq!(actual.batch.num_rows(), native.batch.num_rows());
        for i in 0..actual.batch.num_rows() {
            for col in 0..2 {
                assert_eq!(
                    actual
                        .batch
                        .column(col)
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .unwrap()
                        .value(i),
                    native
                        .batch
                        .column(col)
                        .as_any()
                        .downcast_ref::<Int64Array>()
                        .unwrap()
                        .value(i)
                );
            }
            assert!(
                (actual
                    .batch
                    .column(2)
                    .as_any()
                    .downcast_ref::<Float64Array>()
                    .unwrap()
                    .value(i)
                    - native
                        .batch
                        .column(2)
                        .as_any()
                        .downcast_ref::<Float64Array>()
                        .unwrap()
                        .value(i))
                .abs()
                    < 1e-6
            );
        }
    }
}

#[tokio::test]
async fn compiler_pushes_a_closed_relationship_to_its_owning_engine() {
    use serde_json::json;
    let request = json!({"version":1,"dialect":"duckdb","language":"cypher",
        "query":"MATCH (s:Document)-[e:SIMILAR_TO]->(t:Document) RETURN s.id, t.id, e.score",
        "engines":{"local":{"dialect":"duckdb"},"remote":{"dialect":"postgres"}},"execution_engine":"local",
        "tables":[{"name":"documents","engine":"remote","columns":[{"name":"id","data_type":"int64"},{"name":"embedding","data_type":"list:float64"}]}],
        "nodes":[{"label":"Document","table":"documents","id":"id","properties":{"id":"id","embedding":"embedding"}}],
        "computed_relationships":[{"name":"SIMILAR_TO","source":"Document","target":"Document","predicate":"source.id <> target.id","properties":{"score":"vector.cosine_similarity(source.embedding,target.embedding)"},"order_by":[{"expression":"score","direction":"desc"}],"limit_per_source":1}]
    });
    let request = serde_json::from_value(request).unwrap();
    let result = orchiddb::compiler::compile(request).await.unwrap();
    assert_eq!(result.transfers.len(), 1, "{result:?}");
    assert!(
        result.transfers[0].sql.contains("<=>"),
        "{:?}",
        result.transfers
    );
    assert!(result.transfers[0].sql.contains("PARTITION BY"));
    assert!(!result.sql.contains("documents"));
}

#[tokio::test]
async fn explicit_candidate_stage_reranks_only_its_candidates() {
    let rule = r#"
[edge.RERANKED]
source = "Document"
target = "Document"
order_by = [{expression="score",direction="desc"}]
limit_per_source = 1
[edge.RERANKED.candidates]
predicate = "source.id <> target.id"
order_by = [{expression="lexical",direction="desc"}]
limit_per_source = 2
[edge.RERANKED.candidates.properties]
lexical = "text.bm25(source.body,target.body)"
[edge.RERANKED.properties]
score = "vector.maxsim(source.tokens,target.tokens)"
"#;
    let result = execute_lowered(lower(
        mapping(rule),
        "MATCH (s:Document {id:1})-[e:RERANKED]->(t:Document) RETURN t.id, e.score",
    ))
    .await
    .unwrap();
    assert_eq!(rows(&result.batch), vec!["2|1.8"]);
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn mapped_engine_composes_stored_edges_and_native_only_functions() {
    use orchiddb::{
        engine::GraphEngine,
        ir::{functions::logical::LogicalFunction, rel::mapping::EdgeMapping},
    };
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE documents(id BIGINT, embedding DOUBLE[], body VARCHAR, tokens DOUBLE[][]); INSERT INTO documents VALUES (1,[1.0,0.0],'cat',[[1.0,0.0]]),(2,[0.8,0.6],'cat',[[0.8,0.6]]),(3,[0.0,1.0],'dog',[[0.0,1.0]]); CREATE TABLE links(id BIGINT,src BIGINT,dst BIGINT); INSERT INTO links VALUES(1,3,2);").unwrap();
    let mut map = mapping(&COSINE.replace(
        "vector.cosine_similarity(source.embedding, target.embedding)",
        "custom.absolute(source.id - target.id)",
    ));
    map.register_logical_function(LogicalFunction::new(
        "custom.absolute",
        datafusion::functions::math::abs(),
        Default::default(),
    ))
    .unwrap();
    map.map_edge(
        EdgeMapping::table("LINK", "links", "src", "dst", "Document", "Document").with_id("id"),
    );
    let mut engine = GraphEngine::mapped(db, Arc::new(map)).unwrap();
    let result=engine.cypher("MATCH (s:Document {id:1})-[e:SIMILAR_TO]->(t:Document)-[:LINK]->(u:Document) RETURN t.id,u.id,e.score").await.unwrap();
    assert_eq!(rows(&result.returned.batch), vec!["3|2|2"]);
    assert!(result.stats.datafusion_ops > 0);
    for (query, expected) in [
        (
            "g.V().has('Document','id',1).out('SIMILAR_TO').values('body')",
            "dog",
        ),
        (
            "g.V().has('Document','id',1).repeat(out('SIMILAR_TO')).times(1).values('body')",
            "dog",
        ),
    ] {
        let result = engine.gremlin(query).await.unwrap();
        assert_eq!(
            rows(&result.returned.batch),
            vec![expected],
            "{query} {:#?}",
            result.stats
        );
    }
    assert!(
        engine
            .cypher("MATCH ()-[e:SIMILAR_TO]->() SET e.score=1")
            .await
            .is_err()
    );
}

#[test]
fn maxsim_and_bm25_have_reference_semantics() {
    use orchiddb::ir::functions::search::{bm25, maxsim};
    assert_eq!(
        maxsim(
            &[vec![1., 0.], vec![0., 1.]],
            &[vec![0.8, 0.6], vec![0., 1.]]
        )
        .unwrap(),
        1.8
    );
    assert_eq!(maxsim(&[vec![1., 0.]], &[vec![-1., 0.]]).unwrap(), -1.0);
    assert!(maxsim(&[], &[vec![1.]]).is_err());
    assert!(maxsim(&[vec![1.]], &[vec![1., 2.]]).is_err());
    assert!(maxsim(&[vec![f64::NAN]], &[vec![1.]]).is_err());
    let corpus = [
        Some("cat cat dog".into()),
        Some("cat".into()),
        Some("dog".into()),
        None,
    ];
    let expected =
        (1.0_f64 + (4.0 - 2.0 + 0.5) / (2.0 + 0.5)).ln() * 2.2 / (1.0 + 1.2 * (0.25 + 0.75 / 1.25));
    assert!((bm25("CAT cat!", "cat", &corpus) - expected).abs() < 1e-12);
    assert_eq!(bm25("cat", "cat", &[]), 0.);
    assert_eq!(bm25("cat", "", &corpus), 0.);
}

#[tokio::test]
async fn top_k_is_not_recomputed_after_target_filter_and_zero_means_no_edges() {
    let result = execute_lowered(lower(
        mapping(COSINE),
        "MATCH (s:Document {id:1})-[:SIMILAR_TO]->(t:Document) WHERE t.id=4 RETURN t.id",
    ))
    .await
    .unwrap();
    assert_eq!(result.batch.num_rows(), 0);
    let result = execute_lowered(lower(
        mapping(&COSINE.replace("limit_per_source = 1", "limit_per_source = 0")),
        "MATCH ()-[e:SIMILAR_TO]->() RETURN count(e)",
    ))
    .await
    .unwrap();
    assert_eq!(rows(&result.batch), vec!["0"]);
}

#[tokio::test]
async fn scalar_calls_validate_vectors_and_propagate_nulls() {
    use arrow::array::Array;
    for query in [
        "RETURN vector.cosine_similarity([0.0,0.0],[1.0,0.0]) AS score",
        "RETURN vector.dot(null,[1.0,0.0]) AS score",
    ] {
        let result = execute_lowered(lower(mapping(COSINE), query))
            .await
            .unwrap();
        assert!(result.batch.column(0).is_null(0));
    }
    let mismatch = lower(
        mapping(COSINE),
        "RETURN vector.dot([1.0,2.0],[1.0]) AS score",
    );
    assert!(execute_lowered(mismatch).await.is_err());
}

#[tokio::test]
async fn composite_endpoint_keys_partition_and_join_without_collisions() {
    let mut map = GraphMapping::from_toml(
        r#"
[node.D]
table = "documents"
id = ["tenant", "id"]
[node.D.properties]
tenant = "tenant"
id = "id"
x = "x"
[edge.NEAR]
source = "D"
target = "D"
predicate = "source.tenant = target.tenant AND source.id <> target.id"
order_by = [{expression="score",direction="asc"}]
limit_per_source = 1
[edge.NEAR.properties]
score = "abs(source.x - target.x)"
"#,
    )
    .unwrap();
    let batch = RecordBatch::try_from_iter(vec![
        (
            "tenant",
            Arc::new(StringArray::from(vec!["a", "a", "b", "b"])) as ArrayRef,
        ),
        (
            "id",
            Arc::new(Int64Array::from(vec![1, 2, 1, 2])) as ArrayRef,
        ),
        (
            "x",
            Arc::new(Float64Array::from(vec![1., 2., 10., 20.])) as ArrayRef,
        ),
    ])
    .unwrap();
    map.register_table(
        "documents",
        Arc::new(MemTable::try_new(batch.schema(), vec![vec![batch]]).unwrap()),
    );
    let plan = lower(
        map,
        "MATCH (s:D)-[e:NEAR]->(t:D) RETURN s.tenant,s.id,t.tenant,t.id ORDER BY s.tenant,s.id",
    );
    let result = execute_lowered(plan.clone()).await.unwrap();
    assert_eq!(
        rows(&result.batch),
        vec!["a|1|a|2", "a|2|a|1", "b|1|b|2", "b|2|b|1"]
    );
    for dialect in [SqlDialect::Postgres, SqlDialect::DuckDb] {
        sql::unparse(&plan, dialect).unwrap();
    }
}

fn lance_request(uri: &str, text_search: bool) -> serde_json::Value {
    use serde_json::json;
    let (function, metric, column) = if text_search {
        ("text.bm25(source.body,target.body)", "bm25", "body")
    } else {
        (
            "vector.cosine_similarity(source.embedding,target.embedding)",
            "cosine",
            "embedding",
        )
    };
    json!({"version":1,"dialect":"duckdb","language":"cypher",
        "query":"MATCH (s:Document)-[e:SIMILAR_TO]->(t:Document) WHERE s.id=1 RETURN t.id,e.score ORDER BY t.id",
        "engines":{"local":{"dialect":"duckdb"}},"execution_engine":"local",
        "tables":[{"name":"documents","columns":[{"name":"id","data_type":"int64"},{"name":"embedding","data_type":"list:float32"},{"name":"body","data_type":"string"},{"name":"tokens","data_type":"list:list:float32"}]}],
        "nodes":[{"label":"Document","table":"documents","id":"id","properties":{"id":"id","embedding":"embedding","body":"body","tokens":"tokens"}}],
        "search_indexes":[{"table":"documents","column":column,"metric":metric,"backend":{"kind":"lance","uri":uri}}],
        "computed_relationships":[{"name":"SIMILAR_TO","source":"Document","target":"Document","predicate":"source.id <> target.id","properties":{"score":function},"order_by":[{"expression":"score","direction":"desc"}],"limit_per_source":3}]
    })
}
#[tokio::test]
async fn compiler_emits_bound_lance_search_and_no_corpus_scan() {
    for text_search in [false, true] {
        let plan = orchiddb::compiler::compile(
            serde_json::from_value(lance_request("/data/docs.lance", text_search)).unwrap(),
        )
        .await
        .unwrap();
        let search = plan.transfers.iter().find(|t| t.operation.is_some()).unwrap();
        let template = &search.operation.as_ref().unwrap().template;
        assert!(
            template.sql.contains(if text_search {
                "lance_fts"
            } else {
                "lance_vector_search"
            }),
            "{}",
            template.sql
        );
        assert!(
            template.sql.contains("prefilter = true"),
            "{}",
            template.sql
        );
        assert!(!search.sql.contains("array_agg"), "{}", search.sql);
        assert!(
            !template.sql.contains("__orchiddb_logical_"),
            "{}",
            template.sql
        );
        assert!(
            !template.sql.contains("__search_source"),
            "{}",
            template.sql
        );
    }
}

/// Exercise the published extension on its supported DuckDB version without
/// adding a Lance SDK dependency. Set both paths to run this integration test.
#[test]
fn lance_extension_executes_compiled_vector_and_bm25_search() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let (Ok(cli), Ok(extension)) = (
        std::env::var("ORCHIDDB_LANCE_DUCKDB_CLI"),
        std::env::var("ORCHIDDB_LANCE_EXTENSION"),
    ) else {
        return;
    };
    let temp = std::env::temp_dir().join(format!("orchiddb-lance-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp).unwrap();
    let uri = temp.join("docs.lance").to_string_lossy().into_owned();
    let quote = |s: &str| s.replace('\'', "''");
    let run = |sql: &str| {
        let mut child = Command::new(&cli)
            .args(["-json", "-bail"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(
                format!(
                    "LOAD '{}'; SET lambda_syntax='ENABLE_SINGLE_ARROW';\n{sql}\n",
                    quote(&extension)
                )
                .as_bytes(),
            )
            .unwrap();
        let result = child.wait_with_output().unwrap();
        assert!(
            result.status.success(),
            "{}\n{sql}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    };
    run(&format!(
        "COPY (SELECT i::BIGINT id, ('cat ' || i)::VARCHAR body, [i::FLOAT,1::FLOAT]::FLOAT[2] embedding, [[i::FLOAT,1::FLOAT]]::FLOAT[2][] tokens FROM range(1,33) x(i)) TO '{}' (FORMAT lance, mode 'overwrite'); CREATE INDEX vec_idx ON '{}' (embedding) USING IVF_FLAT WITH (num_partitions=1,metric_type='cosine'); CREATE INDEX text_idx ON '{}' (body) USING INVERTED;",
        quote(&uri),
        quote(&uri),
        quote(&uri)
    ));
    let rt = tokio::runtime::Runtime::new().unwrap();
    for (text_search, rerank) in [(false, false), (true, false), (false, true)] {
        let mut request = lance_request(&uri, text_search);
        if rerank {
            let rule = &mut request["computed_relationships"][0];
            rule["candidates"] = serde_json::json!({"predicate":"source.id <> target.id","properties":{"candidate":"vector.cosine_similarity(source.embedding,target.embedding)"},"order_by":[{"expression":"candidate","direction":"desc"}],"limit_per_source":3});
            rule["properties"] =
                serde_json::json!({"score":"vector.maxsim(source.tokens,target.tokens)"});
            rule["limit_per_source"] = 1.into();
        }
        let plan = rt
            .block_on(orchiddb::compiler::compile(
                serde_json::from_value(request).unwrap(),
            ))
            .unwrap();
        let mut plan = serde_json::to_value(plan).unwrap();
        while let Some(transfer) = plan["transfers"].as_array().unwrap().first().cloned() {
            let source_sql = format!(
                "CREATE VIEW documents AS SELECT * FROM '{}'; {};",
                quote(&uri),
                transfer["sql"].as_str().unwrap()
            );
            let input = run(&source_sql);
            let rows: Vec<serde_json::Value> = if input.trim().is_empty() {
                vec![]
            } else {
                serde_json::from_str(&input).unwrap()
            };
            if transfer["operation"].is_object() {
                assert_eq!(
                    rows.len(),
                    1,
                    "seed filter must constrain search inputs: {}",
                    transfer["sql"]
                );
            }
            let columns = if transfer["operation"].is_object() {
                &transfer["operation"]["input_columns"]
            } else {
                &transfer["columns"]
            };
            let values = rows
                .iter()
                .map(|row| {
                    columns
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| row[c["name"].as_str().unwrap()].clone())
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            let hits = if transfer["operation"].is_object() {
                let bound=orchiddb::federation::bind_search_command(serde_json::json!({"plan":plan,"relation":transfer["target_relation"],"rows":values})).unwrap();
                let mut hits = vec![];
                for statement in bound["sql"].as_array().unwrap() {
                    let query = statement.as_str().unwrap();
                    if !text_search && hits.is_empty() {
                        let explain = run(&format!(
                            "EXPLAIN {};",
                            query.replace(
                                "prefilter = true",
                                "prefilter = true, explain_verbose = true"
                            )
                        ));
                        assert!(explain.contains("ANNSubIndex"), "{explain}");
                    }
                    let output = run(query);
                    if !output.trim().is_empty() {
                        let found: Vec<serde_json::Value> = serde_json::from_str(&output)
                            .unwrap_or_else(|e| panic!("{e}: {output}"));
                        hits.extend(found.into_iter().map(|row| {
                            transfer["columns"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|c| row[c["name"].as_str().unwrap()].clone())
                                .collect::<Vec<_>>()
                        }));
                    }
                }
                hits
            } else {
                values
            };
            plan = orchiddb::federation::bind_command(
                serde_json::json!({"plan":plan,"relation":transfer["target_relation"],"rows":hits}),
            )
            .unwrap();
        }
        let output = run(&format!(
            "CREATE VIEW documents AS SELECT * FROM '{}'; {};",
            quote(&uri),
            plan["sql"].as_str().unwrap()
        ));
        let rows: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();
        assert_eq!(rows.len(), if rerank { 1 } else { 3 }, "{rows:?}");
        assert!(rows.iter().all(|h| h["t.id"] != 1));
        if !text_search {
            assert_eq!(
                rows.iter()
                    .map(|h| h["t.id"].as_i64().unwrap())
                    .collect::<Vec<_>>(),
                if rerank { vec![4] } else { vec![2, 3, 4] }
            );
        }
        if rerank {
            assert_eq!(rows[0]["e.score"].as_f64().unwrap(), 5.0);
        }
    }
    std::fs::remove_dir_all(temp).unwrap();
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn declared_lance_search_does_not_retry_as_a_scan() {
    use orchiddb::{
        engine::GraphEngine,
        ir::rel::search::{LegacySearchOptions, SearchIndex, SearchMetric},
    };
    let db = duckdb::Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE documents(id BIGINT, embedding DOUBLE[], body VARCHAR, tokens DOUBLE[][]); INSERT INTO documents VALUES (1,[1.0,0.0],'cat',[[1.0,0.0]]),(2,[0.8,0.6],'cat',[[0.8,0.6]]);").unwrap();
    let mut map = mapping(COSINE);
    map.register_search_index(SearchIndex {
        table: "documents".into(),
        column: "embedding".into(),
        metric: SearchMetric::Cosine,
        backend: LegacySearchOptions {
            kind: "lance".into(),
            options: [("uri".into(), serde_json::json!("/missing/docs.lance"))].into(),
        },
    })
    .unwrap();
    let mut engine = GraphEngine::mapped(db, Arc::new(map)).unwrap();
    let error = engine
        .cypher("MATCH (s:Document)-[e:SIMILAR_TO]->(t:Document) WHERE s.id=1 RETURN t.id,e.score")
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("lance_vector_search"), "{error}");
}

#[tokio::test]
async fn lance_rejects_unpushable_candidate_filters_and_unsupported_exact_metric() {
    for (change, expected) in [
        ("filter", "candidate predicates"),
        ("exact", "exact vector search"),
    ] {
        let mut request = lance_request("/data/docs.lance", false);
        if change == "filter" {
            request["computed_relationships"][0]["predicate"] = "target.id + source.id > 5".into();
        } else {
            request["computed_relationships"][0]["retrieval"] = "exact".into();
        }
        let error = orchiddb::compiler::compile(serde_json::from_value(request).unwrap())
            .await
            .unwrap_err();
        assert!(error.contains(expected), "{error}");
    }
}

#[tokio::test]
async fn ranking_predicates_can_reference_composed_edge_properties() {
    let rule = COSINE.replace(
        "predicate = \"source.id <> target.id\"",
        "predicate = \"source.id <> target.id AND doubled > 1.9\"",
    ) + "\ndoubled = \"score * 2.0\"\n";
    let lowered = lower(
        mapping(&rule),
        "MATCH (s:Document)-[e:SIMILAR_TO]->(t:Document) RETURN s.id,t.id ORDER BY s.id",
    );
    let native = execute_lowered(lowered.clone()).await.unwrap();
    assert_eq!(rows(&native.batch), vec!["2|4", "4|2"]);
    let postgres = sql::unparse(&lowered, SqlDialect::Postgres).unwrap();
    assert!(postgres.contains("CROSS JOIN LATERAL"), "{postgres}");
    #[cfg(feature = "duckdb")]
    {
        let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap();
        let result = sql::execute_prepared(&mut sql::DuckDbExecutor::default(), &prepared).unwrap();
        assert_eq!(rows(&result.batch), rows(&native.batch));
    }
}

#[tokio::test]
async fn cross_engine_search_stays_on_the_target_owner() {
    use serde_json::json;
    for text_search in [false, true] {
        let mut request = lance_request("/data/docs.lance", text_search);
        request["engines"]["source"] = json!({"dialect":"postgres"});
        request["tables"].as_array_mut().unwrap().push(json!({"name":"queries","engine":"source","columns":[{"name":"id","data_type":"int64"},{"name":"embedding","data_type":"list:float32"},{"name":"body","data_type":"string"}]}));
        request["nodes"].as_array_mut().unwrap().push(json!({"label":"Question","table":"queries","id":"id","properties":{"id":"id","embedding":"embedding","body":"body"}}));
        request["computed_relationships"][0]["source"] = "Question".into();
        request["query"] =
            "MATCH (s:Question)-[e:SIMILAR_TO]->(t:Document) WHERE s.id=1 RETURN t.id,e.score"
                .into();
        let plan = orchiddb::compiler::compile(serde_json::from_value(request.clone()).unwrap())
            .await
            .unwrap();
        assert!(plan.transfers.iter().any(|t| t.source_engine == "source"
            && t.sql.contains("queries")
            && !t.sql.contains("documents")));
        assert_eq!(
            plan.transfers
                .iter()
                .find_map(|t| t.operation.as_ref())
                .unwrap()
                .engine,
            "local"
        );
        if !text_search {
            request["engines"]["source"]["dialect"] = "duckdb".into();
            request["engines"]["vectors"] = json!({"dialect":"postgres"});
            request["tables"][0]["engine"] = "vectors".into();
            request["search_indexes"][0]["backend"] = json!({"kind":"pgvector"});
            let plan = orchiddb::compiler::compile(serde_json::from_value(request).unwrap())
                .await
                .unwrap();
            let search = plan
                .transfers
                .iter()
                .find_map(|t| t.operation.as_ref())
                .unwrap();
            assert_eq!(search.engine, "vectors");
            assert_eq!(search.template.dialect, "postgres");
            assert!(search.template.sql.contains("<=>"));
            assert!(!search.template.sql.contains("queries"));
        }
    }
}

#[tokio::test]
async fn direct_rank_expressions_and_property_dependencies_use_search_access() {
    let rule = COSINE.replace(
        "expression = \"score\"",
        "expression = \"vector.cosine_similarity(source.embedding,target.embedding)\"",
    );
    let lowered = lower(
        mapping(&rule),
        "MATCH (s:Document)-[e:SIMILAR_TO]->(t:Document) RETURN s.id,t.id ORDER BY s.id",
    );
    assert!(
        sql::unparse(&lowered, SqlDialect::Postgres)
            .unwrap()
            .contains("CROSS JOIN LATERAL")
    );
    assert_eq!(
        rows(&execute_lowered(lowered).await.unwrap().batch),
        vec!["1|2", "2|4", "3|2", "4|2"]
    );
    let rule = COSINE.replace(
        "score = \"vector.cosine_similarity(source.embedding, target.embedding)\"",
        "score = \"base\"\nbase = \"vector.cosine_similarity(source.embedding,target.embedding)\"",
    );
    let lowered = lower(mapping(&rule), "MATCH ()-[e:SIMILAR_TO]->() RETURN e.score");
    assert!(
        sql::unparse(&lowered, SqlDialect::Postgres)
            .unwrap()
            .contains("CROSS JOIN LATERAL")
    );
}

#[cfg(feature = "duckdb")]
#[test]
fn dependent_search_binds_lance_fixed_size_vectors() {
    use datafusion::common::ScalarValue;
    let array = arrow::array::FixedSizeListArray::from_iter_primitive::<
        arrow::datatypes::Float32Type,
        _,
        _,
    >([Some(vec![Some(1.0), Some(2.0)])], 2);
    let template = sql::search::SearchTemplate {
        sql: "SELECT $1 AS vector".into(),
        parameters: 1,
        dialect: "duckdb".into(),
    };
    let query = template
        .bind(&[ScalarValue::FixedSizeList(Arc::new(array))])
        .unwrap();
    let connection = duckdb::Connection::open_in_memory().unwrap();
    let returned = connection
        .prepare(&query)
        .unwrap()
        .query_arrow([])
        .unwrap()
        .collect::<Vec<_>>();
    let vectors = returned[0]
        .column(0)
        .as_any()
        .downcast_ref::<arrow::array::FixedSizeListArray>()
        .unwrap();
    assert_eq!(vectors.value_length(), 2);
    let vector = vectors.value(0);
    let values = vector
        .as_any()
        .downcast_ref::<arrow::array::Float32Array>()
        .unwrap();
    assert_eq!(values.values().as_ref(), &[1.0, 2.0]);
}

#[tokio::test]
async fn zero_lance_limit_does_not_open_a_dataset() {
    let mut request = lance_request("/does/not/exist.lance", false);
    request["computed_relationships"][0]["limit_per_source"] = 0.into();
    let plan = orchiddb::compiler::compile(serde_json::from_value(request).unwrap())
        .await
        .unwrap();
    let template = &plan
        .transfers
        .iter()
        .find_map(|t| t.operation.as_ref())
        .unwrap()
        .template;
    assert!(template.sql.contains("WHERE FALSE"));
    assert!(!template.sql.contains("lance_vector_search"));
}
