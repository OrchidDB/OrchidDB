
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
    estimate_scan_with(name, rows, |_| {})
}

fn estimate_scan_with(name: &str, rows: f64, configure: impl FnOnce(&mut SourceStatistics)) -> LogicalPlan {
    use std::sync::Arc;
    use arrow::datatypes::{Field, Schema};
    use datafusion::{datasource::{empty::EmptyTable, provider_as_source}, logical_expr::LogicalPlanBuilder};
    let uniform = ColumnStatistics { observations: 10, sample_distinct: 5,
        estimated_distinct: Some(5.0), average_width: 8.0, ..Default::default() };
    let schema = Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, false),
        Field::new("y", DataType::Int64, false),
        Field::new("items", DataType::List(Arc::new(Field::new("item", DataType::Int64, true))), true)]));
    let mut source = SourceStatistics { estimated_rows: Some(rows), estimated_bytes: Some(rows * 24.0),
        columns: BTreeMap::from([("x".into(), uniform.clone()), ("y".into(), uniform),
            ("items".into(), ColumnStatistics {list: Some(ListStatistics {parents: 10, total_lengths: 30,
                ..Default::default()}), ..Default::default()})]),
        groups: vec![GroupStatistics {columns: vec!["x".into(), "y".into()], observations: 10,
            sample_distinct: 7, ..Default::default()}], ..Default::default() };
    configure(&mut source);
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

#[test]
fn incomplete_statistics_do_not_invent_expansion_or_access_costs() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for omit_column in [false,true] {
        let scan=estimate_scan_with("source",10.0,|source| {
            if omit_column {source.columns.remove("items");}
            else {source.columns.get_mut("items").unwrap().list=None;}
        });
        let plan=LogicalPlanBuilder::from(scan).unnest_column(datafusion::common::Column::from_name("items"))
            .unwrap().build().unwrap();
        let result=estimate(&plan);
        assert!(result.estimated_rows.is_none());
        assert!(result.estimated_expanded_rows.is_none());
        assert!(result.estimated_work.is_none());
    }
    for missing_rows in [false,true] {
        let scan=estimate_scan_with("source",10.0,|source| {
            if missing_rows {source.estimated_rows=None;} else {source.estimated_bytes=None;}
        });
        assert!(source_access_cost(&scan,&["x".into()]).is_none());
        let result=estimate(&scan);
        assert!(result.estimated_work.is_none());
        assert_eq!(result.estimated_input_rows.is_none(),missing_rows);
    }
}

#[test]
fn distinct_estimates_include_null_groups_and_sample_ndv_fallback() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for (nulls,expected) in [(0,5.0),(2,6.0)] {
        let scan=estimate_scan_with("source",100.0,|source| {
            let column=source.columns.get_mut("x").unwrap();
            column.nulls=nulls;column.estimated_distinct=None;
        });
        let plan=LogicalPlanBuilder::from(scan).project(vec![col("x")]).unwrap().distinct().unwrap().build().unwrap();
        close(estimate(&plan).estimated_rows,expected);
    }
}

#[test]
fn scan_predicates_and_residual_conjunctions_are_applied_once() {
    let mut scan=estimate_scan("source",100.0);
    let LogicalPlan::TableScan(table)=&mut scan else {panic!()};
    table.filters=vec![col("x").eq(lit(1_i64)),col("x").eq(lit(1_i64))];
    close(estimate(&scan).estimated_rows,20.0);
    let mut state=State {rows:Some(100.0),columns:BTreeMap::from([("x".into(),sample()),("y".into(),sample())]),..Default::default()};
    filter(&mut state,&col("x").eq(lit(1)).and(col("y").eq(lit(2))));
    close(state.rows,12.0);
    filter(&mut state,&col("x").eq(lit(1)).and(col("y").eq(lit(2))));
    close(state.rows,12.0);
    let mut unknown=State {rows:Some(100.0),..Default::default()};
    filter(&mut unknown,&col("x").eq(lit(1)).and(col("y").eq(lit(2))));
    assert!(unknown.rows.is_none());
}

#[test]
fn join_probability_tolerates_unmatched_and_null_sample_values() {
    let state=|values:Vec<(&str,u64)>|State {columns:BTreeMap::from([("x".into(),ColumnStatistics {
        observations:10,sample_distinct:2,estimated_distinct:Some(2.0),
        frequent:values.into_iter().map(|(v,n)|(v.to_owned(),n)).collect(),..Default::default()
    })]),..Default::default()};
    // Null equality cannot add a matching heavy-hitter contribution; unmatched
    // sampled values still leave an uncertain overlapping tail.
    let left=state(vec![("null",2),("1",4)]);
    let right=state(vec![("null",2),("2",4)]);
    close(join_probability(&left,&right,&[(col("x"),col("x"))]),0.5);
    let missing=State::default();
    assert!(join_probability(&left,&missing,&[(col("x"),col("x"))]).is_none());
    assert!(join_probability(&left,&right,&[(col("x"),col("x")),(col("y"),col("y"))]).is_none());
    let empty=state(vec![]);
    close(join_probability(&left,&empty,&[(col("x"),col("x"))]),0.5);
}

#[test]
fn filter_probability_keeps_uncollected_sources_and_unknown_predicates_unknown() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let scan=estimate_scan("source",100.0);
    close(filter_probability(&scan,&col("x").eq(lit(1_i64))),0.2);
    assert!(filter_probability(&scan,&col("missing").eq(lit(1_i64))).is_none());
    let empty=LogicalPlanBuilder::empty(false).build().unwrap();
    assert!(explain(&empty).is_empty());
    assert!(estimate(&empty).estimated_rows.is_none());
    assert!(filter_probability(&empty,&col("x").eq(lit(1_i64))).is_none());
    let no_observations=BTreeMap::from([("x".into(),ColumnStatistics::default())]);
    assert!(selectivity(&col("x").in_list(vec![lit(1)],false),&no_observations).is_none());
    let unknown=BTreeMap::new();
    assert!(selectivity(&col("x").is_null(),&unknown).is_none());
    assert!(selectivity(&col("x").in_list(vec![],false),&unknown).is_none());
}

#[test]
fn projected_cases_propagate_unknown_branch_and_else_nullability() {
    let columns=BTreeMap::from([("x".into(),sample())]);
    for (then,otherwise) in [(col("missing"),lit(1)),(lit(1),col("missing"))] {
        let expression=Expr::Case(Case::new(None,vec![(Box::new(col("x").eq(lit(1))),Box::new(then))],Some(Box::new(otherwise))));
        assert!(nonnull(&expression,&columns).is_none());
    }
    let direct=Expr::Case(Case::new(Some(Box::new(col("x"))),vec![(Box::new(lit(1)),Box::new(lit("one")))],None));
    assert!(nonnull(&direct,&columns).is_none());
    let projected=State {rows:Some(4.0),columns:BTreeMap::from([("x".into(),ColumnStatistics {estimated_distinct:None,..sample()})]),..Default::default()};
    let expression=Expr::Case(Case::new(None,vec![(Box::new(col("x").eq(lit(1))),Box::new(lit(1)))],None));
    assert_eq!(projected_column(&expression,&projected).unwrap().estimated_distinct,Some(3.0));
}

#[test]
fn estimated_joins_use_computed_key_fallback_without_claiming_unknown_join_kinds() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let cast=|name:&str|Expr::Cast(Cast::new(Box::new(col(name)),DataType::Int64));
    let plan=LogicalPlanBuilder::from(estimate_scan("l",10.0))
        .join_on(estimate_scan("r",10.0),JoinType::Inner,vec![cast("l.x").eq(cast("r.x"))])
        .unwrap().build().unwrap();
    close(estimate(&plan).estimated_rows,20.0);
    for kind in [JoinType::RightSemi,JoinType::RightAnti] {
        let plan=LogicalPlanBuilder::from(estimate_scan("l",10.0))
            .join_on(estimate_scan("r",10.0),kind,vec![col("l.x").eq(col("r.x"))])
            .unwrap().build().unwrap();
        assert!(estimate(&plan).estimated_rows.is_none());
    }
    let plan=LogicalPlanBuilder::from(estimate_scan("l",10.0))
        .join_on(estimate_scan("r",10.0),JoinType::Inner,vec![col("l.x").eq(col("l.y"))])
        .unwrap().build().unwrap();
    assert!(estimate(&plan).estimated_rows.is_none());
}

#[test]
fn cost_optimizer_orders_total_predicates_by_expected_comparisons() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let expensive=col("x").in_list(vec![lit(1_i64),lit(2_i64),lit(3_i64),lit(4_i64)],false);
    let selective=col("y").eq(lit(1_i64));
    for consecutive in [false,true] {
        let builder=LogicalPlanBuilder::from(estimate_scan("source",100.0));
        let plan=if consecutive {builder.filter(expensive.clone()).unwrap().filter(selective.clone()).unwrap()}
            else {builder.filter(expensive.clone().and(selective.clone())).unwrap()}.build().unwrap();
        let (optimized,decisions)=super::super::optimize::optimize(plan).unwrap();
        let LogicalPlan::Filter(residual) = &optimized else {panic!("residual filter must remain")};
        close(input_rows(&residual.input),16.0);
        let Expr::BinaryExpr(predicate) = &residual.predicate else {panic!("conjunction must remain")};
        assert_eq!(predicate.op,Operator::And);
        assert!(predicate.left.to_string().contains("y"));
        assert!(predicate.right.to_string().contains("IN"));
        let decision=decisions.iter().find(|d|d.optimization=="order_pure_filters");
        if consecutive {
            // DataFusion merges consecutive filters parent-first, so the
            // selective outer predicate is already first before costing.
            assert!(decision.is_none());
        } else {
            let decision=decision.expect("cost-based predicate ordering");
            // Inexact pushdown leaves 100 * 0.8 * 0.2 = 16 estimated rows.
            close(Some(decision.estimated_work_before),76.8);
            close(Some(decision.estimated_work_after),28.8);
            assert!(decision.after[0].contains("y"));
            assert!(decision.after[1].contains("IN"));
        }
        let (_,again)=super::super::optimize::optimize(optimized).unwrap();
        assert!(again.iter().all(|d|d.optimization!="order_pure_filters"));
    }
}

#[test]
fn cost_optimizer_preserves_error_boundaries_and_unknown_costs() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    for predicate in [
        (col("x")/lit(0_i64)).eq(lit(1_i64)).and(col("y").eq(lit(1_i64))),
        Expr::Cast(Cast::new(Box::new(col("x")),DataType::Int8)).eq(lit(1_i8)).and(col("y").eq(lit(1_i64))),
    ] {
        let plan=LogicalPlanBuilder::from(estimate_scan("source",100.0)).filter(predicate).unwrap().build().unwrap();
        let (_,decisions)=super::super::optimize::optimize(plan).unwrap();
        assert!(decisions.iter().all(|d|d.optimization!="order_pure_filters"));
    }
    for missing_rows in [false,true] {
        let scan=estimate_scan_with("source",100.0,|source| {
            if missing_rows {source.estimated_rows=None;} else {source.columns.clear();}
        });
        let plan=LogicalPlanBuilder::from(scan).filter(col("x").eq(lit(1_i64)).and(col("y").eq(lit(1_i64))))
            .unwrap().build().unwrap();
        let (_,decisions)=super::super::optimize::optimize(plan).unwrap();
        assert!(decisions.iter().all(|d|d.optimization!="order_pure_filters"));
    }
}

#[tokio::test]
async fn optimizer_exposes_constant_case_guards_without_global_folding() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    crate::ir::diagnostics::without_constant_folding(async {
        let expensive=col("x").in_list(vec![lit(1_i64),lit(2_i64),lit(3_i64),lit(4_i64)],false);
        let selective=col("y").eq(lit(1_i64));
        let body=expensive.and(selective);
        for guards in [vec![],vec![lit(false)],vec![lit(ScalarValue::Boolean(None))],vec![lit(false),lit(true)]] {
            let arms=guards.into_iter().map(|guard|(Box::new(guard),Box::new(body.clone()))).collect();
            let guarded=Expr::Case(Case::new(None,arms,Some(Box::new(body.clone()))));
            let plan=LogicalPlanBuilder::from(estimate_scan("source",100.0)).filter(guarded).unwrap().build().unwrap();
            let (optimized,decisions)=super::super::optimize::optimize(plan).unwrap();
            assert!(decisions.iter().any(|d|d.optimization=="order_pure_filters"));
            let LogicalPlan::Filter(filter)=optimized else {panic!("filter must remain")};
            let Expr::BinaryExpr(and)=filter.predicate else {panic!("constant CASE guard should be exposed")};
            assert_eq!(and.op,Operator::And);
            assert!(and.left.to_string().contains("y"));
            assert!(and.right.to_string().contains("IN"));
        }
    }).await;
}

#[tokio::test]
async fn optimizer_keeps_dynamic_and_null_case_guards_intact() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    crate::ir::diagnostics::without_constant_folding(async {
        let body=col("x").eq(lit(1_i64)).and(col("y").eq(lit(1_i64)));
        for expression in [
            Expr::Case(Case::new(None,vec![(Box::new(col("x").eq(lit(1_i64))),Box::new(body.clone()))],Some(Box::new(lit(false))))),
            Expr::Case(Case::new(None,vec![(Box::new(lit(false)),Box::new(body.clone()))],None)),
            Expr::Case(Case::new(Some(Box::new(col("x"))),vec![(Box::new(lit(1_i64)),Box::new(body))],Some(Box::new(lit(false))))),
        ] {
            let plan=LogicalPlanBuilder::from(estimate_scan("source",100.0)).filter(expression).unwrap().build().unwrap();
            let (optimized,decisions)=super::super::optimize::optimize(plan).unwrap();
            assert!(decisions.iter().all(|d|d.optimization!="order_pure_filters"));
            let LogicalPlan::Filter(filter)=optimized else {panic!("guard must remain")};
            assert!(matches!(filter.predicate,Expr::Case(_)));
        }
    }).await;
}

#[test]
fn optimizer_handles_null_predicates_boolean_wrappers_and_no_statistics() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let empty=LogicalPlanBuilder::empty(false).build().unwrap();
    let (unchanged,decisions)=super::super::optimize::optimize(empty.clone()).unwrap();
    assert_eq!(unchanged,empty);assert!(decisions.is_empty());
    for predicate in [col("y").is_null(),col("y").is_not_null(),
        Expr::IsTrue(Box::new(col("y").eq(lit(1_i64))))] {
        let expensive=col("x").in_list(vec![lit(1_i64),lit(2_i64),lit(3_i64),lit(4_i64)],false);
        let plan=LogicalPlanBuilder::from(estimate_scan("source",100.0)).filter(expensive.and(predicate)).unwrap().build().unwrap();
        let (optimized,decisions)=super::super::optimize::optimize(plan).unwrap();
        assert_eq!(optimized.schema().fields().len(),3);
        assert!(decisions.iter().all(|d|d.estimated_work_after<d.estimated_work_before));
    }
}

#[test]
fn constant_case_selectivity_obeys_first_true_guard_and_unknown_boundaries() {
    let columns=BTreeMap::from([("x".into(),sample())]);
    let unknown=col("missing").eq(lit(1));
    let case=|arms:Vec<(Expr,Expr)>,other:Option<Expr>|Expr::Case(Case::new(None,
        arms.into_iter().map(|(a,b)|(Box::new(a),Box::new(b))).collect(),other.map(Box::new)));
    close(selectivity(&case(vec![(lit(false),unknown.clone()),(lit(ScalarValue::Boolean(None)),unknown.clone()),
        (lit(true),col("x").eq(lit(1))),(lit(true),unknown.clone())],Some(unknown.clone())),&columns),0.4);
    close(selectivity(&case(vec![(lit(false),unknown.clone())],None),&columns),0.0);
    close(selectivity(&case(vec![(lit(false),unknown.clone())],Some(lit(true))),&columns),1.0);
    assert!(selectivity(&case(vec![(col("x").eq(lit(1)),lit(true))],Some(lit(false))),&columns).is_none());
    assert!(selectivity(&case(vec![(lit(true),unknown.clone())],Some(lit(false))),&columns).is_none());
    assert!(selectivity(&case(vec![(lit(false),lit(true))],Some(unknown)),&columns).is_none());
}
