//! SQL-only, database-free boundary for embedded language bindings.
//!
//! Schemas and function signatures come from the caller. No connection, catalog
//! discovery, DDL, data copying, or SQL execution happens in this module.
use crate::ir::{
    catalog::PropertyGraph,
    functions::{
        FunctionKind, FunctionOverload, FunctionRegistry, OperatorTable, with_operator_table,
    },
    rel::{
        RelBackend, RelBackendOptions,
        mapping::{EdgeMapping, GraphMapping, NodeMapping},
        sql::{SqlDialect, unparse},
    },
    value::Value,
};
use arrow::datatypes::{DataType, Field, Schema, TimeUnit};
use datafusion::{
    common::{DFSchema, DataFusionError, tree_node::TreeNodeRecursion},
    logical_expr::{Expr, LogicalPlan},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileRequest {
    pub version: u32,
    pub dialect: String,
    pub language: String,
    pub query: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, serde_json::Value>,
    pub tables: Vec<Table>,
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(default)]
    pub functions: Vec<Function>,
    #[serde(default)]
    pub ontology: Ontology,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub name: String,
    pub columns: Vec<Column>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub name: String,
    pub data_type: String,
    #[serde(default = "yes")]
    pub nullable: bool,
}
fn yes() -> bool {
    true
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub label: String,
    pub table: String,
    pub id: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edge {
    pub label: String,
    pub table: String,
    pub id: String,
    pub source: String,
    pub target: String,
    pub source_label: String,
    pub target_label: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Function {
    pub name: String,
    pub target: String,
    pub parameters: Vec<String>,
    pub returns: String,
    #[serde(default)]
    pub aggregate: bool,
}
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ontology {
    #[serde(default)]
    pub classes: Vec<OntologyClass>,
    #[serde(default)]
    pub properties: Vec<OntologyProperty>,
    #[serde(default)]
    pub relationships: Vec<OntologyRelationship>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyClass {
    pub iri: String,
    pub label: String,
    pub identity: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyProperty {
    pub iri: String,
    pub label: String,
    pub property: String,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OntologyRelationship {
    pub iri: String,
    pub label: String,
    pub source_label: String,
    pub target_label: String,
}
#[derive(Debug, Serialize)]
pub struct CompiledSql {
    pub version: u32,
    pub dialect: String,
    pub sql: String,
    pub fields: Vec<String>,
}

/// Supported schema types are explicit. Unknown JDBC/extension types must be
/// cast in a caller-owned view, never silently interpreted as strings.
pub fn data_type(value: &str) -> Result<DataType, String> {
    Ok(match value {
        "boolean" => DataType::Boolean,
        "int8" => DataType::Int8,
        "int16" => DataType::Int16,
        "int32" => DataType::Int32,
        "int64" => DataType::Int64,
        "float32" => DataType::Float32,
        "float64" => DataType::Float64,
        "string" => DataType::Utf8,
        "binary" => DataType::Binary,
        "date" => DataType::Date32,
        "timestamp" => DataType::Timestamp(TimeUnit::Microsecond, None),
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
fn parameter(v: &serde_json::Value) -> Result<Value, String> {
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

/// Compile a read query using only schema metadata. SQL is specialized to typed
/// parameter values; caches must include the entire request, including values.
pub async fn compile(request: CompileRequest) -> Result<CompiledSql, String> {
    if request.version != 1 {
        return Err("unsupported compiler protocol version".into());
    }
    let dialect = match request.dialect.as_str() {
        "duckdb" => SqlDialect::DuckDb,
        "postgres" => SqlDialect::Postgres,
        other => return Err(format!("unsupported SQL dialect `{other}`")),
    };
    let mut mapping = GraphMapping::new();
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
        schemas.insert(table.name.clone(), schema);
    }
    let check = |table: &str, column: &str, id: bool| -> Result<(), String> {
        let schema = schemas
            .get(table)
            .ok_or_else(|| format!("unregistered table `{table}`"))?;
        let field = schema
            .field_with_name(column)
            .map_err(|_| format!("missing column `{table}.{column}`"))?;
        if id
            && !matches!(
                field.data_type(),
                DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64
            )
        {
            return Err(format!(
                "identity `{table}.{column}` must be a signed integer"
            ));
        }
        Ok(())
    };
    let mut labels = BTreeSet::new();
    for node in &request.nodes {
        if node.label.is_empty() || !labels.insert(&node.label) {
            return Err("empty or duplicate node label".into());
        }
        check(&node.table, &node.id, true)?;
        let mut n = NodeMapping::table(&node.label, &node.table, &node.id);
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
            check(&edge.table, c, true)?;
        }
        let mut e = EdgeMapping::table(
            &edge.label,
            &edge.table,
            &edge.source,
            &edge.target,
            &edge.source_label,
            &edge.target_label,
        )
        .with_id(&edge.id);
        for (p, c) in &edge.properties {
            check(&edge.table, c, false)?;
            e = e.property(p, c);
        }
        mapping.map_edge(e);
    }
    let mut registry = FunctionRegistry::new(Arc::new(DeclaredCatalog(request.dialect.clone())));
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
    let mapping = Arc::new(mapping);
    let lowered = with_operator_table(Arc::new(registry), || -> Result<_, String> {
        let plan = match request.language.as_str() {
            "cypher" => {
                let mut parsed = crate::language::cypher::parser::parse_query(&request.query)
                    .map_err(|e| e.to_string())?;
                crate::language::cypher::parameters::bind_parameters(&mut parsed, &parameters)?;
                crate::language::cypher::planner::CypherPlanner::new()
                    .plan(&parsed)
                    .map_err(|e| e.to_string())?
            }
            "gremlin" => {
                if !parameters.is_empty() {
                    return Err("bindings are currently supported only for Cypher".into());
                }
                let parsed = crate::language::gremlin::parser::parse_traversal(&request.query)
                    .map_err(|e| e.to_string())?;
                crate::language::gremlin::planner::GremlinPlanner::new()
                    .plan(&parsed)
                    .map_err(|e| e.to_string())?
            }
            "sparql" => {
                if !parameters.is_empty() {
                    return Err("bindings are currently supported only for Cypher".into());
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
                crate::language::sparql::SparqlPlanner::new("mapped")
                    .with_ontology(o)
                    .plan_str(&request.query)
                    .map_err(|e| e.to_string())?
            }
            other => return Err(format!("unsupported query language `{other}`")),
        };
        crate::ir::analysis::validate_read_capabilities(
            &plan,
            crate::ir::analysis::ReadCapabilities::LOCAL_DUCKDB,
        )
        .map_err(|e| e.to_string())?;
        RelBackend::with_options(RelBackendOptions {
            mapping: Some(mapping.clone()),
            ..Default::default()
        })
        .lower(&plan, &PropertyGraph::new())
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
            if !external.contains(&name) && !recursive_sources.contains(&name) {
                return Err(DataFusionError::Plan(format!(
                    "query requires engine-managed materialization of `{name}`; SQL-only compilation cannot execute it"
                )));
            }
        }
        Ok(TreeNodeRecursion::Continue)
    }).map_err(|e| e.to_string())?;
    let sql = unparse(&lowered, dialect).map_err(|e| e.to_string())?;
    Ok(CompiledSql {
        version: 1,
        dialect: request.dialect,
        sql,
        fields: lowered.fields,
    })
}

pub async fn compile_json(input: &str) -> Result<String, String> {
    let request =
        serde_json::from_str(input).map_err(|e| format!("invalid compiler request: {e}"))?;
    serde_json::to_string(&compile(request).await?).map_err(|e| e.to_string())
}
