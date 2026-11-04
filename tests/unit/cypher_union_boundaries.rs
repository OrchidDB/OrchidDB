use super::*;

fn context(graph:&PropertyGraph)->LoweringContext<'_> {
    LoweringContext{island_memo:None,graph,options:RelBackendOptions{native_values:false,..Default::default()},
        policy:GraphPlanPolicy::cypher(),language:Language::Cypher,scan_counter:0,correlate_plan:None,
        rdf_typed_terms_used:false,scan_logical_labels:false,gremlin_label_binds:Default::default(),
        unordered_collects:Default::default(),unordered_distincts:Default::default()}
}
fn values(rows:Vec<Vec<Value>>)->Node {
    Node::GraphValues{bindings:vec!["value".into(),"retained".into()],rows,bulk:None}
}

#[tokio::test]
async fn unboxed_cypher_union_types_only_null_columns_and_preserves_multiplicity() {
    let graph=PropertyGraph::new();
    let nulls=values(vec![vec![Value::Null,Value::Long(1)];2]);
    let integer=values(vec![vec![Value::Long(7),Value::Long(1)]]);
    for all in [false,true] {
        for reverse in [false,true] {
            for align in [UnionAlign::ByPosition,UnionAlign::ByVariableName] {
                let (left,right)=if reverse {(&integer,&nulls)} else {(&nulls,&integer)};
                let lowered=context(&graph).lower_union(all,align,left,right).unwrap();
                let batches=super::super::execute_test_plan(lowered.plan).await.unwrap();
                let mut rows=vec![];
                for batch in batches {for row in 0..batch.num_rows() {
                    let value=ScalarValue::try_from_array(batch.column(0),row).unwrap();
                    let retained=ScalarValue::try_from_array(batch.column(1),row).unwrap();
                    assert_eq!(retained,ScalarValue::Int64(Some(1)));
                    rows.push(value);
                }}
                assert_eq!(rows.len(),if all {3}else{2});
                assert_eq!(rows.iter().filter(|value|value.is_null()).count(),if all {2}else{1});
                assert_eq!(rows.iter().filter(|value|**value==ScalarValue::Int64(Some(7))).count(),1);
            }
        }
    }
}

#[test]
fn unboxed_cypher_union_rejects_lossy_concrete_type_coercion() {
    let graph=PropertyGraph::new();
    let integer=values(vec![vec![Value::Long(9007199254740993),Value::Long(1)]]);
    for other in [Value::String("9007199254740993".into()),Value::Float(9007199254740992.0)] {
        let other=values(vec![vec![other,Value::Long(1)]]);
        let error=context(&graph).lower_union(false,UnionAlign::ByPosition,&integer,&other).unwrap_err();
        assert!(error.to_string().contains("Cypher UNION column coercion requires native values"),"{error}");
    }
}
