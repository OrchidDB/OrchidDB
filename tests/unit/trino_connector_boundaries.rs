use super::*;
use std::collections::{BTreeMap,BTreeSet};
use arrow::datatypes::{Schema,Field};
use datafusion::logical_expr::{LogicalPlanBuilder,lit};
use datafusion::prelude::col;

fn table_function(value:serde_json::Value)->ConnectorSource {
    ConnectorSource::TableFunction{catalog:"cat".into(),schema:"system".into(),function:"source".into(),
        arguments:BTreeMap::from([("ARG".into(),value)]),argument_columns:BTreeMap::from([("argument".into(),"ARG".into())])}
}
#[test]
fn table_function_literals_preserve_null_boolean_and_quoted_argument_values() {
    for (value,literal,ty,sql_type) in [(serde_json::Value::Null,"NULL",DataType::Utf8,"VARCHAR"),
        (serde_json::json!(true),"TRUE",DataType::Boolean,"BOOLEAN"),
        (serde_json::json!(false),"FALSE",DataType::Boolean,"BOOLEAN"),
        (serde_json::json!(17),"17",DataType::Int64,"BIGINT"),
        (serde_json::json!("a'b"),"'a''b'",DataType::Utf8,"VARCHAR")] {
        let source=table_function(value);
        assert_eq!(source.relation_sql().unwrap(),format!("TABLE(\"cat\".\"system\".\"source\"(\"ARG\" => {literal}))"));
        assert_eq!(source.column("argument",&ty).unwrap(),format!("CAST({literal} AS {sql_type})"));
    }
    for value in [serde_json::json!([]),serde_json::json!({})] {
        let source=table_function(value);
        assert!(source.relation_sql().unwrap_err().to_string().contains("scalar JSON literals"));
        assert!(source.column("argument",&DataType::Utf8).unwrap_err().to_string().contains("scalar arguments"));
    }
    let mut source=table_function(serde_json::json!(1));
    let ConnectorSource::TableFunction{arguments,..}=&mut source else {panic!()};
    arguments.insert("".into(),serde_json::json!(2));
    assert!(source.relation_sql().unwrap_err().to_string().contains("argument name must be nonempty"));
}
#[test]
fn connector_identifiers_and_json_columns_use_declared_schema_contracts() {
    for field in ["catalog","schema","function"] {
        let mut source=serde_json::to_value(table_function(serde_json::Value::Null)).unwrap();
        source[field]=serde_json::json!("");
        assert!(serde_json::from_value::<ConnectorSource>(source).unwrap().relation_sql().unwrap_err().to_string().contains("identifiers must be nonempty"));
    }
    let source=ConnectorSource::Table{catalog:"cat".into(),schema:"public".into(),table:"people".into(),json_columns:BTreeSet::from(["payload".into()])};
    assert_eq!(source.relation_sql().unwrap(),"\"cat\".\"public\".\"people\"");
    assert_eq!(source.column("payload",&DataType::Utf8).unwrap(),"CAST(json_format(\"payload\") AS VARCHAR)");
    assert_eq!(source.column("id",&DataType::Int64).unwrap(),"CAST(\"id\" AS BIGINT)");
    for field in ["catalog","schema","table"] {
        let mut invalid=serde_json::to_value(&source).unwrap();invalid[field]=serde_json::json!("");
        assert!(serde_json::from_value::<ConnectorSource>(invalid).unwrap().relation_sql().is_err());
    }
}
#[test]
fn connector_scan_lowering_keeps_pushed_filters_limits_and_zero_column_cardinality() {
    use crate::ir::rel::sql::{DuckDbExecutor,SqlExecutor,SqlValue};
    let source=ConnectorSource::Table{catalog:"memory".into(),schema:"main".into(),table:"people".into(),json_columns:BTreeSet::new()};
    let schema=Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,true),Field::new("name",DataType::Utf8,true)]));
    for projection in [None,Some(vec![]),Some(vec![1])] {
        let provider=source.clone().provider(schema.clone()).unwrap();
        let plan=LogicalPlanBuilder::scan("people",datafusion::datasource::provider_as_source(provider),projection.clone()).unwrap().build().unwrap();
        let LogicalPlan::TableScan(mut scan)=plan else {panic!()};
        scan.filters=vec![col("people.id").gt(lit(7_i64))];scan.fetch=Some(2);
        let sql=lower(&LogicalPlan::TableScan(scan)).unwrap().unwrap().to_string();
        assert!(sql.contains("\"id\" > 7"),"{sql}");
        assert!(!sql.contains("\"people\".\"id\""),"{sql}");
        assert!(sql.ends_with("LIMIT 2"),"{sql}");
        // These casts and SELECT clauses have identical semantics on DuckDB;
        // execution catches references to columns dropped before the filter.
        let mut database=DuckDbExecutor::new();
        let rows=database.run(&["CREATE TABLE people(id BIGINT,name VARCHAR)".into(),
            "INSERT INTO people VALUES (7,'a'),(8,'b'),(9,'c'),(6,'d')".into()],&sql).unwrap_or_else(|e|panic!("{sql}: {e}"));
        let expected=match projection {
            None=>vec![vec![SqlValue::Int(8),SqlValue::Text("b".into())],vec![SqlValue::Int(9),SqlValue::Text("c".into())]],
            Some(ref columns) if columns.is_empty()=>vec![vec![SqlValue::Int(1)],vec![SqlValue::Int(1)]],
            _=>vec![vec![SqlValue::Text("b".into())],vec![SqlValue::Text("c".into())]],
        };
        assert_eq!(rows,expected,"{sql}");
    }
}

#[cfg(feature="trino")]
#[tokio::test]
#[ignore="requires Trino"]
async fn connector_projected_filters_execute_on_trino() {
    use crate::execution::trino::{TrinoOptions,TrinoSession};
    use futures::StreamExt;
    let endpoint=std::env::var("ORCHIDDB_TEST_TRINO_URL").or_else(|_|std::env::var("ORCHIDDB_TEST_OPA_URL")).expect("Trino URL");
    let trino=TrinoSession::source(TrinoOptions::new(endpoint,"orchid_test")).unwrap();
    let source=ConnectorSource::Table{catalog:"memory".into(),schema:"main".into(),table:"people".into(),json_columns:BTreeSet::new()};
    let schema=Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,true),Field::new("name",DataType::Utf8,true)]));
    for projection in [None,Some(vec![]),Some(vec![1])] {
        let plan=LogicalPlanBuilder::scan("people",datafusion::datasource::provider_as_source(source.clone().provider(schema.clone()).unwrap()),projection.clone()).unwrap().build().unwrap();
        let LogicalPlan::TableScan(mut scan)=plan else {panic!()};
        scan.filters=vec![col("people.id").gt(lit(7_i64))];scan.fetch=Some(2);
        let statement=lower(&LogicalPlan::TableScan(scan)).unwrap().unwrap().to_string().replace("\"memory\".\"main\".\"people\"","people");
        let sql=format!("WITH people(id,name) AS (VALUES (7,'a'),(8,'b'),(9,'c'),(6,'d')) {statement}");
        let mut stream=trino.execute(&sql,vec![]).await.unwrap_or_else(|e|panic!("{sql}: {e}"));
        let mut rows=vec![];
        while let Some(batch)=stream.next().await {
            let batch=batch.unwrap();
            for index in 0..batch.num_rows() {rows.push(batch.columns().iter().map(|column|arrow::util::display::array_value_to_string(column.as_ref(),index).unwrap()).collect::<Vec<_>>());}
        }
        rows.sort();
        let expected=match projection {
            None=>vec![vec!["8","b"],vec!["9","c"]],
            Some(ref columns) if columns.is_empty()=>vec![vec!["1"],vec!["1"]],
            _=>vec![vec!["b"],vec!["c"]],
        };
        assert_eq!(rows,expected,"{sql}");
    }
}

#[test]
fn constrained_connectors_keep_their_source_and_reject_missing_filter_columns() {
    use crate::ir::rel::constraints::{self,ConstraintCatalog,Constraint,Fact};
    let schema=Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,true)]));
    let source=ConnectorSource::Table{catalog:"cat".into(),schema:"main".into(),table:"people".into(),json_columns:BTreeSet::new()};
    let mut catalog=ConstraintCatalog::default();
    catalog.insert("people",Constraint::enforced("unique_id",Fact::Unique{columns:vec!["id".into()],nulls_equal:false}));
    let provider=constraints::bind("people",source.provider(schema).unwrap(),&catalog,None).unwrap();
    let plan=LogicalPlanBuilder::scan("people",datafusion::datasource::provider_as_source(provider),None).unwrap().build().unwrap();
    assert!(lower(&plan).unwrap().unwrap().to_string().contains("\"cat\".\"main\".\"people\""));
    let LogicalPlan::TableScan(mut scan)=plan else {panic!()};
    scan.filters=vec![col("missing").gt(lit(1_i64))];
    assert!(lower(&LogicalPlan::TableScan(scan)).unwrap_err().to_string().contains("missing"));
}

#[test]
fn monitoring_timestamps_are_converted_to_utc_at_the_declared_precision() {
    for source in [ConnectorSource::Loki{catalog:"loki".into(),query:"{}".into(),start:"2024-01-01T00:00:00Z".into(),end:"2024-01-02T00:00:00Z".into()},
        ConnectorSource::Prometheus{catalog:"prom".into(),metric:"up".into(),start:"2024-01-01T00:00:00Z".into(),end:"2024-01-02T00:00:00Z".into()}] {
        assert_eq!(source.column("timestamp",&DataType::Timestamp(TimeUnit::Millisecond,None)).unwrap(),"CAST(timestamp AT TIME ZONE 'UTC' AS TIMESTAMP(3))");
    }
}
