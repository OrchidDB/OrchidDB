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

#[test]
fn binding_column_exclusion_preserves_neighbor_names_and_schema_order() {
    let mut names = vec!["n".to_string(), id_col("n"), label_col("n"), node_labels_col("n"),
        src_id_col("n"), src_label_col("n"), dst_id_col("n"), dst_label_col("n"),
        path_len_col("n"), path_inner_col("n"), prop_col("n", "odd.property"),
        "n2".into(), id_col("n2"), prop_col("n2", "x"), "unrelated".into()];
    names.push(id_col("other"));
    let plan = LogicalPlanBuilder::empty(true).project(names.iter().enumerate()
        .map(|(i,name)| lit(i as i64).alias(name)).collect::<Vec<_>>()).unwrap().build().unwrap();
    let columns = |expressions: Vec<Expr>| expressions.into_iter().map(|expr| {
        let Expr::Column(column) = expr else { panic!("expected preserved column") }; column.name
    }).collect::<Vec<_>>();
    assert_eq!(columns(existing_columns_excluding_bindings(&plan, &["n", "other"])),
        vec!["n2".to_string(), id_col("n2"), prop_col("n2", "x"), "unrelated".into()]);
    assert_eq!(columns(existing_columns_excluding_bindings(&plan, &[])), names);
    assert_eq!(columns(existing_columns_excluding_bindings(&plan, &["absent"])), names);
}

#[test]
fn binding_duplication_preserves_encoded_values_properties_labels_and_endpoint_types() {
    for edge in [false, true] {
        let from = "original"; let to = "copy.with.dot";
        let mut fields = vec![
            lit(i64::MAX).alias(id_col(from)), lit("Person").alias(label_col(from)),
            lit("encoded properties").alias(from),
            lit(ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Utf8(Some("Person".into()))], &DataType::Utf8))).alias(node_labels_col(from)),
            lit(ScalarValue::Int16(Some(7))).alias(prop_col(from, "age")),
            lit("neighbor").alias(prop_col("original2", "age")),
        ];
        if edge { fields.extend([
            lit("Source").alias(src_label_col(from)), lit(ScalarValue::Int32(Some(1))).alias(src_id_col(from)),
            lit("Target").alias(dst_label_col(from)), lit("uuid-key").alias(dst_id_col(from)),
        ]); }
        let plan = LogicalPlanBuilder::empty(true).project(fields).unwrap().build().unwrap();
        let copied = duplicate_binding_projection_only(&plan, from, to).unwrap();
        let copied_plan = LogicalPlanBuilder::from(plan.clone()).project(copied.clone()).unwrap().build().unwrap();
        assert_eq!(copied_plan.schema().fields().len(), if edge {9} else {5});
        for expr in &copied {
            let Expr::Alias(alias) = expr else { panic!("expected alias") };
            let Expr::Column(column) = alias.expr.as_ref() else { panic!("copy must not cast values") };
            let original_type = plan.schema().field_with_unqualified_name(&column.name).unwrap().data_type();
            assert_eq!(copied_plan.schema().field_with_unqualified_name(&alias.name).unwrap().data_type(), original_type);
            assert!(is_binding_column(&alias.name, to));
        }
        assert_eq!(has_binding_shape(&copied_plan, to), Some(if edge {BindingShape::Edge} else {BindingShape::Node}));
        let full = LogicalPlanBuilder::from(plan.clone()).project(duplicate_binding_projection(&plan, from, to).unwrap()).unwrap().build().unwrap();
        assert_eq!(full.schema().fields().len(), plan.schema().fields().len()+copied.len());
        assert!(has_exact_col(&full, &prop_col("original2", "age")));
    }
    let scalar = value_plan(lit(1_i64));
    let error = duplicate_binding_projection_only(&scalar, "key", "copy").unwrap_err();
    assert!(error.to_string().contains("not an element binding"), "{error}");
}
