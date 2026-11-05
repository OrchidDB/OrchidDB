use super::*;
use super::super::{Constraint, ConstraintCatalog, Fact, bind};
use super::super::properties::Origin;
use arrow::{array::{Int64Array, RecordBatch, StringArray}, datatypes::{DataType, Field, Schema}};
use datafusion::{datasource::{MemTable,provider_as_source},prelude::{col,lit}};
use std::sync::Arc;
use std::ops::Not;

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

#[test]
fn key_lineage_accepts_only_lossless_integer_and_text_layout_casts() {
    use datafusion::logical_expr::expr::Cast;
    let cases = [
        (DataType::Int8, vec![DataType::Int8, DataType::Int16, DataType::Int32, DataType::Int64]),
        (DataType::Int16, vec![DataType::Int16, DataType::Int32, DataType::Int64]),
        (DataType::Int32, vec![DataType::Int32, DataType::Int64]),
        (DataType::Int64, vec![DataType::Int64]),
        (DataType::UInt8, vec![DataType::UInt8, DataType::UInt16, DataType::UInt32, DataType::UInt64, DataType::Int16, DataType::Int32, DataType::Int64]),
        (DataType::UInt16, vec![DataType::UInt16, DataType::UInt32, DataType::UInt64, DataType::Int32, DataType::Int64]),
        (DataType::UInt32, vec![DataType::UInt32, DataType::UInt64, DataType::Int64]),
        (DataType::UInt64, vec![DataType::UInt64]),
        (DataType::Utf8, vec![DataType::Utf8, DataType::LargeUtf8, DataType::Utf8View]),
        (DataType::LargeUtf8, vec![DataType::Utf8, DataType::LargeUtf8, DataType::Utf8View]),
        (DataType::Utf8View, vec![DataType::Utf8, DataType::LargeUtf8, DataType::Utf8View]),
        (DataType::Float64, vec![DataType::Float64]),
    ];
    for (source, allowed) in &cases {
        let schema = datafusion::common::DFSchema::try_from(Schema::new(vec![Field::new("key", source.clone(), true)])).unwrap();
        for (target, _) in &cases {
            let expr = Expr::Cast(Cast::new(Box::new(col("key").alias("nested")), target.clone())).alias("key_copy");
            let safe = allowed.contains(target);
            assert_eq!(input_column(&expr, &schema), safe.then_some(0), "{source:?} -> {target:?}");
            assert_eq!(harmless(&expr, &schema), safe, "{source:?} -> {target:?}");
        }
        let missing = Expr::Cast(Cast::new(Box::new(col("missing")), source.clone()));
        assert_eq!(input_column(&missing, &schema), None);
        assert!(!harmless(&missing, &schema));
    }
}

#[test]
fn nullable_equality_keys_require_direct_comparison_not_dependency_closure() {
    let mut facts = PlanProperties::default();
    facts.equality_keys = vec![vec![2, 3]];
    facts.dependencies = vec![(vec![1],vec![2]),(vec![0],vec![1]),(vec![2],vec![0])];
    assert_eq!(facts.closure(&[0]), [0,1,2].into_iter().collect());
    assert!(!facts.unique_on(&[0,3], false));
    assert!(facts.unique_on(&[2,3], false));
    assert!(!facts.unique_on(&[2,3], true));
    assert!(!facts.unique_on(&[2], false));
    facts.unique_keys.push(vec![2,3]);
    assert!(facts.unique_on(&[0,3], true));
    assert!(!facts.unique_on(&[], false));
    facts.dependencies.push((vec![],vec![3]));
    assert!(facts.unique_on(&[0], true));
}

#[test]
fn constraint_expression_safety_tracks_references_without_hiding_errors() {
    let source = scan(None, false);
    let schema = source.schema();
    for expr in [col("people.id").eq(lit(1i64)), col("people.id").not_eq(lit(1i64)),
        col("people.id").lt(lit(1i64)), col("people.id").lt_eq(lit(1i64)),
        col("people.id").gt(lit(1i64)), col("people.id").gt_eq(lit(1i64)),
        col("people.payload").is_null().or(col("people.payload").is_not_null()),
        col("people.id").eq(lit(1i64)).and(col("people.payload").is_null()).not().alias("predicate")] {
        assert!(harmless(&expr, schema), "{expr:?}");
    }
    for expr in [col("people.id").eq(lit("bad")), col("people.id") + lit(1i64),
        col("missing").eq(lit(1i64)), col("people.payload").like(lit("%x%"))] {
        assert!(!harmless(&expr, schema), "{expr:?}");
    }
    let expr = (col("people.id") + col("missing")).eq(col("people.id"))
        .and(col("people.payload").is_null()).alias("predicate");
    assert_eq!(referenced(&expr, schema), [0,1].into_iter().collect());
    assert!(referenced(&lit(7i64), schema).is_empty());
    assert_eq!(input_column(&lit(7i64), schema), None);
}

#[test]
fn projection_facts_preserve_duplicate_lineage_and_nonnull_literals() {
    let source = scan(None, true);
    let projection = LogicalPlanBuilder::from(source).project(vec![
        col("people.payload").alias("value"), col("people.id").alias("first_id"),
        col("people.id").alias("second_id"), lit(7i64).alias("constant"),
        lit(datafusion::common::ScalarValue::Int64(None)).alias("null_value"),
    ]).unwrap().build().unwrap();
    let facts = analyze(&projection);
    assert_eq!(facts.origins, vec![Some(origin("payload")),Some(origin("id")),Some(origin("id")),None,None]);
    assert!(facts.unique_on(&[1], true));
    assert!(facts.unique_on(&[2], true));
    assert_eq!(facts.closure(&[2]), (0..5).collect());
    assert!(facts.non_null.contains(&3));
    assert!(!facts.non_null.contains(&4));
    assert_eq!(facts.constants.len(),1);
    assert!(facts.constants.contains_key(&3));
    assert!(facts.complete_sources.contains("people"));
    assert!(facts.removable);
    let filtered = LogicalPlanBuilder::from(projection).filter(col("value").is_not_null().and(col("first_id").gt(lit(0i64)))).unwrap().build().unwrap();
    let facts = analyze(&filtered);
    assert!(facts.non_null.contains(&0));
    assert!(facts.complete_sources.is_empty());
    assert!(facts.removable);
    let sorted = LogicalPlanBuilder::from(filtered).sort(vec![col("first_id").sort(true,true)]).unwrap().build().unwrap();
    assert!(!analyze(&sorted).removable);
}

#[test]
fn view_scan_facts_remap_lineage_and_fence_filtered_or_limited_row_domains() {
    use datafusion::catalog::view::ViewTable;
    for projection in [None, Some(vec![1,0]), Some(vec![1])] {
        for (filters, fetch, complete, removable) in [
            (vec![], None, true, true),
            (vec![col("id").gt(lit(0i64))], None, false, true),
            (vec![], Some(1), false, true),
            (vec![col("id").eq(lit("invalid"))], None, false, false),
        ] {
            let view = Arc::new(ViewTable::new(scan(None,true),None));
            // Preserve the provider boundary: the general builder inlines unfiltered views.
            let plan = LogicalPlan::TableScan(datafusion::logical_expr::TableScan::try_new(
                "view_people", provider_as_source(view), projection.clone(), filters, fetch).unwrap());
            let facts = analyze(&plan);
            let columns = projection.clone().unwrap_or(vec![0,1]);
            assert_eq!(facts.origins, columns.iter().map(|i|Some(origin(if *i==0 {"id"} else {"payload"}))).collect::<Vec<_>>());
            assert_eq!(facts.complete_sources.contains("people"), complete);
            // A pushed predicate whose referenced column is pruned cannot be
            // proven harmless from the projected scan schema.
            assert_eq!(facts.removable, removable && (complete || fetch.is_some() || columns.contains(&0)));
            assert_eq!(facts.unique_on(&(0..columns.len()).collect::<Vec<_>>(), true), columns.contains(&0));
        }
    }
}

#[test]
fn foreign_key_match_proofs_require_complete_targets_and_exact_composite_lineage() {
    use super::super::properties::ForeignKey;
    let left = PlanProperties {
        non_null: [0,1].into_iter().collect(),
        foreign_keys: vec![ForeignKey { columns:vec![0,1],target:"parent".into(),references:vec!["tenant".into(),"id".into()],evidence:"parent_fk".into() }],
        ..Default::default()
    };
    let right = PlanProperties {
        complete_sources: ["parent".into()].into_iter().collect(),
        origins: vec![Some(Origin{table:"parent".into(),column:"id".into()}),Some(Origin{table:"parent".into(),column:"tenant".into()})],
        ..Default::default()
    };
    assert!(guaranteed_match(&left,&right,&[(1,0),(0,1)]));
    for pairs in [vec![],vec![(0,1)],vec![(0,0),(1,1)],vec![(0,1),(1,8)],vec![(0,1),(0,1)],vec![(0,1),(1,0),(1,0)]] {
        assert!(!guaranteed_match(&left,&right,&pairs),"{pairs:?}");
    }
    let mut nullable = left.clone(); nullable.non_null.remove(&1);
    assert!(!guaranteed_match(&nullable,&right,&[(0,1),(1,0)]));
    let mut filtered = right.clone(); filtered.complete_sources.clear();
    assert!(!guaranteed_match(&left,&filtered,&[(0,1),(1,0)]));
    let mut wrong_table = right.clone(); wrong_table.origins[0].as_mut().unwrap().table="other".into();
    assert!(!guaranteed_match(&left,&wrong_table,&[(0,1),(1,0)]));
    let mut missing = right.clone(); missing.origins[0]=None;
    assert!(!guaranteed_match(&left,&missing,&[(0,1),(1,0)]));
}

#[test]
fn same_source_match_proofs_reject_nullable_missing_or_mismatched_origins() {
    let left = analyze(&scan(None,true));
    let right = left.clone();
    assert!(same_rows(&left,&right,&[(0,0)]));
    assert!(guaranteed_match(&left,&right,&[(0,0)]));
    for pairs in [vec![],vec![(1,1)],vec![(0,1)],vec![(0,99)],vec![(99,0)]] {
        assert!(!same_rows(&left,&right,&pairs),"{pairs:?}");
    }
    let mut missing_left = left.clone(); missing_left.origins[0]=None;
    assert!(!same_rows(&missing_left,&right,&[(0,0)]));
    let mut missing_right = right.clone(); missing_right.origins[0]=None;
    assert!(!same_rows(&left,&missing_right,&[(0,0)]));
    let mut partial = right; partial.complete_sources.clear();
    assert!(!same_rows(&left,&partial,&[(0,0)]));
}

#[tokio::test]
async fn payload_restoration_preserves_selection_through_relational_wrappers() {
    let base = scan(Some(vec![0]),true);
    let plans = vec![
        LogicalPlanBuilder::from(base.clone()).alias("person").unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base.clone()).filter(col("people.id").eq(lit(1i64))).unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base.clone()).sort(vec![col("people.id").sort(false,true)]).unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base.clone()).limit(1,Some(1)).unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base.clone()).project(vec![col("people.id").alias("renamed")]).unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base.clone()).distinct_on(vec![col("people.id")],vec![col("people.id")],None).unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base.clone()).window(vec![datafusion::functions_window::expr_fn::row_number().alias("ordinal")]).unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base).project(vec![col("people.id"),lit(3i64).alias("__constraint_payload"),lit(4i64).alias("__constraint_payload_")]).unwrap().build().unwrap(),
    ];
    for (variant, plan) in plans.into_iter().enumerate() {
        let before = crate::ir::rel::execute_test_plan(plan.clone()).await.unwrap();
        let expected_ids = before.iter().flat_map(|b|b.column(0).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
        let (restored,index) = expose_occurrence_column(&plan,0,&origin("payload")).unwrap().unwrap();
        assert_eq!(occurrence_column(&restored,0,&origin("payload")),Some(index),"variant {variant}");
        for column in plan.schema().columns() { assert!(restored.schema().index_of_column(&column).is_ok(),"lost {column:?} in variant {variant}"); }
        if variant==7 { assert_eq!(restored.schema().field(index).name(),"__constraint_payload__"); }
        let after = crate::ir::rel::execute_test_plan(restored).await.unwrap();
        let ids = after.iter().flat_map(|b|b.column(0).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
        let payload = after.iter().flat_map(|b|b.column(index).as_any().downcast_ref::<StringArray>().unwrap().iter().map(|s|s.map(str::to_owned)).collect::<Vec<_>>()).collect::<Vec<_>>();
        assert_eq!(ids,expected_ids,"variant {variant}");
        assert_eq!(payload,ids.iter().map(|id|if *id==1 {Some("Ada".into())} else {None}).collect::<Vec<_>>(),"variant {variant}");
    }
}

#[tokio::test]
async fn payload_restoration_keeps_self_join_occurrences_separate() {
    let left = LogicalPlanBuilder::from(scan(Some(vec![0]),true)).alias("left_person").unwrap().build().unwrap();
    let right = LogicalPlanBuilder::from(scan(Some(vec![0]),true)).alias("right_person").unwrap().build().unwrap();
    let joined = LogicalPlanBuilder::from(left).join(right,JoinType::Inner,(vec!["left_person.id"],vec!["right_person.id"]),None).unwrap().build().unwrap();
    for key in [0,1] {
        let (restored,index) = expose_named_occurrence_column(&joined,key,&origin("payload"),"person_payload").unwrap().unwrap();
        let key_column = joined.schema().columns()[key].clone();
        let restored_key = restored.schema().index_of_column(&key_column).unwrap();
        assert_eq!(occurrence_column(&restored,restored_key,&origin("payload")),Some(index));
        let other_column = joined.schema().columns()[1-key].clone();
        let other_key = restored.schema().index_of_column(&other_column).unwrap();
        assert_eq!(occurrence_column(&restored,other_key,&origin("payload")),None);
        let batches = crate::ir::rel::execute_test_plan(restored).await.unwrap();
        let mut rows = batches.iter().flat_map(|b|{
            let ids=b.column(restored_key).as_any().downcast_ref::<Int64Array>().unwrap();
            let payload=b.column(index).as_any().downcast_ref::<StringArray>().unwrap();
            (0..b.num_rows()).map(|i|(ids.value(i),payload.iter().nth(i).unwrap().map(str::to_owned))).collect::<Vec<_>>()
        }).collect::<Vec<_>>();
        rows.sort();
        assert_eq!(rows,vec![(1,Some("Ada".into())),(2,None)]);
    }
}

#[test]
fn payload_restoration_respects_computed_keys_and_row_merging_barriers() {
    let base = scan(Some(vec![0]),true);
    let computed = LogicalPlanBuilder::from(base.clone()).project(vec![(col("people.id")+lit(1i64)).alias("computed")]).unwrap().build().unwrap();
    let distinct = LogicalPlanBuilder::from(base.clone()).distinct().unwrap().build().unwrap();
    let union = LogicalPlanBuilder::from(base.clone()).union(base.clone()).unwrap().build().unwrap();
    for plan in [computed,distinct,union] {
        assert!(expose_occurrence_column(&plan,0,&origin("payload")).unwrap().is_none());
    }
    let window = LogicalPlanBuilder::from(base.clone()).window(vec![datafusion::functions_window::expr_fn::row_number().alias("ordinal")]).unwrap().build().unwrap();
    assert!(expose_occurrence_column(&window,1,&origin("payload")).unwrap().is_none());
    let selected = LogicalPlanBuilder::from(base.clone()).distinct_on(vec![col("people.id")],vec![(col("people.id")+lit(1i64)).alias("computed")],None).unwrap().build().unwrap();
    assert!(expose_occurrence_column(&selected,0,&origin("payload")).unwrap().is_none());
    for kind in [JoinType::LeftSemi,JoinType::LeftAnti,JoinType::RightSemi,JoinType::RightAnti] {
        let left = LogicalPlanBuilder::from(base.clone()).alias("a").unwrap().build().unwrap();
        let right = LogicalPlanBuilder::from(base.clone()).alias("b").unwrap().build().unwrap();
        let join = LogicalPlanBuilder::from(left).join(right,kind,(vec!["a.id"],vec!["b.id"]),None).unwrap().build().unwrap();
        assert!(expose_occurrence_column(&join,0,&origin("payload")).unwrap().is_none());
    }
    let missing_origin = Origin{table:"missing".into(),column:"payload".into()};
    let wrappers = [
        LogicalPlanBuilder::from(base.clone()).project(vec![col("people.id")]).unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base.clone()).alias("person").unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base.clone()).distinct_on(vec![col("people.id")],vec![col("people.id")],None).unwrap().build().unwrap(),
        LogicalPlanBuilder::from(base).window(vec![datafusion::functions_window::expr_fn::row_number().alias("ordinal")]).unwrap().build().unwrap(),
    ];
    for plan in wrappers { assert!(expose_occurrence_column(&plan,0,&missing_origin).unwrap().is_none()); }
}

#[test]
fn equijoin_proofs_distinguish_residuals_null_awareness_and_lossy_casts() {
    use super::super::properties::equijoin_pairs;
    use datafusion::logical_expr::expr::Cast;
    let left=LogicalPlanBuilder::from(scan(None,true)).alias("l").unwrap().build().unwrap();
    let right=LogicalPlanBuilder::from(scan(None,true)).alias("r").unwrap().build().unwrap();
    let LogicalPlan::Join(mut join)=LogicalPlanBuilder::from(left).join(right,JoinType::Inner,(vec!["l.id"],vec!["r.id"]),None).unwrap().build().unwrap() else {panic!("join")};
    assert_eq!(equijoin_pairs(&join,false),Some(vec![(0,0)]));
    join.null_aware=true;assert!(equijoin_pairs(&join,true).is_none());join.null_aware=false;
    join.on=vec![(col("l.id"),col("r.payload"))];assert!(equijoin_pairs(&join,true).is_none());
    join.on=vec![(col("missing"),col("r.id"))];assert!(equijoin_pairs(&join,true).is_none());
    join.on.clear();
    join.filter=Some(col("r.id").eq(col("l.id")).and(col("l.payload").is_not_null()));
    assert_eq!(equijoin_pairs(&join,true),Some(vec![(0,0)]));assert!(equijoin_pairs(&join,false).is_none());
    join.filter=Some(col("r.id").eq(col("l.id")));
    assert_eq!(equijoin_pairs(&join,false),Some(vec![(0,0)]));
    join.filter=Some(col("l.id").eq(col("r.payload")));
    assert_eq!(equijoin_pairs(&join,true),Some(vec![]));assert!(equijoin_pairs(&join,false).is_none());
    join.filter=None;
    let cast=|name:&str|Expr::Cast(Cast::new(Box::new(col(name)),DataType::Int32));
    join.on=vec![(cast("l.id"),cast("r.id"))];assert!(equijoin_pairs(&join,true).is_none());
    join.on=vec![(col("l.id").alias("left_key"),col("r.id").alias("right_key"))];
    assert_eq!(equijoin_pairs(&join,false),Some(vec![(0,0)]));
}

#[test]
fn semi_and_anti_join_facts_follow_the_returned_side_and_mark_joins_are_proof_barriers() {
    let left=LogicalPlanBuilder::from(scan(None,true)).alias("l").unwrap().build().unwrap();
    let right=LogicalPlanBuilder::from(scan(None,true)).project(vec![col("people.payload"),col("people.id")]).unwrap().alias("r").unwrap().build().unwrap();
    for kind in [JoinType::LeftSemi,JoinType::LeftAnti,JoinType::RightSemi,JoinType::RightAnti,JoinType::LeftMark,JoinType::RightMark] {
        let plan=LogicalPlanBuilder::from(left.clone()).join(right.clone(),kind,(vec!["l.id"],vec!["r.id"]),None).unwrap().build().unwrap();
        let facts=analyze(&plan);
        assert!(facts.complete_sources.is_empty());assert!(!facts.removable);
        if matches!(kind,JoinType::LeftMark|JoinType::RightMark) {
            assert!(facts.unique_keys.is_empty());assert!(facts.non_null.is_empty());assert!(facts.origins.iter().all(Option::is_none));
        } else {
            let (source,key)=if matches!(kind,JoinType::RightSemi|JoinType::RightAnti) {(&right,1)} else {(&left,0)};
            assert_eq!(facts.origins,analyze(source).origins);assert!(facts.unique_on(&[key],true));
            assert!(facts.non_null.contains(&key));assert!(!facts.non_null.contains(&(1-key)));
        }
    }
}

#[test]
fn distinct_on_and_grouping_sets_do_not_invent_projected_keys() {
    use datafusion::logical_expr::GroupingSet;
    let source=scan(None,false);
    for (on,select,unique) in [
        (vec![col("people.id")],vec![col("people.payload")],false),
        (vec![col("people.id")+lit(1i64)],vec![col("people.payload")],false),
        (vec![col("people.id")],vec![col("people.payload"),col("people.id").alias("key")],true),
    ] {
        let plan=LogicalPlanBuilder::from(source.clone()).distinct_on(on,select,None).unwrap().build().unwrap();
        let facts=analyze(&plan);
        assert_eq!(facts.unique_on(&(0..plan.schema().fields().len()).collect::<Vec<_>>(),true),unique);
        assert!(facts.complete_sources.is_empty());assert!(!facts.removable);
    }
    let grouped=LogicalPlanBuilder::from(source).aggregate(vec![Expr::GroupingSet(GroupingSet::GroupingSets(vec![vec![col("people.id")],vec![]]))],Vec::<Expr>::new()).unwrap().build().unwrap();
    let facts=analyze(&grouped);
    assert!(facts.unique_keys.is_empty());assert!(facts.equality_keys.is_empty());assert!(!facts.removable);
}

#[tokio::test]
async fn grouping_reduction_avoids_internal_alias_collisions_and_preserves_null_payloads() {
    use datafusion::functions_aggregate::expr_fn::{count,sum};
    use datafusion::common::ScalarValue;
    let plan=LogicalPlanBuilder::from(scan(None,true)).aggregate(vec![col("people.id"),col("people.payload")],vec![
        count(lit(1i64)).alias("__constraint_group_1"),sum(col("people.id")).alias("total"),
    ]).unwrap().build().unwrap();
    let (rewritten,proofs)=optimize(plan.clone()).unwrap();
    assert!(proofs.iter().any(|proof|proof.rule=="reduce_grouping"));
    // Rewriting tightens functional dependencies; the external row schema stays identical.
    assert_eq!(rewritten.schema().as_arrow(),plan.schema().as_arrow());
    assert_eq!(rewritten.schema().columns(),plan.schema().columns());
    let batches=crate::ir::rel::execute_test_plan(rewritten).await.unwrap();
    let mut rows=batches.iter().flat_map(|b|(0..b.num_rows()).map(move |r|(0..b.num_columns()).map(|c|ScalarValue::try_from_array(b.column(c),r).unwrap()).collect::<Vec<_>>())).collect::<Vec<_>>();
    rows.sort_by(|a,b|a.partial_cmp(b).unwrap());
    assert_eq!(rows,vec![
        vec![ScalarValue::Int64(Some(1)),ScalarValue::Utf8(Some("Ada".into())),ScalarValue::Int64(Some(1)),ScalarValue::Int64(Some(1))],
        vec![ScalarValue::Int64(Some(2)),ScalarValue::Utf8(None),ScalarValue::Int64(Some(1)),ScalarValue::Int64(Some(2))],
    ]);
}

#[tokio::test]
async fn contained_union_handles_reversed_inputs_and_keeps_disjoint_selections() {
    let base=scan(None,false);
    let first=LogicalPlanBuilder::from(base.clone()).filter(col("people.id").eq(lit(1i64))).unwrap().build().unwrap();
    let second=LogicalPlanBuilder::from(base.clone()).filter(col("people.id").eq(lit(2i64))).unwrap().build().unwrap();
    for (left,right,contained) in [(base.clone(),first.clone(),true),(first.clone(),base,true),(first,second,false)] {
        let plan=LogicalPlanBuilder::from(left).union(right).unwrap().distinct().unwrap().build().unwrap();
        let (rewritten,proofs)=optimize(plan.clone()).unwrap();
        assert_eq!(proofs.iter().any(|p|p.rule=="contained_union"),contained);assert_eq!(rewritten.schema(),plan.schema());
        let batches=crate::ir::rel::execute_test_plan(rewritten).await.unwrap();
        let mut ids=batches.iter().flat_map(|b|b.column(0).as_any().downcast_ref::<Int64Array>().unwrap().values().to_vec()).collect::<Vec<_>>();
        ids.sort();assert_eq!(ids,vec![1,2]);
    }
}
