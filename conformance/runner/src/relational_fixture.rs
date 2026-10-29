//! Schema-free upstream fixtures hosted by ordinary mapped SQL tables.
//! Schema discovery inspects input plans and fixture declarations only. It never
//! executes a query to infer its answer, and never retries in a managed graph.
use arrow::datatypes::{DataType, Field};
use datafusion::common::ScalarValue;
use duckdb::Connection;
use orchiddb::{
    engine::GraphEngine,
    ir::{
        expr::{IrExpr, Lit},
        plan::{GraphPlan, Node, SetMode},
        rel::{
            mapping::{EdgeMapping, ForeignKeyEndpoint, GraphMapping, NodeMapping},
            sql::SqlDialect,
        },
    },
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
};

type Result<T> = std::result::Result<T, String>;
type Edge = (String, String, String);
type Owner = (bool, String);
type TableIdentity = (bool, String, String, String);
fn expression_owners(
    expr: &IrExpr,
    entities: &BTreeMap<String, BTreeSet<Owner>>,
) -> Option<BTreeSet<Owner>> {
    match expr {
        IrExpr::Binding(binding) => entities.get(binding).cloned(),
        IrExpr::Call { name, args } if name == "select_key_or_binding_pop" => args
            .get(1)
            .and_then(|value| expression_owners(value, entities)),
        _ => None,
    }
}
const LABELS: &str = "labels";
fn primary_key(edge: bool) -> &'static str {
    if edge { "edge_id" } else { "node_id" }
}
fn quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}
fn text(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn identifier(name: &str, fallback: &str) -> String {
    let mut result = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            result.push(c.to_ascii_lowercase());
        } else if !result.ends_with('_') {
            result.push('_');
        }
    }
    let result = result.trim_matches('_');
    if result.is_empty() {
        fallback.into()
    } else if result.starts_with(|c: char| c.is_ascii_digit()) {
        format!("{fallback}_{result}")
    } else {
        result.into()
    }
}
fn property(key: &str) -> String {
    if ["node_id", "edge_id", "source_id", "target_id", LABELS]
        .iter()
        .any(|reserved| key.eq_ignore_ascii_case(reserved))
    {
        format!("{key}_property")
    } else {
        key.into()
    }
}
fn sql_type(kind: &DataType) -> Result<String> {
    if let DataType::List(field) = kind { return Ok(format!("{}[]", sql_type(field.data_type())?)); }
    // Text-backed temporal domains need their nominal envelope in persistent
    // storage: a plain VARCHAR cannot distinguish a temporal from user text.
    if orchiddb::ir::functions::domain::descriptor(kind) == Some(("cypher_temporal", &DataType::Utf8)) {
        return Ok("STRUCT(\"__orchiddb_domain_cypher_temporal\" VARCHAR)".into());
    }
    SqlDialect::DuckDb.sql_type(kind).map_err(|e| e.to_string())
}
fn scalar_literal(value: ScalarValue) -> Result<String> {
    SqlDialect::DuckDb
        .exchange_literal(value.clone(), value.data_type())
        .map_err(|e| e.to_string())
}
fn union_type() -> DataType {
    orchiddb::ir::functions::scalar_union::data_type()
}
fn merge_type(left: &DataType, right: &DataType) -> Result<DataType> {
    if left == right || *right == DataType::Null {
        return Ok(left.clone());
    }
    if *left == DataType::Null {
        return Ok(right.clone());
    }
    if orchiddb::ir::functions::scalar_union::supports_type(left)
        && orchiddb::ir::functions::scalar_union::supports_type(right)
    {
        return Ok(union_type());
    }
    if let (DataType::List(a), DataType::List(b)) = (left, right) {
        return Ok(DataType::List(Arc::new(Field::new(
            "item",
            merge_type(a.data_type(), b.data_type())?,
            true,
        ))));
    }
    Err(format!(
        "Relational fixture schema cannot losslessly combine {left} and {right}"
    ))
}
fn value_scalar(value: &Value, declared: Option<&str>) -> Result<ScalarValue> {
    if value.is_null() {
        return Ok(ScalarValue::Null);
    }
    Ok(match declared {
        Some("Byte") => ScalarValue::Int8(Some(
            value
                .as_i64()
                .ok_or("invalid Byte fixture")?
                .try_into()
                .map_err(|_| "invalid Byte fixture")?,
        )),
        Some("Short") => ScalarValue::Int16(Some(
            value
                .as_i64()
                .ok_or("invalid Short fixture")?
                .try_into()
                .map_err(|_| "invalid Short fixture")?,
        )),
        Some("Integer") => ScalarValue::Int32(Some(
            value
                .as_i64()
                .ok_or("invalid Integer fixture")?
                .try_into()
                .map_err(|_| "invalid Integer fixture")?,
        )),
        Some("Long") => ScalarValue::Int64(Some(value.as_i64().ok_or("invalid Long fixture")?)),
        Some("Float") => {
            ScalarValue::Float32(Some(value.as_f64().ok_or("invalid Float fixture")? as f32))
        }
        Some("Double") => {
            ScalarValue::Float64(Some(value.as_f64().ok_or("invalid Double fixture")?))
        }
        Some("Boolean") => {
            ScalarValue::Boolean(Some(value.as_bool().ok_or("invalid Boolean fixture")?))
        }
        Some("String") => {
            ScalarValue::Utf8(Some(value.as_str().ok_or("invalid String fixture")?.into()))
        }
        Some(other) => return Err(format!("Unmapped relational fixture type {other}")),
        None => match value {
            Value::Bool(v) => ScalarValue::Boolean(Some(*v)),
            Value::String(v) => ScalarValue::Utf8(Some(v.clone())),
            Value::Number(v) if v.is_i64() => ScalarValue::Int64(v.as_i64()),
            Value::Number(v) => ScalarValue::Float64(v.as_f64()),
            Value::Array(values) => {
                let values = values
                    .iter()
                    .map(|v| value_scalar(v, None))
                    .collect::<Result<Vec<_>>>()?;
                let kind = values
                    .iter()
                    .try_fold(DataType::Null, |kind, v| merge_type(&kind, &v.data_type()))?;
                let values = values
                    .into_iter()
                    .map(|v| v.cast_to(&kind).map_err(|e| e.to_string()))
                    .collect::<Result<Vec<_>>>()?;
                ScalarValue::List(ScalarValue::new_list_nullable(&values, &kind))
            }
            _ => return Err("Fixture property has no lossless relational representation".into()),
        },
    })
}

#[derive(Clone, Default, PartialEq)]
struct Schema {
    nodes: BTreeSet<String>,
    edges: BTreeSet<Edge>,
    // Query expression hints; physical types belong to each element type.
    properties: BTreeMap<String, DataType>,
    owned: BTreeMap<Owner, BTreeMap<String, DataType>>,
    key_types: BTreeMap<Owner, DataType>,
    foreign_keys: BTreeMap<Edge, ForeignKeyEndpoint>,
}
impl Schema {
    fn add_property(&mut self, owner: &Owner, name: &str, kind: DataType) -> Result<()> {
        let fields = self.owned.entry(owner.clone()).or_default();
        let kind = merge_type(fields.get(name).unwrap_or(&DataType::Null), &kind)?;
        fields.insert(name.into(), kind.clone());
        let hint = merge_type(self.properties.get(name).unwrap_or(&DataType::Null), &kind)
            .unwrap_or(DataType::Null);
        self.properties.insert(name.into(), hint);
        Ok(())
    }
    fn owners(&self) -> BTreeSet<Owner> {
        self.nodes
            .iter()
            .map(|label| (false, label.clone()))
            .chain(self.edges.iter().map(|edge| (true, edge.0.clone())))
            .collect()
    }
    fn targets(
        &self,
        expr: &IrExpr,
        entities: &BTreeMap<String, BTreeSet<Owner>>,
    ) -> BTreeSet<Owner> {
        expression_owners(expr, entities).unwrap_or_else(|| self.owners())
    }
}

pub struct RelationalFixture {
    pub engine: GraphEngine,
    diagnostic_mapping: Arc<GraphMapping>,
    storage: Connection,
    schema: Schema,
    installed: BTreeMap<String, BTreeMap<String, DataType>>,
    fixtures: VecDeque<(String, Value)>,
    table_names: BTreeMap<TableIdentity, String>,
    creation_order: Vec<String>,
    table_ddl: std::cell::RefCell<Option<Vec<String>>>,
    pub allow_relationship_expansion: bool,
}
impl RelationalFixture {
    pub fn new() -> Result<Self> {
        let storage = Connection::open_in_memory().map_err(|e| e.to_string())?;
        storage
            .execute_batch("CREATE SEQUENCE node_ids; CREATE SEQUENCE edge_ids")
            .map_err(|e| e.to_string())?;
        let engine = GraphEngine::mapped(
            storage.try_clone().map_err(|e| e.to_string())?,
            Arc::new(GraphMapping::new()),
        )?;
        Ok(Self {
            engine,
            diagnostic_mapping: Arc::new(GraphMapping::new()),
            storage,
            schema: Schema::default(),
            installed: BTreeMap::new(),
            fixtures: VecDeque::new(),
            table_names: BTreeMap::new(),
            creation_order: Vec::new(),
            table_ddl: Default::default(),
            allow_relationship_expansion: true,
        })
    }
    pub fn reset(&mut self) -> Result<()> {
        self.table_ddl.get_mut().take();
        for table in self.creation_order.iter().rev() {
            self.storage
                .execute_batch(&format!("DROP TABLE {}", quote(table)))
                .map_err(|e| e.to_string())?;
        }
        self.schema = Schema::default();
        self.installed.clear();
        self.table_names.clear();
        self.creation_order.clear();
        self.storage.execute_batch("DROP SEQUENCE node_ids; DROP SEQUENCE edge_ids; CREATE SEQUENCE node_ids; CREATE SEQUENCE edge_ids").map_err(|e| e.to_string())?;
        self.engine.clear_table_procedures();
        self.diagnostic_mapping = Arc::new(GraphMapping::new());
        self.engine.set_mapping(self.diagnostic_mapping.clone())
    }
    fn node_table(&self, label: &str) -> String {
        self.table_names[&(false, label.into(), String::new(), String::new())].clone()
    }
    fn edge_table(&self, edge: &Edge) -> String {
        self.table_names[&(true, edge.0.clone(), edge.1.clone(), edge.2.clone())].clone()
    }
    fn declare_names(&mut self) {
        let identities = self
            .schema
            .nodes
            .iter()
            .map(|label| (false, label.clone(), String::new(), String::new()))
            .chain(
                self.schema
                    .edges
                    .iter()
                    .map(|edge| (true, edge.0.clone(), edge.1.clone(), edge.2.clone())),
            )
            .collect::<Vec<_>>();
        for identity in identities {
            if self.table_names.contains_key(&identity) {
                continue;
            }
            let mut base = identifier(
                &identity.1,
                if identity.0 { "relationship" } else { "node" },
            );
            if identity.0
                && self
                    .schema
                    .edges
                    .iter()
                    .filter(|edge| edge.0 == identity.1)
                    .count()
                    > 1
            {
                base = format!(
                    "{}_{}_{}",
                    base,
                    identifier(&identity.2, "node"),
                    identifier(&identity.3, "node")
                );
            }
            let mut candidate = base.clone();
            let mut suffix = 2;
            while self.table_names.values().any(|name| name == &candidate)
                || ["node_ids", "edge_ids"].contains(&candidate.as_str())
            {
                candidate = format!("{base}_{suffix}");
                suffix += 1;
            }
            self.table_names.insert(identity, candidate);
        }
    }
    fn fk_layout(
        &self,
        edge: &Edge,
    ) -> Option<(ForeignKeyEndpoint, String, String, String, String)> {
        let direction = *self.schema.foreign_keys.get(edge)?;
        let (child, parent) = if direction == ForeignKeyEndpoint::Source {
            (&edge.1, &edge.2)
        } else {
            (&edge.2, &edge.1)
        };
        let prefix = self.edge_table(edge);
        let column = if direction == ForeignKeyEndpoint::Source {
            format!("{prefix}_id")
        } else {
            format!("{prefix}_source_id")
        };
        Some((
            direction,
            child.clone(),
            parent.clone(),
            column,
            format!("{prefix}_edge_id"),
        ))
    }
    fn sync_schema(&mut self) -> Result<()> {
        self.declare_names();
        for fields in self.schema.owned.values_mut() {
            for kind in fields.values_mut() {
                if *kind == DataType::Null {
                    *kind = union_type();
                }
            }
        }
        let mut mapping = GraphMapping::new();
        // Relationship references are ordinary columns, not enforced SQL foreign keys.
        // Tables can therefore be declared in stable label order, including cycles.
        for label in self.schema.nodes.clone() {
            let table = self.node_table(&label);
            let fields = self
                .schema
                .owned
                .get(&(false, label.clone()))
                .cloned()
                .unwrap_or_default();
            let mut physical = fields.clone();
            let mut extras = Vec::new();
            for edge in self.schema.foreign_keys.keys() {
                let (_, child, parent, column, id) = self.fk_layout(edge).unwrap();
                if child != label {
                    continue;
                }
                let parent_type = self
                    .schema
                    .key_types
                    .get(&(false, parent.clone()))
                    .cloned()
                    .unwrap_or(DataType::Int64);
                let edge_type = self
                    .schema
                    .key_types
                    .get(&(true, edge.0.clone()))
                    .cloned()
                    .unwrap_or(DataType::Int64);
                extras.push(format!(
                    "{} {}",
                    quote(&column),
                    sql_type(&parent_type)?
                ));
                extras.push(format!(
                    "{} {} UNIQUE DEFAULT nextval('edge_ids')",
                    quote(&id),
                    sql_type(&edge_type)?
                ));
                for (key, kind) in self
                    .schema
                    .owned
                    .get(&(true, edge.0.clone()))
                    .into_iter()
                    .flatten()
                {
                    let column = format!("{}_{}", self.edge_table(edge), key);
                    if physical.insert(column.clone(), kind.clone()).is_some() {
                        return Err(format!(
                            "Relationship property column collides with node property: {column}"
                        ));
                    }
                }
            }
            let key_type = self
                .schema
                .key_types
                .get(&(false, label.clone()))
                .cloned()
                .unwrap_or(DataType::Int64);
            self.sync_table(&table, false, &physical, &key_type, &extras)?;
            let mut node =
                NodeMapping::table(&label, &table, primary_key(false)).labels_column(LABELS);
            for key in fields.keys() {
                node = node.property(key, property(key));
            }
            mapping.map_node(node);
        }
        for edge in self.schema.edges.iter().cloned().collect::<Vec<_>>() {
            let fields = self
                .schema
                .owned
                .get(&(true, edge.0.clone()))
                .cloned()
                .unwrap_or_default();
            let mut m = if let Some((direction, child, _, column, id)) = self.fk_layout(&edge) {
                let (source, target) = if direction == ForeignKeyEndpoint::Source {
                    ("node_id", column.as_str())
                } else {
                    (column.as_str(), "node_id")
                };
                EdgeMapping::table(
                    &edge.0,
                    self.node_table(&child),
                    source,
                    target,
                    &edge.1,
                    &edge.2,
                )
                .foreign_key(direction)
                .with_id(id)
            } else {
                let table = self.edge_table(&edge);
                let key_type = self
                    .schema
                    .key_types
                    .get(&(true, edge.0.clone()))
                    .cloned()
                    .unwrap_or(DataType::Int64);
                self.sync_table(&table, true, &fields, &key_type, &[])?;
                EdgeMapping::table(&edge.0, table, "source_id", "target_id", &edge.1, &edge.2)
                    .with_id(primary_key(true))
            };
            for key in fields.keys() {
                let column = if self.schema.foreign_keys.contains_key(&edge) {
                    format!("{}_{}", self.edge_table(&edge), key)
                } else {
                    property(key)
                };
                m = m.property(key, column);
            }
            mapping.map_edge_partition(m);
        }
        self.diagnostic_mapping = Arc::new(mapping);
        self.engine.set_mapping(self.diagnostic_mapping.clone())
    }
    fn sync_table(
        &mut self,
        table: &str,
        edge: bool,
        fields: &BTreeMap<String, DataType>,
        key_type: &DataType,
        extra_columns: &[String],
    ) -> Result<()> {
        if !self.installed.contains_key(table) {
            self.table_ddl.get_mut().take();
            let extra = if edge {
                "source_id BIGINT NOT NULL, target_id BIGINT NOT NULL".into()
            } else {
                format!("{} VARCHAR[] NOT NULL", quote(LABELS))
            };
            let extra = std::iter::once(extra)
                .chain(extra_columns.iter().cloned())
                .collect::<Vec<_>>()
                .join(", ");
            let sequence = if edge { "edge_ids" } else { "node_ids" };
            self.storage
                .execute_batch(&format!(
                    "CREATE TABLE {} ({} {} PRIMARY KEY DEFAULT nextval('{sequence}'), {extra})",
                    quote(table),
                    quote(primary_key(edge)),
                    sql_type(key_type)?
                ))
                .map_err(|e| e.to_string())?;
            self.installed.insert(table.into(), BTreeMap::new());
            self.creation_order.push(table.into());
        }
        for (key, inferred) in fields {
            let kind = if *inferred == DataType::Null {
                union_type()
            } else {
                inferred.clone()
            };
            let column = quote(&property(key));
            let existing = self.installed.get(table).unwrap().get(key);
            if existing == Some(&kind) {
                continue;
            }
            self.table_ddl.get_mut().take();
            let definition = sql_type(&kind)?;
            let sql = if let Some(old) = existing {
                let using = if kind == union_type() {
                    let (tag, integer, string, boolean) = match old {
                        DataType::Boolean => (3, "NULL".into(), "NULL".into(), column.clone()),
                        DataType::Utf8 => (2, "NULL".into(), column.clone(), "NULL".into()),
                        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                            (1, column.clone(), "NULL".into(), "NULL".into())
                        }
                        _ => {
                            return Err(format!(
                                "Cannot migrate relational fixture property {key} from {old} to {kind}"
                            ));
                        }
                    };
                    format!(
                        "CASE WHEN {column} IS NULL THEN NULL ELSE struct_pack(__cypher_scalar_tag := {tag}::TINYINT, integer := {integer}::BIGINT, string := {string}::VARCHAR, boolean := {boolean}::BOOLEAN) END"
                    )
                } else {
                    format!("CAST({column} AS {definition})")
                };
                format!(
                    "ALTER TABLE {} ALTER COLUMN {column} TYPE {definition} USING {using}",
                    quote(table)
                )
            } else {
                format!(
                    "ALTER TABLE {} ADD COLUMN {column} {definition}",
                    quote(table)
                )
            };
            self.storage
                .execute_batch(&sql)
                .map_err(|e| e.to_string())?;
            self.installed
                .get_mut(table)
                .unwrap()
                .insert(key.clone(), kind);
        }
        Ok(())
    }
    pub fn prepare(&mut self, plan: &GraphPlan) -> Result<()> {
        if !orchiddb::ir::exec::contains_mutation(&plan.root) {
            return Ok(());
        }
        let before = self.schema.clone();
        self.inspect_plan(plan)?;
        if self.schema == before { return Ok(()); }
        self.sync_schema()
    }
    /// Declare the complete scenario before executing its first GIVEN query.
    /// Only fresh child bindings prove a maximum degree of one across writes;
    /// any later creation involving an existing child leaves a junction table.
    pub fn configure_cypher(&mut self, plans: &[GraphPlan]) -> Result<()> {
        let mut candidates = BTreeMap::<String, (bool, bool)>::new();
        for plan in plans {
            let before = self.schema.clone();
            if self.inspect_plan(plan).is_err() {
                // Invalid query inputs must still reach the normal diagnostic
                // path at execution, not fail a preceding fixture query.
                self.schema = before;
            }
            let mut inspected = plan.clone();
            orchiddb::ir::rules::graph::walk(
                &mut inspected.root,
                &mut |_, _| {},
                &mut |node, _| {
                    if let Node::GraphCreate { nodes, edges, .. } = node {
                        let fresh = nodes
                            .iter()
                            .filter_map(|node| node.bind.as_ref())
                            .collect::<BTreeSet<_>>();
                        for edge in edges.iter() {
                            let degree = candidates
                                .entry(edge.rel_type.clone())
                                .or_insert((true, true));
                            degree.0 &= fresh.contains(&edge.src)
                                && edges
                                    .iter()
                                    .filter(|other| {
                                        other.rel_type == edge.rel_type && other.src == edge.src
                                    })
                                    .count()
                                    == 1;
                            degree.1 &= fresh.contains(&edge.dst)
                                && edges
                                    .iter()
                                    .filter(|other| {
                                        other.rel_type == edge.rel_type && other.dst == edge.dst
                                    })
                                    .count()
                                    == 1;
                        }
                    }
                },
            );
        }
        for edge in &self.schema.edges {
            if let Some((source, destination)) = candidates.get(&edge.0) {
                if *destination && (!*source || edge.1 <= edge.2) {
                    self.schema
                        .foreign_keys
                        .insert(edge.clone(), ForeignKeyEndpoint::Destination);
                } else if *source {
                    self.schema
                        .foreign_keys
                        .insert(edge.clone(), ForeignKeyEndpoint::Source);
                }
            }
        }
        self.sync_schema()
    }
    fn inspect_plan(&mut self, plan: &GraphPlan) -> Result<()> {
        if !orchiddb::ir::exec::contains_mutation(&plan.root) {
            return Ok(());
        }
        let mut bindings = BTreeMap::<String, IrExpr>::new();
        let mut labels = BTreeMap::<String, BTreeSet<String>>::new();
        let mut edges = Vec::<(String, String, String)>::new();
        let mut entities = BTreeMap::<String, BTreeSet<Owner>>::new();
        fn visit(
            node: &Node,
            schema: &mut Schema,
            bindings: &mut BTreeMap<String, IrExpr>,
            labels: &mut BTreeMap<String, BTreeSet<String>>,
            edges: &mut Vec<Edge>,
            entities: &mut BTreeMap<String, BTreeSet<Owner>>,
        ) -> Result<()> {
            match node {
                Node::GraphProject { items, .. } => {
                    for item in items {
                        if let Some(owners) = expression_owners(&item.expr, entities) {
                            labels.insert(
                                item.alias.clone(),
                                owners
                                    .iter()
                                    .filter(|(edge, _)| !edge)
                                    .map(|(_, label)| label.clone())
                                    .collect(),
                            );
                            entities.insert(item.alias.clone(), owners);
                        }
                        bindings.insert(item.alias.clone(), resolve(&item.expr, bindings, 0));
                    }
                }
                Node::GraphBind { bind, expr, .. } => {
                    let current = IrExpr::Binding("current".into());
                    if let Some(owners) =
                        expression_owners(expr.as_ref().unwrap_or(&current), entities)
                    {
                        labels.insert(
                            bind.clone(),
                            owners
                                .iter()
                                .filter(|(edge, _)| !edge)
                                .map(|(_, label)| label.clone())
                                .collect(),
                        );
                        entities.insert(bind.clone(), owners);
                    }
                }
                Node::GraphCurrentProject { expr, .. } => {
                    if let Some(owners) = expression_owners(expr, entities) {
                        labels.insert(
                            "current".into(),
                            owners
                                .iter()
                                .filter(|(edge, _)| !edge)
                                .map(|(_, label)| label.clone())
                                .collect(),
                        );
                        entities.insert("current".into(), owners);
                    }
                }
                Node::GraphUnwind {
                    input_expr, bind, ..
                } => {
                    let resolved = resolve(input_expr, bindings, 0);
                    if let IrExpr::List(items) = resolved {
                        if let Some(item) = items.first() {
                            bindings.insert(bind.clone(), item.clone());
                        }
                    }
                }
                Node::GraphNodeScan {
                    binding,
                    labels: selection,
                    ..
                } => {
                    let available = labels.get(binding).unwrap_or(&schema.nodes);
                    let names = selected_labels(selection, available);
                    labels.insert(binding.clone(), names.clone());
                    entities.insert(
                        binding.clone(),
                        names.into_iter().map(|name| (false, name)).collect(),
                    );
                }
                Node::GraphExpand {
                    target,
                    target_labels,
                    rel_binding,
                    rel_types,
                    ..
                } => {
                    let available = labels.get(target).unwrap_or(&schema.nodes);
                    let names = selected_labels(target_labels, available);
                    labels.insert(target.clone(), names.clone());
                    entities.insert(
                        target.clone(),
                        names.into_iter().map(|name| (false, name)).collect(),
                    );
                    if let Some(binding) = rel_binding {
                        let types = schema.edges.iter().map(|edge| edge.0.clone()).collect();
                        entities.insert(
                            binding.clone(),
                            selected_labels(rel_types, &types)
                                .into_iter()
                                .map(|name| (true, name))
                                .collect(),
                        );
                    }
                }
                Node::GraphCreate {
                    nodes,
                    edges: created,
                    ..
                } => {
                    for node in nodes {
                        schema.nodes.insert(node.label.clone());
                        if let Some(binding) = &node.bind {
                            labels.insert(binding.clone(), BTreeSet::from([node.label.clone()]));
                            entities.insert(
                                binding.clone(),
                                BTreeSet::from([(false, node.label.clone())]),
                            );
                        }
                        if let Some(properties) = &node.properties {
                            add_map(
                                schema,
                                &(false, node.label.clone()),
                                &resolve(properties, bindings, 0),
                                entities,
                            )?;
                        }
                    }
                    for edge in created {
                        edges.push((edge.rel_type.clone(), edge.src.clone(), edge.dst.clone()));
                        if let Some(binding) = &edge.bind {
                            entities.insert(
                                binding.clone(),
                                BTreeSet::from([(true, edge.rel_type.clone())]),
                            );
                        }
                        if let Some(properties) = &edge.properties {
                            add_map(
                                schema,
                                &(true, edge.rel_type.clone()),
                                &resolve(properties, bindings, 0),
                                entities,
                            )?;
                        }
                    }
                }
                Node::GraphSetProperty { items, .. } => {
                    for item in items {
                        let value = resolve(&item.value, bindings, 0);
                        for owner in schema.targets(&item.target, entities) {
                            if item.mode == SetMode::Property {
                                schema.add_property(
                                    &owner,
                                    &item.key,
                                    expression_type(&value, &schema.properties)?,
                                )?;
                            } else if matches!(item.mode, SetMode::Replace | SetMode::Merge) {
                                add_map(schema, &owner, &value, entities)?;
                            }
                        }
                    }
                }
                Node::GraphProcedureCall {
                    name, args, yields, ..
                } if name.starts_with("gremlin.mutation.") => {
                    let resolved = args
                        .iter()
                        .map(|arg| resolve(&arg.value, bindings, 0))
                        .collect::<Vec<_>>();
                    if name.ends_with("add_vertex") || name.ends_with("add_edge") {
                        let edge = name.ends_with("add_edge");
                        let Some(IrExpr::Lit(Lit::String(label))) = resolved.first() else {
                            return Err(
                                "Relational fixture requires a statically declared element label"
                                    .into(),
                            );
                        };
                        if edge {
                            let binding = |index: usize| match args.get(index).map(|arg| &arg.value)
                            {
                                Some(IrExpr::Binding(name)) => name.clone(),
                                _ => String::new(),
                            };
                            edges.push((label.clone(), binding(1), binding(2)));
                        } else {
                            schema.nodes.insert(label.clone());
                        }
                        let owner = (edge, label.clone());
                        for binding in yields {
                            entities.insert(binding.clone(), BTreeSet::from([owner.clone()]));
                            if !edge {
                                labels.insert(binding.clone(), BTreeSet::from([label.clone()]));
                            }
                            bindings.remove(binding);
                        }
                        // Creation may append property-cardinality records after
                        // the map. The map's ABI position does not change.
                        if let Some(properties) = resolved.get(if edge { 3 } else { 1 }) {
                            add_map(schema, &owner, properties, entities)?;
                        }
                    } else if name.ends_with("merge_create") && resolved.len() >= 6 {
                        let edge = matches!(&resolved[2], IrExpr::Lit(Lit::Bool(true)));
                        // Merge criteria and onCreate maps declare the same ordinary
                        // columns as addV/addE. Tokens are structural, not properties.
                        let label = resolved[..2].iter().find_map(merge_label)
                            .unwrap_or_else(|| if edge { "edge" } else { "vertex" }.into());
                        let owner = (edge, label.clone());
                        if edge {
                            let binding = |index: usize| match &args[index].value {
                                IrExpr::Binding(name) => name.clone(),
                                _ => String::new(),
                            };
                            edges.push((label.clone(), binding(3), binding(4)));
                        } else {
                            schema.nodes.insert(label.clone());
                        }
                        for index in [0, 1, 5] {
                            add_map(schema, &owner, &resolved[index], entities)?;
                        }
                        for binding in yields {
                            entities.insert(binding.clone(), BTreeSet::from([owner.clone()]));
                            if !edge {
                                labels.insert(binding.clone(), BTreeSet::from([label.clone()]));
                            }
                            bindings.remove(binding);
                        }
                    } else if name.ends_with("merge_update") && resolved.len() >= 2 {
                        for owner in schema.targets(&args[0].value, entities) {
                            add_map(schema, &owner, &resolved[1], entities)?;
                        }
                    } else if name.ends_with("property_native") && args.len() >= 3 {
                        if let IrExpr::Lit(Lit::String(key)) = &resolved[1] {
                            for owner in schema.targets(&args[0].value, entities) {
                                schema.add_property(
                                    &owner,
                                    key,
                                    expression_type(&resolved[2], &schema.properties)?,
                                )?;
                            }
                        }
                    }
                }
                _ => {}
            }
            Ok(())
        }
        // A MERGE match arm may update a label declared by its create arm.
        // Discover declarations before assigning properties in either arm.
        let mut declared = plan.clone();
        orchiddb::ir::rules::graph::walk(&mut declared.root, &mut |_, _| {}, &mut |node, _| {
            if let Node::GraphCreate { nodes, .. } = node {
                self.schema
                    .nodes
                    .extend(nodes.iter().map(|node| node.label.clone()));
            }
            if let Node::GraphProcedureCall { name, args, .. } = node {
                if name == "gremlin.mutation.add_vertex" {
                    if let Some(IrExpr::Lit(Lit::String(label))) = args.first().map(|arg| &arg.value) {
                        self.schema.nodes.insert(label.clone());
                    }
                } else if name == "gremlin.mutation.merge_create" && args.len() >= 3
                    && matches!(args[2].value, IrExpr::Lit(Lit::Bool(false))) {
                    if let Some(label) = args[..2].iter().find_map(|arg| merge_label(&arg.value)) {
                        self.schema.nodes.insert(label);
                    }
                }
            }
        });
        let mut inspected = plan.clone();
        let mut result = Ok(());
        orchiddb::ir::rules::graph::walk(&mut inspected.root, &mut |_, _| {}, &mut |node, _| {
            if result.is_ok() {
                result = visit(
                    node,
                    &mut self.schema,
                    &mut bindings,
                    &mut labels,
                    &mut edges,
                    &mut entities,
                );
            }
        });
        result?;
        for (kind, src, dst) in edges {
            let sources = labels
                .get(&src)
                .filter(|set| !set.is_empty())
                .unwrap_or(&self.schema.nodes);
            let targets = labels
                .get(&dst)
                .filter(|set| !set.is_empty())
                .unwrap_or(&self.schema.nodes);
            for a in sources {
                for b in targets {
                    self.schema
                        .edges
                        .insert((kind.clone(), a.clone(), b.clone()));
                }
            }
        }
        Ok(())
    }
    pub fn install(&mut self, fixture: &Value) -> Result<()> {
        self.reset()?;
        if fixture["allow_null_property_values"].as_bool() == Some(true) {
            return Err("relational fixture cannot distinguish stored-null properties from absent property columns".into());
        }
        let nodes = fixture["nodes"]
            .as_array()
            .ok_or("Fixture nodes must be an array")?;
        let edges = fixture["edges"]
            .as_array()
            .ok_or("Fixture edges must be an array")?;
        let mut addresses = BTreeMap::new();
        for node in nodes {
            let label = node["label"].as_str().ok_or("Fixture node label missing")?;
            self.schema.nodes.insert(label.into());
            let id = node["id"]
                .as_i64()
                .ok_or("Relational fixture requires integer element identities")?;
            addresses.insert(id, label.to_string());
            if let Some(records) = node["property_records"].as_array() {
                let mut keys = BTreeSet::new();
                for record in records {
                    if !keys.insert(record["key"].as_str().unwrap_or(""))
                        || record["meta"].as_object().is_some_and(|m| !m.is_empty())
                    {
                        return Err("fixture has multi/meta-properties not represented by ordinary property columns".into());
                    }
                }
            }
        }
        for edge in edges {
            let src = addresses
                .get(&edge["src"].as_i64().ok_or("Invalid source ID")?)
                .ok_or("Unknown source ID")?;
            let dst = addresses
                .get(&edge["dst"].as_i64().ok_or("Invalid target ID")?)
                .ok_or("Unknown target ID")?;
            self.schema.edges.insert((
                edge["label"].as_str().ok_or("Missing edge label")?.into(),
                src.clone(),
                dst.clone(),
            ));
        }
        for (edge, items) in [(false, nodes), (true, edges)] {
            for item in items {
                let owner = (edge, item["label"].as_str().unwrap().to_owned());
                let kind = value_scalar(&item["id"], item["id_type"].as_str())?.data_type();
                if self
                    .schema
                    .key_types
                    .get(&owner)
                    .is_some_and(|previous| previous != &kind)
                {
                    return Err("Relational fixture requires consistent identity types within each element table".into());
                }
                self.schema.key_types.insert(owner, kind);
                for (key, value) in item["properties"]
                    .as_object()
                    .ok_or("Fixture properties must be an object")?
                {
                    self.schema.add_property(
                        &(edge, item["label"].as_str().unwrap().into()),
                        key,
                        value_scalar(value, item["property_types"][key].as_str())?.data_type(),
                    )?;
                }
            }
        }
        if !self.allow_relationship_expansion {
            for edge in self.schema.edges.clone() {
                let rows = edges
                    .iter()
                    .filter(|row| {
                        row["label"].as_str() == Some(edge.0.as_str())
                            && addresses[&row["src"].as_i64().unwrap()] == edge.1
                            && addresses[&row["dst"].as_i64().unwrap()] == edge.2
                    })
                    .collect::<Vec<_>>();
                let unique = |key: &str| {
                    rows.iter()
                        .map(|row| row[key].as_i64().unwrap())
                        .collect::<BTreeSet<_>>()
                        .len()
                        == rows.len()
                };
                if unique("dst") && (!unique("src") || edge.1 <= edge.2) {
                    self.schema
                        .foreign_keys
                        .insert(edge, ForeignKeyEndpoint::Destination);
                } else if unique("src") {
                    self.schema
                        .foreign_keys
                        .insert(edge, ForeignKeyEndpoint::Source);
                }
            }
        }
        for (sequence, items) in [("node_ids", nodes), ("edge_ids", edges)] {
            let start = items
                .iter()
                .filter_map(|item| item["id"].as_i64())
                .max()
                .unwrap_or(0)
                .max(0)
                .checked_add(1)
                .ok_or("Fixture identity exceeds sequence range")?;
            self.storage
                .execute_batch(&format!(
                    "DROP SEQUENCE {sequence}; CREATE SEQUENCE {sequence} START {start}"
                ))
                .map_err(|e| e.to_string())?;
        }
        self.sync_schema()?;
        self.storage
            .execute_batch("BEGIN")
            .map_err(|e| e.to_string())?;
        let result: Result<()> = (|| {
            for (edge, items) in [(false, nodes), (true, edges)] {
                let mut inserts = Vec::<((String, Vec<String>), Vec<String>)>::new();
                for item in items {
                    let label = item["label"].as_str().unwrap();
                    if edge {
                        let identity = (
                            label.to_string(),
                            addresses[&item["src"].as_i64().unwrap()].clone(),
                            addresses[&item["dst"].as_i64().unwrap()].clone(),
                        );
                        if let Some((direction, child, _, column, _)) = self.fk_layout(&identity) {
                            let (child_id, parent_id) = if direction == ForeignKeyEndpoint::Source {
                                (&item["src"], &item["dst"])
                            } else {
                                (&item["dst"], &item["src"])
                            };
                            let mut sets = vec![format!("{}={}", quote(&column), parent_id)];
                            for (key, value) in item["properties"].as_object().unwrap() {
                                let declared = item["property_types"][key].as_str();
                                let scalar = if self.schema.owned[&(true, label.into())][key]
                                    == union_type()
                                {
                                    let semantic = orchiddb::ir::catalog::import::typed_property(
                                        value, declared,
                                    )?;
                                    orchiddb::ir::functions::scalar_union::scalar(&semantic)
                                        .map_err(|e| e.to_string())?
                                } else {
                                    value_scalar(value, declared)?
                                };
                                sets.push(format!(
                                    "{}={}",
                                    quote(&format!("{}_{}", self.edge_table(&identity), key)),
                                    scalar_literal(scalar)?
                                ));
                            }
                            self.storage
                                .execute_batch(&format!(
                                    "UPDATE {} SET {} WHERE node_id={}",
                                    quote(&self.node_table(&child)),
                                    sets.join(","),
                                    child_id
                                ))
                                .map_err(|e| e.to_string())?;
                            continue;
                        }
                    }
                    let mut columns = vec![quote(primary_key(edge))];
                    let mut values =
                        vec![item["id"].as_i64().ok_or("Invalid element ID")?.to_string()];
                    let table = if edge {
                        let src = item["src"].as_i64().unwrap();
                        let dst = item["dst"].as_i64().unwrap();
                        columns.extend(["source_id".into(), "target_id".into()]);
                        values.extend([src.to_string(), dst.to_string()]);
                        self.edge_table(&(
                            label.into(),
                            addresses[&src].clone(),
                            addresses[&dst].clone(),
                        ))
                    } else {
                        columns.push(quote(LABELS));
                        values.push(format!("[{}]", text(label)));
                        // Preserve fixture relationship IDs independently of
                        // the node IDs that own the reference columns.
                        for identity in self.schema.foreign_keys.keys() {
                            let (direction, child, _, _, id) = self.fk_layout(identity).unwrap();
                            if child != label {
                                continue;
                            }
                            let endpoint = if direction == ForeignKeyEndpoint::Source {
                                "src"
                            } else {
                                "dst"
                            };
                            let incoming = edges.iter().find(|edge| {
                                edge["label"] == identity.0
                                    && edge[endpoint] == item["id"]
                                    && addresses[&edge["src"].as_i64().unwrap()] == identity.1
                                    && addresses[&edge["dst"].as_i64().unwrap()] == identity.2
                            });
                            if let Some(incoming) = incoming {
                                columns.push(quote(&id));
                                values.push(incoming["id"].to_string());
                            }
                        }
                        self.node_table(label)
                    };
                    for (key, value) in item["properties"].as_object().unwrap() {
                        let mut scalar = value_scalar(value, item["property_types"][key].as_str())?;
                        if self.schema.owned[&(edge, label.into())][key] == union_type() {
                            let semantic = orchiddb::ir::catalog::import::typed_property(
                                value,
                                item["property_types"][key].as_str(),
                            )?;
                            scalar = orchiddb::ir::functions::scalar_union::scalar(&semantic)
                                .map_err(|e| e.to_string())?;
                        }
                        columns.push(quote(&property(key)));
                        values.push(scalar_literal(scalar)?);
                    }
                    let key = (table, columns);
                    if inserts.last().is_none_or(|(previous, _)| previous != &key) {
                        inserts.push((key, Vec::new()));
                    }
                    inserts.last_mut().unwrap().1.push(format!("({})", values.join(",")));
                }
                for ((table, columns), rows) in inserts {
                    for batch in rows.chunks(256) {
                        self.storage.execute_batch(&format!(
                            "INSERT INTO {} ({}) VALUES {}",
                            quote(&table), columns.join(","), batch.join(","),
                        )).map_err(|e| e.to_string())?;
                    }
                }
            }
            Ok(())
        })();
        self.storage
            .execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })
            .map_err(|e| e.to_string())?;
        result?;
        if let Some(key) = fixture["fixture_key"].as_str() {
            self.fixtures.retain(|(name, _)| name != key);
            self.fixtures.push_back((key.into(), fixture.clone()));
            if self.fixtures.len() > 16 {
                self.fixtures.pop_front();
            }
        }
        Ok(())
    }
    pub fn restore(&mut self, key: &str) -> Result<()> {
        let fixture = self
            .fixtures
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, fixture)| fixture.clone())
            .ok_or_else(|| format!("Unknown cached fixture: {key}"))?;
        self.install(&fixture)
    }
    /// Compile-only diagnostic using exactly the catalog installed above.
    pub async fn diagnostic_sql(&self, plan: &GraphPlan) -> Result<Value> {
        self.diagnostic_sql_with_estimates(plan, false).await
    }
    pub async fn diagnostic_sql_with_estimates(&self, plan: &GraphPlan, estimates: bool) -> Result<Value> {
        let mut result = orchiddb::ir::diagnostics::conformance_sql_with_estimates(
            self.storage.try_clone().map_err(|e| e.to_string())?,
            self.diagnostic_mapping.clone(),
            plan,
            Arc::new(self.engine.procedure_catalog().clone()),
            estimates,
        )
        .await?;
        result["schema_sql"] = serde_json::json!(self.schema_sql()?);
        result["storage_tables"] = serde_json::json!(self.storage_tables());
        Ok(result)
    }
    pub fn schema_sql(&self) -> Result<Vec<String>> {
        let mut statement = self
            .storage
            .prepare("SELECT sql FROM duckdb_sequences() ORDER BY sequence_name")
            .map_err(|e| e.to_string())?;
        let mut statements = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        if let Some(tables) = self.table_ddl.borrow().as_ref() {
            statements.extend(tables.iter().cloned());
            return Ok(statements);
        }
        let sequence_count = statements.len();
        // Table definitions are invalidated by every harness DDL operation.
        for table in &self.creation_order {
            let mut columns = self.storage.prepare("SELECT column_name, data_type, column_default, is_nullable FROM duckdb_columns() WHERE table_name=? ORDER BY column_index")
                .map_err(|e| e.to_string())?;
            let columns = columns
                .query_map([table], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, bool>(3)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            let mut definitions = columns
                .into_iter()
                .map(|(name, kind, default, nullable)| {
                    format!(
                        "{} {kind}{}{}",
                        quote(&name),
                        default
                            .map(|expr| format!(" DEFAULT {expr}"))
                            .unwrap_or_default(),
                        if nullable { "" } else { " NOT NULL" }
                    )
                })
                .collect::<Vec<_>>();
            // DuckDB 1.5.2 omits self-referencing FK text from duckdb_tables()
            // and constraint_text. Structured catalog fields retain the keys.
            let mut constraints = self.storage.prepare("SELECT constraint_type, constraint_text, referenced_table, to_json(constraint_column_names)::VARCHAR, to_json(referenced_column_names)::VARCHAR FROM duckdb_constraints() WHERE table_name=? ORDER BY constraint_index")
                .map_err(|e| e.to_string())?;
            let constraints = constraints
                .query_map([table], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<String>>(4)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for (kind, text, parent, columns, referenced) in constraints {
                if kind == "NOT NULL" {
                    continue;
                }
                if kind == "FOREIGN KEY" {
                    let columns: Vec<String> =
                        serde_json::from_str(&columns).map_err(|e| e.to_string())?;
                    let referenced: Vec<String> =
                        serde_json::from_str(&referenced.ok_or("Missing FK referenced columns")?)
                            .map_err(|e| e.to_string())?;
                    definitions.push(format!(
                        "FOREIGN KEY ({}) REFERENCES {}({})",
                        columns
                            .iter()
                            .map(|c| quote(c))
                            .collect::<Vec<_>>()
                            .join(", "),
                        quote(&parent.ok_or("Missing FK referenced table")?),
                        referenced
                            .iter()
                            .map(|c| quote(c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ));
                } else {
                    definitions.push(text);
                }
            }
            statements.push(format!(
                "CREATE TABLE {} (\n  {}\n);",
                quote(table),
                definitions.join(",\n  ")
            ));
        }
        *self.table_ddl.borrow_mut() = Some(statements[sequence_count..].to_vec());
        Ok(statements)
    }
    pub fn storage_tables(&self) -> Vec<String> {
        self.installed.keys().cloned().collect()
    }
    pub fn snapshot(&mut self) -> Result<orchiddb::engine::CypherStateSnapshot> {
        let mut snapshot = self.engine.cypher_state_snapshot()?;
        // Mapping ordinals can change when input queries declare another table.
        // Side-effect observations identify stored rows by label and primary key,
        // so schema registration alone cannot look like deletion and recreation.
        snapshot.nodes.clear();
        snapshot.relationships.clear();
        let mut scans = self
            .schema
            .nodes
            .iter()
            .map(|label| {
                (
                    false,
                    label.clone(),
                    self.node_table(label),
                    "node_id".to_string(),
                    String::new(),
                )
            })
            .collect::<Vec<_>>();
        for edge in &self.schema.edges {
            if let Some((_, child, _, column, id)) = self.fk_layout(edge) {
                scans.push((
                    true,
                    edge.0.clone(),
                    self.node_table(&child),
                    id,
                    format!(" WHERE {} IS NOT NULL", quote(&column)),
                ));
            } else {
                scans.push((
                    true,
                    edge.0.clone(),
                    self.edge_table(edge),
                    "edge_id".into(),
                    String::new(),
                ));
            }
        }
        let mut stable = BTreeMap::new();
        for (edge, identity, table, key, filter) in scans {
            let mut query = self
                .storage
                .prepare(&format!(
                    "SELECT {} FROM {}{}",
                    quote(&key),
                    quote(&table),
                    filter
                ))
                .map_err(|e| e.to_string())?;
            let keys = query
                .query_map([], |row| row.get::<_, i64>(0))
                .map_err(|e| e.to_string())?;
            let target = if edge {
                &mut snapshot.relationships
            } else {
                &mut snapshot.nodes
            };
            for key in keys {
                let key = key.map_err(|e| e.to_string())?;
                let id = serde_json::json!({"type":"string", "value":format!("{identity}:{key}")});
                stable.insert((edge, key), id.clone());
                target.push(vec![id]);
            }
        }
        // The fixture allocates globally unique integer node/edge keys. Apply
        // its stable schema-independent identities to property owners too;
        // the engine snapshot preserves the original typed source keys.
        for (edge, properties) in [(false, &mut snapshot.node_properties), (true, &mut snapshot.edge_properties)] {
            for row in properties {
                let key = row[0]["value"].as_i64();
                let id = key.and_then(|key| stable.get(&(edge, key)))
                    .ok_or_else(|| format!("property owner missing from fixture snapshot: {}", row[0]))?;
                row[0] = id.clone();
            }
        }
        Ok(snapshot)
    }
    #[cfg(test)]
    pub fn sql(&self, sql: &str) -> Result<i64> {
        self.storage
            .query_row(sql, [], |row| row.get(0))
            .map_err(|e| e.to_string())
    }
}

fn selected_labels(
    selection: &orchiddb::ir::plan::LabelExpr,
    available: &BTreeSet<String>,
) -> BTreeSet<String> {
    use orchiddb::ir::plan::LabelExpr;
    if let LabelExpr::AnyOf(names) | LabelExpr::AllOf(names) = selection {
        let selected = names.iter().cloned().collect::<BTreeSet<_>>();
        if !selected.is_empty() {
            return selected;
        }
    }
    available.clone()
}

fn resolve(expr: &IrExpr, bindings: &BTreeMap<String, IrExpr>, depth: usize) -> IrExpr {
    if depth > 32 {
        return expr.clone();
    }
    match expr {
        IrExpr::Binding(name) => bindings
            .get(name)
            // Values were resolved at their definition. Resolving them again
            // against later bindings captures reassigned Gremlin variables
            // and can expand a history expression exponentially.
            .cloned()
            .unwrap_or_else(|| expr.clone()),
        IrExpr::Call { name, args } => IrExpr::Call {
            name: name.clone(),
            args: args
                .iter()
                .map(|value| resolve(value, bindings, depth + 1))
                .collect(),
        },
        IrExpr::List(values) => IrExpr::List(
            values
                .iter()
                .map(|value| resolve(value, bindings, depth + 1))
                .collect(),
        ),
        IrExpr::Binary { op, lhs, rhs } => IrExpr::Binary {
            op: *op,
            lhs: Box::new(resolve(lhs, bindings, depth + 1)),
            rhs: Box::new(resolve(rhs, bindings, depth + 1)),
        },
        _ => expr.clone(),
    }
}
fn merge_label(expr: &IrExpr) -> Option<String> {
    let IrExpr::Call { name, args } = expr else { return None; };
    if name != "map_literal" || args.len() != 2 { return None; }
    let (IrExpr::List(keys), IrExpr::List(values)) = (&args[0], &args[1]) else { return None; };
    keys.iter().zip(values).find_map(|(key, value)| {
        let IrExpr::Call { name, args } = key else { return None; };
        if name == "gremlin_token_literal" && matches!(args.as_slice(), [IrExpr::Lit(Lit::String(token))] if token == "label") {
            if let IrExpr::Lit(Lit::String(label)) = value { return Some(label.clone()); }
        }
        None
    })
}

fn add_map(
    schema: &mut Schema,
    owner: &Owner,
    expr: &IrExpr,
    entities: &BTreeMap<String, BTreeSet<Owner>>,
) -> Result<()> {
    if let IrExpr::Binding(binding) = expr {
        let fields = entities
            .get(binding)
            .into_iter()
            .flatten()
            .filter_map(|source| schema.owned.get(source))
            .flat_map(|fields| fields.iter().map(|(key, kind)| (key.clone(), kind.clone())))
            .collect::<Vec<_>>();
        for (key, kind) in fields {
            schema.add_property(owner, &key, kind)?;
        }
    }
    if let IrExpr::Call { name, args } = expr {
        if name == "cypher_property_map" || name == "properties" {
            for arg in args {
                add_map(schema, owner, arg, entities)?;
            }
        } else if name == "map" {
            for pair in args.chunks_exact(2) {
                if let IrExpr::Lit(Lit::String(key)) = &pair[0] {
                    schema.add_property(
                        owner,
                        key,
                        expression_type(&pair[1], &schema.properties)?,
                    )?;
                }
            }
        } else if name == "map_literal" && args.len() == 2 {
            if let (IrExpr::List(keys), IrExpr::List(values)) = (&args[0], &args[1]) {
                for (key, value) in keys.iter().zip(values) {
                    if let IrExpr::Lit(Lit::String(key)) = key {
                        schema.add_property(
                            owner,
                            key,
                            expression_type(value, &schema.properties)?,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}
fn expression_type(expr: &IrExpr, properties: &BTreeMap<String, DataType>) -> Result<DataType> {
    Ok(match expr {
        IrExpr::Lit(Lit::Scalar(value)) => value.data_type(),
        IrExpr::Lit(Lit::Int(_)) => DataType::Int64,
        IrExpr::Lit(Lit::Float(_)) => DataType::Float64,
        IrExpr::Lit(Lit::Bool(_)) => DataType::Boolean,
        IrExpr::Lit(Lit::String(_)) => DataType::Utf8,
        IrExpr::Property { name, .. } => properties.get(name).cloned().unwrap_or(DataType::Null),
        IrExpr::List(values) => DataType::List(Arc::new(Field::new(
            "item",
            values.iter().try_fold(DataType::Null, |kind, value| {
                merge_type(&kind, &expression_type(value, properties)?)
            })?,
            true,
        ))),
        IrExpr::Binary { lhs, rhs, .. } => {
            let left = expression_type(lhs, properties)?;
            let right = expression_type(rhs, properties)?;
            if left.is_numeric()
                && right.is_numeric()
                && (matches!(left, DataType::Float32 | DataType::Float64)
                    || matches!(right, DataType::Float32 | DataType::Float64))
            {
                DataType::Float64
            } else {
                merge_type(&left, &right)?
            }
        }
        IrExpr::Call { name, args } => match name.as_str() {
            "cypher_property_value" if args.len() == 1 => expression_type(&args[0], properties)?,
            "cypher_temporal.date" => orchiddb::ir::functions::domain::data_type("cypher_temporal", DataType::Date32),
            "cypher_temporal.localtime" | "cypher_temporal.time" | "cypher_temporal.localdatetime" | "cypher_temporal.datetime" | "cypher_temporal.duration" =>
                orchiddb::ir::functions::domain::data_type("cypher_temporal", DataType::Utf8),
            "cast_byte" => DataType::Int8,
            "cast_short" => DataType::Int16,
            "cast_int" => DataType::Int32,
            "cast_long" | "size" | "length" | "tointeger" => DataType::Int64,
            "cast_float" => DataType::Float32,
            "cast_double" | "tofloat" | "rand" => DataType::Float64,
            "tostring" | "toString" => DataType::Utf8,
            "split" => DataType::List(Arc::new(Field::new("item", DataType::Utf8, true))),
            "range" => DataType::List(Arc::new(Field::new("item", DataType::Int64, true))),
            "coalesce" => args.iter().try_fold(DataType::Null, |kind, arg| {
                merge_type(&kind, &expression_type(arg, properties)?)
            })?,
            _ => DataType::Null,
        },
        _ => DataType::Null,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fixture_one_to_many_uses_reference_columns_with_original_edge_ids() {
        let mut fixture = RelationalFixture::new().unwrap();
        fixture.allow_relationship_expansion = false;
        fixture
            .install(&serde_json::json!({"fixture_key":"family", "nodes":[
            {"id":1,"label":"Person","properties":{"name":"parent"}},
            {"id":2,"label":"Person","properties":{"name":"first"}},
            {"id":3,"label":"Person","properties":{"name":"second"}}],
            "edges":[{"id":91,"label":"KNOWS","src":1,"dst":2,"properties":{"weight":4}},
                     {"id":92,"label":"KNOWS","src":1,"dst":3,"properties":{"weight":5}}]}))
            .unwrap();
        assert_eq!(fixture.storage_tables(), vec!["person"]);
        let ddl = fixture.schema_sql().unwrap().join("\n");
        assert!(
            ddl.contains("\"knows_source_id\" BIGINT") && !ddl.contains("REFERENCES"),
            "{ddl}"
        );
        Connection::open_in_memory()
            .unwrap()
            .execute_batch(&ddl)
            .unwrap();
        assert_eq!(fixture.sql("SELECT count(*) FROM person child JOIN person parent ON child.knows_source_id=parent.node_id WHERE parent.name='parent'").unwrap(), 2);
        assert_eq!(
            fixture
                .sql("SELECT knows_edge_id FROM person WHERE name='first'")
                .unwrap(),
            91
        );
        assert_eq!(
            fixture
                .sql(
                    "SELECT count(*) FROM duckdb_constraints() WHERE constraint_type='FOREIGN KEY'"
                )
                .unwrap(),
            0
        );
        let read = cypher(
            &mut fixture,
            "MATCH (a)-[r:KNOWS]->(b) RETURN a.name,b.name,r.weight",
        )
        .await
        .unwrap();
        assert_eq!(read.returned.batch.num_rows(), 2);
        let before = fixture.snapshot().unwrap();
        assert_eq!(before.relationships.len(), 2);
        // Node properties remain writable when other nodes reference this row.
        cypher(
            &mut fixture,
            "MATCH (p:Person {name:'parent'}) SET p.name='renamed'",
        )
        .await
        .unwrap();
        assert_eq!(
            fixture
                .sql("SELECT count(*) FROM person WHERE name='renamed'")
                .unwrap(),
            1
        );
        cypher(
            &mut fixture,
            "MATCH (a)-[r:KNOWS]->(b {name:'first'}) SET r.weight=8",
        )
        .await
        .unwrap();
        assert_eq!(
            fixture
                .sql("SELECT knows_weight FROM person WHERE name='first'")
                .unwrap(),
            8
        );
        cypher(
            &mut fixture,
            "MATCH ()-[r:KNOWS]->(b {name:'first'}) DELETE r",
        )
        .await
        .unwrap();
        assert_eq!(fixture.sql("SELECT count(*) FROM person").unwrap(), 3);
        assert_eq!(fixture.snapshot().unwrap().relationships.len(), 1);
        fixture.restore("family").unwrap();
        assert_eq!(fixture.snapshot().unwrap().relationships.len(), 2);
        fixture.reset().unwrap();
    }

    #[tokio::test]
    async fn cypher_cardinality_accounts_for_every_query_in_a_scenario() {
        let mut fixture = RelationalFixture::new().unwrap();
        let setup = "CREATE (p:Parent {name:'P'}), (a:Child {name:'A'}), (b:Child {name:'B'}), (p)-[:HAS {weight:1}]->(a), (p)-[:HAS {weight:2}]->(b)";
        let parse = |query| {
            orchiddb::language::cypher::preparation::prepare(query, &BTreeMap::new(), None).unwrap()
        };
        fixture.configure_cypher(&[parse(setup)]).unwrap();
        cypher(&mut fixture, setup).await.unwrap();
        assert_eq!(fixture.storage_tables(), vec!["child", "parent"]);
        assert_eq!(
            fixture
                .sql("SELECT count(*) FROM child JOIN parent ON child.has_source_id=parent.node_id")
                .unwrap(),
            2
        );
        assert_eq!(fixture.snapshot().unwrap().relationships.len(), 2);
        fixture.reset().unwrap();
        let expand = "MATCH (p:Parent), (a:Child) CREATE (p)-[:HAS]->(a)";
        fixture
            .configure_cypher(&[parse(setup), parse(expand)])
            .unwrap();
        cypher(&mut fixture, setup).await.unwrap();
        cypher(&mut fixture, expand).await.unwrap();
        assert_eq!(fixture.sql("SELECT count(*) FROM has").unwrap(), 4);
        assert_eq!(fixture.storage_tables(), vec!["child", "has", "parent"]);
    }

    #[tokio::test]
    async fn same_relationship_type_writes_across_reference_column_partitions() {
        let mut fixture = RelationalFixture::new().unwrap();
        let query = "CREATE (:A)<-[:R {weight:1}]-(:B)<-[:R {weight:2}]-(:C)";
        let plan = orchiddb::language::cypher::preparation::prepare(
            query, &BTreeMap::new(), None,
        ).unwrap();
        fixture.configure_cypher(&[plan]).unwrap();
        assert_eq!(fixture.schema.foreign_keys.len(), 2);
        cypher(&mut fixture, query).await.unwrap();
        assert_eq!(fixture.storage_tables(), vec!["a", "b", "c"]);
        assert_eq!(fixture.snapshot().unwrap().relationships.len(), 2);
        let result = cypher(&mut fixture, "MATCH ()-[r:R]->() RETURN r.weight ORDER BY r.weight").await.unwrap();
        assert_eq!(result.returned.batch.num_rows(), 2);
        cypher(&mut fixture, "MATCH ()-[r:R]->() SET r.weight=r.weight+10").await.unwrap();
        cypher(&mut fixture, "MATCH ()-[r:R]->() DELETE r").await.unwrap();
        assert!(fixture.snapshot().unwrap().relationships.is_empty());
        assert_eq!(fixture.snapshot().unwrap().nodes.len(), 3);
    }

    #[tokio::test]
    async fn reference_columns_support_cycles_and_atomic_detach_delete() {
        for query in [
            "CREATE (a:Person {name:'self'})-[:KNOWS]->(a)",
            "CREATE (a:Person {name:'A'}), (b:Person {name:'B'}), (a)-[:KNOWS]->(b), (b)-[:KNOWS]->(a)",
            "CREATE (a:Alpha), (b:Beta), (a)-[:FIRST]->(b), (b)-[:SECOND]->(a)",
        ] {
            let mut fixture = RelationalFixture::new().unwrap();
            let plan = orchiddb::language::cypher::preparation::prepare(
                query, &BTreeMap::new(), None,
            ).unwrap();
            fixture.configure_cypher(&[plan]).unwrap();
            assert!(!fixture.schema.foreign_keys.is_empty());
            cypher(&mut fixture, query).await.unwrap();
            let snapshot = fixture.snapshot().unwrap();
            assert!(!snapshot.relationships.is_empty());
            assert_eq!(fixture.storage_tables().len(), fixture.schema.nodes.len());
            let ddl = fixture.schema_sql().unwrap().join("\n");
            assert!(!ddl.contains("REFERENCES"), "{ddl}");
            Connection::open_in_memory().unwrap().execute_batch(&ddl).unwrap();
            cypher(&mut fixture, "MATCH (n) DETACH DELETE n").await.unwrap();
            let snapshot = fixture.snapshot().unwrap();
            assert!(snapshot.nodes.is_empty());
            assert!(snapshot.relationships.is_empty());
        }
    }

    async fn cypher(
        fixture: &mut RelationalFixture,
        query: &str,
    ) -> Result<orchiddb::engine::QueryResult> {
        let plan = orchiddb::language::cypher::preparation::prepare(
            query,
            &BTreeMap::new(),
            Some(fixture.engine.procedure_catalog()),
        )
        .map_err(|e| e.to_string())?;
        fixture.prepare(&plan)?;
        fixture.engine.execute_plan(&plan).await
    }

    #[tokio::test]
    async fn cached_ddl_tracks_schema_changes_and_live_sequences() {
        let mut fixture = RelationalFixture::new().unwrap();
        cypher(&mut fixture, "CREATE (:Person {name:'A'})").await.unwrap();
        let first = fixture.schema_sql().unwrap();
        assert!(fixture.table_ddl.borrow().is_some());
        let mapping = fixture.diagnostic_mapping.clone();
        cypher(&mut fixture, "CREATE (:Person {name:'B'})").await.unwrap();
        assert!(Arc::ptr_eq(&mapping, &fixture.diagnostic_mapping));
        let second = fixture.schema_sql().unwrap();
        assert_ne!(first[0..2], second[0..2], "sequence state must remain live");
        assert_eq!(first[2..], second[2..]);
        cypher(&mut fixture, "MATCH (p:Person) SET p.age=30").await.unwrap();
        assert!(!Arc::ptr_eq(&mapping, &fixture.diagnostic_mapping));
        assert!(fixture.schema_sql().unwrap().join("\n").contains("\"age\" BIGINT"));
        fixture.reset().unwrap();
        assert!(!fixture.schema_sql().unwrap().join("\n").contains("person"));
    }

    #[tokio::test]
    async fn batched_fixture_import_preserves_sparse_properties_and_restore() {
        let mut fixture = RelationalFixture::new().unwrap();
        let nodes = (1..=600).map(|id| serde_json::json!({"id":id,"label":"Person",
            "properties":if id == 300 { serde_json::json!({"name":"sparse"}) }
                else { serde_json::json!({"name":format!("P{id}"),"age":id}) }
        })).collect::<Vec<_>>();
        fixture.install(&serde_json::json!({"fixture_key":"batch","nodes":nodes,"edges":[]})).unwrap();
        assert_eq!(fixture.sql("SELECT count(*) FROM person").unwrap(),600);
        assert_eq!(fixture.sql("SELECT count(*) FROM person WHERE age IS NULL").unwrap(),1);
        cypher(&mut fixture,"MATCH (p:Person) DETACH DELETE p").await.unwrap();
        fixture.restore("batch").unwrap();
        assert_eq!(fixture.sql("SELECT sum(age) FROM person").unwrap(),180000);
    }

    #[tokio::test]
    async fn writes_and_reads_use_physical_tables() {
        let mut fixture = RelationalFixture::new().unwrap();
        let created = cypher(&mut fixture, "CREATE (a:Person {name:'Alice',score:2}), (b:Person {name:'Bob',score:3}), (a)-[:KNOWS {weight:1}]->(b) RETURN a.name").await.unwrap();
        let people = quote(&fixture.node_table("Person"));
        let knows = quote(&fixture.edge_table(&("KNOWS".into(), "Person".into(), "Person".into())));
        assert_eq!(people, "\"person\"");
        assert_eq!(knows, "\"knows\"");
        assert_eq!(fixture.sql("SELECT count(*) FROM duckdb_columns() WHERE table_name='person' AND column_name IN ('name','score')").unwrap(), 2);
        assert_eq!(fixture.sql("SELECT count(*) FROM duckdb_columns() WHERE (table_name='person' AND column_name='weight') OR (table_name='knows' AND column_name IN ('name','score'))").unwrap(), 0);
        assert_eq!(
            fixture
                .sql(&format!("SELECT count(DISTINCT node_id) FROM {people}"))
                .unwrap(),
            2
        );
        assert_eq!(fixture.sql(&format!("SELECT count(*) FROM {knows} r JOIN {people} a ON r.source_id=a.node_id JOIN {people} b ON r.target_id=b.node_id WHERE a.name='Alice' AND b.name='Bob'")).unwrap(), 1);
        assert!(created.stats.write_queries.iter().any(|statement| {
            statement.sql.contains("INSERT INTO")
                && statement
                    .sql
                    .to_lowercase()
                    .contains(&people.to_lowercase())
        }));
        let read = cypher(
            &mut fixture,
            "MATCH (p:Person) RETURN p.name ORDER BY p.name",
        )
        .await
        .unwrap();
        assert_eq!(read.returned.batch.num_rows(), 2);
        assert!(
            read.stats
                .sql_queries
                .iter()
                .any(|sql| sql.to_lowercase().contains(&people.to_lowercase()))
        );
        assert!(
            !read
                .stats
                .sql_queries
                .iter()
                .any(|sql| sql.contains("__graph_rel_nodes"))
        );
        let updated = cypher(
            &mut fixture,
            "MATCH (p:Person {name:'Alice'}) SET p.score=p.score+5 RETURN p.score",
        )
        .await
        .unwrap();
        assert_eq!(
            fixture
                .sql(&format!("SELECT score FROM {people} WHERE name='Alice'"))
                .unwrap(),
            7
        );
        assert!(
            updated
                .stats
                .write_queries
                .iter()
                .any(|statement| statement.sql.contains("UPDATE"))
        );
        cypher(
            &mut fixture,
            "MATCH (p:Person {name:'Alice'}) DETACH DELETE p",
        )
        .await
        .unwrap();
        assert_eq!(
            fixture
                .sql(&format!("SELECT count(*) FROM {people}"))
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .sql(&format!("SELECT count(*) FROM {knows}"))
                .unwrap(),
            0
        );
        assert_eq!(fixture.sql("SELECT count(*) FROM information_schema.tables WHERE table_name LIKE '__orchiddb_%'").unwrap(), 0);
    }

    #[tokio::test]
    async fn fixtures_restore_table_contents_and_failed_writes_are_atomic() {
        let mut fixture = RelationalFixture::new().unwrap();
        fixture.install(&serde_json::json!({"fixture_key":"people", "nodes":[{"id":1,"label":"Person","properties":{"name":"original","score":2}}],"edges":[]})).unwrap();
        cypher(&mut fixture, "MATCH (p:Person) SET p.score=9")
            .await
            .unwrap();
        fixture.restore("people").unwrap();
        let people = quote(&fixture.node_table("Person"));
        assert_eq!(
            fixture.sql(&format!("SELECT score FROM {people}")).unwrap(),
            2
        );
        fixture
            .storage
            .execute_batch(&format!(
                "ALTER TABLE {people} ALTER COLUMN name SET NOT NULL"
            ))
            .unwrap();
        assert!(
            cypher(
                &mut fixture,
                "CREATE (:Person {name:'temporary'}), (:Person {name:null})"
            )
            .await
            .is_err()
        );
        assert_eq!(
            fixture
                .sql(&format!("SELECT count(*) FROM {people}"))
                .unwrap(),
            1
        );
        fixture.reset().unwrap();
        assert!(fixture.storage_tables().is_empty());
        assert_eq!(
            fixture
                .sql("SELECT count(*) FROM information_schema.tables")
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn unlabelled_nodes_and_partitioned_edges_receive_unique_keys() {
        let mut fixture = RelationalFixture::new().unwrap();
        cypher(&mut fixture, "CREATE (a:Alpha {name:'A'}), (b:Beta {name:'B'}), (a)-[:LINK]->(b), (b)-[:LINK]->(a), ({name:'unlabelled'})").await.unwrap();
        let read = cypher(&mut fixture, "MATCH (a)-[r:LINK]->(b) RETURN a.name,b.name")
            .await
            .unwrap();
        assert_eq!(
            read.returned.batch.num_rows(),
            2,
            "SQL: {:?}; sources: {:?}",
            read.stats.sql_queries,
            read.stats.native_source_queries
        );
        let snapshot = fixture.engine.cypher_state_snapshot().unwrap();
        assert_eq!(snapshot.nodes.len(), 3);
        assert_eq!(snapshot.relationships.len(), 2);
        assert_eq!(
            fixture
                .sql(&format!(
                    "SELECT count(*) FROM {}",
                    quote(&fixture.node_table(""))
                ))
                .unwrap(),
            1
        );
    }

    #[tokio::test]
    async fn declaring_another_label_does_not_change_existing_snapshot_identities() {
        let mut fixture = RelationalFixture::new().unwrap();
        cypher(&mut fixture, "CREATE (:Zed {name:'Z'})")
            .await
            .unwrap();
        let before = fixture.snapshot().unwrap();
        cypher(&mut fixture, "CREATE (:Alpha {name:'A'})")
            .await
            .unwrap();
        let after = fixture.snapshot().unwrap();
        assert_eq!(after.nodes.len(), before.nodes.len() + 1);
        assert!(before.nodes.iter().all(|row| after.nodes.contains(row)));
        assert!(
            before
                .node_properties
                .iter()
                .all(|row| after.node_properties.contains(row))
        );
    }

    #[tokio::test]
    async fn property_columns_have_types_specific_to_their_table() {
        let mut fixture = RelationalFixture::new().unwrap();
        cypher(&mut fixture,"CREATE (:Person {score:1}), (:Software {score:'stable'}), (:person {name:'case-sensitive label'})").await.unwrap();
        assert_eq!(fixture.sql("SELECT score FROM person").unwrap(), 1);
        assert_eq!(
            fixture
                .sql("SELECT count(*) FROM software WHERE score='stable'")
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .sql("SELECT count(*) FROM person_2 WHERE name='case-sensitive label'")
                .unwrap(),
            1
        );
        assert_eq!(fixture.sql("SELECT count(*) FROM duckdb_columns() WHERE table_name='software' AND column_name='name'").unwrap(),0);
        let schema = fixture.schema_sql().unwrap().join("\n");
        assert!(schema.contains("CREATE TABLE \"person\" ("));
        assert!(!schema.contains("property_score"));
    }

    #[tokio::test]
    async fn maps_merge_branches_and_numeric_expressions_declare_owned_columns() {
        let mut fixture = RelationalFixture::new().unwrap();
        cypher(&mut fixture, "CREATE (:Person {bornIn:'Ohio', score:-1.5})")
            .await
            .unwrap();
        cypher(
            &mut fixture,
            "MATCH (p:Person) SET p += {name:'Alice', age:30}",
        )
        .await
        .unwrap();
        cypher(
            &mut fixture,
            "MATCH (p:Person) MERGE (c:City) ON MATCH SET c.name=p.bornIn",
        )
        .await
        .unwrap();
        cypher(
            &mut fixture,
            "MATCH (p:Person) MERGE (c:City) ON MATCH SET c.name=p.bornIn",
        )
        .await
        .unwrap();
        cypher(
            &mut fixture,
            "MATCH (p:Person), (c:City) MERGE (p)-[r:BORN_IN]->(c) ON CREATE SET r=p",
        )
        .await
        .unwrap();
        assert_eq!(
            fixture
                .sql("SELECT count(*) FROM person WHERE name='Alice' AND age=30 AND score=-1.5")
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .sql("SELECT count(*) FROM city WHERE name='Ohio'")
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .sql("SELECT count(*) FROM born_in WHERE name='Alice' AND age=30")
                .unwrap(),
            1
        );
        assert_eq!(fixture.sql("SELECT count(*) FROM duckdb_columns() WHERE table_name='city' AND column_name='age'").unwrap(),0);
    }














}
