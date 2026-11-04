use super::*;
use crate::ir::rel::sql::{SqlDialect,unparse_plan};

fn evaluate(plan: LogicalPlan) -> Option<String> {
    for name in ["postgres","trino"] {
        unparse_plan(plan.clone(),SqlDialect::resolve(name).unwrap()).unwrap();
    }
    let sql = unparse_plan(plan,SqlDialect::DuckDb).unwrap();
    duckdb::Connection::open_in_memory().unwrap().query_row(&sql,[],|row| row.get(0)).unwrap_or_else(|e|panic!("{sql}: {e}"))
}
fn value_plan(value: Expr) -> LogicalPlan {
    LogicalPlanBuilder::empty(true).project(vec![value.alias("key")]).unwrap().build().unwrap()
}

#[test]
fn identity_tokens_keep_binary_bytes_and_composite_component_boundaries() {
    let bytes = identity_token_expr(lit(ScalarValue::Binary(Some(vec![0,255]))),&DataType::Binary);
    let text = identity_token_expr(lit("00ff"),&DataType::Utf8);
    let binary_token = evaluate(value_plan(bytes)).unwrap();
    let text_token = evaluate(value_plan(text)).unwrap();
    assert!(binary_token.ends_with(":4:00ff"),"{binary_token}");
    assert!(text_token.ends_with(":4:00ff"),"{text_token}");
    assert_ne!(binary_token,text_token,"equal display bytes must not conflate identity domains");
    let kind = DataType::Struct(vec![Field::new("left",DataType::Utf8,false),Field::new("right",DataType::Utf8,false)].into());
    let token = |left:&str,right:&str| evaluate(value_plan(identity_token_expr(df_core::named_struct(vec![lit("left"),lit(left),lit("right"),lit(right)]),&kind))).unwrap();
    let a = token("a","bc");
    let b = token("ab","c");
    assert_ne!(a,b,"component boundaries must remain injective");
    assert!(a.starts_with("tuple2:"));
}

#[test]
fn widened_identity_display_selects_the_active_original_key_and_preserves_null() {
    let tuple = DataType::Struct(vec![Field::new("part",DataType::Utf8,false)].into());
    let union = crate::ir::identity::identity_union_type([DataType::Utf8,tuple.clone()]);
    for (value,kind,expected) in [
        (lit("plain"),DataType::Utf8,Some("plain".to_string())),
        (lit(ScalarValue::Utf8(None)),DataType::Utf8,None),
        (df_core::named_struct(vec![lit("part"),lit("nested")]),tuple.clone(),Some(evaluate(value_plan(identity_token_expr(df_core::named_struct(vec![lit("part"),lit("nested")]),&tuple))).unwrap())),
    ] {
        let input = value_plan(cast_identity_expr(value,&kind,&union));
        let rendered = identity_text_expr(&input,"key".into());
        let plan = LogicalPlanBuilder::from(input).project(vec![rendered.alias("display")]).unwrap().build().unwrap();
        assert_eq!(evaluate(plan),expected);
    }
}

#[test]
fn all_binary_arrow_layouts_preserve_literal_bytes_in_identity_tokens() {
    for bytes in [vec![],vec![0,255],vec![39,92,0,127,128]] {
        let hex = bytes.iter().map(|b|format!("{b:02x}")).collect::<String>();
        for value in [ScalarValue::Binary(Some(bytes.clone())),ScalarValue::LargeBinary(Some(bytes.clone())),ScalarValue::BinaryView(Some(bytes.clone())),ScalarValue::FixedSizeBinary(bytes.len() as i32,Some(bytes.clone()))] {
            let kind = value.data_type();
            let token = evaluate(value_plan(identity_token_expr(lit(value),&kind))).unwrap();
            assert!(token.ends_with(&format!(":{}:{hex}",hex.len())),"{kind}: {token}");
        }
    }
}

#[cfg(feature = "trino")]
#[tokio::test]
#[ignore = "requires PostgreSQL and Trino"]
async fn binary_identity_literals_execute_on_postgres_and_trino() {
    use crate::execution::trino::{TrinoOptions,TrinoSession};
    use futures::StreamExt;
    let pg = std::env::var("GRAPH_PG_URL").expect("GRAPH_PG_URL");
    let endpoint = std::env::var("ORCHIDDB_TEST_TRINO_URL").or_else(|_|std::env::var("ORCHIDDB_TEST_OPA_URL")).expect("Trino URL");
    let mut trino = TrinoSession::source(TrinoOptions::new(endpoint,"orchid_test")).unwrap();
    for bytes in [vec![],vec![0,255],vec![39,92,0,127,128]] {
        for value in [ScalarValue::Binary(Some(bytes.clone())),ScalarValue::LargeBinary(Some(bytes.clone())),ScalarValue::BinaryView(Some(bytes.clone())),ScalarValue::FixedSizeBinary(bytes.len() as i32,Some(bytes.clone()))] {
            let kind = value.data_type();
            let plan = value_plan(identity_token_expr(lit(value),&kind));
            let expected = evaluate(plan.clone()).unwrap();
            let sql = unparse_plan(plan.clone(),SqlDialect::Postgres).unwrap();
            let result = std::process::Command::new("psql").args([&pg,"-X","-A","-t","-v","ON_ERROR_STOP=1","-c",&sql]).output().unwrap();
            assert!(result.status.success(),"{sql}: {}",String::from_utf8_lossy(&result.stderr));
            assert_eq!(String::from_utf8(result.stdout).unwrap().trim(),expected);
            let sql = unparse_plan(plan,SqlDialect::resolve("trino").unwrap()).unwrap();
            let mut stream = trino.execute(&sql,vec![]).await.unwrap();
            let mut rows = vec![];
            while let Some(batch) = stream.next().await {
                let batch = batch.unwrap();
                for index in 0..batch.num_rows() {
                    rows.push(arrow::util::display::array_value_to_string(batch.column(0).as_ref(),index).unwrap());
                }
            }
            assert_eq!(rows,vec![expected],"{sql}");
        }
    }
}
