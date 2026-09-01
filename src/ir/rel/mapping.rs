//! "Bring your own schema" mapping: point graph labels and edge types at an
//! existing relational schema instead of tables generated from a
//! [`PropertyGraph`](crate::ir::catalog::PropertyGraph).
//!
//! A [`GraphMapping`] describes, for every node label and edge type, which
//! user table (or SQL query) backs it, which column is the element id, and
//! how graph property names map onto source columns. When a mapping is
//! installed on [`RelBackendOptions`](super::RelBackendOptions), the lowering
//! resolves every scan through the mapping instead of the property-graph
//! catalog:
//!
//! * **Table-backed** labels/edge types lower to a `TableScan` of the user
//!   table wrapped in a projection that renames (and casts) source columns
//!   into the binding-prefixed columns the rest of the lowering expects
//!   (`p__id`, `p__label`, `p__prop__name`, ...).
//! * **Query-backed** labels/edge types are parsed with datafusion-sql into a
//!   `LogicalPlan` that is spliced in as a subplan, so DataFusion's optimizer
//!   pushes filters and projections straight through the defining query. When
//!   the plan is unparsed to SQL for an external engine the query appears as
//!   an inlined derived table.
//!
//! Data access is engine-neutral:
//!
//! * **In-process (DataFusion)**: register real providers
//!   ([`register_table`](GraphMapping::register_table) with a `MemTable`, or
//!   [`register_view`](GraphMapping::register_view) for a SQL-defined view)
//!   and execute the lowered plan directly.
//! * **External SQL (DuckDB/Postgres)**: the same table names are assumed to
//!   exist in the target database;
//!   [`physical_table_names`](GraphMapping::physical_table_names) tells the
//!   SQL layer which scan leaves must *not* be materialized because they are
//!   the user's own tables/views.
//!
//! A mapping can be built programmatically or loaded from a small TOML
//! subset via [`GraphMapping::from_toml`] (see the docs for the format).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use arrow::datatypes::DataType;
use datafusion::catalog::TableProvider;
use datafusion::catalog::view::ViewTable;
use datafusion::common::config::ConfigOptions;
use datafusion::common::{DFSchema, TableReference};
use datafusion::datasource::{MemTable, provider_as_source};
use datafusion::error::{DataFusionError, Result as DFResult};
use datafusion::logical_expr::{
    AggregateUDF, Cast, Expr, LogicalPlan, LogicalPlanBuilder, ScalarUDF, TableSource, WindowUDF,
};
use datafusion::prelude::lit;
use datafusion::scalar::ScalarValue;
use datafusion::sql::planner::{ContextProvider, SqlToRel};

use crate::ir::plan::LabelExpr;

use super::{
    LoweredNode, LoweringContext, PropertyDef, RelError, RelResult, col_exact, dst_id_col,
    dst_label_col, edge_schema, id_col, label_col, node_schema, prop_col, src_id_col,
    src_label_col,
};

/// Where the rows for a mapped label or edge type come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MappedSource {
    /// An existing table (or view) in the user's schema, referenced by name.
    Table(String),
    /// A SQL `SELECT` defining the rows. Parsed with datafusion-sql against
    /// the mapping's registered tables; inlined as a derived table when the
    /// plan is unparsed for an external engine.
    Query(String),
}

/// Maps one node label onto the user's schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMapping {
    pub label: String,
    pub source: MappedSource,
    /// Column holding the node id. Must be integer-typed (it is cast to
    /// `BIGINT`); ids only need to be unique within the label.
    pub id_column: String,
    /// Graph property name -> source column name.
    pub properties: BTreeMap<String, String>,
}

impl NodeMapping {
    pub fn table(
        label: impl Into<String>,
        table: impl Into<String>,
        id_column: impl Into<String>,
    ) -> Self {
        Self::new(label, MappedSource::Table(table.into()), id_column)
    }

    pub fn query(
        label: impl Into<String>,
        sql: impl Into<String>,
        id_column: impl Into<String>,
    ) -> Self {
        Self::new(label, MappedSource::Query(sql.into()), id_column)
    }

    pub fn new(
        label: impl Into<String>,
        source: MappedSource,
        id_column: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            source,
            id_column: id_column.into(),
            properties: BTreeMap::new(),
        }
    }

    /// Map graph property `property` onto source column `column`.
    pub fn property(mut self, property: impl Into<String>, column: impl Into<String>) -> Self {
        self.properties.insert(property.into(), column.into());
        self
    }
}

/// Maps one edge type onto the user's schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeMapping {
    pub rel_type: String,
    pub source: MappedSource,
    /// Column holding the source-node id (matches the `src_label` node
    /// mapping's id values).
    pub src_column: String,
    /// Column holding the destination-node id.
    pub dst_column: String,
    pub src_label: String,
    pub dst_label: String,
    /// Optional column holding a distinct edge id. Defaults to `src_column`;
    /// set it when parallel edges must stay distinguishable.
    pub id_column: Option<String>,
    /// Graph property name -> source column name.
    pub properties: BTreeMap<String, String>,
}

impl EdgeMapping {
    pub fn table(
        rel_type: impl Into<String>,
        table: impl Into<String>,
        src_column: impl Into<String>,
        dst_column: impl Into<String>,
        src_label: impl Into<String>,
        dst_label: impl Into<String>,
    ) -> Self {
        Self::new(
            rel_type,
            MappedSource::Table(table.into()),
            src_column,
            dst_column,
            src_label,
            dst_label,
        )
    }

    pub fn query(
        rel_type: impl Into<String>,
        sql: impl Into<String>,
        src_column: impl Into<String>,
        dst_column: impl Into<String>,
        src_label: impl Into<String>,
        dst_label: impl Into<String>,
    ) -> Self {
        Self::new(
            rel_type,
            MappedSource::Query(sql.into()),
            src_column,
            dst_column,
            src_label,
            dst_label,
        )
    }

    pub fn new(
        rel_type: impl Into<String>,
        source: MappedSource,
        src_column: impl Into<String>,
        dst_column: impl Into<String>,
        src_label: impl Into<String>,
        dst_label: impl Into<String>,
    ) -> Self {
        Self {
            rel_type: rel_type.into(),
            source,
            src_column: src_column.into(),
            dst_column: dst_column.into(),
            src_label: src_label.into(),
            dst_label: dst_label.into(),
            id_column: None,
            properties: BTreeMap::new(),
        }
    }

    /// Use `column` as the distinct edge id.
    pub fn with_id(mut self, column: impl Into<String>) -> Self {
        self.id_column = Some(column.into());
        self
    }

    /// Map graph property `property` onto source column `column`.
    pub fn property(mut self, property: impl Into<String>, column: impl Into<String>) -> Self {
        self.properties.insert(property.into(), column.into());
        self
    }
}

/// The full label/edge-type -> relational-schema mapping, plus the table
/// providers (schemas and, in-process, data) the mapped sources resolve
/// against.
#[derive(Default)]
pub struct GraphMapping {
    nodes: BTreeMap<String, NodeMapping>,
    edges: BTreeMap<String, EdgeMapping>,
    tables: BTreeMap<String, Arc<dyn TableProvider>>,
}

impl fmt::Debug for GraphMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GraphMapping")
            .field("nodes", &self.nodes)
            .field("edges", &self.edges)
            .field("tables", &self.tables.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl GraphMapping {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a node-label mapping.
    pub fn map_node(&mut self, mapping: NodeMapping) -> &mut Self {
        self.nodes.insert(mapping.label.clone(), mapping);
        self
    }

    /// Register (or replace) an edge-type mapping.
    pub fn map_edge(&mut self, mapping: EdgeMapping) -> &mut Self {
        self.edges.insert(mapping.rel_type.clone(), mapping);
        self
    }

    /// Register a provider for a physical table name referenced by
    /// table-backed mappings or query-backed SQL. For in-process execution
    /// the provider carries the data (e.g. a `MemTable`); for external SQL
    /// execution only its schema matters.
    pub fn register_table(
        &mut self,
        name: impl Into<String>,
        provider: Arc<dyn TableProvider>,
    ) -> &mut Self {
        self.tables.insert(name.into(), provider);
        self
    }

    /// Register `name` as a SQL-defined view over previously registered
    /// tables. In-process it executes as a real DataFusion view (so filters
    /// and projections push through it); on an external engine the name is
    /// expected to exist as a table or view in the target database.
    pub fn register_view(&mut self, name: impl Into<String>, sql: &str) -> RelResult<&mut Self> {
        let plan = self.plan_sql(sql)?;
        self.tables.insert(
            name.into(),
            Arc::new(ViewTable::new(plan, Some(sql.to_string()))),
        );
        Ok(self)
    }

    pub fn node(&self, label: &str) -> Option<&NodeMapping> {
        self.nodes.get(label)
    }

    pub fn edge(&self, rel_type: &str) -> Option<&EdgeMapping> {
        self.edges.get(rel_type)
    }

    pub fn labels(&self) -> Vec<String> {
        self.nodes.keys().cloned().collect()
    }

    pub fn rel_types(&self) -> Vec<String> {
        self.edges.keys().cloned().collect()
    }

    /// Physical table names the mapping resolves against. When generating SQL
    /// for an external engine these are the user's own tables/views and must
    /// not be materialized by the SQL layer.
    pub fn physical_table_names(&self) -> BTreeSet<String> {
        self.tables.keys().cloned().collect()
    }

    /// Build the source plan for a mapped source: a bare scan for
    /// table-backed sources, the parsed query plan for query-backed ones.
    fn source_plan(&self, source: &MappedSource) -> RelResult<LogicalPlan> {
        match source {
            MappedSource::Table(name) => {
                let provider = self.tables.get(name).ok_or_else(|| {
                    RelError::Unsupported(format!(
                        "mapping references table `{name}` but no provider/schema is registered; \
                         call GraphMapping::register_table or register_table_schema"
                    ))
                })?;
                Ok(LogicalPlanBuilder::scan(
                    name.clone(),
                    provider_as_source(Arc::clone(provider)),
                    None,
                )?
                .build()?)
            }
            MappedSource::Query(sql) => self.plan_sql(sql),
        }
    }

    /// Parse a SQL `SELECT` against the registered tables using
    /// datafusion-sql.
    fn plan_sql(&self, sql: &str) -> RelResult<LogicalPlan> {
        let mut statements = datafusion::sql::parser::DFParser::parse_sql(sql)
            .map_err(|err| RelError::Unsupported(format!("mapping query parse: {err}")))?;
        if statements.len() != 1 {
            return Err(RelError::Unsupported(format!(
                "mapping query must be a single statement, got {}",
                statements.len()
            )));
        }
        let statement = statements.pop_front().expect("one statement");
        let provider = MappingContextProvider::new(self);
        let planner = SqlToRel::new(&provider);
        planner
            .statement_to_plan(statement)
            .map_err(|err| RelError::Unsupported(format!("mapping query plan: {err}")))
    }
}

/// Register a schema-only table (an empty `MemTable`). Useful when the data
/// lives solely in an external database and only SQL generation is needed.
pub fn schema_only_provider(schema: arrow::datatypes::SchemaRef) -> Arc<dyn TableProvider> {
    Arc::new(MemTable::try_new(schema, vec![Vec::new()]).expect("empty memtable"))
}

impl GraphMapping {
    /// Convenience for [`schema_only_provider`] + [`register_table`](Self::register_table).
    pub fn register_table_schema(
        &mut self,
        name: impl Into<String>,
        schema: arrow::datatypes::SchemaRef,
    ) -> &mut Self {
        self.register_table(name, schema_only_provider(schema))
    }
}

// ---------------------------------------------------------------------------
// Scan lowering
// ---------------------------------------------------------------------------

/// Lower a node scan through the mapping. Produces, per mapped label, a
/// projection over the source plan that renames columns into the
/// binding-prefixed shape (`{b}__id`, `{b}__label`, `{b}__prop__*`), unioned
/// when the scan covers several labels.
pub(super) fn lower_mapped_node_scan(
    ctx: &mut LoweringContext<'_>,
    mapping: &GraphMapping,
    binding: &str,
    labels: &LabelExpr,
) -> RelResult<LoweredNode> {
    let labels = resolve_names(labels, || mapping.labels(), "node label")?;
    let mut sources = Vec::new();
    for label in &labels {
        // Mirror the catalog path: unmapped labels scan as empty, they are
        // not an error.
        let Some(node) = mapping.node(label) else {
            continue;
        };
        let plan = mapping.source_plan(&node.source)?;
        sources.push((node, plan));
    }

    let mut defs = BTreeMap::<String, DataType>::new();
    for (node, plan) in &sources {
        for (property, column) in &node.properties {
            let data_type = source_column_type(plan, column, &format!("label `{}`", node.label))?;
            merge_def(&mut defs, property, data_type)?;
        }
    }

    if sources.is_empty() {
        let schema = node_schema(binding, &[]);
        return ctx.scan_batches("nodes", vec![arrow::array::RecordBatch::new_empty(schema)]);
    }

    let mut branches = Vec::new();
    for (node, plan) in sources {
        let mut exprs = vec![
            id_expr(&plan, &node.id_column, &format!("label `{}`", node.label))?
                .alias(id_col(binding)),
            lit(node.label.as_str()).alias(label_col(binding)),
        ];
        exprs.extend(property_exprs(binding, &plan, &node.properties, &defs)?);
        branches.push(LogicalPlanBuilder::from(plan).project(exprs)?.build()?);
    }
    Ok(LoweredNode::new(union_all(branches)?))
}

/// Lower an edge scan through the mapping, producing the edge binding shape
/// (`{b}__id/__label/__src_label/__src_id/__dst_label/__dst_id/__prop__*`).
pub(super) fn lower_mapped_rel_scan(
    ctx: &mut LoweringContext<'_>,
    mapping: &GraphMapping,
    binding: &str,
    types: &LabelExpr,
) -> RelResult<LoweredNode> {
    let rel_types = resolve_names(types, || mapping.rel_types(), "relationship type")?;
    let mut sources = Vec::new();
    for rel_type in &rel_types {
        let Some(edge) = mapping.edge(rel_type) else {
            continue;
        };
        let plan = mapping.source_plan(&edge.source)?;
        sources.push((edge, plan));
    }

    let mut defs = BTreeMap::<String, DataType>::new();
    for (edge, plan) in &sources {
        for (property, column) in &edge.properties {
            let data_type =
                source_column_type(plan, column, &format!("edge type `{}`", edge.rel_type))?;
            merge_def(&mut defs, property, data_type)?;
        }
    }

    if sources.is_empty() {
        let schema = edge_schema(binding, &[]);
        return ctx.scan_batches("edges", vec![arrow::array::RecordBatch::new_empty(schema)]);
    }

    let mut branches = Vec::new();
    for (edge, plan) in sources {
        let what = format!("edge type `{}`", edge.rel_type);
        let id_column = edge.id_column.as_deref().unwrap_or(&edge.src_column);
        let mut exprs = vec![
            id_expr(&plan, id_column, &what)?.alias(id_col(binding)),
            lit(edge.rel_type.as_str()).alias(label_col(binding)),
            lit(edge.src_label.as_str()).alias(src_label_col(binding)),
            id_expr(&plan, &edge.src_column, &what)?.alias(src_id_col(binding)),
            lit(edge.dst_label.as_str()).alias(dst_label_col(binding)),
            id_expr(&plan, &edge.dst_column, &what)?.alias(dst_id_col(binding)),
        ];
        exprs.extend(property_exprs(binding, &plan, &edge.properties, &defs)?);
        branches.push(LogicalPlanBuilder::from(plan).project(exprs)?.build()?);
    }
    Ok(LoweredNode::new(union_all(branches)?))
}

fn resolve_names(
    expr: &LabelExpr,
    all: impl FnOnce() -> Vec<String>,
    what: &str,
) -> RelResult<Vec<String>> {
    let mut out = match expr {
        LabelExpr::Any => all(),
        LabelExpr::AnyOf(names) => names.clone(),
        LabelExpr::AllOf(names) if names.len() == 1 => names.clone(),
        LabelExpr::AllOf(names) => {
            return Err(RelError::Unsupported(format!(
                "multi-{what} scan {names:?} through a mapping"
            )));
        }
        LabelExpr::Not(_) => {
            return Err(RelError::Unsupported(format!("negated {what} scan")));
        }
    };
    out.sort();
    out.dedup();
    Ok(out)
}

fn merge_def(
    defs: &mut BTreeMap<String, DataType>,
    property: &str,
    data_type: DataType,
) -> RelResult<()> {
    match defs.get(property) {
        Some(existing) if *existing != data_type => Err(RelError::Unsupported(format!(
            "mapped property `{property}` has mixed types `{existing:?}` and `{data_type:?}`"
        ))),
        Some(_) => Ok(()),
        None => {
            defs.insert(property.to_string(), data_type);
            Ok(())
        }
    }
}

/// Property projection expressions in `defs` order; labels that do not map a
/// property project a typed NULL so union branches align.
fn property_exprs(
    binding: &str,
    plan: &LogicalPlan,
    properties: &BTreeMap<String, String>,
    defs: &BTreeMap<String, DataType>,
) -> RelResult<Vec<Expr>> {
    let mut out = Vec::with_capacity(defs.len());
    for (property, data_type) in defs {
        let expr = match properties.get(property) {
            Some(column) => col_exact(resolve_column(plan, column)?),
            None => lit(ScalarValue::try_from(data_type).map_err(|err| {
                RelError::Unsupported(format!(
                    "no null literal for mapped property type {data_type:?}: {err}"
                ))
            })?),
        };
        out.push(expr.alias(prop_col(binding, property)));
    }
    Ok(out)
}

/// Reference an id column, cast to `BIGINT` so ids line up across tables and
/// with join synthesis. Non-integer id columns are rejected up front.
fn id_expr(plan: &LogicalPlan, column: &str, what: &str) -> RelResult<Expr> {
    let data_type = source_column_type(plan, column, what)?;
    let is_integer = matches!(
        data_type,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
    );
    if !is_integer {
        return Err(RelError::Unsupported(format!(
            "{what}: id column `{column}` must be integer-typed, found {data_type:?}"
        )));
    }
    let column = col_exact(resolve_column(plan, column)?);
    Ok(if data_type == DataType::Int64 {
        column
    } else {
        Expr::Cast(Cast::new(Box::new(column), DataType::Int64))
    })
}

fn source_column_type(plan: &LogicalPlan, column: &str, what: &str) -> RelResult<DataType> {
    let name = resolve_column(plan, column)?;
    schema_field_type(plan.schema(), &name).ok_or_else(|| {
        RelError::Unsupported(format!(
            "{what}: source has no column `{column}` (available: {})",
            plan.schema()
                .fields()
                .iter()
                .map(|field| field.name().as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })
}

/// Resolve a mapped column name against the source plan schema, exactly
/// first, then case-insensitively when unambiguous.
fn resolve_column(plan: &LogicalPlan, column: &str) -> RelResult<String> {
    let schema = plan.schema();
    if schema.fields().iter().any(|field| field.name() == column) {
        return Ok(column.to_string());
    }
    let mut matches = schema
        .fields()
        .iter()
        .filter(|field| field.name().eq_ignore_ascii_case(column));
    if let Some(found) = matches.next()
        && matches.next().is_none()
    {
        return Ok(found.name().clone());
    }
    // Leave resolution errors to source_column_type, which lists candidates.
    Ok(column.to_string())
}

fn schema_field_type(schema: &DFSchema, name: &str) -> Option<DataType> {
    schema
        .fields()
        .iter()
        .find(|field| field.name() == name)
        .map(|field| field.data_type().clone())
}

fn union_all(mut branches: Vec<LogicalPlan>) -> RelResult<LogicalPlan> {
    let first = branches.remove(0);
    let mut builder = LogicalPlanBuilder::from(first);
    for branch in branches {
        builder = builder.union(branch)?;
    }
    Ok(builder.build()?)
}

// ---------------------------------------------------------------------------
// SQL planning support for query-backed sources
// ---------------------------------------------------------------------------

struct MappingContextProvider<'a> {
    mapping: &'a GraphMapping,
    options: ConfigOptions,
    udfs: Vec<Arc<ScalarUDF>>,
    udafs: Vec<Arc<AggregateUDF>>,
    udwfs: Vec<Arc<WindowUDF>>,
}

impl<'a> MappingContextProvider<'a> {
    fn new(mapping: &'a GraphMapping) -> Self {
        Self {
            mapping,
            options: ConfigOptions::default(),
            udfs: datafusion::functions::all_default_functions(),
            udafs: datafusion::functions_aggregate::all_default_aggregate_functions(),
            udwfs: datafusion::functions_window::all_default_window_functions(),
        }
    }
}

impl ContextProvider for MappingContextProvider<'_> {
    fn get_table_source(&self, name: TableReference) -> DFResult<Arc<dyn TableSource>> {
        let table = name.table();
        match self.mapping.tables.get(table) {
            Some(provider) => Ok(provider_as_source(Arc::clone(provider))),
            None => Err(DataFusionError::Plan(format!(
                "mapping query references unknown table `{table}`; register it on the GraphMapping"
            ))),
        }
    }

    fn get_function_meta(&self, name: &str) -> Option<Arc<ScalarUDF>> {
        let lower = name.to_ascii_lowercase();
        self.udfs
            .iter()
            .find(|udf| udf.name() == lower || udf.aliases().iter().any(|alias| alias == &lower))
            .cloned()
    }

    fn get_aggregate_meta(&self, name: &str) -> Option<Arc<AggregateUDF>> {
        let lower = name.to_ascii_lowercase();
        self.udafs
            .iter()
            .find(|udaf| udaf.name() == lower || udaf.aliases().iter().any(|alias| alias == &lower))
            .cloned()
    }

    fn get_window_meta(&self, name: &str) -> Option<Arc<WindowUDF>> {
        let lower = name.to_ascii_lowercase();
        self.udwfs
            .iter()
            .find(|udwf| udwf.name() == lower || udwf.aliases().iter().any(|alias| alias == &lower))
            .cloned()
    }

    fn get_variable_type(&self, _variable_names: &[String]) -> Option<DataType> {
        None
    }

    fn options(&self) -> &ConfigOptions {
        &self.options
    }

    fn udf_names(&self) -> Vec<String> {
        self.udfs.iter().map(|udf| udf.name().to_string()).collect()
    }

    fn udaf_names(&self) -> Vec<String> {
        self.udafs
            .iter()
            .map(|udaf| udaf.name().to_string())
            .collect()
    }

    fn udwf_names(&self) -> Vec<String> {
        self.udwfs
            .iter()
            .map(|udwf| udwf.name().to_string())
            .collect()
    }
}

// ---------------------------------------------------------------------------
// TOML (de)serialization — a hand-rolled subset, no new dependencies
// ---------------------------------------------------------------------------

impl GraphMapping {
    /// Parse a mapping from a small TOML subset. Table providers are *not*
    /// part of the serialized form; register them afterwards.
    ///
    /// ```toml
    /// [node.Person]
    /// table = "customers"        # or: query = "SELECT ..."
    /// id = "cust_id"
    ///
    /// [node.Person.properties]
    /// name = "full_name"
    /// age = "age"
    ///
    /// [edge.ORDERED]
    /// table = "orders"
    /// src = "cust_id"
    /// dst = "order_id"
    /// src_label = "Person"
    /// dst_label = "Order"
    /// edge_id = "order_id"       # optional
    ///
    /// [edge.ORDERED.properties]
    /// total = "total"
    /// ```
    pub fn from_toml(input: &str) -> RelResult<Self> {
        let sections = parse_toml_sections(input)?;
        let mut mapping = GraphMapping::new();
        for (path, entries) in &sections {
            match path.as_slice() {
                [kind, name] if kind == "node" => {
                    let source = section_source(entries, &format!("node.{name}"))?;
                    let id = require_key(entries, "id", &format!("node.{name}"))?;
                    let mut node = NodeMapping::new(name.clone(), source, id);
                    if let Some(props) = sections.get(&vec![
                        "node".to_string(),
                        name.clone(),
                        "properties".to_string(),
                    ]) {
                        for (property, column) in props {
                            node.properties.insert(property.clone(), column.clone());
                        }
                    }
                    mapping.map_node(node);
                }
                [kind, name] if kind == "edge" => {
                    let at = format!("edge.{name}");
                    let source = section_source(entries, &at)?;
                    let mut edge = EdgeMapping::new(
                        name.clone(),
                        source,
                        require_key(entries, "src", &at)?,
                        require_key(entries, "dst", &at)?,
                        require_key(entries, "src_label", &at)?,
                        require_key(entries, "dst_label", &at)?,
                    );
                    if let Some(id) = entries.get("edge_id") {
                        edge.id_column = Some(id.clone());
                    }
                    if let Some(props) = sections.get(&vec![
                        "edge".to_string(),
                        name.clone(),
                        "properties".to_string(),
                    ]) {
                        for (property, column) in props {
                            edge.properties.insert(property.clone(), column.clone());
                        }
                    }
                    mapping.map_edge(edge);
                }
                [kind, _name, last]
                    if last == "properties" && (kind == "node" || kind == "edge") =>
                {
                    // handled with the parent section
                }
                other => {
                    return Err(RelError::Unsupported(format!(
                        "unexpected mapping section [{}]",
                        other.join(".")
                    )));
                }
            }
        }
        Ok(mapping)
    }

    /// Render the mapping in the same TOML subset [`from_toml`](Self::from_toml)
    /// reads. Providers are not serialized.
    pub fn to_toml(&self) -> String {
        fn quote(value: &str) -> String {
            format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
        }
        fn source_line(source: &MappedSource) -> String {
            match source {
                MappedSource::Table(table) => format!("table = {}", quote(table)),
                MappedSource::Query(sql) => format!("query = {}", quote(sql)),
            }
        }
        let mut out = String::new();
        for (label, node) in &self.nodes {
            out.push_str(&format!("[node.{label}]\n"));
            out.push_str(&source_line(&node.source));
            out.push('\n');
            out.push_str(&format!("id = {}\n", quote(&node.id_column)));
            if !node.properties.is_empty() {
                out.push_str(&format!("\n[node.{label}.properties]\n"));
                for (property, column) in &node.properties {
                    out.push_str(&format!("{property} = {}\n", quote(column)));
                }
            }
            out.push('\n');
        }
        for (rel_type, edge) in &self.edges {
            out.push_str(&format!("[edge.{rel_type}]\n"));
            out.push_str(&source_line(&edge.source));
            out.push('\n');
            out.push_str(&format!("src = {}\n", quote(&edge.src_column)));
            out.push_str(&format!("dst = {}\n", quote(&edge.dst_column)));
            out.push_str(&format!("src_label = {}\n", quote(&edge.src_label)));
            out.push_str(&format!("dst_label = {}\n", quote(&edge.dst_label)));
            if let Some(id) = &edge.id_column {
                out.push_str(&format!("edge_id = {}\n", quote(id)));
            }
            if !edge.properties.is_empty() {
                out.push_str(&format!("\n[edge.{rel_type}.properties]\n"));
                for (property, column) in &edge.properties {
                    out.push_str(&format!("{property} = {}\n", quote(column)));
                }
            }
            out.push('\n');
        }
        out
    }
}

fn section_source(entries: &BTreeMap<String, String>, at: &str) -> RelResult<MappedSource> {
    match (entries.get("table"), entries.get("query")) {
        (Some(table), None) => Ok(MappedSource::Table(table.clone())),
        (None, Some(query)) => Ok(MappedSource::Query(query.clone())),
        (Some(_), Some(_)) => Err(RelError::Unsupported(format!(
            "[{at}] sets both `table` and `query`; pick one"
        ))),
        (None, None) => Err(RelError::Unsupported(format!(
            "[{at}] needs a `table` or `query` key"
        ))),
    }
}

fn require_key(entries: &BTreeMap<String, String>, key: &str, at: &str) -> RelResult<String> {
    entries
        .get(key)
        .cloned()
        .ok_or_else(|| RelError::Unsupported(format!("[{at}] is missing required key `{key}`")))
}

type TomlSections = BTreeMap<Vec<String>, BTreeMap<String, String>>;

/// Parse the TOML subset: `[dotted.section]` headers and `key = "string"`
/// entries. `#` comments and blank lines are ignored.
fn parse_toml_sections(input: &str) -> RelResult<TomlSections> {
    let mut sections: TomlSections = BTreeMap::new();
    let mut current: Option<Vec<String>> = None;
    for (number, raw) in input.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let err = |message: String| {
            RelError::Unsupported(format!("mapping toml line {}: {message}", number + 1))
        };
        if let Some(rest) = line.strip_prefix('[') {
            let Some(inner) = rest.strip_suffix(']') else {
                return Err(err(format!("unterminated section header `{line}`")));
            };
            let path = inner
                .split('.')
                .map(|part| part.trim().trim_matches('"').to_string())
                .collect::<Vec<_>>();
            if path.iter().any(String::is_empty) {
                return Err(err(format!("empty segment in section `[{inner}]`")));
            }
            sections.entry(path.clone()).or_default();
            current = Some(path);
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(err(format!("expected `key = \"value\"`, got `{line}`")));
        };
        let Some(section) = &current else {
            return Err(err("key outside any [section]".to_string()));
        };
        let key = key.trim().trim_matches('"').to_string();
        let value = parse_toml_string(value.trim())
            .map_err(|message| err(format!("value for `{key}`: {message}")))?;
        sections
            .get_mut(section)
            .expect("section exists")
            .insert(key, value);
    }
    Ok(sections)
}

/// Parse a double-quoted TOML string with `\"` and `\\` escapes. Trailing
/// `#` comments after the closing quote are ignored.
fn parse_toml_string(input: &str) -> Result<String, String> {
    let mut chars = input.chars();
    if chars.next() != Some('"') {
        return Err(format!("expected a double-quoted string, got `{input}`"));
    }
    let mut out = String::new();
    let mut closed = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' => {
                closed = true;
                break;
            }
            '\\' => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                other => return Err(format!("unsupported escape `\\{other:?}`")),
            },
            other => out.push(other),
        }
    }
    if !closed {
        return Err(format!("unterminated string `{input}`"));
    }
    let rest = chars.as_str().trim();
    if !rest.is_empty() && !rest.starts_with('#') {
        return Err(format!("unexpected trailing content `{rest}`"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toml_round_trip() {
        let toml = r#"
# BYOS mapping
[node.Person]
table = "customers"
id = "cust_id"

[node.Person.properties]
name = "full_name"
age = "age"

[node.Vip]
query = "SELECT cust_id, full_name FROM customers WHERE age >= 30"
id = "cust_id"

[node.Vip.properties]
name = "full_name"

[edge.ORDERED]
table = "orders"
src = "cust_id"
dst = "order_id"
src_label = "Person"
dst_label = "Order"
edge_id = "order_id"

[edge.ORDERED.properties]
total = "total"
"#;
        let mapping = GraphMapping::from_toml(toml).expect("parse");
        let person = mapping.node("Person").expect("person");
        assert_eq!(person.source, MappedSource::Table("customers".into()));
        assert_eq!(person.id_column, "cust_id");
        assert_eq!(person.properties.get("name").unwrap(), "full_name");
        let vip = mapping.node("Vip").expect("vip");
        assert!(matches!(vip.source, MappedSource::Query(_)));
        let ordered = mapping.edge("ORDERED").expect("ordered");
        assert_eq!(ordered.src_label, "Person");
        assert_eq!(ordered.id_column.as_deref(), Some("order_id"));
        assert_eq!(ordered.properties.get("total").unwrap(), "total");

        let reparsed = GraphMapping::from_toml(&mapping.to_toml()).expect("reparse");
        assert_eq!(reparsed.nodes, mapping.nodes);
        assert_eq!(reparsed.edges, mapping.edges);
    }

    #[test]
    fn toml_rejects_bad_sections_and_values() {
        assert!(GraphMapping::from_toml("[wat.Person]\ntable = \"t\"").is_err());
        assert!(GraphMapping::from_toml("[node.Person]\nid = \"x\"").is_err());
        assert!(
            GraphMapping::from_toml("[node.Person]\ntable = \"t\"\nquery = \"q\"\nid = \"x\"")
                .is_err()
        );
        assert!(GraphMapping::from_toml("[node.Person]\ntable = unquoted\nid = \"x\"").is_err());
    }

    #[test]
    fn query_planning_resolves_registered_tables() {
        use arrow::datatypes::{DataType, Field, Schema};
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "customers",
            Arc::new(Schema::new(vec![
                Field::new("cust_id", DataType::Int64, false),
                Field::new("age", DataType::Int64, true),
            ])),
        );
        let plan = mapping
            .plan_sql("SELECT cust_id FROM customers WHERE age > 30")
            .expect("plan view sql");
        assert!(format!("{}", plan.display_indent()).contains("TableScan: customers"));
        assert!(mapping.plan_sql("SELECT * FROM missing").is_err());
    }
}
