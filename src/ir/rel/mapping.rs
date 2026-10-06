//! "Bring your own schema" mapping: point graph labels and edge types at an
//! existing relational schema instead of tables generated from a
//! [`PropertyGraph`](crate::ir::catalog::PropertyGraph).
//!
//! A [`GraphMapping`] describes, for every node label and edge type, which
//! user table (or SQL query) backs it, which ordered columns form the element id, and
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
//! A mapping can be built programmatically or loaded from TOML via [`GraphMapping::from_toml`] (see the docs for the format).

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

pub mod computed;
pub use computed::{ComputedRelationship, RelationshipOrder, RelationshipStage, SortDirection, NullOrder, RetrievalMode};
use super::{
    LoweredNode, LoweringContext, RelError, RelResult, col_exact, dst_id_col,
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
    /// Declarative endpoint expressions compiled into a relational plan.
    Computed(Box<ComputedRelationship>),
}

/// Ordered physical columns forming an element key or relationship endpoint.
/// A single string remains source-compatible with the scalar-key constructors.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyColumns(Vec<String>);
impl KeyColumns {
    pub fn columns(&self) -> &[String] {
        &self.0
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn contains(&self, column: &str) -> bool {
        self.0.iter().any(|c| c == column)
    }
    pub fn validate(&self) -> RelResult<()> {
        if self.0.is_empty()
            || self.0.iter().any(String::is_empty)
            || self.0.iter().collect::<BTreeSet<_>>().len() != self.0.len()
        {
            return Err(RelError::Unsupported(
                "key columns must be nonempty, distinct, and ordered".into(),
            ));
        }
        Ok(())
    }
    pub(crate) fn data_type(&self, schema: &arrow::datatypes::Schema) -> Result<DataType, String> {
        self.validate().map_err(|e| e.to_string())?;
        let types = self
            .0
            .iter()
            .map(|column| {
                schema
                    .field_with_name(column)
                    .map(|f| f.data_type().clone())
                    .map_err(|e| e.to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        if types.iter().any(|t| !is_scalar_identity_type(t)) {
            return Err("key columns must each have a scalar identity type".into());
        }
        Ok(if types.len() == 1 {
            types.into_iter().next().unwrap()
        } else {
            DataType::Struct(
                types
                    .into_iter()
                    .enumerate()
                    .map(|(i, t)| Arc::new(arrow::datatypes::Field::new(format!("k{i}"), t, true)))
                    .collect::<Vec<_>>()
                    .into(),
            )
        })
    }
    /// A typed SQL tuple. Field names are positional, not source column names.
    pub(crate) fn sql(&self, qualifier: Option<&str>) -> String {
        let columns = self
            .0
            .iter()
            .map(|c| {
                let quoted = format!("\"{}\"", c.replace('"', "\"\""));
                qualifier.map_or(quoted.clone(), |q| format!("{q}.{quoted}"))
            })
            .collect::<Vec<_>>();
        if columns.len() == 1 {
            columns[0].clone()
        } else {
            format!(
                "struct_pack({})",
                columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| format!("k{i} := {c}"))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
    }
    pub(crate) fn present_sql(&self) -> String {
        self.0
            .iter()
            .map(|c| format!("\"{}\" IS NOT NULL", c.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" AND ")
    }
    pub(crate) fn values(
        &self,
        key: &crate::ir::ElementId,
    ) -> Result<BTreeMap<String, ScalarValue>, String> {
        let values = key.components();
        if values.len() != self.len() {
            return Err("key component count does not match physical columns".into());
        }
        Ok(self.0.iter().cloned().zip(values).collect())
    }
    fn toml(&self) -> String {
        if self.len() == 1 {
            serde_json::to_string(&self.0[0]).unwrap()
        } else {
            serde_json::to_string(&self.0).unwrap()
        }
    }
}
impl From<&String> for KeyColumns {
    fn from(column: &String) -> Self {
        Self(vec![column.clone()])
    }
}
impl From<&KeyColumns> for KeyColumns {
    fn from(columns: &KeyColumns) -> Self {
        columns.clone()
    }
}
impl From<String> for KeyColumns {
    fn from(column: String) -> Self {
        Self(vec![column])
    }
}
impl From<&str> for KeyColumns {
    fn from(column: &str) -> Self {
        Self(vec![column.into()])
    }
}
impl<S: Into<String>, const N: usize> From<[S; N]> for KeyColumns {
    fn from(columns: [S; N]) -> Self {
        Self(columns.into_iter().map(Into::into).collect())
    }
}
impl<S: Into<String>> From<Vec<S>> for KeyColumns {
    fn from(columns: Vec<S>) -> Self {
        Self(columns.into_iter().map(Into::into).collect())
    }
}
impl std::fmt::Display for KeyColumns {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.toml())
    }
}

impl<'de> serde::Deserialize<'de> for KeyColumns {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(untagged)]
        enum Input {
            Column(String),
            Columns(Vec<String>),
        }
        let key = match Input::deserialize(deserializer)? {
            Input::Column(c) => Self::from(c),
            Input::Columns(c) => Self::from(c),
        };
        key.validate().map_err(serde::de::Error::custom)?;
        Ok(key)
    }
}
impl serde::Serialize for KeyColumns {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.len() == 1 {
            serializer.serialize_str(&self.0[0])
        } else {
            serde::Serialize::serialize(&self.0, serializer)
        }
    }
}

/// Maps one node label onto the user's schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeMapping {
    pub label: String,
    pub source: MappedSource,
    /// Ordered columns holding the node id. Each component is a non-null scalar
    /// and keeps its source type; the complete key must be unique within a label.
    pub id_column: KeyColumns,
    /// Graph property name -> source column name.
    pub properties: BTreeMap<String, String>,
}

impl NodeMapping {
    pub fn table(
        label: impl Into<String>,
        table: impl Into<String>,
        id_column: impl Into<KeyColumns>,
    ) -> Self {
        Self::new(label, MappedSource::Table(table.into()), id_column)
    }

    pub fn query(
        label: impl Into<String>,
        sql: impl Into<String>,
        id_column: impl Into<KeyColumns>,
    ) -> Self {
        Self::new(label, MappedSource::Query(sql.into()), id_column)
    }

    pub fn new(
        label: impl Into<String>,
        source: MappedSource,
        id_column: impl Into<KeyColumns>,
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

/// Endpoint whose row owns a foreign-key-backed relationship.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ForeignKeyEndpoint {
    #[serde(rename = "src")]
    Source,
    #[serde(rename = "dst")]
    Destination,
}

/// Maps one edge type onto the user's schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeMapping {
    /// When set, the edge is an FK on this endpoint's row, not an independent row.
    pub foreign_key: Option<ForeignKeyEndpoint>,
    pub rel_type: String,
    pub source: MappedSource,
    /// Ordered columns holding the source-node id (matches the `src_label` node
    /// mapping's id values).
    pub src_column: KeyColumns,
    /// Ordered columns holding the destination-node id.
    pub dst_column: KeyColumns,
    pub src_label: String,
    pub dst_label: String,
    /// Optional ordered columns holding a distinct edge id. Defaults to `src_column`;
    /// set it when parallel edges must stay distinguishable.
    pub id_column: Option<KeyColumns>,
    /// Graph property name -> source column name.
    pub properties: BTreeMap<String, String>,
}

impl EdgeMapping {
    pub fn table(
        rel_type: impl Into<String>,
        table: impl Into<String>,
        src_column: impl Into<KeyColumns>,
        dst_column: impl Into<KeyColumns>,
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
        src_column: impl Into<KeyColumns>,
        dst_column: impl Into<KeyColumns>,
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
        src_column: impl Into<KeyColumns>,
        dst_column: impl Into<KeyColumns>,
        src_label: impl Into<String>,
        dst_label: impl Into<String>,
    ) -> Self {
        Self {
            foreign_key: None,
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

    /// Store the relationship on the selected endpoint's row. The other endpoint
    /// column is the foreign key; the child primary key is also the edge identity.
    /// No relationship table or independently generated edge id is needed.
    pub fn foreign_key(mut self, child: ForeignKeyEndpoint) -> Self {
        self.foreign_key = Some(child);
        self.id_column = Some(match child {
            ForeignKeyEndpoint::Source => self.src_column.clone(),
            ForeignKeyEndpoint::Destination => self.dst_column.clone(),
        });
        self
    }

    /// (child label, child key column, parent label, FK column).
    pub fn foreign_key_columns(&self) -> Option<(&str, &KeyColumns, &str, &KeyColumns)> {
        self.foreign_key.map(|child| match child {
            ForeignKeyEndpoint::Source => (
                self.src_label.as_str(),
                &self.src_column,
                self.dst_label.as_str(),
                &self.dst_column,
            ),
            ForeignKeyEndpoint::Destination => (
                self.dst_label.as_str(),
                &self.dst_column,
                self.src_label.as_str(),
                &self.src_column,
            ),
        })
    }

    /// Use `column` as the distinct edge id.
    pub fn with_id(mut self, column: impl Into<KeyColumns>) -> Self {
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
#[derive(Default, Clone)]
pub struct GraphMapping {
    pub(crate) rdf: super::rdf::RdfDatasetMapping,
    pub(crate) source_metadata: BTreeMap<String, super::source_metadata::SourceMetadata>,
    logical_functions: BTreeMap<String, Arc<ScalarUDF>>,
    nodes: BTreeMap<String, NodeMapping>,
    edges: BTreeMap<String, EdgeMapping>,
    tables: BTreeMap<String, Arc<dyn TableProvider>>,
    representation_sources: BTreeMap<String, super::representation::RepresentationSource>,
    statistics: Option<Arc<super::statistics::StatisticsSnapshot>>,
    collection_sources: BTreeMap<String, super::collection_source::CollectionSource>,
    logical_sources: BTreeMap<String, super::layout::LogicalSource>,
    constraints: super::constraints::ConstraintCatalog,
    constraint_scope: Option<String>,
    view_dependencies: BTreeMap<String, BTreeSet<String>>,
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
    /// Declare RDF vocabulary over a source already in this graph catalog.
    pub fn map_rdf(&mut self, rule: super::rdf_mapping::RdfMapping) -> &mut Self {
        self.rdf.map_relational(rule); self
    }
    pub fn rdf_mapping(&self) -> super::rdf::RdfDatasetMapping {
        let mut rdf = self.rdf.clone();
        let mut tables = self.tables.clone();
        for name in self.collection_sources.keys().chain(self.representation_sources.keys()) {
            match self.constrained_provider(name) {
                Ok(provider) => { tables.insert(name.clone(), provider); }
                Err(_) => { tables.remove(name); }
            }
        }
        rdf.extend_tables(&tables); rdf
    }
    pub fn with_rdf_mapping(mut self, rdf: super::rdf::RdfDatasetMapping) -> Self {
        self.tables.extend(rdf.registered_tables()); self.rdf = rdf; self
    }
    pub fn new() -> Self {
        Self::default()
    }
    /// Register physical source facts. Backend-specific options are validated
    /// by the owning engine when it lowers the relational operation.
    pub fn register_source_metadata(&mut self, metadata: super::source_metadata::SourceMetadata) -> RelResult<&mut Self> {
        if metadata.table.is_empty() || metadata.format.as_ref().is_some_and(String::is_empty) {
            return Err(RelError::Unsupported("source metadata needs a table and a nonempty format when specified".into()));
        }
        let mut indexes = std::collections::HashSet::new();
        for index in &metadata.indexes {
            if index.column.is_empty() || !indexes.insert((&index.column, &index.metric)) {
                return Err(RelError::Unsupported("source indexes need unique column and metric pairs".into()));
            }
        }
        if self.source_metadata.contains_key(&metadata.table) {
            return Err(RelError::Unsupported(format!("duplicate source metadata for {}", metadata.table)));
        }
        self.source_metadata.insert(metadata.table.clone(), metadata);
        Ok(self)
    }

    /// Compatibility input; execution only consumes source metadata.
    pub fn register_search_index(&mut self, index: super::search::SearchIndex) -> RelResult<&mut Self> {
        let metadata = super::source_metadata::SourceMetadata::from(index);
        if let Some(existing) = self.source_metadata.get_mut(&metadata.table) {
            if existing.format != metadata.format || existing.options != metadata.options {
                return Err(RelError::Unsupported(format!("conflicting source metadata for {}", metadata.table)));
            }
            let index = metadata.indexes.into_iter().next().unwrap();
            if index.column.is_empty() || existing.indexes.iter().any(|i| i.column == index.column && i.metric == index.metric) {
                return Err(RelError::Unsupported("source indexes need unique column and metric pairs".into()));
            }
            existing.indexes.push(index);
            return Ok(self);
        }
        self.register_source_metadata(metadata)
    }

    /// Validate row ownership before either compiling reads or executing writes.
    pub fn validate_foreign_keys(&self) -> RelResult<()> {
        for node in self.nodes.values() {
            node.id_column.validate()?;
        }
        for edge in self.edges.values() {
            edge.src_column.validate()?;
            edge.dst_column.validate()?;
            if let Some(id) = &edge.id_column {
                id.validate()?;
            }
            for (label, endpoint) in [
                (&edge.src_label, &edge.src_column),
                (&edge.dst_label, &edge.dst_column),
            ] {
                if let Some(node) = self.node(label) {
                    if endpoint.len() != node.id_column.len() {
                        return Err(RelError::Unsupported(format!(
                            "relationship `{}` endpoint `{label}` has the wrong key arity",
                            edge.rel_type
                        )));
                    }
                }
            }
            let Some((child_label, child_key, parent_label, fk)) = edge.foreign_key_columns()
            else {
                continue;
            };
            let invalid = |reason: &str| {
                RelError::Unsupported(format!(
                    "foreign-key relationship `{}`: {reason}",
                    edge.rel_type
                ))
            };
            let child = self
                .node(child_label)
                .ok_or_else(|| invalid("missing child mapping"))?;
            let parent = self
                .node(parent_label)
                .ok_or_else(|| invalid("missing parent mapping"))?;
            let MappedSource::Table(table) = &edge.source else {
                return Err(invalid("requires a writable child table"));
            };
            if edge.source != child.source
                || &child.id_column != child_key
                || edge.id_column.as_ref() != Some(child_key)
            {
                return Err(invalid(
                    "source and edge identity must match the child table and primary key",
                ));
            }
            let MappedSource::Table(parent_table) = &parent.source else {
                return Err(invalid("parent must be table-backed"));
            };
            if let (Some(child_provider), Some(parent_provider)) =
                (self.tables.get(table), self.tables.get(parent_table))
            {
                let schema = child_provider.schema();
                child_key.data_type(&schema).map_err(|e| invalid(&e))?;
                if fk.data_type(&schema).map_err(|e| invalid(&e))?
                    != parent
                        .id_column
                        .data_type(&parent_provider.schema())
                        .map_err(|e| invalid(&e))?
                {
                    return Err(invalid(
                        "FK component types must match the parent key in declared order",
                    ));
                }
            }
            if self
                .nodes
                .values()
                .any(|node| node.source == edge.source && &node.id_column != child_key)
            {
                return Err(invalid(
                    "all node mappings of the child table must use the same primary key",
                ));
            }
            let owns = |column: &str| {
                !child_key.contains(column)
                    && (fk.contains(column) || edge.properties.values().any(|c| c == column))
            };
            if self
                .nodes
                .values()
                .any(|node| node.source == edge.source && node.properties.values().any(|c| owns(c)))
                || edge
                    .properties
                    .values()
                    .any(|c| fk.contains(c) && !child_key.contains(c))
            {
                return Err(invalid(
                    "FK and relationship property columns cannot also be mapped as node properties, except shared primary-key components",
                ));
            }
            for other in self
                .edges
                .values()
                .filter(|other| other.rel_type != edge.rel_type && other.source == edge.source)
            {
                let Some((_, other_key, _, other_fk)) = other.foreign_key_columns() else {
                    return Err(invalid(
                        "cannot share its child table with an independent edge-row mapping",
                    ));
                };
                if other_key != child_key
                    || other_fk == fk
                    || other_fk.columns().iter().any(|c| owns(c))
                    || other.properties.values().any(|c| owns(c))
                {
                    return Err(invalid(
                        "relationship columns must have one write owner; only primary-key components may be shared",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Supply engine-neutral constraints. The caller owns enforcement and revision validity.
    pub fn set_constraints(&mut self, catalog: super::constraints::ConstraintCatalog) -> &mut Self {
        self.constraints = catalog;
        self
    }
    pub fn constraints(&self) -> &super::constraints::ConstraintCatalog {
        &self.constraints
    }
    /// Activate facts validated in this exact caller-owned immutable snapshot.
    /// Never reuse a mapping with this scope after writes or snapshot changes.
    pub fn set_constraint_scope(&mut self, scope: Option<String>) -> &mut Self {
        self.constraint_scope = scope;
        self
    }
    pub fn validate_constraints(&self) -> RelResult<()> {
        let schemas = self
            .tables
            .iter()
            .map(|(n, p)| (n.clone(), p.schema()))
            .collect();
        self.constraints
            .validate(&schemas)
            .map_err(RelError::Unsupported)
    }
    fn constrained_provider(&self, name: &str) -> DFResult<Arc<dyn TableProvider>> {
        self.validate_constraints()
            .map_err(|e| DataFusionError::Plan(e.to_string()))?;
        let p = self
            .tables
            .get(name)
            .ok_or_else(|| DataFusionError::Plan(format!("unknown mapped table {name}")))?;
        // Rebind view definitions against the current supplied catalog, so a
        // replacement catalog cannot leave old proofs captured inside a view.
        let p = if let Some(source) = self.representation_sources.get(name) {
            self.build_representation(source).map_err(|e| DataFusionError::Plan(e.to_string()))?.0
        } else if let Some(source) = self.collection_sources.get(name) {
            let parent = self.source_plan(&MappedSource::Table(source.table.clone()))
                .map_err(|e| DataFusionError::Plan(e.to_string()))?;
            Arc::new(ViewTable::new(source.plan(parent, self).map_err(|e| DataFusionError::Plan(e.to_string()))?, None)) as Arc<dyn TableProvider>
        } else if p.get_logical_plan().is_some() {
            if let Some(sql) = p.get_table_definition() {
                Arc::new(ViewTable::new(
                    self.plan_sql(sql)
                        .map_err(|e| DataFusionError::Plan(e.to_string()))?,
                    Some(sql.into()),
                )) as Arc<dyn TableProvider>
            } else {
                p.clone()
            }
        } else {
            p.clone()
        };
        // Collection relations must inline; a constraint wrapper would hide
        // the expansion behind a table name that does not exist externally.
        // Facts on the parent are still attached inside the expanded plan.
        if self.collection_sources.contains_key(name) { return Ok(p); }
        super::constraints::bind(name, p, &self.constraints, self.constraint_scope.as_deref())
    }
    /// Plan a relational query against the supplied mapping schemas and constraints.
    pub fn relational_plan(&self, sql: &str) -> RelResult<LogicalPlan> {
        self.plan_sql(sql)
    }

    /// Degree upper bounds and endpoint integrity derived from supplied facts.
    pub fn relationship_multiplicity(
        &self,
        rel_type: &str,
    ) -> RelResult<super::constraints::RelationshipMultiplicity> {
        use super::constraints::{RelationshipMultiplicity, analyze};
        let edge = self
            .edge(rel_type)
            .ok_or_else(|| RelError::Unsupported(format!("unknown relationship {rel_type}")))?;
        let plan = self.source_plan(&edge.source)?;
        let p = analyze(&plan);
        let index = |name: &str| {
            plan.schema()
                .index_of_column_by_name(None, name)
                .ok_or_else(|| RelError::Unsupported(format!("missing endpoint {name}")))
        };
        let src = edge
            .src_column
            .columns()
            .iter()
            .map(|c| index(c))
            .collect::<RelResult<Vec<_>>>()?;
        let dst = edge
            .dst_column
            .columns()
            .iter()
            .map(|c| index(c))
            .collect::<RelResult<Vec<_>>>()?;
        let exists = |indices: &[usize], label: &str| -> RelResult<bool> {
            let node = self
                .node(label)
                .ok_or_else(|| RelError::Unsupported(format!("unknown endpoint label {label}")))?;
            let target = self.source_plan(&node.source)?;
            let t = analyze(&target);
            let keys = node
                .id_column
                .columns()
                .iter()
                .map(|name| {
                    target
                        .schema()
                        .index_of_column_by_name(None, name)
                        .ok_or_else(|| RelError::Unsupported("missing target identity".into()))
                })
                .collect::<RelResult<Vec<_>>>()?;
            if keys.len() != indices.len() || !indices.iter().all(|i| p.non_null.contains(i)) {
                return Ok(false);
            }
            let target_origins = keys
                .iter()
                .map(|i| t.origins.get(*i).and_then(|o| o.as_ref()))
                .collect::<Option<Vec<_>>>();
            let Some(target_origins) = target_origins else {
                return Ok(false);
            };
            Ok(p.foreign_keys.iter().any(|f| {
                f.columns == indices
                    && t.complete_sources.contains(&f.target)
                    && target_origins.iter().all(|o| o.table == f.target)
                    && f.references
                        == target_origins
                            .iter()
                            .map(|o| o.column.clone())
                            .collect::<Vec<_>>()
            }) || indices.iter().zip(&target_origins).all(|(i, target)| {
                p.origins
                    .get(*i)
                    .and_then(|o| o.as_ref())
                    .is_some_and(|o| t.complete_sources.contains(&o.table) && o == *target)
            }))
        };
        Ok(RelationshipMultiplicity {
            at_most_one_outgoing: p.unique_on(&src, false),
            at_most_one_incoming: p.unique_on(&dst, false),
            source_endpoint_exists: exists(&src, &edge.src_label)?,
            target_endpoint_exists: exists(&dst, &edge.dst_label)?,
        })
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
        let name = name.into();
        self.view_dependencies.remove(&name);
        self.logical_sources.remove(&name);
        self.collection_sources.remove(&name);
        self.representation_sources.remove(&name);
        self.tables.insert(name, provider);
        // TOML catalogs bind once their physical schemas/providers are supplied.
        // Incomplete or invalid declarations remain unbound.
        for (name, definition) in &self.logical_sources {
            if let Ok(provider) = super::layout::LayoutProvider::try_new(definition.clone(), &self.tables) {
                self.tables.insert(name.clone(), Arc::new(provider));
            } else {
                self.tables.remove(name);
            }
        }
        self.refresh_derived();
        self
    }

    fn refresh_derived(&mut self) {
        // Binding schemas must not validate facts against a half-built catalog.
        // constrained_provider attaches the current facts when a source is used.
        let constraints = std::mem::take(&mut self.constraints);
        let collections = self.collection_sources.values().cloned().collect::<Vec<_>>();
        let representations = self.representation_sources.values().cloned().collect::<Vec<_>>();
        for name in self.collection_sources.keys().chain(self.representation_sources.keys()) { self.tables.remove(name); }
        // Bind a dependency DAG, independent of TOML declaration ordering.
        for _ in 0..collections.len() + representations.len() {
            let before = self.tables.len();
            for source in &collections {
                if !self.tables.contains_key(&source.name) { let _ = self.bind_collection(source); }
            }
            for source in &representations {
                if !self.tables.contains_key(&source.name) {
                    if let Ok((provider, dependencies)) = self.build_representation(source) {
                        self.tables.insert(source.name.clone(), provider);
                        self.view_dependencies.insert(source.name.clone(), dependencies);
                    }
                }
            }
            if self.tables.len() == before { break; }
        }
        self.constraints = constraints;
    }

    fn check_source_cycle(&self, name: &str, dependencies: &BTreeSet<String>) -> RelResult<()> {
        let mut pending = dependencies.iter().cloned().collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(dependency) = pending.pop() {
            if dependency == name { return Err(RelError::Unsupported(format!("cyclic mapped source {name}"))); }
            if seen.insert(dependency.clone()) {
                pending.extend(self.view_dependencies.get(&dependency).into_iter().flatten().cloned());
            }
        }
        Ok(())
    }

    fn build_representation(&self, source: &super::representation::RepresentationSource) -> RelResult<(Arc<dyn TableProvider>, BTreeSet<String>)> {
        use super::representation::{RepresentationInput, RepresentationProvider};
        let mut plans = Vec::new();
        let mut dependencies = BTreeSet::new();
        for candidate in &source.representations {
            // Check named dependencies before resolving their providers, including replacements.
            let (plan, inputs) = match &candidate.source {
                RepresentationInput::Table {name} => {
                    let inputs = BTreeSet::from([name.clone()]);
                    self.check_source_cycle(&source.name, &inputs)?;
                    (self.source_plan(&MappedSource::Table(name.clone()))?, inputs)
                }
                RepresentationInput::Query {sql} => {
                    let (plan, inputs) = self.plan_sql_with_dependencies(sql)?;
                    self.check_source_cycle(&source.name, &inputs)?;
                    (plan, inputs)
                }
            };
            dependencies.extend(inputs);
            plans.push(plan);
        }
        Ok((Arc::new(RepresentationProvider::try_new(source.clone(), plans, &self.tables)?), dependencies))
    }

    /// Register equivalent derived relations and materializations under one name.
    pub fn register_representation_source(&mut self, source: super::representation::RepresentationSource) -> RelResult<&mut Self> {
        if self.tables.contains_key(&source.name) && !self.representation_sources.contains_key(&source.name) {
            return Err(RelError::Unsupported(format!("representation name `{}` is already registered", source.name)));
        }
        let (provider, dependencies) = self.build_representation(&source)?;
        self.tables.insert(source.name.clone(), provider);
        self.view_dependencies.insert(source.name.clone(), dependencies);
        self.representation_sources.insert(source.name.clone(), source);
        self.refresh_derived();
        Ok(self)
    }
    /// Portable metadata for explicit statistics generation; only registered sources are included.
    pub fn statistics_request(&self) -> RelResult<serde_json::Value> {
        use serde_json::json;
        let mut tables=Vec::new();
        for (name,p) in &self.tables {
            if self.logical_sources.contains_key(name) || self.collection_sources.contains_key(name) || self.representation_sources.contains_key(name) || p.get_logical_plan().is_some() {continue;}
            let columns=p.schema().fields().iter().map(|f|Ok(json!({"name":f.name(),"data_type":super::statistics::type_name(f.data_type()).map_err(RelError::Unsupported)?,"nullable":f.is_nullable()}))).collect::<RelResult<Vec<_>>>()?;
            tables.push(json!({"name":name,"columns":columns}));
        }
        let nodes=self.nodes.values().filter_map(|m|if let MappedSource::Table(table)=&m.source{Some(json!({"label":m.label,"table":table,"id":m.id_column.columns(),"properties":m.properties}))}else{None}).collect::<Vec<_>>();
        let edges=self.edges.values().filter_map(|m|if let MappedSource::Table(table)=&m.source{Some(json!({"label":m.rel_type,"table":table,"source":m.src_column.columns(),"target":m.dst_column.columns(),"source_label":m.src_label,"target_label":m.dst_label,"properties":m.properties}))}else{None}).collect::<Vec<_>>();
        Ok(json!({"version":1,"dialect":"duckdb","language":"cypher","query":"RETURN 1", "tables":tables,"nodes":nodes,"edges":edges,"rdf":self.rdf.relational,"logical_sources":self.logical_sources.values().collect::<Vec<_>>(),"collection_sources":self.collection_sources.values().collect::<Vec<_>>(),"representation_sources":self.representation_sources.values().collect::<Vec<_>>()}))
    }
    /// Install optional generated estimates; no data access or constraint discovery.
    pub fn set_statistics(&mut self, snapshot: Arc<super::statistics::StatisticsSnapshot>) -> RelResult<&mut Self> {
        snapshot.validate_estimates().map_err(RelError::Unsupported)?;
        self.clear_statistics();
        for (name, stats) in &snapshot.sources {
            if let Some(p) = self.tables.get(name).cloned() {
                if super::statistics::schema_fingerprint(p.schema().as_ref()) == stats.schema {
                    self.tables.insert(name.clone(), super::statistics::StatisticsProvider::wrap(p, snapshot.clone(), name.clone()));
                }
            }
        }
        self.statistics = Some(snapshot);
        self.refresh_statistics_sources();
        Ok(self)
    }
    pub fn statistics(&self) -> Option<&Arc<super::statistics::StatisticsSnapshot>> { self.statistics.as_ref() }
    pub fn clear_statistics(&mut self) {
        self.statistics = None;
        for p in self.tables.values_mut() {
            if let Some(stats) = p.as_any().downcast_ref::<super::statistics::StatisticsProvider>() { *p = stats.inner.clone(); }
        }
        self.refresh_statistics_sources();
    }
    fn refresh_statistics_sources(&mut self) {
        for source in self.logical_sources.values() {
            if let Ok(provider) = super::layout::LayoutProvider::try_new(source.clone(), &self.tables) {self.tables.insert(source.name.clone(), Arc::new(provider));}
        }
        self.refresh_derived();
    }
    pub fn representation_source(&self, name: &str) -> Option<&super::representation::RepresentationSource> {
        self.representation_sources.get(name)
    }
    pub(crate) fn source_access_cost(&self, source:&MappedSource, keys:&KeyColumns)->Option<(f64,f64,f64,f64)> {
        self.statistics()?;
        let selected=super::representation::select(self.source_plan(source).ok()?).ok()?;
        super::statistics::source_access_cost(&selected.plan,keys.columns())
    }
    pub(crate) fn derived_source_sql(&self, source: &MappedSource, filters: &[Expr]) -> RelResult<Option<String>> {
        match source {
            MappedSource::Table(name) if !self.representation_sources.contains_key(name) && !self.collection_sources.contains_key(name) => return Ok(None),
            MappedSource::Query(_) if self.representation_sources.is_empty() && self.collection_sources.is_empty() => return Ok(None),
            MappedSource::Query(sql) => {
                // Database-owned queries need no registered schema. Replan only
                // queries that reference catalog-defined derived relations.
                use datafusion::sql::sqlparser::{ast::visit_relations, dialect::GenericDialect, parser::Parser};
                let statements = Parser::parse_sql(&GenericDialect {}, sql)
                    .map_err(|e| RelError::Unsupported(format!("mapping query parse: {e}")))?;
                let derived = visit_relations(&statements, |name| {
                    let name = name.0.iter().filter_map(|p| p.as_ident().map(|i| i.value.clone())).collect::<Vec<_>>().join(".");
                    if self.representation_sources.contains_key(&name) || self.collection_sources.contains_key(&name) {
                        std::ops::ControlFlow::Break(())
                    } else { std::ops::ControlFlow::Continue(()) }
                });
                if derived.is_continue() { return Ok(None); }
            }
            _ => (),
        }
        let mut plan = LogicalPlanBuilder::from(self.source_plan(source)?);
        for filter in filters { plan = plan.filter(filter.clone())?; }
        Ok(Some(super::sql::unparse_plan(plan.build()?, super::sql::SqlDialect::DuckDb)
            .map_err(|e| RelError::Unsupported(e.to_string()))?))
    }

    fn bind_collection(&mut self, source: &super::collection_source::CollectionSource) -> RelResult<()> {
        self.check_source_cycle(&source.name, &BTreeSet::from([source.table.clone()]))?;
        let provider = self.tables.get(&source.table).ok_or_else(|| RelError::Unsupported(format!("unregistered collection parent `{}`", source.table)))?;
        source.validate(&provider.schema())?;
        let plan = source.plan(self.source_plan(&MappedSource::Table(source.table.clone()))?, self)?;
        self.view_dependencies.insert(source.name.clone(), BTreeSet::from([source.table.clone()]));
        self.tables.insert(source.name.clone(), Arc::new(ViewTable::new(plan, None)));
        Ok(())
    }

    /// Expand one list column as a named read-only relation.
    pub fn register_collection_source(&mut self, source: super::collection_source::CollectionSource) -> RelResult<&mut Self> {
        if self.tables.contains_key(&source.name) && !self.collection_sources.contains_key(&source.name) {
            return Err(RelError::Unsupported(format!("collection source name `{}` is already registered", source.name)));
        }
        self.bind_collection(&source)?;
        self.collection_sources.insert(source.name.clone(), source);
        self.refresh_derived();
        Ok(self)
    }
    pub fn collection_source(&self, name: &str) -> Option<&super::collection_source::CollectionSource> {
        self.collection_sources.get(name)
    }
    pub fn table_schema(&self, name: &str) -> Option<arrow::datatypes::SchemaRef> {
        self.tables.get(name).map(|p| p.schema())
    }
    /// DuckDB runtime derived query; predicates still apply to the expanded rows.
    pub fn collection_sql(&self, name: &str) -> Option<String> {
        if !self.collection_sources.contains_key(name) { return None; }
        self.derived_source_sql(&MappedSource::Table(name.into()), &[]).ok().flatten()
    }

    /// Register equivalent physical tables under a logical table name. Node,
    /// edge, RDF and SQL mappings can all refer to this name.
    pub fn register_logical_source(&mut self, source: super::layout::LogicalSource) -> RelResult<&mut Self> {
        let name = source.name.clone();
        if self.tables.contains_key(&name) && !self.logical_sources.contains_key(&name) {
            return Err(RelError::Unsupported(format!("logical source name `{name}` is already registered")));
        }
        let provider = super::layout::LayoutProvider::try_new(source.clone(), &self.tables)?;
        self.logical_sources.insert(name.clone(), source);
        self.view_dependencies.insert(name.clone(), self.logical_sources[&name].layouts.iter().map(|l|l.table.clone()).collect());
        self.tables.insert(name, Arc::new(provider));
        self.refresh_derived();
        Ok(self)
    }

    pub fn logical_source(&self, name: &str) -> Option<&super::layout::LogicalSource> {
        self.logical_sources.get(name)
    }

    /// Resolve a runtime lookup using the same cost model as logical scans.
    pub fn resolve_table(&self, name: &str, filters: &[Expr]) -> String {
        self.tables.get(name).and_then(|p| p.as_any().downcast_ref::<super::layout::LayoutProvider>())
            .map(|p| p.choose(filters).0.table.clone()).unwrap_or_else(|| name.into())
    }

    /// Register `name` as a SQL-defined view over previously registered
    /// tables. In-process it executes as a real DataFusion view (so filters
    /// and projections push through it); on an external engine the name is
    /// expected to exist as a table or view in the target database.
    pub fn register_view(&mut self, name: impl Into<String>, sql: &str) -> RelResult<&mut Self> {
        let name = name.into();
        let (plan, dependencies) = self.plan_sql_with_dependencies(sql)?;
        let mut pending = dependencies.iter().cloned().collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        while let Some(dependency) = pending.pop() {
            if dependency == name {
                return Err(RelError::Unsupported(format!("cyclic mapped view {name}")));
            }
            if seen.insert(dependency.clone()) {
                pending.extend(
                    self.view_dependencies
                        .get(&dependency)
                        .into_iter()
                        .flatten()
                        .cloned(),
                );
            }
        }
        self.view_dependencies.insert(name.clone(), dependencies);
        self.tables
            .insert(name, Arc::new(ViewTable::new(plan, Some(sql.to_string()))));
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
        self.tables
            .keys()
            .cloned()
            .chain(
                self.nodes
                    .values()
                    .map(|m| &m.source)
                    .chain(self.edges.values().map(|m| &m.source))
                    .filter_map(|source| match source {
                        MappedSource::Table(name) => Some(name.clone()),
                        MappedSource::Query(_) | MappedSource::Computed(_) => None,
                    }),
            )
            .collect()
    }

    /// Build the source plan for a mapped source: a bare scan for
    /// table-backed sources, the parsed query plan for query-backed ones.
    pub(crate) fn source_plan(&self, source: &MappedSource) -> RelResult<LogicalPlan> {
        match source {
            MappedSource::Table(name) => {
                let _provider = self.tables.get(name).ok_or_else(|| {
                    RelError::Unsupported(format!(
                        "mapping references table `{name}` but no provider/schema is registered; \
                         call GraphMapping::register_table or register_table_schema"
                    ))
                })?;
                Ok(LogicalPlanBuilder::scan(
                    name.clone(),
                    provider_as_source(self.constrained_provider(name)?),
                    None,
                )?
                .build()?)
            }
            MappedSource::Query(sql) => self.plan_sql(sql),
            MappedSource::Computed(rule) => computed::plan(self, rule),
        }
    }

    /// Bind an immutable scalar expression against an explicit row schema.
    pub(crate) fn bind_scalar(&self, text: &str, schema: &DFSchema) -> RelResult<Expr> {
        use datafusion::sql::{sqlparser::{parser::Parser,dialect::GenericDialect,tokenizer::Token},planner::PlannerContext};
        use datafusion::common::tree_node::{TreeNode,TreeNodeRecursion};
        let mut parser=Parser::new(&GenericDialect {}).try_with_sql(text).map_err(|e|RelError::Unsupported(format!("mapping expression: {e}")))?;
        let mut ast=parser.parse_expr().map_err(|e|RelError::Unsupported(format!("mapping expression: {e}")))?;
        normalize_json_literals(&mut ast)?;
        if parser.peek_token().token!=Token::EOF {return Err(RelError::Unsupported("mapping expressions cannot contain statements or trailing SQL".into()));}
        let provider=MappingContextProvider::new(self);
        let expression=SqlToRel::new(&provider).sql_to_expr(ast,schema,&mut PlannerContext::new())?;
        expression.apply(|expr| {
            if matches!(expr,Expr::AggregateFunction(_)|Expr::WindowFunction(_)|Expr::ScalarSubquery(_)|Expr::Exists(_)|Expr::InSubquery(_)) {return Err(DataFusionError::Plan("mapping expressions must be scalar, without subqueries, aggregates or windows".into()));}
            if let Expr::ScalarFunction(call)=expr {if !crate::ir::functions::immutable_call(&call.func,&call.args,schema)? {return Err(DataFusionError::Plan("mapping expressions must be immutable".into()));}}

            Ok(TreeNodeRecursion::Continue)
        })?;
        Ok(expression.transform_up(|expr| {
            if let Expr::Alias(alias) = expr {
                Ok(datafusion::common::tree_node::Transformed::yes(*alias.expr))
            } else {
                Ok(datafusion::common::tree_node::Transformed::no(expr))
            }
        })?.data)
    }
    /// Parse a SQL `SELECT` against the registered tables using datafusion-sql.
    fn plan_sql(&self, sql: &str) -> RelResult<LogicalPlan> {
        Ok(self.plan_sql_with_dependencies(sql)?.0)
    }
    fn plan_sql_with_dependencies(&self, sql: &str) -> RelResult<(LogicalPlan, BTreeSet<String>)> {
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
        let plan = planner
            .statement_to_plan(statement)
            .map_err(|err| RelError::Unsupported(format!("mapping query plan: {err}")))?;
        Ok((plan, provider.requested.into_inner()))
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
        mapping.validate_foreign_keys()?;
        let mut plan = mapping.source_plan(&edge.source)?;
        if let Some((_, _, _, fk)) = edge.foreign_key_columns() {
            for column in fk.columns() {
                let column = resolve_column(&plan, column)?;
                plan = LogicalPlanBuilder::from(plan)
                    .filter(col_exact(column).is_not_null())?
                    .build()?;
            }
        }
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
        let id_column = edge.id_column.as_ref().unwrap_or(&edge.src_column);
        let mut exprs = vec![
            id_expr(&plan, id_column, &what)?.alias(id_col(binding)),
            lit(edge.rel_type.as_str()).alias(label_col(binding)),
            lit(edge.src_label.as_str()).alias(src_label_col(binding)),
            endpoint_expr(mapping, &plan, &edge.src_column, &edge.src_label, &what)?
                .alias(src_id_col(binding)),
            lit(edge.dst_label.as_str()).alias(dst_label_col(binding)),
            endpoint_expr(mapping, &plan, &edge.dst_column, &edge.dst_label, &what)?
                .alias(dst_id_col(binding)),
        ];
        exprs.extend(property_exprs(binding, &plan, &edge.properties, &defs)?);
        branches.push(LogicalPlanBuilder::from(plan).project(exprs)?.build()?);
    }
    Ok(LoweredNode::new(union_all(branches)?))
}

pub(super) fn resolve_names(
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

/// Reference an id column in its source type. Branches of multi-label scans
/// are reconciled afterwards by [`union_all`].
pub(crate) fn id_expr(plan: &LogicalPlan, columns: &KeyColumns, what: &str) -> RelResult<Expr> {
    columns.validate()?;
    let mut args = Vec::new();
    let mut fields = Vec::new();
    for (i, column) in columns.columns().iter().enumerate() {
        let kind = source_column_type(plan, column, what)?;
        if !is_scalar_identity_type(&kind) {
            return Err(RelError::Unsupported(format!(
                "{what}: key component `{column}` is not scalar"
            )));
        }
        let value = col_exact(resolve_column(plan, column)?);
        if columns.len() == 1 {
            return Ok(value);
        }
        let name = format!("k{i}");
        fields.push(Arc::new(arrow::datatypes::Field::new(&name, kind, true)));
        args.extend([lit(name), value]);
    }
    Ok(Expr::Cast(Cast::new(
        Box::new(datafusion::functions::core::expr_fn::named_struct(args)),
        DataType::Struct(fields.into()),
    )))
}

fn endpoint_expr(
    mapping: &GraphMapping,
    plan: &LogicalPlan,
    columns: &KeyColumns,
    label: &str,
    what: &str,
) -> RelResult<Expr> {
    let node = mapping
        .node(label)
        .ok_or_else(|| RelError::Unsupported(format!("unmapped endpoint `{label}`")))?;
    let parent = mapping.source_plan(&node.source)?;
    let kind = node
        .id_column
        .data_type(parent.schema().as_arrow())
        .map_err(RelError::Unsupported)?;
    // Endpoint columns may have different source widths. Normalize each tuple
    // to its node mapping before joins or heterogeneous identity widening.
    Ok(Expr::Cast(Cast::new(
        Box::new(id_expr(plan, columns, what)?),
        kind,
    )))
}

fn is_integer_type(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
    )
}

/// Non-null scalar key types. Equality follows the execution engine, including
/// floating-point equality. Nested values are not scalar identities.
pub(crate) fn is_scalar_identity_type(data_type: &DataType) -> bool {
    if let DataType::Dictionary(_, value_type) = data_type {
        return is_scalar_identity_type(value_type);
    }
    is_integer_type(data_type)
        || matches!(
            data_type,
            DataType::Boolean
                | DataType::Float16
                | DataType::Float32
                | DataType::Float64
                | DataType::Utf8
                | DataType::LargeUtf8
                | DataType::Utf8View
                | DataType::Binary
                | DataType::LargeBinary
                | DataType::BinaryView
                | DataType::FixedSizeBinary(_)
                | DataType::Decimal32(_, _)
                | DataType::Decimal64(_, _)
                | DataType::Decimal128(_, _)
                | DataType::Decimal256(_, _)
                | DataType::Date32
                | DataType::Date64
                | DataType::Timestamp(_, _)
                | DataType::Time32(_)
                | DataType::Time64(_)
                | DataType::Duration(_)
                | DataType::Interval(_)
        )
}

pub(crate) fn is_identity_type(kind: &DataType) -> bool {
    is_scalar_identity_type(kind)
        || matches!(kind, DataType::Struct(fields)
        if fields.len() > 1 && fields.iter().all(|f| is_scalar_identity_type(f.data_type())))
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

/// Union per-label branches. Each label keeps its native id type; when
/// branches disagree, the union alone carries a shared type: `Int64` for
/// integers of different widths, otherwise text. Identity stays qualified by
/// the label column, so the text form cannot merge elements of different labels.
fn union_all(mut branches: Vec<LogicalPlan>) -> RelResult<LogicalPlan> {
    if branches.len() > 1 {
        let width = branches[0].schema().fields().len();
        let mut targets = BTreeMap::new();
        for index in 0..width {
            let name = branches[0].schema().field(index).name().clone();
            if ![super::ID_SUFFIX, super::SRC_ID_SUFFIX, super::DST_ID_SUFFIX]
                .iter()
                .any(|suffix| name.ends_with(suffix))
            {
                continue;
            }
            let types = branches
                .iter()
                .map(|branch| branch.schema().field(index).data_type().clone())
                .collect::<BTreeSet<_>>();
            if types.len() > 1 {
                let target = crate::ir::identity::identity_union_type(types.into_iter());
                targets.insert(index, target);
            }
        }
        if !targets.is_empty() {
            branches = branches
                .into_iter()
                .map(|branch| {
                    let exprs = branch
                        .schema()
                        .fields()
                        .iter()
                        .enumerate()
                        .map(|(index, field)| {
                            let column = col_exact(field.name().clone());
                            match targets.get(&index) {
                                Some(target) if field.data_type() != target => {
                                    super::columns::cast_identity_expr(
                                        column,
                                        field.data_type(),
                                        target,
                                    )
                                    .alias(field.name())
                                }
                                _ => column,
                            }
                        })
                        .collect::<Vec<_>>();
                    Ok(LogicalPlanBuilder::from(branch).project(exprs)?.build()?)
                })
                .collect::<RelResult<Vec<_>>>()?;
        }
    }
    let first = branches.remove(0);
    let mut builder = LogicalPlanBuilder::from(first);
    for branch in branches {
        builder = builder.union(branch)?;
    }
    Ok(builder.build()?)
}

/// Preserve the logical JSON type in SQL-like mapping expressions.
pub(crate) fn normalize_json_literals(expr: &mut datafusion::sql::sqlparser::ast::Expr) -> RelResult<()> {
    use datafusion::sql::sqlparser::{ast,parser::Parser,dialect::GenericDialect};
    let flow=ast::visit_expressions_mut(expr,|expr| {
        if let ast::Expr::TypedString(literal)=expr {
            if matches!(literal.data_type,ast::DataType::JSON|ast::DataType::JSONB) {
                let Some(text)=literal.value.clone().into_string() else {return std::ops::ControlFlow::Break(RelError::Unsupported("JSON literal requires text".into()));};
                if let Err(error)=crate::ir::functions::domain::json_scalar(&text) {return std::ops::ControlFlow::Break(RelError::Unsupported(error.to_string()));}
                let text=format!("json.parse('{}')",text.replace('\'',"''"));
                match Parser::new(&GenericDialect{}).try_with_sql(&text).and_then(|mut parser|parser.parse_expr()) {
                    Ok(replacement)=>*expr=replacement,
                    Err(error)=>return std::ops::ControlFlow::Break(RelError::Unsupported(error.to_string())),
                }
            }
        }
        std::ops::ControlFlow::Continue(())
    });
    match flow {std::ops::ControlFlow::Break(error)=>Err(error),_=>Ok(())}
}

// ---------------------------------------------------------------------------
// SQL planning support for query-backed sources
// ---------------------------------------------------------------------------

struct MappingContextProvider<'a> {
    mapping: &'a GraphMapping,
    requested: std::cell::RefCell<BTreeSet<String>>,
    options: ConfigOptions,
}

impl<'a> MappingContextProvider<'a> {
    fn new(mapping: &'a GraphMapping) -> Self {
        Self {
            mapping,
            requested: Default::default(),
            options: ConfigOptions::default(),
        }
    }
}

impl ContextProvider for MappingContextProvider<'_> {
    fn get_table_source(&self, name: TableReference) -> DFResult<Arc<dyn TableSource>> {
        let qualified = name.to_string();
        let table = if self.mapping.tables.contains_key(&qualified) {
            qualified.as_str()
        } else {
            name.table()
        };
        self.requested.borrow_mut().insert(table.to_string());
        match self.mapping.tables.get(table) {
            Some(_) => Ok(provider_as_source(
                self.mapping.constrained_provider(table)?,
            )),
            None => Err(DataFusionError::Plan(format!(
                "mapping query references unknown table `{table}`; register it on the GraphMapping"
            ))),
        }
    }

    fn get_function_meta(&self, name: &str) -> Option<Arc<ScalarUDF>> {
        let lower = name.to_ascii_lowercase();
        self.mapping.logical_functions.get(&lower).cloned()
            .or_else(|| crate::ir::functions::search::function(&lower))
            .or_else(|| crate::ir::functions::json::function(&lower))
            .or_else(|| crate::ir::functions::catalog_scalar(&lower))
    }

    fn get_aggregate_meta(&self, name: &str) -> Option<Arc<AggregateUDF>> {
        let lower = name.to_ascii_lowercase();
        crate::ir::functions::json::aggregate(&lower).or_else(|| datafusion::functions_aggregate::all_default_aggregate_functions().into_iter().find(|f|f.name()==lower || f.aliases().contains(&lower)))
    }

    fn get_window_meta(&self, name: &str) -> Option<Arc<WindowUDF>> {
        let lower = name.to_ascii_lowercase();
        datafusion::functions_window::all_default_window_functions().into_iter().find(|f|f.name()==lower || f.aliases().contains(&lower))
    }

    fn get_variable_type(&self, _variable_names: &[String]) -> Option<DataType> {
        None
    }

    fn options(&self) -> &ConfigOptions {
        &self.options
    }

    fn udf_names(&self) -> Vec<String> {
        self.mapping.logical_functions.keys().cloned().collect()
    }

    fn udaf_names(&self) -> Vec<String> {
        datafusion::functions_aggregate::all_default_aggregate_functions().iter().map(|f|f.name().into()).collect()
    }

    fn udwf_names(&self) -> Vec<String> {
        datafusion::functions_window::all_default_window_functions().iter().map(|f|f.name().into()).collect()
    }
}

// ---------------------------------------------------------------------------
// TOML (de)serialization
// ---------------------------------------------------------------------------

impl GraphMapping {
    /// Parse a mapping from TOML. Table providers are *not*
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
        let mut document: toml::Value = toml::from_str(input)
            .map_err(|e| RelError::Unsupported(format!("mapping TOML: {e}")))?;
        let source_metadata = document.as_table_mut().and_then(|t| t.remove("source_metadata"))
            .map(|v| v.try_into::<Vec<super::source_metadata::SourceMetadata>>()).transpose()
            .map_err(|e| RelError::Unsupported(e.to_string()))?.unwrap_or_default();
        let indexes = document.as_table_mut().and_then(|t|t.remove("search_indexes"))
            .map(|v|v.try_into::<Vec<super::search::SearchIndex>>()).transpose().map_err(|e|RelError::Unsupported(e.to_string()))?.unwrap_or_default();
        let mut computed = Vec::new();
        if let Some(edges) = document.get_mut("edge").and_then(toml::Value::as_table_mut) {
            let names = edges.iter().filter(|(_, v)| v.get("source").is_some() || v.get("target").is_some()).map(|(k, _)| k.clone()).collect::<Vec<_>>();
            for name in names {
                let mut value = edges.remove(&name).unwrap();
                value.as_table_mut().ok_or_else(|| RelError::Unsupported("edge must be a table".into()))?.insert("name".into(), toml::Value::String(name));
                computed.push(value.try_into::<ComputedRelationship>().map_err(|e| RelError::Unsupported(format!("computed relationship: {e}")))?);
            }
        }
        let sections = parse_toml_sections(&toml::to_string(&document).map_err(|e| RelError::Unsupported(e.to_string()))?)?;
        let mut mapping = GraphMapping::new();
        for (path, entries) in &sections {
            match path.as_slice() {
                [kind] if kind == "representation_sources" => {
                    let json = require_key(entries, "catalog", "representation_sources")?;
                    let sources: Vec<super::representation::RepresentationSource> = serde_json::from_str(&json)
                        .map_err(|e| RelError::Unsupported(format!("representation source catalog: {e}")))?;
                    for source in sources {
                        if mapping.representation_sources.insert(source.name.clone(), source).is_some() {
                            return Err(RelError::Unsupported("duplicate representation source".into()));
                        }
                    }
                }
                [kind] if kind == "collection_sources" => {
                    let json = require_key(entries, "catalog", "collection_sources")?;
                    let sources: Vec<super::collection_source::CollectionSource> = serde_json::from_str(&json)
                        .map_err(|e| RelError::Unsupported(format!("collection source catalog: {e}")))?;
                    for source in sources {
                        if mapping.collection_sources.insert(source.name.clone(), source).is_some() {
                            return Err(RelError::Unsupported("duplicate collection source".into()));
                        }
                    }
                }
                [kind] if kind == "logical_sources" => {
                    let json = require_key(entries, "catalog", "logical_sources")?;
                    let sources: Vec<super::layout::LogicalSource> = serde_json::from_str(&json)
                        .map_err(|e| RelError::Unsupported(format!("logical source catalog: {e}")))?;
                    for source in sources {
                        if mapping.logical_sources.insert(source.name.clone(), source).is_some() {
                            return Err(RelError::Unsupported("duplicate logical source".into()));
                        }
                    }
                }
                [kind] if kind == "constraints" => {
                    let json = require_key(entries, "catalog", "constraints")?;
                    mapping.constraints = serde_json::from_str(&json)
                        .map_err(|e| RelError::Unsupported(format!("constraint catalog: {e}")))?;
                    // Snapshot activation is deliberately not serialized.
                }
                [kind, name] if kind == "node" => {
                    let source = section_source(entries, &format!("node.{name}"))?;
                    let id = require_columns(entries, "id", &format!("node.{name}"))?;
                    let mut node = NodeMapping::new(name.clone(), source, id);
                    if let Some(props) = sections.get(&vec![
                        "node".to_string(),
                        name.clone(),
                        "properties".to_string(),
                    ]) {
                        for (property, column) in props {
                            node.properties
                                .insert(property.clone(), column.string()?.to_owned());
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
                        require_columns(entries, "src", &at)?,
                        require_columns(entries, "dst", &at)?,
                        require_key(entries, "src_label", &at)?,
                        require_key(entries, "dst_label", &at)?,
                    );
                    if let Some(child) = entries.get("foreign_key") {
                        edge = edge.foreign_key(match child.string()? {
                            "src" => ForeignKeyEndpoint::Source,
                            "dst" => ForeignKeyEndpoint::Destination,
                            _ => {
                                return Err(RelError::Unsupported(format!(
                                    "{at}.foreign_key must be src or dst"
                                )));
                            }
                        });
                    }
                    if let Some(id) = entries.get("edge_id") {
                        edge.id_column = Some(id.columns()?);
                    }
                    if let Some(props) = sections.get(&vec![
                        "edge".to_string(),
                        name.clone(),
                        "properties".to_string(),
                    ]) {
                        for (property, column) in props {
                            edge.properties
                                .insert(property.clone(), column.string()?.to_owned());
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
        for metadata in source_metadata { mapping.register_source_metadata(metadata)?; }
        for index in indexes { mapping.register_search_index(index)?; }
        for rule in computed { mapping.map_computed_relationship(rule)?; }
        mapping.validate_foreign_keys()?;
        Ok(mapping)
    }

    /// Render the mapping in the TOML format [`from_toml`](Self::from_toml)
    /// reads. Providers are not serialized.
    ///
    /// # Panics
    /// Panics when metadata cannot be represented in TOML, for example JSON
    /// null options. Use [`try_to_toml`](Self::try_to_toml) for arbitrary metadata.
    pub fn to_toml(&self) -> String {
        self.try_to_toml().expect("mapping must be representable in TOML; use try_to_toml for arbitrary metadata")
    }

    /// Render a mapping without losing source options that TOML cannot express.
    /// JSON-only values such as null remain valid catalog metadata but produce
    /// an error here. Providers are not serialized.
    pub fn try_to_toml(&self) -> RelResult<String> {
        fn quote(value: &str) -> String {
            serde_json::to_string(value).expect("serializable mapping string")
        }
        fn source_line(source: &MappedSource) -> String {
            match source {
                MappedSource::Table(table) => format!("table = {}", quote(table)),
                MappedSource::Query(sql) => format!("query = {}", quote(sql)),
                MappedSource::Computed(_) => unreachable!("serialized separately"),
            }
        }
        let mut out = if self.source_metadata.is_empty() { String::new() } else {
            toml::to_string(&BTreeMap::from([("source_metadata", self.source_metadata.values().collect::<Vec<_>>())]))
                .map_err(|e| RelError::Unsupported(format!("source metadata cannot be represented in TOML: {e}")))?
        };
        for (label, node) in &self.nodes {
            out.push_str(&format!("[node.{label}]\n"));
            out.push_str(&source_line(&node.source));
            out.push('\n');
            out.push_str(&format!("id = {}\n", node.id_column.toml()));
            if !node.properties.is_empty() {
                out.push_str(&format!("\n[node.{label}.properties]\n"));
                for (property, column) in &node.properties {
                    out.push_str(&format!("{property} = {}\n", quote(column)));
                }
            }
            out.push('\n');
        }
        for (rel_type, edge) in &self.edges {
            if let MappedSource::Computed(rule) = &edge.source {
                let mut value = toml::Value::try_from(rule.as_ref()).map_err(|e| RelError::Unsupported(format!("relationship TOML: {e}")))?;
                value.as_table_mut().unwrap().remove("name");
                let document = toml::Value::Table(toml::map::Map::from_iter([("edge".into(), toml::Value::Table(toml::map::Map::from_iter([(rel_type.clone(), value)])))]));
                out.push_str(&toml::to_string(&document).map_err(|e| RelError::Unsupported(format!("relationship TOML: {e}")))?);
                out.push('\n');
                continue;
            }
            out.push_str(&format!("[edge.{rel_type}]\n"));
            out.push_str(&source_line(&edge.source));
            out.push('\n');
            out.push_str(&format!("src = {}\n", edge.src_column.toml()));
            out.push_str(&format!("dst = {}\n", edge.dst_column.toml()));
            out.push_str(&format!("src_label = {}\n", quote(&edge.src_label)));
            out.push_str(&format!("dst_label = {}\n", quote(&edge.dst_label)));
            if let Some(child) = edge.foreign_key {
                out.push_str(&format!(
                    "foreign_key = {}\n",
                    quote(match child {
                        ForeignKeyEndpoint::Source => "src",
                        ForeignKeyEndpoint::Destination => "dst",
                    })
                ));
            }
            if let Some(id) = &edge.id_column {
                out.push_str(&format!("edge_id = {}\n", id.toml()));
            }
            if !edge.properties.is_empty() {
                out.push_str(&format!("\n[edge.{rel_type}.properties]\n"));
                for (property, column) in &edge.properties {
                    out.push_str(&format!("{property} = {}\n", quote(column)));
                }
            }
            out.push('\n');
        }
        if !self.representation_sources.is_empty() {
            out.push_str(&format!("[representation_sources]\ncatalog = {}\n",
                quote(&serde_json::to_string(&self.representation_sources.values().collect::<Vec<_>>()).expect("serializable representations"))));
        }
        if !self.collection_sources.is_empty() {
            out.push_str(&format!("[collection_sources]\ncatalog = {}\n",
                quote(&serde_json::to_string(&self.collection_sources.values().collect::<Vec<_>>()).expect("serializable collections"))));
        }
        if !self.logical_sources.is_empty() {
            out.push_str(&format!("[logical_sources]\ncatalog = {}\n",
                quote(&serde_json::to_string(&self.logical_sources.values().collect::<Vec<_>>()).expect("serializable layouts"))));
        }
        if !self.constraints.tables.is_empty() || !self.constraints.revision.is_empty() {
            out.push_str(&format!(
                "[constraints]\ncatalog = {}\n",
                quote(&serde_json::to_string(&self.constraints).expect("serializable catalog"))
            ));
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
enum TomlValue {
    String(String),
    Columns(Vec<String>),
}
impl TomlValue {
    fn string(&self) -> RelResult<&str> {
        match self {
            Self::String(s) => Ok(s),
            _ => Err(RelError::Unsupported(
                "expected a string, not a key-column array".into(),
            )),
        }
    }
    fn columns(&self) -> RelResult<KeyColumns> {
        let key = match self {
            Self::String(s) => KeyColumns::from(s.clone()),
            Self::Columns(c) => KeyColumns::from(c.clone()),
        };
        key.validate()?;
        Ok(key)
    }
}
fn section_source(entries: &BTreeMap<String, TomlValue>, at: &str) -> RelResult<MappedSource> {
    match (entries.get("table"), entries.get("query")) {
        (Some(table), None) => Ok(MappedSource::Table(table.string()?.to_owned())),
        (None, Some(query)) => Ok(MappedSource::Query(query.string()?.to_owned())),
        _ => Err(RelError::Unsupported(format!(
            "[{at}] requires exactly one of table or query"
        ))),
    }
}
fn require_key(entries: &BTreeMap<String, TomlValue>, key: &str, at: &str) -> RelResult<String> {
    entries
        .get(key)
        .ok_or_else(|| RelError::Unsupported(format!("[{at}] is missing required key `{key}`")))?
        .string()
        .map(str::to_owned)
}
fn require_columns(
    entries: &BTreeMap<String, TomlValue>,
    key: &str,
    at: &str,
) -> RelResult<KeyColumns> {
    entries
        .get(key)
        .ok_or_else(|| RelError::Unsupported(format!("[{at}] is missing required key `{key}`")))?
        .columns()
}
type TomlSections = BTreeMap<Vec<String>, BTreeMap<String, TomlValue>>;

/// Parse the TOML subset: `[dotted.section]` headers and `key = "string"`
/// entries. `#` comments and blank lines are ignored.
fn parse_toml_sections(input: &str) -> RelResult<TomlSections> {
    fn visit(value: &toml::Value, path: Vec<String>, result: &mut TomlSections) -> RelResult<()> {
        let table = value.as_table().ok_or_else(|| RelError::Unsupported("expected mapping table".into()))?;
        if (path.len()==2 && matches!(path[0].as_str(), "node" | "edge")) || (table.is_empty() && !path.is_empty() && !(path.len()==1 && matches!(path[0].as_str(), "node" | "edge"))) {
            result.entry(path.clone()).or_default();
        }
        for (key, value) in table {
            if value.is_table() {
                let mut child = path.clone(); child.push(key.clone()); visit(value, child, result)?;
            } else {
                let value = if let Some(s) = value.as_str() { TomlValue::String(s.into()) }
                else if let Some(a) = value.as_array() { TomlValue::Columns(a.iter().map(|v| v.as_str().map(str::to_owned).ok_or_else(|| RelError::Unsupported("expected string key columns".into()))).collect::<RelResult<Vec<_>>>()?) }
                else { return Err(RelError::Unsupported(format!("unsupported mapping value {key}"))); };
                result.entry(path.clone()).or_default().insert(key.clone(), value);
            }
        }
        Ok(())
    }
    let document: toml::Value = toml::from_str(input).map_err(|e| RelError::Unsupported(e.to_string()))?;
    let mut result = BTreeMap::new(); visit(&document, vec![], &mut result)?; Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_function_resolution_preserves_case_aliases_and_namespaces() {
        let mapping = GraphMapping::new();
        let provider = MappingContextProvider::new(&mapping);
        assert_eq!(provider.get_function_meta("ChAr_LeNgTh").unwrap().name(),"character_length");
        assert_eq!(provider.get_aggregate_meta("MeAn").unwrap().name(), "avg");
        assert_eq!(
            provider.get_window_meta("RoW_NuMbEr").unwrap().name(),
            "row_number"
        );
        assert!(provider.get_function_meta("row_number").is_none());
        assert!(provider.get_aggregate_meta("character_length").is_none());
        assert!(provider.get_window_meta("__missing").is_none());
        // Table dependency tracking remains scoped to each mapping query.
        let other = MappingContextProvider::new(&mapping);
        provider.requested.borrow_mut().insert("customers".into());
        assert!(other.requested.borrow().is_empty());
        mapping
            .plan_sql("SELECT ChAr_LeNgTh('hello'), MeAn(3) OVER (), RoW_NuMbEr() OVER ()")
            .unwrap();
        mapping.plan_sql("SELECT sqrt(9.0), char_length('hello')").unwrap();
        assert!(provider.get_function_meta("fn.sqrt").is_none());
    }

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
        assert_eq!(person.id_column.columns(), &["cust_id"]);
        assert_eq!(person.properties.get("name").unwrap(), "full_name");
        let vip = mapping.node("Vip").expect("vip");
        assert!(matches!(vip.source, MappedSource::Query(_)));
        let ordered = mapping.edge("ORDERED").expect("ordered");
        assert_eq!(ordered.src_label, "Person");
        assert_eq!(
            ordered.id_column.as_ref().map(KeyColumns::columns),
            Some(["order_id".to_string()].as_slice())
        );
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
