use super::*;
use crate::ir::rel::{
    source_metadata::SourceMetadata,
    statistics::{SourceStatistics, StatisticsProvider, StatisticsSnapshot},
};
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::{
    datasource::{empty::EmptyTable, provider_as_source},
    logical_expr::{LogicalPlanBuilder, col, lit},
};
use serde_json::json;
use std::collections::BTreeMap;

fn metadata(options: serde_json::Value) -> SourceMetadata {
    SourceMetadata {
        table: "source".into(),
        format: Some("table_function".into()),
        options: serde_json::from_value(options).unwrap(),
        indexes: vec![],
    }
}
fn scan(
    p: Arc<dyn TableProvider>,
    projection: Option<Vec<usize>>,
    filters: Vec<Expr>,
    fetch: Option<usize>,
) -> LogicalPlan {
    let LogicalPlan::TableScan(mut scan) =
        LogicalPlanBuilder::scan("source", provider_as_source(p), projection)
            .unwrap()
            .build()
            .unwrap()
    else {
        unreachable!()
    };
    scan.filters = filters;
    scan.fetch = fetch;
    LogicalPlan::TableScan(scan)
}
fn statistics(p: Arc<dyn TableProvider>) -> Arc<dyn TableProvider> {
    StatisticsProvider::wrap(
        p,
        Arc::new(StatisticsSnapshot {
            version: 1,
            revision: "test-revision".into(),
            mapping: String::new(),
            collected_at: String::new(),
            sources: BTreeMap::from([("source".into(), SourceStatistics::default())]),
            relationships: vec![],
            rdf: vec![],
            report: Default::default(),
        }),
        "source".into(),
    )
}

#[test]
fn table_function_quoted_names_defaults_and_hidden_filters_execute() {
    let schema = Arc::new(Schema::new(vec![
        Field::new("flag", DataType::Boolean, false),
        Field::new("message", DataType::Utf8, false),
        Field::new("out", DataType::Utf8, false),
    ]));
    let source=provider(&metadata(json!({"function":["f\"n"],"arguments":[{"column":"flag","value":false},{"column":"message","name":"text\"arg","value":"O'Reilly"}]})),schema).unwrap().unwrap();
    assert_eq!(source.table_type(), TableType::Base);
    let filter = col("source.out").eq(lit("O'Reilly"));
    assert_eq!(
        source.supports_filters_pushdown(&[&filter]).unwrap(),
        vec![TableProviderFilterPushDown::Inexact]
    );
    let catalog=serde_json::from_value(json!({"tables":{"source":[{"name":"out_not_null","fact":{"kind":"non_null","columns":["out"]},"evidence":{"kind":"enforced"}}]}})).unwrap();
    let wrapped =
        crate::ir::rel::constraints::bind("source", statistics(source.clone()), &catalog, None)
            .unwrap();
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE MACRO \"f\"\"n\"(a, \"text\"\"arg\") AS TABLE SELECT CASE WHEN a THEN 'wrong' ELSE \"text\"\"arg\" END AS \"out\"").unwrap();
    for source in [source, wrapped.clone(), statistics(wrapped)] {
        let plan = scan(
            source,
            Some(vec![2]),
            vec![filter.clone(), lit(false).eq(col("source.flag"))],
            Some(1),
        );
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let sql = lower(&plan, dialect).unwrap().unwrap().to_string();
            assert!(
                sql.contains("\"f\"\"n\"")
                    && sql.contains("\"text\"\"arg\"")
                    && sql.contains("O''Reilly")
                    && sql.contains("LIMIT 1"),
                "{sql}"
            );
            if dialect == SqlDialect::DuckDb {
                assert_eq!(
                    connection
                        .query_row(&sql, [], |row| row.get::<_, String>(0))
                        .unwrap(),
                    "O'Reilly"
                );
            }
        }
    }
}

#[test]
fn optional_arguments_keep_virtual_nulls_and_empty_projection_cardinality() {
    let schema = Arc::new(Schema::new(vec![
        Field::new("flag", DataType::Int64, true),
        Field::new("out", DataType::Int64, false),
    ]));
    let source=provider(&metadata(json!({"function":["optional_input"],"arguments":[{"column":"flag","name":"flag","optional":true}]})),schema).unwrap().unwrap();
    let connection = duckdb::Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE MACRO optional_input(flag := 7) AS TABLE SELECT flag AS out")
        .unwrap();
    let plan = scan(source.clone(), None, vec![], None);
    let sql = lower(&plan, SqlDialect::DuckDb)
        .unwrap()
        .unwrap()
        .to_string();
    let actual = connection
        .query_row(&sql, [], |r| {
            Ok((r.get::<_, Option<i64>>(0)?, r.get::<_, i64>(1)?))
        })
        .unwrap();
    assert_eq!(actual, (None, 7));
    let plan = scan(source, Some(vec![]), vec![], Some(0));
    let sql = lower(&plan, SqlDialect::DuckDb)
        .unwrap()
        .unwrap()
        .to_string();
    assert_eq!(
        connection
            .query_row(&format!("SELECT count(*) FROM ({sql})"), [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[test]
fn table_function_input_binding_rejects_ranges_nulls_and_correlations() {
    let scalar = lit(7_i64);
    for expression in [
        col("source.arg").eq(scalar.clone()),
        scalar.clone().eq(col("source.arg")),
    ] {
        assert_eq!(binding(&expression, "arg"), Some(&scalar));
    }
    for expression in [
        col("arg").eq(lit(datafusion::common::ScalarValue::Int64(None))),
        col("arg").gt(scalar.clone()),
        col("arg").eq(col("other")),
        col("arg").eq(scalar.clone()).or(col("arg").eq(lit(8_i64))),
        lit(true),
        col("different").eq(scalar),
    ] {
        assert!(binding(&expression, "arg").is_none(), "{expression}");
    }
    let schema = Arc::new(Schema::new(vec![Field::new("out", DataType::Int64, false)]));
    let plain = scan(Arc::new(EmptyTable::new(schema)), None, vec![], None);
    assert!(!push_filters(plain.clone()).unwrap().1);
    assert!(lower(&plain, SqlDialect::DuckDb).unwrap().is_none());
    let empty = LogicalPlanBuilder::empty(true).build().unwrap();
    assert!(lower(&empty, SqlDialect::DuckDb).unwrap().is_none());
}

#[test]
fn table_function_rejects_invalid_default_shapes_and_argument_names() {
    let schema = Arc::new(Schema::new(vec![
        Field::new("arg", DataType::Int64, true),
        Field::new("other", DataType::Int64, true),
        Field::new("out", DataType::Int64, true),
    ]));
    for options in [
        json!({"function":[]}),
        json!({"function":["schema",""]}),
        json!({"function":["f"],"arguments":[{"column":"missing"}]}),
        json!({"function":["f"],"arguments":[{"column":"arg"},{"column":"arg"}]}),
        json!({"function":["f"],"arguments":[{"column":"arg","name":""}]}),
        json!({"function":["f"],"arguments":[{"column":"arg","name":"same"},{"column":"other","name":"same"}]}),
        json!({"function":["f"],"arguments":[{"column":"arg","optional":true}]}),
        json!({"function":["f"],"arguments":[{"column":"arg","name":"first"},{"column":"other"}]}),
        json!({"function":["f"],"arguments":[{"column":"arg","value":[]}]}),
        json!({"function":["f"],"arguments":[{"column":"arg","value":{}}]}),
        json!({"function":["f"],"arguments":[{"column":"arg"},{"column":"other"},{"column":"out"}]}),
    ] {
        let error = provider(&metadata(options), schema.clone())
            .err().expect("invalid metadata must fail")
            .to_string();
        assert!(error.contains("table function source"), "{error}");
    }
    let mut ignored = metadata(json!({"unrecognized":true}));
    ignored.format = None;
    assert!(provider(&ignored, schema).unwrap().is_none());
}
