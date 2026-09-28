//! Equivalent relational representations of a named logical source.
//!
//! Definitions remain plans until predicates have reached each source occurrence.
//! Selection expands only the chosen alternative, before physical placement.
use super::layout::{LayoutDecision, LayoutProvider, TableLayout};
use arrow::datatypes::{Field, Schema, SchemaRef};
use datafusion::{
    catalog::{Session, TableProvider},
    common::{
        DataFusionError, Result,
        tree_node::{Transformed, TreeNodeRecursion},
    },
    datasource::{provider_as_source, source_as_provider},
    logical_expr::{Expr, LogicalPlan, LogicalPlanBuilder, TableProviderFilterPushDown, TableType},
    physical_plan::ExecutionPlan,
};
use serde::{Deserialize, Serialize};
use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RepresentationInput {
    /// A registered physical, layout, collection, or representation source.
    Table { name: String },
    /// A read-only relational definition, including joins and aggregations.
    Query { sql: String },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Representation {
    pub name: String,
    pub source: RepresentationInput,
    #[serde(default)]
    pub generation: String,
    /// Canonical output name -> source column. Empty means identity mapping.
    #[serde(default)]
    pub columns: BTreeMap<String, String>,
    /// Complete physical-source partition summaries used to cost this plan.
    #[serde(default)]
    pub statistics: Vec<TableLayout>,
    /// Optional mean elements per parent row for a single-list expansion.
    #[serde(default)]
    pub average_list_length: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepresentationSource {
    pub name: String,
    #[serde(default)]
    pub generation: String,
    pub default_representation: String,
    /// Caller asserts equal rows, values, and duplicate multiplicities.
    pub representations: Vec<Representation>,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct RepresentationEstimate {
    pub estimated_bytes: Option<u64>,
    pub estimated_files: Option<u64>,
    pub estimated_input_rows: Option<u64>,
    pub estimated_expanded_rows: Option<u64>,
    pub estimated_cost: Option<u64>,
}
#[derive(Debug, Clone, Serialize)]
pub struct RepresentationCandidate {
    pub name: String,
    pub reason: String,
    #[serde(flatten)]
    pub estimate: RepresentationEstimate,
}
#[derive(Debug, Clone, Serialize)]
pub struct RepresentationDecision {
    pub source: String,
    pub representation: String,
    pub definition: RepresentationInput,
    pub generation: String,
    pub predicates: Vec<String>,
    #[serde(flatten)]
    pub estimate: RepresentationEstimate,
    pub scans: Vec<LayoutDecision>,
    pub candidates: Vec<RepresentationCandidate>,
}
#[derive(Debug)]
pub(crate) struct RepresentationProvider {
    definition: RepresentationSource,
    plans: Vec<LogicalPlan>,
    schema: SchemaRef,
}
fn invalid(s: impl Into<String>) -> DataFusionError {
    DataFusionError::Plan(s.into())
}

impl RepresentationProvider {
    pub(crate) fn try_new(
        definition: RepresentationSource,
        mut plans: Vec<LogicalPlan>,
        providers: &BTreeMap<String, Arc<dyn TableProvider>>,
    ) -> Result<Self> {
        let default = definition
            .representations
            .iter()
            .position(|r| r.name == definition.default_representation)
            .ok_or_else(|| {
                invalid("representation source requires a registered default_representation")
            })?;
        if definition.name.is_empty()
            || definition.representations[default].generation != definition.generation
        {
            return Err(invalid(
                "representation source requires a nonempty name and a current default generation",
            ));
        }
        let mut names = BTreeSet::new();
        for (r, plan) in definition.representations.iter().zip(&mut plans) {
            if r.name.is_empty() || !names.insert(&r.name) {
                return Err(invalid("duplicate or empty representation name"));
            }
            if r.average_list_length
                .is_some_and(|x| !x.is_finite() || x < 0.0)
            {
                return Err(invalid(
                    "average_list_length must be finite and nonnegative",
                ));
            }
            plan.apply_with_subqueries(|node| {
                if matches!(
                    node,
                    LogicalPlan::Dml(_)
                        | LogicalPlan::Ddl(_)
                        | LogicalPlan::Statement(_)
                        | LogicalPlan::Copy(_)
                        | LogicalPlan::Explain(_)
                        | LogicalPlan::Analyze(_)
                ) {
                    return Err(invalid(
                        "representation definitions must be read-only queries",
                    ));
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
            if !r.columns.is_empty() {
                let columns = r
                    .columns
                    .iter()
                    .map(|(name, source)| {
                        if name.is_empty() {
                            return Err(invalid("empty representation output column"));
                        }
                        let index = plan
                            .schema()
                            .index_of_column_by_name(None, source)
                            .ok_or_else(|| {
                                invalid(format!("missing representation column {source}"))
                            })?;
                        Ok(Expr::Column(plan.schema().columns()[index].clone()).alias(name))
                    })
                    .collect::<Result<Vec<_>>>()?;
                *plan = LogicalPlanBuilder::from(plan.clone())
                    .project(columns)?
                    .build()?;
            }
        }
        let canonical = plans[default].schema().as_arrow().clone();
        let mut nullable: Vec<_> = canonical.fields().iter().map(|f| f.is_nullable()).collect();
        for plan in &mut plans {
            if plan.schema().fields().len() != canonical.fields().len() {
                return Err(invalid(
                    "representations must have the same canonical columns",
                ));
            }
            let mut columns = Vec::new();
            for (i, field) in canonical.fields().iter().enumerate() {
                let index = plan
                    .schema()
                    .index_of_column_by_name(None, field.name())
                    .ok_or_else(|| invalid(format!("missing canonical column {}", field.name())))?;
                let other = plan.schema().field(index);
                if other.data_type() != field.data_type() {
                    return Err(invalid(format!(
                        "representation type mismatch for {}",
                        field.name()
                    )));
                }
                nullable[i] |= other.is_nullable();
                columns.push(Expr::Column(plan.schema().columns()[index].clone()));
            }
            *plan = LogicalPlanBuilder::from(plan.clone())
                .project(columns)?
                .build()?;
        }
        let schema = Arc::new(Schema::new(
            canonical
                .fields()
                .iter()
                .enumerate()
                .map(|(i, f)| Field::new(f.name(), f.data_type().clone(), nullable[i]))
                .collect::<Vec<_>>(),
        ));
        // Cost-only wrappers make filters visible even when a physical provider
        // (for example MemTable) does not implement predicate pushdown itself.
        for (r, plan) in definition.representations.iter().zip(&mut plans) {
            let mut statistics = BTreeMap::new();
            for layout in &r.statistics {
                if statistics.contains_key(&layout.table) {
                    return Err(invalid("duplicate representation scan statistics"));
                }
                let provider = LayoutProvider::for_statistics(layout.clone(), providers)?;
                statistics.insert(layout.table.clone(), Arc::new(provider));
            }
            let mut used = BTreeSet::new();
            *plan = plan.clone().transform_up_with_subqueries(|node| {
                let LogicalPlan::TableScan(mut scan) = node else { return Ok(Transformed::no(node)); };
                if let Some(provider) = statistics.get(&scan.table_name.to_string()) {
                    if super::layout::layout_provider(&source_as_provider(&scan.source)?).is_some() {
                        return Err(invalid("supply partition statistics on the logical layout source, not again on its representation"));
                    }
                    used.insert(scan.table_name.to_string());
                    scan.source = provider_as_source(provider.clone());
                    return Ok(Transformed::yes(LogicalPlan::TableScan(scan)));
                }
                Ok(Transformed::no(LogicalPlan::TableScan(scan)))
            })?.data;
            if used.len() != statistics.len() {
                return Err(invalid(
                    "representation statistics reference a table not scanned by its definition",
                ));
            }
        }
        Ok(Self {
            definition,
            plans,
            schema,
        })
    }
}
#[async_trait::async_trait]
impl TableProvider for RepresentationProvider {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn schema(&self) -> SchemaRef {
        self.schema.clone()
    }
    fn table_type(&self) -> TableType {
        TableType::View
    }
    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> Result<Vec<TableProviderFilterPushDown>> {
        Ok(vec![TableProviderFilterPushDown::Inexact; filters.len()])
    }
    async fn scan(
        &self,
        _: &dyn Session,
        _: Option<&Vec<usize>>,
        _: &[Expr],
        _: Option<usize>,
    ) -> Result<Arc<dyn ExecutionPlan>> {
        Err(invalid(
            "resolve representation sources with representation::select before physical execution",
        ))
    }
}
pub(crate) fn provider(p: &Arc<dyn TableProvider>) -> Option<&RepresentationProvider> {
    if let Some(p) = p.as_any().downcast_ref::<RepresentationProvider>() {
        return Some(p);
    }
    if let Some(p) = p
        .as_any()
        .downcast_ref::<super::constraints::ConstrainedProvider>()
    {
        return provider(&p.inner);
    }
    None
}
#[derive(Debug)]
pub struct SelectedPlan {
    pub plan: LogicalPlan,
    pub layout_selections: Vec<LayoutDecision>,
    pub representation_selections: Vec<RepresentationDecision>,
}
/// Shared resolution for compilation, SQL preparation, and DAG execution.
pub fn select(plan: LogicalPlan) -> Result<SelectedPlan> {
    stacker::maybe_grow(8 * 1024 * 1024, 32 * 1024 * 1024, || select_inner(plan, 0))
}
fn select_inner(plan: LogicalPlan, depth: usize) -> Result<SelectedPlan> {
    if depth > 64 {
        return Err(invalid("representation dependency depth exceeds 64"));
    }
    let mut found = false;
    plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            found |= provider(&source_as_provider(&scan.source)?).is_some();
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    let mut choices = Vec::new();
    let mut layout_choices = Vec::new();
    let plan = if found {
        super::layout::push_filters(plan)?
    } else {
        plan
    };
    let plan = plan
        .transform_up_with_subqueries(|node| {
            let LogicalPlan::TableScan(scan) = &node else {
                return Ok(Transformed::no(node));
            };
            let p = source_as_provider(&scan.source)?;
            let Some(p) = provider(&p) else {
                return Ok(Transformed::no(node));
            };
            let default = p
                .definition
                .representations
                .iter()
                .position(|r| r.name == p.definition.default_representation)
                .unwrap();
            let mut alternatives = Vec::new();
            let mut candidates = Vec::new();
            for (r, definition) in p.definition.representations.iter().zip(&p.plans) {
                if r.generation != p.definition.generation {
                    alternatives.push(None);
                    candidates.push(RepresentationCandidate {
                        name: r.name.clone(),
                        reason: "stale generation".into(),
                        estimate: Default::default(),
                    });
                    continue;
                }
                let mut plan =
                    LogicalPlanBuilder::from(definition.clone()).alias(scan.table_name.clone())?;
                for filter in &scan.filters {
                    plan = plan.filter(filter.clone())?;
                }
                let selected =
                    select_inner(super::layout::push_filters(plan.build()?)?, depth + 1)?;
                let estimate = estimate(&selected, r.average_list_length)?;
                candidates.push(RepresentationCandidate {
                    name: r.name.clone(),
                    reason: if estimate.estimated_cost.is_some() {
                        "eligible"
                    } else {
                        "unknown scan statistics"
                    }
                    .into(),
                    estimate,
                });
                alternatives.push(Some(selected));
            }
            let mut best = default;
            if let Some(mut cost) = candidates[default].estimate.estimated_cost {
                for (i, candidate) in candidates.iter().enumerate() {
                    if let Some(next) = candidate.estimate.estimated_cost {
                        if next < cost {
                            best = i;
                            cost = next;
                        }
                    }
                }
            }
            candidates[best].reason = if candidates[default].estimate.estimated_cost.is_none() {
                "selected default; unknown default cost"
            } else {
                "selected lowest estimated cost; default wins ties"
            }
            .into();
            let selected = alternatives[best].take().unwrap();
            let r = &p.definition.representations[best];
            let scans = selected.layout_selections.clone();
            choices.push(RepresentationDecision {
                source: p.definition.name.clone(),
                representation: r.name.clone(),
                definition: r.source.clone(),
                generation: r.generation.clone(),
                predicates: scan.filters.iter().map(ToString::to_string).collect(),
                estimate: candidates[best].estimate.clone(),
                scans,
                candidates,
            });
            choices.extend(selected.representation_selections);
            layout_choices.extend(selected.layout_selections);
            let mut builder = LogicalPlanBuilder::from(selected.plan);
            if let Some(projection) = &scan.projection {
                let columns = builder.schema().columns();
                builder = builder.project(
                    projection
                        .iter()
                        .map(|i| Expr::Column(columns[*i].clone()))
                        .collect::<Vec<_>>(),
                )?;
            }
            Ok(Transformed::yes(builder.build()?))
        })?
        .data;
    let (plan, layouts) = super::layout::select(plan)?;
    layout_choices.extend(layouts);
    Ok(SelectedPlan {
        plan,
        layout_selections: layout_choices,
        representation_selections: choices,
    })
}
fn estimate(selected: &SelectedPlan, average: Option<f64>) -> Result<RepresentationEstimate> {
    let mut scans = 0;
    let mut unnests = 0;
    let mut joins = 0;
    selected.plan.apply_with_subqueries(|p| {
        match p {
            LogicalPlan::TableScan(_) => scans += 1,
            LogicalPlan::Unnest(_) => unnests += 1,
            LogicalPlan::Join(_) => joins += 1,
            _ => {}
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    let sum = |values: Vec<Option<u64>>| {
        values
            .into_iter()
            .try_fold(0_u64, |n, v| v.map(|v| n.saturating_add(v)))
    };
    if scans != selected.layout_selections.len() {
        return Ok(RepresentationEstimate::default());
    }
    let bytes = sum(selected
        .layout_selections
        .iter()
        .map(|s| s.estimated_bytes)
        .collect());
    let files = sum(selected
        .layout_selections
        .iter()
        .map(|s| s.estimated_files)
        .collect());
    let rows = sum(selected
        .layout_selections
        .iter()
        .map(|s| s.estimated_rows)
        .collect());
    let expanded = if unnests == 1 && joins == 0 {
        rows.zip(average)
            .map(|(n, f)| ((n as f64) * f).ceil() as u64)
    } else {
        None
    };
    let cost = bytes.zip(files).map(|(b, f)| {
        b.saturating_add(f.saturating_mul(65536))
            .saturating_add(expanded.unwrap_or(0).saturating_mul(8))
    });
    Ok(RepresentationEstimate {
        estimated_bytes: bytes,
        estimated_files: files,
        estimated_input_rows: rows,
        estimated_expanded_rows: expanded,
        estimated_cost: cost,
    })
}
