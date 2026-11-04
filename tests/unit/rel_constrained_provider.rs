use super::*;
use datafusion::logical_expr::{LogicalPlanBuilder, lit};

#[derive(Debug)]
struct View {
    plan: LogicalPlan,
    default: Expr,
    fail_filters: bool,
}
impl TableProvider for View {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        Arc::new(self.plan.schema().as_arrow().clone())
    }
    fn table_type(&self) -> TableType {
        TableType::View
    }
    fn get_logical_plan(&self) -> Option<Cow<'_, LogicalPlan>> {
        Some(Cow::Borrowed(&self.plan))
    }
    fn get_column_default(&self, name: &str) -> Option<&Expr> {
        (name == "id").then_some(&self.default)
    }
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> Result<Vec<TableProviderFilterPushDown>> {
        if self.fail_filters {
            return Err(datafusion::common::DataFusionError::Plan(
                "filter capability unavailable".into(),
            ));
        }
        Ok(vec![TableProviderFilterPushDown::Exact; filters.len()])
    }
}

#[test]
fn constrained_views_keep_their_boundary_and_delegate_source_defaults_and_filters() {
    let catalog: ConstraintCatalog = serde_json::from_value(serde_json::json!({"tables":{"source":[
        {"name":"known_id","fact":{"kind":"non_null","columns":["id"]},"evidence":{"kind":"enforced"}}
    ]}})).unwrap();
    for fail_filters in [false, true] {
        let inner: Arc<dyn TableProvider> = Arc::new(View {
            plan: LogicalPlanBuilder::empty(true)
                .project(vec![lit(7_i64).alias("id")])
                .unwrap()
                .build()
                .unwrap(),
            default: lit(7_i64),
            fail_filters,
        });
        assert!(inner.get_logical_plan().is_some());
        let unconstrained = bind("other", inner.clone(), &catalog, None).unwrap();
        assert!(Arc::ptr_eq(&inner, &unconstrained));
        let wrapped = bind("source", inner.clone(), &catalog, None).unwrap();
        assert!(wrapped.as_any().is::<ConstrainedProvider>());
        assert_eq!(wrapped.schema(), inner.schema());
        assert_eq!(wrapped.table_type(), TableType::View);
        assert!(
            wrapped.get_logical_plan().is_none(),
            "constraint facts must survive view inlining"
        );
        assert_eq!(wrapped.get_column_default("id"), Some(&lit(7_i64)));
        assert_eq!(wrapped.get_column_default("missing"), None);
        assert!(
            wrapped.constraints().unwrap().is_empty(),
            "source proofs must not become weaker generic uniqueness facts"
        );
        let concrete = wrapped
            .as_any()
            .downcast_ref::<ConstrainedProvider>()
            .unwrap();
        assert!(format!("{concrete:?}").contains("ConstrainedProvider"));
        let predicate = lit(true);
        let filters = wrapped.supports_filters_pushdown(&[&predicate, &predicate]);
        if fail_filters {
            assert!(
                filters
                    .unwrap_err()
                    .to_string()
                    .contains("filter capability unavailable")
            );
        } else {
            assert_eq!(
                filters.unwrap(),
                vec![TableProviderFilterPushDown::Exact; 2]
            );
        }
    }
}
