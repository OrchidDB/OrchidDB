//! Equivalent physical layouts of one versioned logical row set.
//!
//! Metadata is supplied by the caller (for Iceberg, from manifests). Selection
//! never removes a query predicate and never reads data to discover statistics.
use arrow::datatypes::SchemaRef;
use datafusion::{
    catalog::{Session, TableProvider},
    common::{
        DataFusionError, Result, ScalarValue,
        tree_node::{Transformed, TreeNodeRecursion},
    },
    datasource::{provider_as_source, source_as_provider},
    logical_expr::{
        Expr, LogicalPlan, LogicalPlanBuilder, Operator, TableProviderFilterPushDown, TableType,
    },
    physical_plan::ExecutionPlan,
};
use serde::{Deserialize, Serialize};
use std::{any::Any, collections::BTreeMap, sync::Arc};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LogicalSource {
    pub name: String,
    /// Caller assertion: candidates with this generation contain the same rows,
    /// values and duplicate multiplicities. This is not an Iceberg snapshot ID.
    #[serde(default)]
    pub generation: String,
    pub default_table: String,
    pub layouts: Vec<TableLayout>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableLayout {
    pub table: String,
    #[serde(default)]
    pub generation: String,
    /// Identifies the physical snapshot used to obtain this metadata.
    #[serde(default)]
    pub snapshot: String,
    #[serde(default)]
    pub specs: Vec<PartitionSpec>,
    /// None means unknown, whereas an empty list means a known empty table.
    pub partitions: Option<Vec<PartitionStatistics>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionSpec {
    pub spec_id: i32,
    pub fields: Vec<PartitionField>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionField {
    pub source_column: String,
    pub source_id: i32,
    pub field_id: i32,
    pub transform: PartitionTransform,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PartitionTransform {
    Identity,
    Bucket { buckets: u32 },
    Truncate { width: u32 },
    Year,
    Month,
    Day,
    Hour,
    Void,
    Unknown { name: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartitionStatistics {
    pub spec_id: i32,
    pub bytes: u64,
    pub files: u64,
    #[serde(default)]
    pub delete_bytes: u64,
    #[serde(default)]
    pub rows: Option<u64>,
    /// Transformed partition values keyed by partition field ID; null is JSON null.
    #[serde(default)]
    pub values: BTreeMap<i32, Option<String>>,
    /// Inclusive bounds in SOURCE column coordinates, not transformed partition
    /// coordinates. Missing bounds mean unknown. Adapters must include all files.
    #[serde(default)]
    pub bounds: BTreeMap<String, ColumnBounds>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColumnBounds {
    /// Scalar text parsed using the canonical Arrow column type.
    pub min: Option<String>,
    pub max: Option<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LayoutDecision {
    pub source: String,
    pub table: String,
    pub generation: String,
    pub snapshot: String,
    pub estimated_bytes: Option<u64>,
    pub estimated_files: Option<u64>,
    pub estimated_rows: Option<u64>,
    pub partition_specs: Vec<PartitionSpec>,
    pub candidates: Vec<LayoutCandidate>,
}
#[derive(Debug, Clone, Serialize)]
pub struct LayoutCandidate {
    pub table: String,
    pub reason: String,
    pub estimated_bytes: Option<u64>,
    pub estimated_files: Option<u64>,
}
#[derive(Debug)]
pub(crate) struct LayoutProvider {
    pub definition: LogicalSource,
    providers: BTreeMap<String, Arc<dyn TableProvider>>,
    schema: SchemaRef,
}
fn invalid(message: impl Into<String>) -> DataFusionError {
    DataFusionError::Plan(message.into())
}
impl LayoutProvider {
    /// Attach cost metadata to one physical scan without introducing a public alias.
    pub(crate) fn for_statistics(layout: TableLayout, providers: &BTreeMap<String, Arc<dyn TableProvider>>) -> Result<Self> {
        let name = layout.table.clone();
        let mut provider = Self::try_new(LogicalSource {
            name: format!("__scan_statistics_{name}"), generation: layout.generation.clone(),
            default_table: name.clone(), layouts: vec![layout],
        }, providers)?;
        provider.definition.name = name;
        Ok(provider)
    }

    pub(crate) fn try_new(
        definition: LogicalSource,
        providers: &BTreeMap<String, Arc<dyn TableProvider>>,
    ) -> Result<Self> {
        if definition.name.is_empty() || definition.layouts.is_empty() {
            return Err(invalid("logical source requires a name and layouts"));
        }
        let default = definition
            .layouts
            .iter()
            .find(|l| l.table == definition.default_table)
            .ok_or_else(|| invalid("logical source default_table must name a layout"))?;
        if default.generation != definition.generation {
            return Err(invalid("default layout has a stale generation"));
        }
        let schema = providers
            .get(&default.table)
            .ok_or_else(|| invalid(format!("unregistered layout {}", default.table)))?
            .schema();
        let mut selected = BTreeMap::new();
        for layout in &definition.layouts {
            if layout.table == definition.name || selected.contains_key(&layout.table) {
                return Err(invalid("duplicate or recursive layout"));
            }
            let provider = providers
                .get(&layout.table)
                .ok_or_else(|| invalid(format!("unregistered layout {}", layout.table)))?;
            if provider.as_any().is::<Self>() || provider.schema().fields() != schema.fields() {
                return Err(invalid(format!(
                    "layout {} must have the canonical column names, order, types and nullability",
                    layout.table
                )));
            }
            let mut specs = std::collections::BTreeSet::new();
            for spec in &layout.specs {
                if !specs.insert(spec.spec_id) {
                    return Err(invalid("duplicate partition spec ID"));
                }
                let mut fields = std::collections::BTreeSet::new();
                for field in &spec.fields {
                    schema.field_with_name(&field.source_column)?;
                    if !fields.insert(field.field_id) {
                        return Err(invalid("duplicate partition field ID"));
                    }
                    if matches!(
                        field.transform,
                        PartitionTransform::Bucket { buckets: 0 }
                            | PartitionTransform::Truncate { width: 0 }
                    ) {
                        return Err(invalid("partition transform argument must be positive"));
                    }
                }
            }
            for partition in layout.partitions.iter().flatten() {
                if !specs.contains(&partition.spec_id) {
                    return Err(invalid("partition references an unknown spec ID"));
                }
                for (column, bounds) in &partition.bounds {
                    let kind = schema.field_with_name(column)?.data_type();
                    let min = bounds
                        .min
                        .as_ref()
                        .map(|v| ScalarValue::try_from_string(v.clone(), kind))
                        .transpose()?;
                    let max = bounds
                        .max
                        .as_ref()
                        .map(|v| ScalarValue::try_from_string(v.clone(), kind))
                        .transpose()?;
                    if let (Some(min), Some(max)) = (min, max) {
                        if min.partial_cmp(&max).is_none_or(|c| c.is_gt()) {
                            return Err(invalid("invalid partition bounds"));
                        }
                    }
                }
            }
            selected.insert(layout.table.clone(), provider.clone());
        }
        Ok(Self {
            definition,
            providers: selected,
            schema,
        })
    }
    pub(crate) fn choose(&self, filters: &[Expr]) -> (&TableLayout, LayoutDecision) {
        let estimates: Vec<_> = self
            .definition
            .layouts
            .iter()
            .map(|layout| {
                if layout.generation != self.definition.generation {
                    return None;
                }
                layout.partitions.as_ref().map(|partitions| {
                    partitions
                        .iter()
                        .filter(|p| {
                            !filters.iter().any(|f| {
                                impossible(
                                    f,
                                    p,
                                    &self.schema,
                                    layout.specs.iter().find(|s| s.spec_id == p.spec_id),
                                )
                            })
                        })
                        .fold((0_u64, 0_u64), |(b, f), p| {
                            (
                                b.saturating_add(p.bytes).saturating_add(p.delete_bytes),
                                f.saturating_add(p.files),
                            )
                        })
                }).or_else(|| {
                    let stats = super::statistics::statistics_provider(self.providers.get(&layout.table)?)?;
                    stats.source.estimated_bytes.map(|b| (b.ceil() as u64, 0))
                })
            })
            .collect();
        // File counts are comparable only when every eligible candidate supplies them.
        let compare_files = self.definition.layouts.iter().all(|l| l.partitions.is_some());
        let default = self
            .definition
            .layouts
            .iter()
            .position(|l| l.table == self.definition.default_table)
            .unwrap();
        // Unknown default cost cannot establish an improvement over the default.
        let mut best = default;
        if estimates[default].is_some() {
            for (i, estimate) in estimates.iter().enumerate() {
                if let (Some((b, f)), Some((bb, bf))) = (estimate, estimates[best]) {
                    let cost = u128::from(*b) + u128::from(*f) * if compare_files { 64 * 1024 } else { 0 };
                    let best_cost = u128::from(bb) + u128::from(bf) * 64 * 1024;
                    if cost < best_cost {
                        best = i;
                    }
                }
            }
        }
        let layout = &self.definition.layouts[best];
        let candidates = self
            .definition
            .layouts
            .iter()
            .zip(&estimates)
            .map(|(l, e)| LayoutCandidate {
                table: l.table.clone(),
                reason: if l.generation != self.definition.generation {
                    "stale generation"
                } else if e.is_none() {
                    "unknown statistics"
                } else if l.partitions.is_none() {
                    "collected source estimate; partition/file costs unavailable"
                } else {
                    "eligible"
                }
                .into(),
                estimated_bytes: e.map(|e| e.0),
                estimated_files: e.filter(|_| l.partitions.is_some()).map(|e| e.1),
            })
            .collect();
        (
            layout,
            LayoutDecision {
                source: self.definition.name.clone(),
                table: layout.table.clone(),
                generation: self.definition.generation.clone(),
                snapshot: layout.snapshot.clone(),
                estimated_bytes: estimates[best].map(|e| e.0),
                estimated_files: estimates[best].filter(|_| layout.partitions.is_some()).map(|e| e.1),
                estimated_rows: layout.partitions.as_ref().and_then(|parts| parts.iter().filter(|p| !filters.iter().any(|f| impossible(f,p,&self.schema,layout.specs.iter().find(|s|s.spec_id==p.spec_id))))
                    .try_fold(0_u64,|n,p|p.rows.map(|r|n.saturating_add(r)))).or_else(|| super::statistics::statistics_provider(self.providers.get(&layout.table)?).and_then(|s|s.source.estimated_rows.map(|n|n.ceil() as u64))),
                partition_specs: layout.specs.clone(),
                candidates,
            },
        )
    }
}
// Only prove exclusion for direct comparisons in source coordinates. Unknown
// expressions, casts, nulls, transform semantics and parameter values retain all
// potentially matching partitions. These estimates never affect result rows.
fn impossible(
    expr: &Expr,
    partition: &PartitionStatistics,
    schema: &SchemaRef,
    spec: Option<&PartitionSpec>,
) -> bool {
    match expr {
        // SPARQL lowers effective boolean values to IS TRUE. In a filter,
        // this has the same surviving rows as the inner predicate.
        Expr::IsTrue(inner) => impossible(inner, partition, schema, spec),
        Expr::BinaryExpr(b) if b.op == Operator::And => {
            impossible(&b.left, partition, schema, spec)
                || impossible(&b.right, partition, schema, spec)
        }
        Expr::BinaryExpr(b) if b.op == Operator::Or => {
            impossible(&b.left, partition, schema, spec)
                && impossible(&b.right, partition, schema, spec)
        }
        Expr::BinaryExpr(b) => {
            let (column, value, op) = match (b.left.as_ref(), b.right.as_ref()) {
                (Expr::Column(c), Expr::Literal(v, _)) => (c, v, b.op),
                (Expr::Literal(v, _), Expr::Column(c)) => (
                    c,
                    v,
                    match b.op {
                        Operator::Lt => Operator::Gt,
                        Operator::LtEq => Operator::GtEq,
                        Operator::Gt => Operator::Lt,
                        Operator::GtEq => Operator::LtEq,
                        op => op,
                    },
                ),
                _ => return false,
            };
            if value.is_null() {
                return false;
            }
            if let Some(spec) = spec {
                for field in spec
                    .fields
                    .iter()
                    .filter(|f| f.source_column == column.name)
                {
                    if let (Some(Some(text)), Some(boundary)) = (
                        partition.values.get(&field.field_id),
                        transform(value, &field.transform),
                    ) {
                        if let Ok(actual) =
                            ScalarValue::try_from_string(text.clone(), &boundary.data_type())
                        {
                            if let Some(order) = actual.partial_cmp(&boundary) {
                                let identity =
                                    matches!(field.transform, PartitionTransform::Identity);
                                let excluded = match op {
                                    Operator::Eq => !order.is_eq(),
                                    Operator::Lt if identity => order.is_ge(),
                                    Operator::Gt if identity => order.is_le(),
                                    Operator::Lt | Operator::LtEq
                                        if !matches!(
                                            field.transform,
                                            PartitionTransform::Bucket { .. }
                                        ) =>
                                    {
                                        order.is_gt()
                                    }
                                    Operator::Gt | Operator::GtEq
                                        if !matches!(
                                            field.transform,
                                            PartitionTransform::Bucket { .. }
                                        ) =>
                                    {
                                        order.is_lt()
                                    }
                                    _ => false,
                                };
                                if excluded {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
            let Some(bounds) = partition.bounds.get(&column.name) else {
                return false;
            };
            let Ok(field) = schema.field_with_name(&column.name) else {
                return false;
            };
            if value.data_type() != *field.data_type() {
                return false;
            }
            let min = bounds
                .min
                .as_ref()
                .and_then(|v| ScalarValue::try_from_string(v.clone(), field.data_type()).ok());
            let max = bounds
                .max
                .as_ref()
                .and_then(|v| ScalarValue::try_from_string(v.clone(), field.data_type()).ok());
            let lower = min.as_ref().and_then(|v| v.partial_cmp(value));
            let upper = max.as_ref().and_then(|v| v.partial_cmp(value));
            match op {
                Operator::Eq => {
                    lower.is_some_and(|c| c.is_gt()) || upper.is_some_and(|c| c.is_lt())
                }
                Operator::Lt => lower.is_some_and(|c| c.is_ge()),
                Operator::LtEq => lower.is_some_and(|c| c.is_gt()),
                Operator::Gt => upper.is_some_and(|c| c.is_le()),
                Operator::GtEq => upper.is_some_and(|c| c.is_lt()),
                _ => false,
            }
        }
        Expr::InList(list) if !list.negated => list.list.iter().all(|v| {
            impossible(
                &list.expr.as_ref().clone().eq(v.clone()),
                partition,
                schema,
                spec,
            )
        }),
        _ => false,
    }
}
// Iceberg transform coordinates: temporal values are offsets from 1970;
// buckets use Murmur3 x86 32-bit seed zero with the sign bit cleared.
fn transform(value: &ScalarValue, transform: &PartitionTransform) -> Option<ScalarValue> {
    use PartitionTransform::*;
    use ScalarValue as S;
    use chrono::Datelike;
    if value.is_null() {
        return None;
    }
    match transform {
        Identity => Some(value.clone()),
        Bucket { buckets } if *buckets > 0 => {
            let bytes = match value {
                S::Int32(Some(v)) => i64::from(*v).to_le_bytes().to_vec(),
                S::Date32(Some(v)) => i64::from(*v).to_le_bytes().to_vec(),
                S::Int64(Some(v)) | S::TimestampMicrosecond(Some(v), _) => v.to_le_bytes().to_vec(),
                S::TimestampNanosecond(Some(v), _) => v.div_euclid(1000).to_le_bytes().to_vec(),
                S::TimestampMillisecond(Some(v), _) => v.checked_mul(1000)?.to_le_bytes().to_vec(),
                S::TimestampSecond(Some(v), _) => v.checked_mul(1_000_000)?.to_le_bytes().to_vec(),
                S::Utf8(Some(v)) | S::LargeUtf8(Some(v)) => v.as_bytes().to_vec(),
                S::Binary(Some(v)) | S::LargeBinary(Some(v)) => v.clone(),
                _ => return None,
            };
            Some(S::Int32(Some(
                ((murmur3(&bytes) & 0x7fff_ffff) % buckets) as i32,
            )))
        }
        Truncate { width } if *width > 0 => match value {
            S::Int32(Some(v)) => Some(S::Int32(Some(
                i64::from(*v)
                    .div_euclid(i64::from(*width))
                    .checked_mul(i64::from(*width))?
                    .try_into()
                    .ok()?,
            ))),
            S::Int64(Some(v)) => Some(S::Int64(Some(
                v.div_euclid(i64::from(*width))
                    .checked_mul(i64::from(*width))?,
            ))),
            S::Utf8(Some(v)) => Some(S::Utf8(Some(v.chars().take(*width as usize).collect()))),
            _ => None,
        },
        Year | Month | Day | Hour => {
            let seconds = match value {
                S::Date32(Some(v)) => i64::from(*v) * 86400,
                S::TimestampSecond(Some(v), _) => *v,
                S::TimestampMillisecond(Some(v), _) => v.div_euclid(1000),
                S::TimestampMicrosecond(Some(v), _) => v.div_euclid(1_000_000),
                S::TimestampNanosecond(Some(v), _) => v.div_euclid(1_000_000_000),
                _ => return None,
            };
            let result = match transform {
                Day => seconds.div_euclid(86400).try_into().ok()?,
                Hour => seconds.div_euclid(3600).try_into().ok()?,
                Year | Month => {
                    let date = chrono::DateTime::from_timestamp(seconds, 0)?;
                    if matches!(transform, Year) {
                        date.year() - 1970
                    } else {
                        (date.year() - 1970) * 12 + date.month0() as i32
                    }
                }
                _ => unreachable!(),
            };
            Some(S::Int32(Some(result)))
        }
        _ => None,
    }
}
fn murmur3(bytes: &[u8]) -> u32 {
    fn mix(mut k: u32) -> u32 {
        k = k.wrapping_mul(0xcc9e2d51);
        k = k.rotate_left(15);
        k.wrapping_mul(0x1b873593)
    }
    let mut hash = 0_u32;
    let mut chunks = bytes.chunks_exact(4);
    for chunk in &mut chunks {
        hash ^= mix(u32::from_le_bytes(chunk.try_into().unwrap()));
        hash = hash
            .rotate_left(13)
            .wrapping_mul(5)
            .wrapping_add(0xe6546b64);
    }
    let tail = chunks
        .remainder()
        .iter()
        .enumerate()
        .fold(0, |n, (i, b)| n | u32::from(*b) << (8 * i));
    if !chunks.remainder().is_empty() {
        hash ^= mix(tail);
    }
    hash ^= bytes.len() as u32;
    hash ^= hash >> 16;
    hash = hash.wrapping_mul(0x85ebca6b);
    hash ^= hash >> 13;
    hash = hash.wrapping_mul(0xc2b2ae35);
    hash ^ (hash >> 16)
}

#[async_trait::async_trait]
impl TableProvider for LayoutProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    fn table_type(&self) -> TableType {
        TableType::Base
    }
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> Result<Vec<TableProviderFilterPushDown>> {
        // Retain every original predicate above the selected physical scan.
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }
    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        // The shared pass resolves providers before execution. Fail closed if a
        // caller bypasses it; forwarding filters to arbitrary providers is unsafe.
        let _ = (state, projection, filters, limit);
        Err(invalid(
            "logical source must be resolved with layout::select before execution",
        ))
    }
}
pub(crate) fn layout_provider(provider: &Arc<dyn TableProvider>) -> Option<&LayoutProvider> {
    if let Some(p) = provider.as_any().downcast_ref::<LayoutProvider>() {
        return Some(p);
    }
    if let Some(p) = provider
        .as_any()
        .downcast_ref::<super::constraints::ConstrainedProvider>()
    {
        return layout_provider(&p.inner);
    }
    None
}
pub(crate) fn push_filters(plan: LogicalPlan) -> Result<LogicalPlan> {
    use datafusion::optimizer::{
        Optimizer, OptimizerContext, push_down_filter::PushDownFilter,
        simplify_expressions::SimplifyExpressions,
    };
    Optimizer::with_rules(vec![
        Arc::new(SimplifyExpressions::new()),
        Arc::new(PushDownFilter::new()),
    ])
    .optimize(
        plan,
        &OptimizerContext::new().with_skip_failing_rules(false),
        |_, _| {},
    )
}
/// Resolve each scan independently, including scans inside expression subqueries.
/// Filter pushdown runs only on plans containing logical sources.
pub fn select(plan: LogicalPlan) -> Result<(LogicalPlan, Vec<LayoutDecision>)> {
    stacker::maybe_grow(8 * 1024 * 1024, 32 * 1024 * 1024, || select_inner(plan))
}
fn select_inner(plan: LogicalPlan) -> Result<(LogicalPlan, Vec<LayoutDecision>)> {
    let mut found = false;
    plan.apply_with_subqueries(|p| {
        if let LogicalPlan::TableScan(s) = p {
            if let Ok(provider) = source_as_provider(&s.source) {
                found |= layout_provider(&provider).is_some();
            }
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    if !found {
        return Ok((plan, vec![]));
    }
    let plan = push_filters(plan)?;
    let mut decisions = Vec::new();
    let plan = plan
        .transform_up_with_subqueries(|node| {
            let LogicalPlan::TableScan(scan) = &node else {
                return Ok(Transformed::no(node));
            };
            let provider = source_as_provider(&scan.source)?;
            let Some(layouts) = layout_provider(&provider) else {
                return Ok(Transformed::no(node));
            };
            let (layout, decision) = layouts.choose(&scan.filters);
            decisions.push(decision);
            // Build a full scan, apply residuals, then project, preserving the old
            // qualifier and column order. No physical-table constraints are imported.
            let mut builder = LogicalPlanBuilder::scan(
                layout.table.clone(),
                provider_as_source(layouts.providers[&layout.table].clone()),
                None,
            )?
            .alias(scan.table_name.clone())?;
            for filter in &scan.filters {
                builder = builder.filter(filter.clone())?;
            }
            if let Some(projection) = &scan.projection {
                let columns = builder.schema().columns();
                builder = builder.project(
                    projection
                        .iter()
                        .map(|i| Expr::Column(columns[*i].clone()))
                        .collect::<Vec<_>>(),
                )?;
            }
            // Fetch is only a hint. Omitting it avoids limiting before residuals.
            Ok(Transformed::yes(builder.build()?))
        })?
        .data;
    Ok((plan, decisions))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn iceberg_hash_and_transform_vectors() {
        assert_eq!(murmur3(&34_i64.to_le_bytes()), 2017239379);
        assert_eq!(murmur3(b"iceberg"), 1210000089);
        assert_eq!(murmur3(&[0, 1, 2, 3]) as i32, -188683207);
        let day = (chrono::NaiveDate::from_ymd_opt(2017, 11, 16).unwrap()
            - chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
        .num_days() as i32;
        assert_eq!(murmur3(&i64::from(day).to_le_bytes()) as i32, -653330422);
        assert_eq!(
            transform(
                &ScalarValue::Int32(Some(34)),
                &PartitionTransform::Bucket { buckets: 16 }
            ),
            Some(ScalarValue::Int32(Some(3)))
        );
        assert_eq!(
            transform(
                &ScalarValue::Int64(Some(-1)),
                &PartitionTransform::Truncate { width: 10 }
            ),
            Some(ScalarValue::Int64(Some(-10)))
        );
        assert_eq!(
            transform(
                &ScalarValue::TimestampMicrosecond(Some(-1), None),
                &PartitionTransform::Day
            ),
            Some(ScalarValue::Int32(Some(-1)))
        );
        assert_eq!(
            transform(&ScalarValue::Date32(Some(day)), &PartitionTransform::Month),
            Some(ScalarValue::Int32(Some(574)))
        );
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;
    use arrow::{
        array::Int64Array,
        datatypes::{DataType, Field, Schema},
        record_batch::RecordBatch,
    };
    use datafusion::{
        datasource::MemTable,
        prelude::{SessionContext, col, lit},
    };
    #[tokio::test]
    async fn selected_scan_preserves_duplicates_and_nulls() {
        let schema = Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, true)]));
        let mut providers = BTreeMap::new();
        for (name, values) in [
            ("first", vec![Some(1), None, Some(2), Some(2)]),
            ("second", vec![Some(2), Some(1), Some(2), None]),
        ] {
            let batch =
                RecordBatch::try_new(schema.clone(), vec![Arc::new(Int64Array::from(values))])
                    .unwrap();
            providers.insert(
                name.into(),
                Arc::new(MemTable::try_new(schema.clone(), vec![vec![batch]]).unwrap())
                    as Arc<dyn TableProvider>,
            );
        }
        let source: LogicalSource = serde_json::from_value(serde_json::json!({"name":"logical", "default_table":"first", "layouts":[
            {"table":"first", "specs":[{"spec_id":0,"fields":[]}], "partitions":[{"spec_id":0,"bytes":100,"files":1}]},
            {"table":"second", "specs":[{"spec_id":0,"fields":[]}], "partitions":[{"spec_id":0,"bytes":50,"files":1}]}
        ]})).unwrap();
        let provider = Arc::new(LayoutProvider::try_new(source, &providers).unwrap());
        let original = LogicalPlanBuilder::scan("logical", provider_as_source(provider), None)
            .unwrap()
            .filter(col("id").eq(lit(2_i64)).or(col("id").is_null()))
            .unwrap()
            .build()
            .unwrap();
        let (plan, choices) = select(original).unwrap();
        assert_eq!(choices[0].table, "second");
        let batches = SessionContext::new()
            .execute_logical_plan(plan)
            .await
            .unwrap()
            .collect()
            .await
            .unwrap();
        assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(), 3);
        assert_eq!(
            batches
                .iter()
                .map(|b| b.column(0).null_count())
                .sum::<usize>(),
            1
        );
    }
}
