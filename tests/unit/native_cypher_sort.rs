use super::*;
use crate::ir::Row;

#[tokio::test]
async fn typed_cypher_sort_preserves_null_order_stability_and_empty_inputs() {
    let graph = PropertyGraph::new();
    let policy = GraphPlanPolicy::cypher();
    let fields = vec!["value".to_string(),"id".to_string()];
    for empty in [false,true] {
        let values = [Value::Int(10),Value::Int(2),Value::Null,Value::Int(2)];
        let rows = if empty {vec![]} else {values.into_iter().enumerate().map(|(id,value)|
            Row::new().with("value",value).with("id",Value::Int(id as i64))).collect()};
        let batch = result_batch(&fields,ResultForm::RowSet,&rows,&graph,&policy).unwrap();
        let source = provider_as_source(Arc::new(MemTable::try_new(batch.schema(),vec![vec![batch]]).unwrap()));
        let input = LogicalPlanBuilder::scan("sort_values",source,None).unwrap().build().unwrap();
        for (dir,nulls,expected) in [
            (SortDir::Asc,NullsOrder::Last,vec![1,3,0,2]),
            (SortDir::Desc,NullsOrder::First,vec![2,0,1,3]),
        ] {
            for wrapped in [false,true] {
                let mut context = LoweringContext {
                    island_memo:None,graph:&graph,options:RelBackendOptions {native_values:true,..Default::default()},
                    policy:policy.clone(),language:Language::Cypher,scan_counter:0,correlate_plan:None,
                    rdf_typed_terms_used:false,scan_logical_labels:false,gremlin_label_binds:Default::default(),
                    unordered_collects:Default::default(),unordered_distincts:Default::default(),
                };
                let expr = if wrapped {IrExpr::Call {name:"cypher_order_key".into(),args:vec![IrExpr::binding("value")]}} else {IrExpr::binding("value")};
                let lowered = context.native_sort(LoweredNode::new(input.clone()),&[SortKey {expr,dir:dir.clone(),nulls}]).unwrap();
                let batches = super::super::execute_test_plan(lowered.plan).await.unwrap();
                let actual = batches.iter().flat_map(|batch|decode_rows(batch,&fields,&graph).unwrap())
                    .map(|row|row.bindings["id"].as_i64().unwrap()).collect::<Vec<_>>();
                assert_eq!(actual,if empty {vec![]} else {expected.clone()},"{dir:?} {nulls:?} wrapped={wrapped}");
            }
        }
    }
}
