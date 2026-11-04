
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
