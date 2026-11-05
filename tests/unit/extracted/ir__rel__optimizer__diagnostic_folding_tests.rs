
    use super::*;
    #[tokio::test]
    async fn existing_session_observes_scoped_folding_mode() {
        use datafusion::{logical_expr::LogicalPlanBuilder, prelude::lit};
        let context = session();
        let plan = LogicalPlanBuilder::empty(true).project(vec![(lit(1_i64)+lit(2_i64)).alias("result")]).unwrap().build().unwrap();
        crate::ir::diagnostics::without_constant_folding(async {
            let optimized = context.state().optimize(&plan).unwrap();
            assert!(optimized.display_indent().to_string().contains("+"));
        }).await;
        assert!(!context.state().optimize(&plan).unwrap().display_indent().to_string().contains("+"));
    }

#[test]
fn bound_expression_simplification_keeps_names_and_substitutes_scoped_literals() {
    use datafusion::{logical_expr::{LogicalPlanBuilder,Expr},prelude::{col,lit},common::ScalarValue};
    let input=LogicalPlanBuilder::empty(true).project(vec![lit(7_i64).alias("x"),lit(ScalarValue::Int64(None)).alias("y")]).unwrap().alias("scope").unwrap().build().unwrap();
    let plan=LogicalPlanBuilder::from(input).project(vec![(col("scope.x")+lit(2_i64)).alias("answer"),col("scope.y").is_null().alias("missing")]).unwrap().build().unwrap();
    let names=plan.schema().clone();let optimized=simplify_bound_expressions(plan,&session().state()).unwrap();
    assert_eq!(optimized.schema(),&names);
    let LogicalPlan::Projection(projection)=optimized else {panic!("projection preserved")};
    let strip=|expr:Expr|if let Expr::Alias(alias)=expr {*alias.expr} else {expr};
    assert_eq!(strip(projection.expr[0].clone()),lit(9_i64));
    assert_eq!(strip(projection.expr[1].clone()),lit(true));
}

#[tokio::test]
async fn bound_expression_simplification_respects_diagnostic_mode_and_executes_equivalently() {
    use datafusion::{logical_expr::LogicalPlanBuilder,prelude::{col,lit},common::ScalarValue};
    let input=LogicalPlanBuilder::empty(true).project(vec![lit(7_i64).alias("x")]).unwrap().build().unwrap();
    let plan=LogicalPlanBuilder::from(input).filter(col("x").gt(lit(3_i64))).unwrap().project(vec![col("x")]).unwrap().build().unwrap();
    crate::ir::diagnostics::without_constant_folding(async {
        assert_eq!(simplify_bound_expressions(plan.clone(),&session().state()).unwrap(),plan);
    }).await;
    let optimized=simplify_bound_expressions(plan.clone(),&session().state()).unwrap();
    #[cfg(feature="duckdb")]
    for plan in [plan,optimized] {
        let batches=crate::ir::rel::execute_test_plan(plan).await.unwrap();
        assert_eq!(batches.iter().map(|b|b.num_rows()).sum::<usize>(),1);
        assert_eq!(ScalarValue::try_from_array(batches[0].column(0),0).unwrap(),ScalarValue::Int64(Some(7)));
    }
}
