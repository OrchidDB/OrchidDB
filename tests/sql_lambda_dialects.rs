#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    catalog::PropertyGraph,
    diagnostics::without_constant_folding,
    rel::{
        RelBackend, RelBackendOptions,
        sql::{self, SqlDialect},
    },
};

async fn generated(query: &str, dialect: SqlDialect) -> String {
    without_constant_folding(async {
        let plan =
            orchiddb::language::cypher::preparation::prepare(query, &Default::default(), None)
                .unwrap();
        let lowered = RelBackend::with_options(RelBackendOptions {
            native_values: true,
            ..Default::default()
        })
        .with_language_functions(true)
        .lower(&plan, &PropertyGraph::new())
        .unwrap();
        sql::unparse(&lowered, dialect).unwrap_or_else(|e| panic!("{query}: {e}"))
    })
    .await
}
fn cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("WITH [] AS xs RETURN all(x IN xs WHERE true) AS v", "true"),
        ("WITH [] AS xs RETURN any(x IN xs WHERE true) AS v", "false"),
        (
            "WITH [] AS xs RETURN single(x IN xs WHERE true) AS v",
            "false",
        ),
        (
            "WITH [1,2,3] AS xs RETURN size([x IN xs WHERE x > 1 | x*2]) AS v",
            "2",
        ),
        (
            "WITH 2 AS outer, [1,2,3] AS xs RETURN any(x IN xs WHERE x=outer) AS v",
            "true",
        ),
        ("RETURN fn.array_has([1,null],2) AS v", "false"),
        (
            "RETURN fn.array_element(fn.array_sort([2,null,1]),1) IS NULL AS v",
            "true",
        ),
        (
            "RETURN fn.cardinality(fn.array_concat([1],[2,3])) AS v",
            "3",
        ),
        (
            "WITH [true,null] AS xs RETURN single(x IN xs WHERE x) AS v",
            "null",
        ),
        (
            "WITH [true,true,null] AS xs RETURN single(x IN xs WHERE x) AS v",
            "false",
        ),
        (
            "WITH [true] AS xs RETURN single(x IN xs WHERE x) AS v",
            "true",
        ),
        (
            "WITH [false,null] AS xs RETURN any(x IN xs WHERE x) AS v",
            "null",
        ),
        (
            "WITH [false,null] AS xs RETURN all(x IN xs WHERE x) AS v",
            "false",
        ),
        (
            "WITH [true,null,false,true] AS xs RETURN size([x IN xs WHERE x]) AS v",
            "2",
        ),
    ]
}
#[tokio::test]
async fn native_lambdas_have_postgres_and_trino_implementations() {
    for dialect in [SqlDialect::Postgres, SqlDialect::resolve("trino").unwrap()] {
        for (query, _) in cases() {
            let sql = generated(query, dialect).await;
            assert!(!sql.contains("orchid_cypher_"), "{sql}");
            assert!(!sql.to_lowercase().contains("row_number"), "{sql}");
        }
    }
}
#[tokio::test]
#[ignore = "requires local PostgreSQL fixture and GRAPH_PG_URL"]
async fn postgres_lambda_truth_tables_execute() {
    let url = std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL is required");
    for (query, expected) in cases() {
        let sql = generated(query, SqlDialect::Postgres).await;
        let output = std::process::Command::new("psql")
            .args([
                "-X",
                "-qAt",
                "-v",
                "ON_ERROR_STOP=1",
                "-d",
                &url,
                "-c",
                &format!("SELECT coalesce(to_jsonb(v)::text,'null') FROM ({sql}) result"),
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{sql}\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap().trim(),
            expected,
            "{query}\n{sql}"
        );
    }
}
#[cfg(feature = "trino")]
#[tokio::test]
#[ignore = "requires local Trino fixture and ORCHIDDB_TEST_OPA_URL"]
async fn trino_lambda_truth_tables_execute() {
    use orchiddb::execution::trino::{TrinoOptions, TrinoSession};
    let url = std::env::var("ORCHIDDB_TEST_OPA_URL").expect("ORCHIDDB_TEST_OPA_URL is required");
    let session = TrinoSession::source(TrinoOptions::new(url, "admin")).unwrap();
    for (query, expected) in cases() {
        let sql = generated(query, SqlDialect::resolve("trino").unwrap()).await;
        let batches = session
            .execute(&sql, vec![])
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"))
            .collect_batches()
            .await
            .unwrap();
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 1);
        let actual = arrow::util::display::array_value_to_string(batches[0].column(0), 0).unwrap();
        assert_eq!(
            actual,
            if expected == "null" { "" } else { expected },
            "{query}\n{sql}"
        );
    }
}
