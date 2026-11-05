use super::*;
use super::super::{Constraint, ConstraintCatalog, Fact, bind};
use super::super::properties::Origin;
use arrow::{array::{Int64Array, RecordBatch, StringArray}, datatypes::{DataType, Field, Schema}};
use datafusion::{datasource::{MemTable,provider_as_source},prelude::{col,lit}};
use std::sync::Arc;

fn scan(projection: Option<Vec<usize>>, unique: bool) -> LogicalPlan {
    let schema=Arc::new(Schema::new(vec![Field::new("id",DataType::Int64,false),Field::new("payload",DataType::Utf8,true)]));
    let batch=RecordBatch::try_new(schema.clone(),vec![Arc::new(Int64Array::from(vec![1,2])),Arc::new(StringArray::from(vec![Some("Ada"),None]))]).unwrap();
    let table=Arc::new(MemTable::try_new(schema,vec![vec![batch]]).unwrap());
    let mut catalog=ConstraintCatalog::default();
    catalog.insert("people",Constraint::enforced("non_null",Fact::NonNull{columns:vec!["id".into()]}));
    if unique { catalog.insert("people",Constraint::enforced("unique_id",Fact::Unique{columns:vec!["id".into()],nulls_equal:false})); }
    let provider=bind("people",table,&catalog,None).unwrap();
    LogicalPlanBuilder::scan("people",provider_as_source(provider),projection).unwrap().build().unwrap()
}
fn origin(column: &str) -> Origin { Origin {table:"people".into(),column:column.into()} }

#[tokio::test]
async fn pruned_payload_restoration_preserves_source_rows_and_existing_fields() {
    let original=scan(Some(vec![0]),false);
    let (restored,index)=expose_named_occurrence_column(&original,0,&origin("payload"),"restored_payload").unwrap().unwrap();
    assert_eq!(index,1);
    assert_eq!(restored.schema().columns()[0],original.schema().columns()[0]);
    assert_eq!(restored.schema().field(index).name(),"restored_payload");
    assert_eq!(restored.schema().field(index).data_type(),&DataType::Utf8);
    let batches=crate::ir::rel::execute_test_plan(restored).await.unwrap();
    let ids=batches.iter().flat_map(|batch|batch.column(0).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
    let values=batches.iter().flat_map(|batch|batch.column(1).as_any().downcast_ref::<StringArray>().unwrap().iter().map(|v|v.map(str::to_owned)).collect::<Vec<_>>()).collect::<Vec<_>>();
    assert_eq!(ids,vec![1,2]);
    assert_eq!(values,vec![Some("Ada".into()),None]);
    let (same,index)=expose_named_occurrence_column(&original,0,&origin("id"),"unused").unwrap().unwrap();
    assert_eq!(index,0); assert_eq!(same,original);
}

#[test]
fn payload_restoration_requires_matching_origin_and_does_not_cross_aggregation() {
    let input=scan(Some(vec![0]),false);
    for (key,origin) in [(0,Origin{table:"other".into(),column:"payload".into()}),(0,origin("missing")),(99,origin("payload"))] {
        assert!(expose_named_occurrence_column(&input,key,&origin,"restored").unwrap().is_none());
    }
    let aggregate=LogicalPlanBuilder::from(input).aggregate(vec![col("people.id")],Vec::<Expr>::new()).unwrap().build().unwrap();
    assert!(expose_named_occurrence_column(&aggregate,0,&origin("payload"),"restored").unwrap().is_none());
    assert!(occurrence_column(&aggregate,0,&origin("id")).is_none());
}

#[test]
fn containment_normalizes_aliases_but_requires_identical_origins_and_predicates() {
    let base=scan(None,false);
    let selection=|alias:&str,value:i64|LogicalPlanBuilder::from(base.clone()).alias(alias).unwrap()
        .filter(col(format!("{alias}.id")).eq(lit(value))).unwrap().build().unwrap();
    let first=selection("a",1); let same=selection("b",1); let other=selection("b",2);
    assert!(contained(&first,&same));
    assert!(contained(&first,&base));
    assert!(!contained(&base,&first));
    assert!(!contained(&first,&other));
    let limited=LogicalPlanBuilder::from(base.clone()).limit(0,Some(1)).unwrap().build().unwrap();
    assert!(domain(&limited).is_none());
    assert!(!contained(&limited,&base));
    let union=LogicalPlanBuilder::from(base).union(first).unwrap().distinct().unwrap().build().unwrap();
    let (optimized,proofs)=optimize(union).unwrap();
    assert!(proofs.iter().any(|proof|proof.rule=="contained_union"));
    assert!(matches!(optimized,LogicalPlan::Distinct(_)));
}

#[test]
fn optimizer_rule_exposes_proven_distinct_removal_without_changing_schema() {
    let rule=ConstraintOptimizer;
    let config=datafusion::optimizer::OptimizerContext::new();
    assert_eq!(rule.name(),"relational_constraints");
    assert!(rule.supports_rewrite());
    let source=scan(None,true);
    let unchanged=rule.rewrite(source.clone(),&config).unwrap();
    assert!(!unchanged.transformed);
    let distinct=LogicalPlanBuilder::from(source.clone()).distinct().unwrap().build().unwrap();
    let rewritten=rule.rewrite(distinct.clone(),&config).unwrap();
    assert!(rewritten.transformed);
    assert_eq!(rewritten.data.schema(),distinct.schema());
    assert_eq!(rewritten.data,source);
}
