
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
