//! Database-free compilation boundary for embedded language bindings.
//! Produces SQL islands and typed prepared requests for registered engines.
//!
//! Schemas and function signatures come from the caller. No connection, catalog
//! discovery, DDL, data copying, or SQL execution happens in this module.
pub use crate::ir::rel::rdf_mapping::{RdfMapping, RdfTermMapping};
use crate::ir::{
    catalog::PropertyGraph,
    functions::{
        FunctionKind, FunctionOverload, FunctionRegistry, OperatorTable, with_operator_table,
    },
    rel::{
        RelBackend, RelBackendOptions,
        mapping::{EdgeMapping, ForeignKeyEndpoint, GraphMapping, KeyColumns, NodeMapping},
        sql::{SqlDialect, unparse},
    },
    value::Value,
};
use arrow::datatypes::{DataType, Field, IntervalUnit, Schema, TimeUnit};
use datafusion::{
    common::{DFSchema, DataFusionError, tree_node::TreeNodeRecursion},
    logical_expr::{Expr, LogicalPlan},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompileRequest {
    #[serde(default)]
    pub managed_table: Option<String>,
    #[serde(default)]
    pub native_values: bool,
    #[serde(default)]
    pub procedures: BTreeMap<String, CompileProcedure>,
    pub version: u32,
    #[serde(default)]
    pub statistics: Option<std::sync::Arc<crate::ir::rel::statistics::StatisticsSnapshot>>,
    pub dialect: String,
    #[serde(default)]
    pub engines: BTreeMap<String, crate::federation::Engine>,
    #[serde(default)]
    pub execution_engine: Option<String>,
    pub language: String,
    pub query: String,
    #[serde(default)]
    pub authorization: Option<Authorization>,
    #[serde(default)]
    pub parameters: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub bindings: BTreeMap<String, serde_json::Value>,
    pub tables: Vec<Table>,
    #[serde(default)]
    pub representation_sources: Vec<crate::ir::rel::representation::RepresentationSource>,
    #[serde(default)]
    pub collection_sources: Vec<crate::ir::rel::collection_source::CollectionSource>,
    #[serde(default)]
    pub logical_sources: Vec<crate::ir::rel::layout::LogicalSource>,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub rdf: Vec<RdfMapping>,
    /// Existing typed quad mappings, sharing the runtime's RDF term contract.
    #[serde(default)]
    pub rdf_sources: Vec<crate::ir::rel::rdf::IriQuadSource>,
    #[serde(default)]
    pub rdf_graph_names: Option<RdfGraphNames>,
    #[serde(default = "default_rdf_dataset")]
    pub dataset: String,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub computed_relationships: Vec<crate::ir::rel::mapping::ComputedRelationship>,
    #[serde(default)]
    pub search_indexes: Vec<crate::ir::rel::search::SearchIndex>,
    #[serde(default)]
    pub source_metadata: Vec<crate::ir::rel::source_metadata::SourceMetadata>,
    #[serde(default)]
    pub functions: Vec<Function>,
    #[serde(default)]
    pub ontology: Ontology,
    #[serde(default)]
    pub constraints: crate::ir::rel::constraints::ConstraintCatalog,
    #[serde(default)]
    pub constraint_scope: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RdfGraphNames {
    pub table: String,
    pub column: String,
    #[serde(default)]
    pub writable: bool,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub name: String,
    #[serde(default)]
    pub engine: Option<String>,
    pub columns: Vec<Column>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    #[serde(default = "yes")]
    pub nullable: bool,
}
fn default_rdf_dataset() -> String {
    "default".into()
}
fn yes() -> bool {
    true
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub label: String,
    pub table: String,
    /// Optional SQL-only filtered source, while `table` keeps the mapped base source
    /// available for schema validation and identity checking.
    #[serde(default)]
    pub source_query: Option<String>,
    #[serde(default)]
    pub permission_scopes: Vec<PermissionScope>,
    pub id: KeyColumns,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Authorization {
    pub subject_type: String,
    pub subject_id: String,
}
impl Authorization {
    pub fn new(subject_type: impl Into<String>, subject_id: impl Into<String>) -> Self {
        Self {
            subject_type: subject_type.into(),
            subject_id: subject_id.into(),
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionScope {
    pub resource_column: String,
    pub relation: PermissionRelation,
}
impl PermissionScope {
    pub fn new(resource_column: impl Into<String>, relation: PermissionRelation) -> Self {
        Self {
            resource_column: resource_column.into(),
            relation,
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionRelation {
    pub table: String,
    pub resource_type: String,
    pub permission: String,
    #[serde(default = "default_resource_type_column")]
    pub resource_type_column: String,
    #[serde(default = "default_permission_column")]
    pub permission_column: String,
    #[serde(default = "default_resource_id_column")]
    pub resource_id_column: String,
    #[serde(default = "default_subject_type_column")]
    pub subject_type_column: String,
    #[serde(default = "default_subject_relation_column")]
    pub subject_relation_column: String,
    #[serde(default = "default_subject_id_column")]
    pub subject_id_column: String,
}
impl PermissionRelation {
    pub fn flat(
        table: impl Into<String>,
        resource_type: impl Into<String>,
        permission: impl Into<String>,
    ) -> Self {
        Self {
            table: table.into(),
            resource_type: resource_type.into(),
            permission: permission.into(),
            resource_type_column: default_resource_type_column(),
            permission_column: default_permission_column(),
            resource_id_column: default_resource_id_column(),
            subject_type_column: default_subject_type_column(),
            subject_relation_column: default_subject_relation_column(),
            subject_id_column: default_subject_id_column(),
        }
    }

    pub fn with_columns(
        mut self,
        resource_type: impl Into<String>,
        permission: impl Into<String>,
        resource_id: impl Into<String>,
        subject_type: impl Into<String>,
        subject_relation: impl Into<String>,
        subject_id: impl Into<String>,
    ) -> Self {
        self.resource_type_column = resource_type.into();
        self.permission_column = permission.into();
        self.resource_id_column = resource_id.into();
        self.subject_type_column = subject_type.into();
        self.subject_relation_column = subject_relation.into();
        self.subject_id_column = subject_id.into();
        self
    }
}
fn default_resource_type_column() -> String {
    "resource_type".into()
}
fn default_permission_column() -> String {
    "resource_rel".into()
}
fn default_resource_id_column() -> String {
    "resource_id".into()
}
fn default_subject_type_column() -> String {
    "subject_type".into()
}
fn default_subject_relation_column() -> String {
    "subject_rel".into()
}
fn default_subject_id_column() -> String {
    "subject_id".into()
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    #[serde(default)]
    pub source_query: Option<String>,
    #[serde(default)]
    pub foreign_key: Option<ForeignKeyEndpoint>,
    pub label: String,
    pub table: String,
    pub id: KeyColumns,
    pub source: KeyColumns,
    pub target: KeyColumns,
    pub source_label: String,
    pub target_label: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    pub name: String,
    pub target: String,
    pub parameters: Vec<String>,
    pub returns: String,
    #[serde(default)]
    pub aggregate: bool,
}
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ontology {
    #[serde(default)]
    pub classes: Vec<OntologyClass>,
    #[serde(default)]
    pub properties: Vec<OntologyProperty>,
    #[serde(default)]
    pub relationships: Vec<OntologyRelationship>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyClass {
    pub iri: String,
    pub label: String,
    pub identity: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyProperty {
    pub iri: String,
    pub label: String,
    pub property: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyRelationship {
    pub iri: String,
    pub label: String,
    pub source_label: String,
    pub target_label: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompileProcedure {
    pub inputs: Vec<crate::ir::procedures::ProcedureField>,
    pub outputs: Vec<crate::ir::procedures::ProcedureField>,
    pub rows: Vec<Vec<serde_json::Value>>,
}
fn procedure_catalog(request:&CompileRequest)->Result<crate::ir::procedures::ProcedureCatalog,String>{
    request.procedures.iter().map(|(name,p)| {
        let procedure=crate::ir::procedures::TableProcedure {
            signature:crate::ir::procedures::ProcedureSignature {inputs:p.inputs.clone(),outputs:p.outputs.clone()},
            rows:p.rows.iter().map(|row|row.iter().map(parameter).collect()).collect::<Result<_,_>>()?,
        };
        procedure.validate()?;
        Ok((name.clone(),procedure))
    }).collect()
}

#[derive(Debug, Serialize)]
pub struct CompiledSql {
    pub version: u32,
    pub dialect: String,
    pub sql: String,
    pub logical_plan: String,
    pub execution_engine: Option<String>,
    pub transfers: Vec<crate::federation::Transfer>,
    pub fields: Vec<String>,
    pub result_form: String,
    pub field_types: Vec<Option<String>>,
    pub constraint_proofs: Vec<crate::ir::rel::constraints::RewriteProof>,
    pub layout_selections: Vec<crate::ir::rel::layout::LayoutDecision>,
    pub representation_selections: Vec<crate::ir::rel::representation::RepresentationDecision>,
    pub plan_estimates: Vec<crate::ir::rel::statistics::PlanEstimate>,
    pub statistics_usage: Option<String>,
    pub optimizer_decisions: Vec<crate::ir::rel::statistics::OptimizerDecision>,
}

/// Supported schema types are explicit. Unknown JDBC/extension types must be
/// cast in a caller-owned view, never silently interpreted as strings.
pub fn data_type(value: &str) -> Result<DataType, String> {
    Ok(match value {
        "json" => crate::ir::functions::domain::json_type(),
        "null" => DataType::Null,
        "boolean" => DataType::Boolean,
        "int8" => DataType::Int8,
        "int16" => DataType::Int16,
        "int32" => DataType::Int32,
        "int64" => DataType::Int64,
        "uint8" => DataType::UInt8,
        "uint16" => DataType::UInt16,
        "uint32" => DataType::UInt32,
        "uint64" => DataType::UInt64,
        "float32" => DataType::Float32,
        "float64" => DataType::Float64,
        "string" => DataType::Utf8,
        "binary" => DataType::Binary,
        "date" => DataType::Date32,
        "time" => DataType::Time64(TimeUnit::Microsecond),
        "duration" => DataType::Duration(TimeUnit::Microsecond),
        "interval" => DataType::Interval(IntervalUnit::MonthDayNano),
        "timestamp" => DataType::Timestamp(TimeUnit::Microsecond, None),
        _ if value.starts_with("domain:") => {
            let (name, storage) = value[7..].split_once(':').ok_or("domain type requires name and storage type")?;
            if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { return Err("invalid domain name".into()); }
            if name == "json" { return Err("use the json schema type for JSON documents".into()); }
            crate::ir::functions::domain::data_type(name, data_type(storage)?)
        }
        _ if value.starts_with("list:") => {
            DataType::List(Arc::new(Field::new("item", data_type(&value[5..])?, true)))
        }
        _ if value.starts_with("struct_fields:") => {
            let fields: Vec<(String, String)> = serde_json::from_str(&value[14..]).map_err(|e| format!("invalid ordered struct type: {e}"))?;
            let mut names = std::collections::BTreeSet::new();
            if fields.is_empty() || fields.iter().any(|(name, _)| name.is_empty() || !names.insert(name.clone())) {
                return Err("struct requires distinct nonempty field names".into());
            }
            DataType::Struct(fields.into_iter().map(|(name, ty)| Ok(Arc::new(Field::new(name, data_type(&ty)?, true)))).collect::<Result<Vec<_>, String>>()?.into())
        }
        _ if value.starts_with("struct:") => {
            let fields: BTreeMap<String, String> = serde_json::from_str(&value[7..])
                .map_err(|e| format!("invalid struct type: {e}"))?;
            if fields.is_empty() || fields.keys().any(String::is_empty) {
                return Err("struct requires named fields".into());
            }
            DataType::Struct(
                fields
                    .into_iter()
                    .map(|(name, ty)| Ok(Arc::new(Field::new(name, data_type(&ty)?, true))))
                    .collect::<Result<Vec<_>, String>>()?
                    .into(),
            )
        }
        _ if value.starts_with("decimal:") => {
            let parts: Vec<_> = value.split(':').collect();
            if parts.len() != 3 {
                return Err(format!("invalid decimal type: {value}"));
            }
            let p: u8 = parts[1]
                .parse()
                .map_err(|_| format!("invalid precision: {value}"))?;
            let s: i8 = parts[2]
                .parse()
                .map_err(|_| format!("invalid scale: {value}"))?;
            if p == 0 || p > 38 || s < 0 || s as u8 > p {
                return Err(format!("unsupported decimal: {value}"));
            }
            DataType::Decimal128(p, s)
        }
        _ => {
            return Err(format!(
                "unsupported schema type `{value}`; cast it in a source view"
            ));
        }
    })
}
struct DeclaredCatalog(String);
impl OperatorTable for DeclaredCatalog {
    fn engine(&self) -> &str {
        &self.0
    }
    fn overloads(&self, _: &str) -> &[FunctionOverload] {
        &[]
    }
    fn bind(
        &self,
        name: &str,
        _: FunctionKind,
        _: &[Expr],
        _: &DFSchema,
    ) -> datafusion::common::Result<DataType> {
        Err(DataFusionError::Plan(format!(
            "function `{name}` needs a declared signature"
        )))
    }
}
pub(crate) fn parameter(v: &serde_json::Value) -> Result<Value, String> {
    Ok(match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if n.is_f64() {
                Value::Float(
                    n.as_f64()
                        .filter(|v| v.is_finite())
                        .ok_or("non-finite parameter")?,
                )
            } else {
                return Err("integer parameter exceeds signed 64-bit range".into());
            }
        }
        serde_json::Value::Array(a) => {
            Value::List(a.iter().map(parameter).collect::<Result<_, _>>()?)
        }
        serde_json::Value::Object(m) => Value::Map(
            m.iter()
                .map(|(k, v)| Ok((k.clone(), parameter(v)?)))
                .collect::<Result<_, String>>()?,
        ),
    })
}

fn register_tables(request: &CompileRequest, mapping: &mut GraphMapping) -> Result<BTreeMap<String, Arc<Schema>>, String> {
    let mut schemas = BTreeMap::new();
    for table in &request.tables {
        if table.name.is_empty() || schemas.contains_key(&table.name) {
            return Err(format!("empty or duplicate table `{}`", table.name));
        }
        let mut names = BTreeSet::new();
        let fields = table
            .columns
            .iter()
            .map(|c| {
                if c.name.is_empty() || !names.insert(&c.name) {
                    return Err(format!("empty or duplicate column in `{}`", table.name));
                }
                Ok(Field::new(&c.name, data_type(&c.data_type)?, c.nullable))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let schema = Arc::new(Schema::new(fields));
        mapping.register_table_schema(&table.name, schema.clone());
        // Java source mappings carry dialect-quoted identifiers so they can safely
        // address arbitrary catalogs/schemas. SQL inside a mapped query parses those
        // identifiers to their unquoted TableReference spelling; register that alias
        // as well so the nested query resolves to the same schema-only provider.
        if let Some(normalized) = unquote_table_reference(&table.name) {
            if normalized != table.name {
                mapping.register_table_schema(normalized, schema.clone());
            }
        }
        schemas.insert(table.name.clone(), schema);
    }
    Ok(schemas)
}

fn configure_rdf(request: &CompileRequest, mapping: &mut GraphMapping, schemas: &BTreeMap<String, Arc<Schema>>) -> Result<(), String> {
    for rule in &request.rdf {
        mapping.map_rdf(rule.clone());
    }
    if !request.rdf_sources.is_empty() || request.rdf_graph_names.is_some() {
        let mut rdf = mapping.rdf_mapping();
        for source in &request.rdf_sources {
            if !schemas.contains_key(&source.table) {
                return Err(format!("RDF source table `{}` is not registered", source.table));
            }
            rdf.map_typed_quads(&request.dataset, source.clone());
        }
        if let Some(names) = &request.rdf_graph_names {
            let schema = schemas.get(&names.table).ok_or("RDF graph-name table is not registered")?;
            schema.field_with_name(&names.column).map_err(|e| e.to_string())?;
            if names.writable {
                rdf.map_writable_named_graphs(&request.dataset, &names.table, &names.column);
            } else {
                rdf.map_named_graphs(&request.dataset, &names.table, &names.column);
            }
        }
        *mapping = std::mem::take(mapping).with_rdf_mapping(rdf);
    }
    let mut o = crate::language::sparql::OntologyMapping::new();
    for c in &request.ontology.classes {
        o = match &c.identity {
            Some(id) => o.class_with_identity(&c.iri, &c.label, id),
            None => o.class(&c.iri, &c.label),
        };
    }
    for p in &request.ontology.properties {
        o = o.property(&p.iri, &p.label, &p.property);
    }
    for r in &request.ontology.relationships {
        o = o.relationship_between(
            &r.iri,
            &r.label,
            crate::ir::plan::Direction::Out,
            &r.source_label,
            &r.target_label,
        );
    }
    o.apply_to(mapping, &request.dataset)?;
    Ok(())
}

/// Bind the same RDF source declarations for a host-owned update session.
pub fn rdf_mapping(request: &CompileRequest) -> Result<crate::ir::rel::rdf::RdfDatasetMapping, String> {
    if request.version != 1 { return Err("unsupported compiler protocol version".into()); }
    let mut mapping = GraphMapping::new();
    let schemas = register_tables(request, &mut mapping)?;
    configure_rdf(request, &mut mapping, &schemas)?;
    Ok(mapping.rdf_mapping())
}

/// Preserve frontend error classifications without inferring types from text.
pub fn cypher_diagnostic(request: &CompileRequest) -> Option<crate::language::cypher::preparation::PreparationError> {
    if request.language != "cypher" { return None; }
    let parameters = request.parameters.iter().map(|(k, v)| parameter(v).map(|v| (k.clone(), v)))
        .collect::<Result<BTreeMap<_, _>, _>>().ok()?;
    let catalog=procedure_catalog(request).ok()?;
    crate::language::cypher::preparation::prepare(&request.query, &parameters, request.native_values.then_some(&catalog)).err()
}

/// Compile a read query using only schema metadata. SQL is specialized to typed
/// parameter values; caches must include the entire request, including values.
pub async fn compile(request: CompileRequest) -> Result<CompiledSql, String> {
    compile_parsed(request, None).await
}

/// Compile a parsed SPARQL query without a text round trip. Update datasets can
/// distinguish unrestricted named graphs from the empty FROM NAMED set.
pub async fn compile_sparql(request: CompileRequest, query: &crate::spargebra::Query) -> Result<CompiledSql, String> {
    if request.language != "sparql" { return Err("Expected SPARQL compiler request".into()); }
    compile_parsed(request, Some(query)).await
}

/// Database-free frontend preparation shared by SQL-only and kernel hosts.
/// The caller supplies authoritative schemas; no source is scanned here.
pub struct PreparedGraphQuery {
    pub managed_table: Option<String>,
    pub plan: crate::ir::plan::GraphPlan,
    pub graph: PropertyGraph,
    pub mapping: Arc<GraphMapping>,
    pub operators: Arc<dyn OperatorTable>,
}
/// Immutable, thread-safe catalog bindings retained by physical host operators.
/// Mutable PropertyGraph state and executable GraphIR are not retained here.
pub struct PreparedGraphBindings {
    pub managed_table: Option<String>,
    pub mapping: Arc<GraphMapping>,
    pub operators: Arc<dyn OperatorTable>,
    pub procedures: Arc<crate::ir::procedures::ProcedureCatalog>,
}
impl PreparedGraphQuery {
    pub fn bindings(&self) -> PreparedGraphBindings {
        PreparedGraphBindings { managed_table: self.managed_table.clone(), mapping: self.mapping.clone(),
            operators: self.operators.clone(), procedures: self.graph.procedures.clone() }
    }
}
impl PreparedGraphBindings {
    pub fn execution_graph(&self, host: crate::ir::rel::host::SharedHost) -> Result<PropertyGraph, String> {
        with_operator_table(self.operators.clone(), || {
            let mut graph = match &self.managed_table {
                Some(table) => crate::ir::rel::host::managed::ManagedStore::new(table).attach(host)?,
                None => crate::ir::rel::host::mapped_source::attach(host, self.mapping.clone())?,
            };
            graph.procedures = self.procedures.clone();
            Ok(graph)
        })
    }
}
pub fn prepare_graph(request: &CompileRequest) -> Result<PreparedGraphQuery, String> {
    prepare_graph_parsed(request, None)
}
fn prepare_graph_parsed(request: &CompileRequest, sparql: Option<&crate::spargebra::Query>) -> Result<PreparedGraphQuery, String> {
    if request.version != 1 {
        return Err("unsupported compiler protocol version".into());
    }
    if request.language != "gremlin" && !request.bindings.is_empty() {
        return Err("typed bindings require the Gremlin language".into());
    }
    let dialect = SqlDialect::resolve(&request.dialect).map_err(|e| e.to_string())?;
    crate::federation::validate(&request)?;
    let mut mapping = GraphMapping::new();
    let mut schemas = register_tables(&request, &mut mapping)?;
    for source in &request.logical_sources {
        if schemas.contains_key(&source.name) {
            return Err(format!(
                "duplicate table or logical source `{}`",
                source.name
            ));
        }
        let schema = schemas
            .get(&source.default_table)
            .cloned()
            .ok_or_else(|| format!("unregistered default table `{}`", source.default_table))?;
        mapping
            .register_logical_source(source.clone())
            .map_err(|e| e.to_string())?;
        schemas.insert(source.name.clone(), schema);
    }
    // Derived definitions can refer to each other; bind in dependency order.
    let mut names = schemas
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>();
    for name in request
        .collection_sources
        .iter()
        .map(|s| &s.name)
        .chain(request.representation_sources.iter().map(|s| &s.name))
    {
        if !names.insert(name.clone()) {
            return Err(format!("duplicate source `{name}`"));
        }
    }
    let mut collections = request.collection_sources.iter().collect::<Vec<_>>();
    let mut representations = request.representation_sources.iter().collect::<Vec<_>>();
    while !collections.is_empty() || !representations.is_empty() {
        let before = collections.len() + representations.len();
        let mut errors = Vec::new();
        collections.retain(
            |source| match mapping.register_collection_source((*source).clone()) {
                Ok(_) => {
                    schemas.insert(
                        source.name.clone(),
                        mapping.table_schema(&source.name).unwrap(),
                    );
                    false
                }
                Err(e) => {
                    errors.push(format!("{}: {e}", source.name));
                    true
                }
            },
        );
        representations.retain(|source| {
            match mapping.register_representation_source((*source).clone()) {
                Ok(_) => {
                    schemas.insert(
                        source.name.clone(),
                        mapping.table_schema(&source.name).unwrap(),
                    );
                    false
                }
                Err(e) => {
                    errors.push(format!("{}: {e}", source.name));
                    true
                }
            }
        });
        if before == collections.len() + representations.len() {
            return Err(format!(
                "cannot bind derived sources: {}",
                errors.join("; ")
            ));
        }
    }
    if let Some(statistics) = &request.statistics {
        let metadata = serde_json::json!({"tables":request.tables,"logical_sources":request.logical_sources,"collection_sources":request.collection_sources,"representation_sources":request.representation_sources});
        if crate::ir::rel::statistics::mapping_fingerprint(&metadata) != statistics.mapping {
            return Err(
                "statistics snapshot mapping mismatch; regenerate or clear statistics".into(),
            );
        }
    }
    if let Some(statistics) = &request.statistics {
        mapping
            .set_statistics(statistics.clone())
            .map_err(|e| e.to_string())?;
    }
    mapping
        .set_constraints(request.constraints.clone())
        .set_constraint_scope(request.constraint_scope.clone());
    mapping.validate_constraints().map_err(|e| e.to_string())?;
    let check = |table: &str, column: &str, id: bool| -> Result<(), String> {
        let schema = schemas
            .get(table)
            .ok_or_else(|| format!("unregistered table `{table}`"))?;
        let field = schema
            .field_with_name(column)
            .map_err(|_| format!("missing column `{table}.{column}`"))?;
        if id && !crate::ir::rel::mapping::is_identity_type(field.data_type()) {
            return Err(format!(
                "identity `{table}.{column}` requires a non-null scalar type, got {:?}",
                field.data_type()
            ));
        }
        Ok(())
    };
    let mut labels = BTreeSet::new();
    for node in &request.nodes {
        if node.label.is_empty() || !labels.insert(&node.label) {
            return Err("empty or duplicate node label".into());
        }
        node.id.validate().map_err(|e| e.to_string())?;
        for column in node.id.columns() {
            check(&node.table, column, true)?;
        }
        let generated_permission_query = if node.permission_scopes.is_empty() {
            None
        } else {
            let principal = request
                .authorization
                .as_ref()
                .ok_or("query requires a principal because a node has permission scopes")?;
            let mut filters = Vec::new();
            for (index, scope) in node.permission_scopes.iter().enumerate() {
                check(&node.table, &scope.resource_column, false)?;
                let relation = &scope.relation;
                let permission_schema = schemas.get(&relation.table).ok_or_else(|| {
                    format!(
                        "unregistered permission relation table `{}`",
                        relation.table
                    )
                })?;
                for column in [
                    &relation.resource_type_column,
                    &relation.permission_column,
                    &relation.resource_id_column,
                    &relation.subject_type_column,
                    &relation.subject_relation_column,
                    &relation.subject_id_column,
                ] {
                    permission_schema.field_with_name(column).map_err(|_| {
                        format!(
                            "missing permission relation column `{}.{column}`",
                            relation.table
                        )
                    })?;
                }
                filters.push(permission_scope_filter(
                    dialect, node, scope, principal, index, &schemas,
                )?);
            }
            let base = match &node.source_query {
                Some(query) => format!("({query})"),
                None => node.table.clone(),
            };
            Some(format!(
                "SELECT n.* FROM {base} AS n WHERE ({})",
                filters.join(" OR ")
            ))
        };
        let mut n = match generated_permission_query
            .as_deref()
            .or(node.source_query.as_deref())
        {
            Some(sql) => NodeMapping::query(&node.label, sql, &node.id),
            None => NodeMapping::table(&node.label, &node.table, &node.id),
        };
        for (p, c) in &node.properties {
            check(&node.table, c, false)?;
            n = n.property(p, c);
        }
        mapping.map_node(n);
    }
    let mut edge_labels = BTreeSet::new();
    for edge in &request.edges {
        if edge.label.is_empty() || !edge_labels.insert(&edge.label) {
            return Err("empty or duplicate edge label".into());
        }
        if !labels.contains(&edge.source_label) || !labels.contains(&edge.target_label) {
            return Err("edge endpoints must reference mapped node labels".into());
        }
        for c in [&edge.id, &edge.source, &edge.target] {
            c.validate().map_err(|e| e.to_string())?;
            for column in c.columns() {
                check(&edge.table, column, true)?;
            }
        }
        let constructor = if edge.source_query.is_some() { EdgeMapping::query } else { EdgeMapping::table };
        let mut e = constructor(
            &edge.label,
            edge.source_query.as_ref().unwrap_or(&edge.table),
            &edge.source,
            &edge.target,
            &edge.source_label,
            &edge.target_label,
        )
        .with_id(&edge.id);
        if let Some(child) = edge.foreign_key {
            e = e.foreign_key(child);
            if e.id_column.as_ref() != Some(&edge.id) {
                return Err("foreign-key edge ID must match its child key".into());
            }
        }
        for (p, c) in &edge.properties {
            check(&edge.table, c, false)?;
            e = e.property(p, c);
        }
        mapping.map_edge(e);
    }
    for source in &request.source_metadata { mapping.register_source_metadata(source.clone()).map_err(|e|e.to_string())?; }
    for index in &request.search_indexes { mapping.register_search_index(index.clone()).map_err(|e|e.to_string())?; }
    for rule in &request.computed_relationships {
        mapping.map_computed_relationship(rule.clone()).map_err(|e| e.to_string())?;
    }
    mapping.validate_foreign_keys().map_err(|e| e.to_string())?;
    let mut registry = FunctionRegistry::new(crate::ir::functions::active_operator_table().unwrap_or_else(|| Arc::new(DeclaredCatalog(request.dialect.clone()))));
    for f in &request.functions {
        registry
            .register_typed_mapping(
                &f.name,
                &f.target,
                if f.aggregate {
                    FunctionKind::Aggregate
                } else {
                    FunctionKind::Scalar
                },
                f.parameters
                    .iter()
                    .map(|p| data_type(p))
                    .collect::<Result<_, _>>()?,
                data_type(&f.returns)?,
            )
            .map_err(|e| e.to_string())?;
    }
    let parameters = request
        .parameters
        .iter()
        .map(|(k, v)| Ok((k.clone(), parameter(v)?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    configure_rdf(&request, &mut mapping, &schemas)?;
    let mapping = Arc::new(mapping);
    let mut graph=PropertyGraph::new();
    graph.procedures=Arc::new(procedure_catalog(&request)?);
    let operators: Arc<dyn OperatorTable> = Arc::new(registry);
    let plan = with_operator_table(operators.clone(), || -> Result<_, String> {
        let plan = match request.language.as_str() {
            "cypher" => {
                crate::language::cypher::preparation::prepare(&request.query, &parameters, request.native_values.then_some(graph.procedures.as_ref()))
                    .map_err(|e| e.to_string())?
            }
            "gremlin" => {
                if !parameters.is_empty() {
                    return Err("bindings are currently supported only for Cypher".into());
                }
                let bindings = crate::language::gremlin::bindings::bindings(
                    &serde_json::to_value(&request.bindings).map_err(|e| e.to_string())?)?;
                let (source, values) = crate::language::gremlin::callables::prepare(&request.query, &bindings)
                    .map_err(|e| e.to_string())?;
                let parsed = crate::language::gremlin::parser::parse_traversal_with_bindings(&source, &values)
                    .map_err(|e| e.to_string())?;
                crate::language::gremlin::planner::GremlinPlanner::new()
                    .plan(&parsed)
                    .map_err(|e| e.to_string())?
            }
            "sparql" => {
                if !parameters.is_empty() {
                    return Err("bindings are currently supported only for Cypher".into());
                }
                let planner = crate::language::sparql::SparqlPlanner::new(&request.dataset);
                match sparql {
                    Some(query) => planner.plan(query),
                    None => planner.plan_str(&request.query),
                }.map_err(|e| e.to_string())?
            }
            other => return Err(format!("unsupported query language `{other}`")),
        };
        Ok(plan)
    })?;
    graph.mapping = Some(mapping.clone());
    Ok(PreparedGraphQuery { plan, graph, mapping, operators, managed_table: request.managed_table.clone() })
}

async fn compile_parsed(request: CompileRequest, sparql: Option<&crate::spargebra::Query>) -> Result<CompiledSql, String> {
    let PreparedGraphQuery { plan, graph, mapping, operators, managed_table } = prepare_graph_parsed(&request, sparql)?;
    if managed_table.is_some() { return Err("Managed graph queries require the host compiled-kernel adapter".into()); }
    let dialect = SqlDialect::resolve(&request.dialect).map_err(|e| e.to_string())?;
    let mut lowered = with_operator_table(operators, || -> Result<_, String> {
        crate::ir::analysis::validate_read_capabilities(
            &plan,
            crate::ir::analysis::ReadCapabilities {read_procedures:request.native_values,..crate::ir::analysis::ReadCapabilities::LOCAL_DUCKDB},
        )
        .map_err(|e| e.to_string())?;
        RelBackend::with_options(RelBackendOptions {
            native_values: request.native_values,
            mapping: Some(mapping.clone()),
            rdf_datasets: Some(Arc::new(mapping.rdf_mapping())),
            ..Default::default()
        })
        .lower(&plan, &graph)
        .map_err(|e| e.to_string())
    })?;
    let external: BTreeSet<_> = mapping
        .physical_table_names()
        .iter()
        .map(|name| datafusion::common::TableReference::from(name.as_str()).to_string())
        .collect();
    let mut recursive_sources = BTreeSet::new();
    lowered
        .plan
        .apply_with_subqueries(|node| {
            if let LogicalPlan::RecursiveQuery(recursive) = node {
                recursive_sources.insert(recursive.name.clone());
            }
            Ok(TreeNodeRecursion::Continue)
        })
        .map_err(|e| e.to_string())?;
    // Validate scans without collecting or evaluating any internal table. Even
    // constant materializations must be represented in SQL, not executed here.
    lowered.plan.apply_with_subqueries(|node| {
        if let LogicalPlan::TableScan(scan) = node {
            let name = scan.table_name.to_string();
            let generated_range = datafusion::datasource::source_as_provider(&scan.source).ok()
                .is_some_and(|p| p.as_any().is::<crate::ir::rel::range::IntegerRange>());
            if !external.contains(&name) && !recursive_sources.contains(&name) && !generated_range {
                return Err(DataFusionError::Plan(format!(
                    "query requires engine-managed materialization of `{name}`; SQL-only compilation cannot execute it"
                )));
            }
        }
        Ok(TreeNodeRecursion::Continue)
    }).map_err(|e| e.to_string())?;
    let (optimized, constraint_proofs) =
        crate::ir::rel::constraints::optimize(lowered.plan.clone()).map_err(|e| e.to_string())?;
    lowered.plan = optimized;
    if request.language == "sparql"
        && request.rdf.is_empty()
        && request.rdf_sources.is_empty()
        && !lowered.fields.is_empty()
        && lowered.plan.schema().fields().len() > lowered.fields.len()
    {
        let legacy_types = lowered
            .fields
            .iter()
            .filter_map(|name| {
                let dt = crate::ir::rel::rdf::binding_identity_columns(name)[1].clone();
                let iri = constant_string_column(&lowered.plan, &dt)?;
                Some((
                    name.clone(),
                    crate::ir::rel::rdf_mapping::legacy_scalar_type(&iri)?,
                ))
            })
            .collect::<BTreeMap<_, _>>();
        // Preserve the legacy compiler's visible-column result contract.
        lowered.plan = datafusion::logical_expr::LogicalPlanBuilder::from(lowered.plan)
            .project(
                lowered
                    .fields
                    .iter()
                    .map(|name| {
                        let value = Expr::Column(datafusion::common::Column::from_name(name));
                        match legacy_types.get(name) {
                            Some(kind) => Expr::Cast(datafusion::logical_expr::Cast::new(
                                Box::new(value),
                                kind.clone(),
                            ))
                            .alias(name),
                            None => value,
                        }
                    })
                    .collect::<Vec<_>>(),
            )
            .and_then(|p| p.build())
            .map_err(|e| e.to_string())?;
    }
    let selected =
        crate::ir::rel::representation::select(lowered.plan).map_err(|e| e.to_string())?;
    let (optimized, mut optimizer_decisions) =
        crate::ir::rel::statistics::optimize(selected.plan).map_err(|e| e.to_string())?;
    optimizer_decisions.extend(selected.access_decisions);
    lowered.plan = optimized;
    if !request.engines.is_empty() {
        // Preserve source aliases and explicit graph scope barriers. General
        // common-subexpression elimination can erase those SQL scopes.
        use datafusion::optimizer::{Optimizer, OptimizerContext};
        let optimizer = Optimizer::with_rules(vec![
            Arc::new(datafusion::optimizer::simplify_expressions::SimplifyExpressions::new()),
            Arc::new(datafusion::optimizer::push_down_filter::PushDownFilter::new()),
            Arc::new(datafusion::optimizer::optimize_projections::OptimizeProjections::new()),
        ]);
        lowered.plan = optimizer
            .optimize(lowered.plan, &OptimizerContext::new(), |_, _| {})
            .map_err(|e| e.to_string())?;
    }
    lowered.plan=crate::ir::rel::search::bind_seeds(lowered.plan).map_err(|e|e.to_string())?;
    lowered.plan=crate::ir::rel::search::push_source_filters(lowered.plan).map_err(|e|e.to_string())?;
    let (plan, transfers) = crate::federation::route(&request, lowered.plan)?;
    lowered.plan = plan;
    let sql = unparse(&lowered, dialect).map_err(|e| e.to_string())?;
    Ok(CompiledSql {
        execution_engine: request.execution_engine,
        transfers,
        version: 1,
        dialect: request.dialect,
        sql,
        logical_plan: lowered.plan.display_indent().to_string(),
        fields: lowered.fields,
        result_form: format!("{:?}", lowered.result_form),
        field_types: lowered.plan.schema().fields().iter().map(|f| crate::federation::type_name(f.data_type()).ok()).collect(),
        constraint_proofs,
        layout_selections: selected.layout_selections,
        representation_selections: selected.representation_selections,
        plan_estimates: crate::ir::rel::statistics::explain(&lowered.plan),
        statistics_usage: request.statistics.as_ref().map(|s| s.revision.clone()),
        optimizer_decisions,
    })
}

fn permission_scope_filter(
    dialect: SqlDialect,
    node: &Node,
    scope: &PermissionScope,
    principal: &Authorization,
    index: usize,
    schemas: &BTreeMap<String, Arc<Schema>>,
) -> Result<String, String> {
    let q = |identifier: &str| dialect.quote_ident(identifier);
    let g = format!("p{index}");
    let relation = &scope.relation;
    let resource_id = format!("{g}.{}", q(&relation.resource_id_column));
    let subject_id = format!("'{}'", principal.subject_id.replace('\'', "''"));
    let mut conditions = vec![
        format!(
            "{g}.{} = '{}'",
            q(&relation.resource_type_column),
            relation.resource_type.replace('\'', "''")
        ),
        format!(
            "{g}.{} = '{}'",
            q(&relation.permission_column),
            relation.permission.replace('\'', "''")
        ),
        format!(
            "{g}.{} = '{}'",
            q(&relation.subject_type_column),
            principal.subject_type.replace('\'', "''")
        ),
        format!("{g}.{} = ''", q(&relation.subject_relation_column)),
        format!("{g}.{} = {subject_id}", q(&relation.subject_id_column)),
    ];
    let source_id = format!("n.{}", q(&scope.resource_column));
    let data_type = schemas[&node.table]
        .field_with_name(&scope.resource_column)
        .map_err(|_| {
            format!(
                "missing node resource column `{}.{}`",
                node.table, scope.resource_column
            )
        })?
        .data_type();
    let integer_type = if dialect == SqlDialect::DuckDb {
        match data_type {
            DataType::Int8 => Some("TINYINT"),
            DataType::Int16 => Some("SMALLINT"),
            DataType::Int32 => Some("INTEGER"),
            DataType::Int64 => Some("BIGINT"),
            _ => None,
        }
    } else {
        None
    };
    let selected_id = if let Some(integer_type) = integer_type {
        let cast = format!("TRY_CAST({resource_id} AS {integer_type})");
        conditions.push(format!("{cast} IS NOT NULL"));
        conditions.push(format!(
            "CAST({cast} AS VARCHAR) = CAST({resource_id} AS VARCHAR)"
        ));
        cast
    } else {
        conditions.push(format!("CAST({source_id} AS VARCHAR) IN (SELECT CAST({resource_id} AS VARCHAR) FROM {} AS {g} WHERE {})", relation.table, conditions.join(" AND ")));
        return Ok(conditions.pop().expect("membership filter was added"));
    };
    Ok(format!(
        "{source_id} IN (SELECT {selected_id} FROM {} AS {g} WHERE {})",
        relation.table,
        conditions.join(" AND ")
    ))
}

fn unquote_table_reference(value: &str) -> Option<String> {
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut chars = value.chars().peekable();
    let mut quoted = false;
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                chars.next();
                part.push('"');
            }
            '"' => quoted = !quoted,
            '.' if !quoted => {
                if part.is_empty() {
                    return None;
                }
                parts.push(std::mem::take(&mut part));
            }
            _ => part.push(ch),
        }
    }
    if quoted || part.is_empty() {
        return None;
    }
    parts.push(part);
    Some(parts.join("."))
}

pub async fn compile_json(input: &str) -> Result<String, String> {
    let message: serde_json::Value = serde_json::from_str(input).map_err(|e|e.to_string())?;
    if message["op"] == "validate_schema" {
        return serde_json::to_string(&crate::session::Schema::from_value(message["schema"].clone())?).map_err(|e|e.to_string());
    }
    let command: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
    if command.get("op").and_then(serde_json::Value::as_str).is_some_and(|op| matches!(op, "bind_search" | "bind_operation")) {
        return crate::federation::bind_operation_command(command).map(|v|v.to_string());
    }
    if command.get("op").and_then(serde_json::Value::as_str) == Some("bind") {
        return std::panic::catch_unwind(std::panic::AssertUnwindSafe(||
            crate::federation::bind_command(command)
        )).map_err(|_| "invalid exchange data; no SQL was executed".to_string())?
            .map(|v| v.to_string());
    }
    let request: CompileRequest =
        serde_json::from_str(input).map_err(|e| format!("invalid compiler request: {e}"))?;
    if let Some(snapshot) = &request.statistics {
        snapshot.validate()?;
    }
    serde_json::to_string(&compile(request).await?).map_err(|e| e.to_string())
}

fn constant_string_column(plan: &LogicalPlan, name: &str) -> Option<String> {
    fn expression(expr: &Expr, input: &LogicalPlan) -> Option<String> {
        match expr {
            Expr::Alias(alias) => expression(&alias.expr, input),
            Expr::Literal(datafusion::common::ScalarValue::Utf8(Some(value)), _) => {
                Some(value.clone())
            }
            Expr::Column(column) => constant_string_column(input, &column.name),
            _ => None,
        }
    }
    if let LogicalPlan::Projection(projection) = plan {
        let index = projection
            .schema
            .fields()
            .iter()
            .position(|field| field.name() == name)?;
        return expression(&projection.expr[index], &projection.input);
    }
    let inputs = plan
        .inputs()
        .into_iter()
        .filter(|input| input.schema().has_column_with_unqualified_name(name))
        .collect::<Vec<_>>();
    let mut values = inputs
        .iter()
        .map(|input| constant_string_column(input, name));
    let first = values.next()??;
    values
        .all(|value| value.as_ref() == Some(&first))
        .then_some(first)
}
