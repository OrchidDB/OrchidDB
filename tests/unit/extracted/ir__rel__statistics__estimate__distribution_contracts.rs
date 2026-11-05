
    use super::*;
    use datafusion::logical_expr::{col, lit, expr::{Cast, TryCast, Case}};
    use arrow::datatypes::DataType;

    fn sample() -> ColumnStatistics {
        ColumnStatistics {
            observations: 10, nulls: 2, sample_distinct: 3,
            estimated_distinct: Some(3.0),
            frequent: vec![("1".into(), 4), ("2".into(), 3)],
            histogram: vec!["1".into(), "2".into(), "3".into(), "4".into()],
            average_width: 8.0, ..Default::default()
        }
    }
    fn close(actual: Option<f64>, expected: f64) {
        assert!((actual.expect("known estimate")-expected).abs()<1e-12, "{actual:?} != {expected}");
    }

#[test]
fn estimator_tracks_empty_and_singleton_union_branches_without_external_statistics() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for (left,right,expected) in [(false,false,0.0),(false,true,1.0),(true,false,1.0),(true,true,2.0)] {
        let branch=|one|LogicalPlanBuilder::empty(one).project(vec![lit(7_i64).alias("value")]).unwrap().build().unwrap();
        let union=LogicalPlanBuilder::from(branch(left)).union(branch(right)).unwrap().build().unwrap();
        let mut output=vec![];let state=run(&union,"0".into(),&mut output);
        close(state.rows,expected);close(state.bytes,0.0);close(state.input,0.0);close(state.work,0.0);
        assert!(output.is_empty(),"estimates without collected revisions must not be published");
        let limited=LogicalPlanBuilder::from(union).limit(0,Some(1)).unwrap().build().unwrap();
        close(run(&limited,"0".into(),&mut vec![]).rows,expected.min(1.0));
    }
}

#[test]
fn estimator_resolves_nested_field_statistics_and_rejects_dynamic_field_names() {
    use datafusion::functions::core::expr_fn::get_field;
    let columns=BTreeMap::from([("parent".into(),ColumnStatistics{list:Some(ListStatistics{
        elements:BTreeMap::from([("child".into(),sample())]),..Default::default()}),..Default::default()})]);
    let expression=get_field(col("parent"),"child");
    assert_eq!(column(&expression,&columns).unwrap().nulls,2);
    close(nonnull(&expression,&columns),0.8);
    assert!(column(&get_field(col("parent"),"missing"),&columns).is_none());
    assert!(column(&get_field(col("unknown"),"child"),&columns).is_none());
    let Expr::ScalarFunction(mut function)=expression else {panic!()};
    function.args[1]=col("dynamic_name");
    assert!(column(&Expr::ScalarFunction(function),&columns).is_none());
}

#[test]
fn estimator_case_nullability_respects_ordered_arms_and_null_safe_boolean_wrappers() {
    let columns=BTreeMap::from([("x".into(),sample())]);
    let case=Expr::Case(Case::new(None,vec![
        (Box::new(col("x").eq(lit(1))),Box::new(lit(ScalarValue::Int64(None)))),
        (Box::new(col("x").eq(lit(2))),Box::new(lit(9_i64)))],Some(Box::new(lit(1_i64)))));
    close(nonnull(&case,&columns),0.6);
    let wrapped=Expr::BinaryExpr(datafusion::logical_expr::expr::BinaryExpr::new(
        Box::new(col("x").eq(lit(1))),Operator::IsNotDistinctFrom,Box::new(lit(true))));
    close(selectivity(&wrapped,&columns),0.4);
    let unknown=Expr::Case(Case::new(None,vec![(Box::new(col("missing").eq(lit(1))),Box::new(lit(1)))],None));
    assert!(nonnull(&unknown,&columns).is_none());
    close(nonnull(&lit(ScalarValue::Utf8(None)),&columns),0.0);
    close(nonnull(&lit("present"),&columns),1.0);
}
    #[test]
    fn sampled_predicates_handle_nulls_symmetry_duplicates_and_unknowns() {
        let columns = BTreeMap::from([("x".into(), sample())]);
        for (expression, expected) in [
            (col("x").eq(lit(1)), 0.4), (lit(1).eq(col("x")), 0.4),
            (col("x").not_eq(lit(1)), 0.4),
            (col("x").lt(lit(3)), 0.5), (lit(3).gt(col("x")), 0.5),
            (col("x").lt_eq(lit(3)), 0.75), (lit(3).gt_eq(col("x")), 0.75),
            (col("x").gt(lit(3)), 0.25), (lit(3).lt(col("x")), 0.25),
            (col("x").gt_eq(lit(3)), 0.5), (lit(3).lt_eq(col("x")), 0.5),
            (col("x").is_null(), 0.2), (col("x").is_not_null(), 0.8),
            (col("x").eq(lit(1)).and(col("x").eq(lit(2))), 0.12),
            (col("x").eq(lit(1)).or(col("x").eq(lit(2))), 0.58),
            (!col("x").eq(lit(1)), 0.6),
            (col("x").in_list(vec![lit(1),lit(1),lit(2)],false),0.7),
            (col("x").in_list(vec![lit(1),lit(1),lit(2)],true),0.3),
            (Expr::IsTrue(Box::new(col("x").eq(lit(1)))),0.4),
            (lit(true),1.0), (lit(false),0.0),
        ] { close(selectivity(&expression,&columns),expected); }
        for expression in [col("missing").eq(lit(1)), col("x").eq(col("x")),
            col("x")+lit(1), col("x").in_list(vec![col("x")],false), col("x")] {
            assert!(selectivity(&expression,&columns).is_none(),"{expression}");
        }
        assert!(equality(&ColumnStatistics::default(),"1").is_none());
        assert!(equality(&sample(),"99").unwrap()>0.0);
        let fallback=ColumnStatistics {estimated_distinct:None,..sample()};
        assert!(equality(&fallback,"99").unwrap()>0.0);
    }

    #[test]
    fn projections_preserve_distributions_and_distinguish_null_literals() {
        let state=State { rows:Some(100.0), columns:BTreeMap::from([("x".into(),sample())]),..Default::default() };
        for expression in [col("x").alias("renamed"),
            Expr::Cast(Cast::new(Box::new(col("x")),DataType::Float64)),
            Expr::TryCast(TryCast::new(Box::new(col("x")),DataType::Float64))] {
            assert_eq!(column(&expression,&state.columns).unwrap().nulls,2);
        }
        for value in [ScalarValue::Utf8(Some("quoted\"value".into())),
            ScalarValue::LargeUtf8(Some("large".into())),ScalarValue::Utf8View(Some("view".into())),
            ScalarValue::Boolean(Some(true)),ScalarValue::Int64(Some(42)),ScalarValue::Int64(None)] {
            let stats=projected_column(&lit(value.clone()),&state).unwrap();
            assert_eq!(stats.nulls,u64::from(value.is_null()));
            assert_eq!(stats.sample_distinct,u64::from(!value.is_null()));
            assert_eq!(stats.frequent[0].1,1);
        }
        let case=Expr::Case(Case::new(None,vec![(Box::new(col("x").eq(lit(1))),Box::new(lit(1)))],None));
        close(nonnull(&case,&state.columns),0.4);
        let stats=projected_column(&case.alias("choice"),&state).unwrap();
        assert_eq!(stats.nulls,6000);
        assert_eq!(stats.estimated_distinct,Some(3.0));
        assert!(projected_column(&(col("x")+lit(1)),&state).is_none());
        assert!(nonnull(&col("missing"),&state.columns).is_none());
        close(nonnull(&lit(1).alias("one"),&state.columns),1.0);
        close(nonnull(&Expr::Cast(Cast::new(Box::new(lit(1)),DataType::Float64)),&state.columns),1.0);
    }

    #[test]
    fn correlated_tuple_filters_are_counted_once_and_unknowns_stay_unknown() {
        let group=GroupStatistics {columns:vec!["x".into(),"y".into()], observations:10,sample_distinct:2,
            frequent:vec![(vec!["1".into(),"2".into()],8),(vec!["2".into(),"3".into()],2)]};
        let mut state=State {rows:Some(100.0),groups:vec![group],..Default::default()};
        let condition=col("x").eq(lit(1)).and(lit(2).eq(col("y")));
        close(joint_selectivity(&condition,&state),0.8);
        filter(&mut state,&condition); filter(&mut state,&condition);
        close(state.rows,80.0);
        assert!(joint_selectivity(&col("x").eq(lit(1)),&state).is_none());
        assert!(joint_selectivity(&col("x").gt(lit(1)),&state).is_none());
        close(joint_selectivity(&col("x").eq(lit(99)).and(col("y").eq(lit(99))),&state),1.0/11.0);
        let mut state=State {rows:Some(100.0),columns:BTreeMap::from([("x".into(),sample())]),..Default::default()};
        filter(&mut state,&col("x").eq(lit(1)));
        filter(&mut state,&col("source.x").eq(lit(1)));
        close(state.rows,40.0);
        filter(&mut state,&col("unknown").eq(lit(1)));
        assert!(state.rows.is_none());
    }

    #[test]
    fn join_estimates_follow_skew_and_reordered_tuple_keys() {
        let single=State {columns:BTreeMap::from([("x".into(),ColumnStatistics {
            observations:10,sample_distinct:2,frequent:vec![("1".into(),8),("2".into(),2)],..Default::default()
        })]),..Default::default()};
        close(join_probability(&single,&single,&[(col("x"),col("x"))]),0.68);
        close(join_probability(&single,&single,&[]),1.0);
        assert!(join_probability(&single,&single,&[(lit(1),col("x"))]).is_none());
        let tuple=State {groups:vec![GroupStatistics {
            columns:vec!["x".into(),"y".into()],observations:10,sample_distinct:2,
            frequent:vec![(vec!["1".into(),"2".into()],8),(vec!["2".into(),"3".into()],2)]
        }],..Default::default()};
        close(join_probability(&tuple,&tuple,&[(col("y"),col("y")),(col("x"),col("x"))]),0.68);
        let empty=State {columns:BTreeMap::from([("x".into(),ColumnStatistics::default())]),..Default::default()};
        assert!(join_probability(&empty,&single,&[(col("x"),col("x"))]).is_none());
        close(add(Some(9e299),Some(9e299)),1e300);
        assert!(add(None,Some(1.0)).is_none());
    }

fn estimate_scan(name: &str, rows: f64) -> LogicalPlan {
    use std::sync::Arc;
    use arrow::datatypes::{Field, Schema};
    use datafusion::{datasource::{empty::EmptyTable, provider_as_source}, logical_expr::LogicalPlanBuilder};
    let uniform = ColumnStatistics { observations: 10, sample_distinct: 5,
        estimated_distinct: Some(5.0), average_width: 8.0, ..Default::default() };
    let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false),
        Field::new("y", DataType::Int64, false),
        Field::new("items", DataType::List(Arc::new(Field::new("item", DataType::Int64, true))), true)]));
    let source = SourceStatistics { estimated_rows: Some(rows), estimated_bytes: Some(rows * 24.0),
        columns: BTreeMap::from([("x".into(), uniform.clone()), ("y".into(), uniform),
            ("items".into(), ColumnStatistics {list: Some(ListStatistics {parents: 10, total_lengths: 30,
                ..Default::default()}), ..Default::default()})]),
        groups: vec![GroupStatistics {columns: vec!["x".into(), "y".into()], observations: 10,
            sample_distinct: 7, ..Default::default()}], ..Default::default() };
    let snapshot = Arc::new(StatisticsSnapshot {revision: format!("revision-{name}"),
        sources: BTreeMap::from([(name.into(), source)]), version: VERSION, mapping: String::new(),
        collected_at: String::new(), relationships: vec![], rdf: vec![], report: Default::default()});
    let provider = super::super::provider::StatisticsProvider::wrap(Arc::new(EmptyTable::new(schema)), snapshot, name.into());
    LogicalPlanBuilder::scan(name, provider_as_source(provider), None).unwrap().build().unwrap()
}

#[test]
fn collected_estimates_preserve_joint_distributions_through_renames_and_distinct() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let plan = LogicalPlanBuilder::from(estimate_scan("source", 100.0))
        .project(vec![col("x").alias("a"), col("y").alias("b")]).unwrap()
        .alias("renamed").unwrap().build().unwrap();
    let state = run(&plan, "0".into(), &mut vec![]);
    assert_eq!(state.groups[0].columns, vec!["a", "b"]);
    assert!(state.columns.contains_key("renamed.a"));
    let distinct = LogicalPlanBuilder::from(plan.clone()).distinct().unwrap().build().unwrap();
    close(estimate(&distinct).estimated_rows, 7.0);
    let grouped = LogicalPlanBuilder::from(plan).aggregate(vec![col("a")], Vec::<Expr>::new()).unwrap().build().unwrap();
    close(estimate(&grouped).estimated_rows, 5.0);
    let projected = LogicalPlanBuilder::from(estimate_scan("source", 100.0))
        .project(vec![col("x")]).unwrap().distinct().unwrap().build().unwrap();
    close(estimate(&projected).estimated_rows, 5.0);
    let report = explain(&projected);
    assert_eq!(report.last().unwrap().statistics_revisions, vec!["revision-source"]);
    close(report.last().unwrap().estimated_row_width, 8.0);
    close(report.last().unwrap().estimated_scan_bytes, 2400.0);
}

#[test]
fn collected_join_estimates_handle_reversed_keys_residuals_and_join_kinds() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for (kind, expected) in [(JoinType::Inner,20.0),(JoinType::Left,20.0),(JoinType::Right,20.0),
        (JoinType::Full,20.0),(JoinType::LeftSemi,10.0),(JoinType::LeftAnti,0.0)] {
        for reverse in [false,true] {
            let (a,b) = if reverse {(col("r.x"),col("l.x"))} else {(col("l.x"),col("r.x"))};
            let plan = LogicalPlanBuilder::from(estimate_scan("l",10.0))
                .join_on(estimate_scan("r",10.0),kind,vec![a.eq(b)]).unwrap().build().unwrap();
            let estimate = estimate(&plan);
            close(estimate.estimated_rows,expected);
            assert_eq!(estimate.statistics_revisions,vec!["revision-l","revision-r"]);
            close(estimate.estimated_scan_bytes,480.0);
        }
    }
    let plan = LogicalPlanBuilder::from(estimate_scan("l",10.0))
        .join_on(estimate_scan("r",10.0),JoinType::Inner,
            vec![col("l.x").eq(col("r.x")).and(col("l.y").eq(lit(1_i64)))])
        .unwrap().build().unwrap();
    close(estimate(&plan).estimated_rows,4.0);
}

#[test]
fn collected_expansion_uses_mean_length_and_global_aggregation_preserves_empty_row() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for rows in [0.0,10.0] {
        let plan = LogicalPlanBuilder::from(estimate_scan("source",rows))
            .unnest_column(datafusion::common::Column::from_name("items")).unwrap().build().unwrap();
        let result = estimate(&plan);
        close(result.estimated_rows,rows*3.0);
        close(result.estimated_expanded_rows,rows*3.0);
        close(result.estimated_work,rows*56.0);
        let aggregate = LogicalPlanBuilder::from(plan).aggregate(Vec::<Expr>::new(),
            vec![datafusion::functions_aggregate::expr_fn::count(lit(1_i64))]).unwrap().build().unwrap();
        close(estimate(&aggregate).estimated_rows,1.0);
    }
}

#[test]
fn access_cost_uses_joint_key_statistics_and_rejects_unknown_keys() {
    let plan = estimate_scan("source", 100.0);
    assert_eq!(source_access_cost(&plan, &["x".into()]), Some((100.0,2400.0,5.0,3200.0)));
    assert_eq!(source_access_cost(&plan, &["x".into(),"y".into()]), Some((100.0,2400.0,7.0,3200.0)));
    assert!(source_access_cost(&plan, &["missing".into()]).is_none());
    assert!(source_access_cost(&plan, &["y".into(),"x".into()]).is_none());
    assert!(source_access_cost(&plan, &[]).is_none());
    close(input_rows(&plan),100.0);
}

#[test]
fn distinct_on_and_unknown_grouping_keep_estimates_conservative() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let plan = LogicalPlanBuilder::from(estimate_scan("source",100.0))
        .distinct_on(vec![col("x")],vec![col("x"),col("y")],None).unwrap().build().unwrap();
    close(estimate(&plan).estimated_rows,5.0);
    let computed = LogicalPlanBuilder::from(estimate_scan("source",100.0))
        .distinct_on(vec![col("x")+lit(1_i64)],vec![col("x")],None).unwrap().build().unwrap();
    assert!(estimate(&computed).estimated_rows.is_none());
    let grouped = LogicalPlanBuilder::from(estimate_scan("source",100.0))
        .aggregate(vec![col("x")+lit(1_i64)],Vec::<Expr>::new()).unwrap().build().unwrap();
    assert!(estimate(&grouped).estimated_rows.is_none());
    for kind in [JoinType::Left,JoinType::Right,JoinType::Full,JoinType::LeftSemi,JoinType::LeftAnti] {
        let plan = LogicalPlanBuilder::from(estimate_scan("l",10.0))
            .join_on(estimate_scan("r",0.0),kind,vec![col("l.x").eq(col("r.x"))]).unwrap().build().unwrap();
        close(estimate(&plan).estimated_rows,if matches!(kind,JoinType::Left|JoinType::Full|JoinType::LeftAnti){10.0}else{0.0});
    }
}

#[test]
fn projected_constructor_statistics_preserve_ndv_width_and_nullability() {
    use datafusion::functions::{string::expr_fn::concat, encoding::expr_fn::encode};
    let state = State {rows:Some(12.0),columns:BTreeMap::from([("x".into(),sample()),("y".into(),sample())]),..Default::default()};
    let joined = concat(vec![col("x"),col("y")]);
    let statistics = projected_column(&joined,&state).unwrap();
    assert_eq!(statistics.estimated_distinct,Some(9.0));
    assert_eq!(statistics.nulls,0);
    assert_eq!(statistics.average_width,16.0);
    let encoded = encode(col("x"),lit("hex"));
    close(nonnull(&encoded,&state.columns),0.8);
    assert_eq!(projected_column(&encoded,&state).unwrap().estimated_distinct,Some(3.0));
    let cast = Expr::Cast(Cast::new(Box::new(lit(7_i64)),DataType::Utf8));
    assert_eq!(projected_column(&cast,&state).unwrap().frequent,vec![("7".into(),1)]);
    assert!(projected_column(&concat(vec![col("missing")]),&state).is_none());
    let unknown_rows = State {rows:None,..state};
    assert!(projected_column(&joined,&unknown_rows).is_none());
}
