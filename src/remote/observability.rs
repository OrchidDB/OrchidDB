//! PromQL/LogQL access paths for ordinary mapped relations. Predicate pushdown
//! is a sound prefilter: retain the relational continuation for SQL null/type
//! semantics, joins, aggregates, ordering and limits. No HTTP during planning.
//! Prometheus table names identify metrics evaluated on an epoch-aligned 60s
//! grid, not raw scrape samples. `timestamp` is a microsecond timestamp, `value`
//! is float64, and other string columns are labels. Loki adds `line` and optional
//! JSON `labels`/`structured_metadata`. `sample_id` is a stable scalar source key.
//! Both require constant time bounds.
use super::*;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::logical_expr::LogicalPlan;

pub(super) struct ObservabilityAdapter(pub &'static str);
impl RequestAdapter for ObservabilityAdapter {
    fn name(&self) -> &str {
        self.0
    }
    fn lower(&self, plan: &LogicalPlan) -> Result<Option<PreparedOperation>> {
        let Some(s) = scan(plan)? else {
            return Ok(None);
        };
        let mut leaf = None;
        plan.apply(|p| {
            if let LogicalPlan::TableScan(t) = p {
                leaf = Some(t.clone());
            }
            Ok(datafusion::common::tree_node::TreeNodeRecursion::Continue)
        })
        .map_err(|e| e.to_string())?;
        let Some(leaf) = leaf else {
            return Ok(None);
        };
        for f in leaf.projected_schema.fields() {
            let valid = match f.name().as_str() {
                "timestamp" => matches!(
                    f.data_type(),
                    arrow::datatypes::DataType::Timestamp(
                        arrow::datatypes::TimeUnit::Microsecond,
                        _
                    )
                ),
                "value" if self.0 == "prometheus" => {
                    *f.data_type() == arrow::datatypes::DataType::Float64
                }
                "labels" | "structured_metadata" => true,
                _ => *f.data_type() == arrow::datatypes::DataType::Utf8,
            };
            if !valid {
                return Err(format!(
                    "unsupported {} column {}: {}",
                    self.0,
                    f.name(),
                    f.data_type()
                ));
            }
        }
        let (query, start, end) = match crate::observability::range_query(
            self.0,
            leaf.table_name.table(),
            &s.filters,
            None,
            None,
        ) {
            Ok(range) => range,
            Err(e)
                if e.contains("requires constant timestamp")
                    && !matches!(plan, LogicalPlan::TableScan(_)) =>
            {
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let schema = leaf.projected_schema.clone();
        // TableScan filters are part of the continuation too, including filters
        // that DataFusion previously moved into the scan. Restore them explicitly.
        let marker = crate::operations::RequestResult {
            schema: schema.clone(),
        }
        .into_plan();
        let mut replacement = LogicalPlanBuilder::from(marker);
        for predicate in &leaf.filters {
            replacement = replacement
                .filter(predicate.clone())
                .map_err(|e| e.to_string())?;
        }
        let replacement = replacement.build().map_err(|e| e.to_string())?;
        let continuation = plan
            .clone()
            .transform_up(|p| {
                if matches!(p, LogicalPlan::TableScan(_)) {
                    Ok(Transformed::yes(replacement.clone()))
                } else {
                    Ok(Transformed::no(p))
                }
            })
            .map_err(|e| e.to_string())?
            .data;
        let columns: Vec<_> = schema.fields().iter().map(|f| f.name().clone()).collect();
        Ok(Some(PreparedOperation {
            source: None,
            schema,
            template: RequestTemplate {
                adapter: self.0.into(),
                parameters: 0,
                bindings: vec![],
                request: json!({"version":1,"engine":self.0,"operation":"range",
                    "query":query,"start_ns":start,"end_ns":end,"step_seconds":60,"columns":columns}),
            },
            replacement: Some(continuation),
        }))
    }
}
