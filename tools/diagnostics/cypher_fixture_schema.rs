//! Internal corpus fixture adapter: declared schemas outlive the fixture rows.
use arrow::{
    array::RecordBatch,
    datatypes::{DataType, Field, Schema},
};
use chrono::Datelike;
use datafusion::scalar::ScalarValue;
use orchiddb::ir::{
    catalog::PropertyGraph,
    functions::domain,
    rel::{
        mapping::{EdgeMapping, GraphMapping, NodeMapping},
        sql::{self, SqlDialect, TableData},
    },
    temporal::TemporalValue,
    value::Value,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Declaration {
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub label: String,
    pub table: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    pub rel_type: String,
    pub table: String,
    pub src_label: String,
    pub dst_label: String,
    #[serde(default)]
    pub properties: BTreeMap<String, String>,
}
// The graph value carrier records map order/types beside user properties.
// These two canonical keys are transport metadata, not schema columns.
fn property_key(key: &str) -> bool {
    !matches!(
        key,
        orchiddb::ir::value::STRUCT_ORDER_KEY | orchiddb::ir::value::STRUCT_TYPES_KEY
    )
}
fn value_type(v: &Value) -> Result<Option<String>, String> {
    Ok(match v {
        Value::Null => None,
        Value::Bool(_) => Some("BOOLEAN".into()),
        Value::Int(_) | Value::Long(_) => Some("BIGINT".into()),
        Value::Float(_) => Some("DOUBLE".into()),
        Value::String(_) => Some("VARCHAR".into()),
        Value::Temporal(TemporalValue::Date(date)) if (1..=9999).contains(&date.year()) => {
            Some("CYPHER_DATE".into())
        }
        // Match the shared temporal transport. The nominal domain prevents
        // encoded payloads from being treated as ordinary strings by lowering.
        Value::Temporal(TemporalValue::DateTime(_, None)) => {
            Some("CYPHER_TEMPORAL_DATETIME_FIXED".into())
        }
        Value::Temporal(v) => Some(format!("CYPHER_TEMPORAL_{}", v.kind().to_uppercase())),
        Value::List(xs) => {
            let mut ty = None;
            for x in xs {
                merge(&mut ty, value_type(x)?)?;
            }
            ty.map(|t| format!("{t}[]"))
        }
        _ => {
            return Err(format!(
                "fixture value requires an explicit supported SQL representation: {v:?}"
            ));
        }
    })
}
fn merge(a: &mut Option<String>, b: Option<String>) -> Result<(), String> {
    if let Some(b) = b {
        match a {
            Some(a)
                if (a == "CYPHER_DATE" || a.starts_with("CYPHER_TEMPORAL"))
                    && (b == "CYPHER_DATE" || b.starts_with("CYPHER_TEMPORAL")) =>
            {
                if *a != b {
                    *a = if ["CYPHER_TEMPORAL_DATETIME", "CYPHER_TEMPORAL_DATETIME_FIXED"]
                        .contains(&a.as_str())
                        && ["CYPHER_TEMPORAL_DATETIME", "CYPHER_TEMPORAL_DATETIME_FIXED"]
                            .contains(&b.as_str())
                    {
                        "CYPHER_TEMPORAL_DATETIME"
                    } else if ["CYPHER_DATE", "CYPHER_TEMPORAL_DATE"].contains(&a.as_str())
                        && ["CYPHER_DATE", "CYPHER_TEMPORAL_DATE"].contains(&b.as_str())
                    {
                        "CYPHER_TEMPORAL_DATE"
                    } else {
                        "CYPHER_TEMPORAL"
                    }
                    .into();
                }
            }
            Some(a)
                if *a != b
                    && ["BIGINT", "VARCHAR", "BOOLEAN", "CYPHER_SCALAR"].contains(&a.as_str())
                    && ["BIGINT", "VARCHAR", "BOOLEAN", "CYPHER_SCALAR"].contains(&b.as_str()) =>
            {
                *a = "CYPHER_SCALAR".into();
            }
            Some(a) if *a != b => {
                return Err(format!(
                    "heterogeneous fixture types {a} and {b} need lossless union storage; scalar coercion is not supported"
                ));
            }
            None => *a = Some(b),
            _ => {}
        }
    }
    Ok(())
}
pub fn discover(graph: &PropertyGraph) -> Result<Declaration, String> {
    let mut out = Declaration::default();
    for (i, label) in graph.labels().iter().enumerate() {
        let ids = graph.node_ids(label).map_err(|e| e.to_string())?;
        let mut properties = BTreeMap::new();
        for key in graph
            .node_property_keys_with_id(label)
            .into_iter()
            .filter(|key| property_key(key))
        {
            let mut ty = None;
            for id in &ids {
                merge(
                    &mut ty,
                    value_type(&graph.node_property(label, id.clone(), &key))?,
                )?;
            }
            properties.insert(
                key,
                ty.ok_or("all-null or empty-list property needs a declared type")?,
            );
        }
        out.nodes.push(Node {
            label: label.clone(),
            table: format!("conformance_nodes_{i}"),
            properties,
        });
    }
    for rel in graph.rel_types() {
        let mut groups: BTreeSet<(String, String)> = BTreeSet::new();
        for id in graph.edge_ids(&rel) {
            if let Some((src, _, dst, _)) = graph.edge_endpoints(&rel, id) {
                groups.insert((src, dst));
            }
        }
        for (src, dst) in groups {
            let mut properties = BTreeMap::new();
            for key in graph
                .edge_property_keys(&rel)
                .into_iter()
                .filter(|key| property_key(key))
            {
                let mut ty = None;
                for id in graph.edge_ids(&rel) {
                    merge(&mut ty, value_type(&graph.edge_property(&rel, id, &key))?)?;
                }
                properties.insert(
                    key,
                    ty.ok_or("all-null or empty-list property needs a declared type")?,
                );
            }
            out.edges.push(Edge {
                rel_type: rel.clone(),
                table: format!("conformance_edges_{}", out.edges.len()),
                src_label: src,
                dst_label: dst,
                properties,
            });
        }
    }
    Ok(out)
}
fn data_type(ty: &str) -> Result<DataType, String> {
    Ok(if let Some(item) = ty.strip_suffix("[]") {
        DataType::List(Arc::new(Field::new("item", data_type(item)?, true)))
    } else {
        match ty {
            "BIGINT" => DataType::Int64,
            "DOUBLE" => DataType::Float64,
            "VARCHAR" => DataType::Utf8,
            "BOOLEAN" => DataType::Boolean,
            "CYPHER_SCALAR" => orchiddb::ir::functions::scalar_union::data_type(),
            kind if kind.starts_with("CYPHER_TEMPORAL_") => {
                domain::data_type(&kind.to_lowercase(), DataType::Utf8)
            }
            "CYPHER_DATE" => domain::data_type("cypher_temporal", DataType::Date32),
            "CYPHER_TEMPORAL" => domain::data_type("cypher_temporal", DataType::Utf8),
            _ => return Err(format!("unsupported declared fixture type {ty}")),
        }
    })
}
fn scalar(v: Value, ty: &DataType) -> Result<ScalarValue, String> {
    if matches!(v, Value::Null) {
        return ScalarValue::try_from(ty).map_err(|e| e.to_string());
    }
    if orchiddb::ir::functions::scalar_union::is_type(ty) {
        return orchiddb::ir::functions::scalar_union::scalar(&v).map_err(|e| e.to_string());
    }
    Ok(match (v, ty) {
        (Value::Int(n) | Value::Long(n), DataType::Int64) => ScalarValue::Int64(Some(n)),
        (Value::Int(n) | Value::Long(n), DataType::Float64) if (n as f64) as i128 == n as i128 => {
            ScalarValue::Float64(Some(n as f64))
        }
        (Value::Float(n), DataType::Float64) => ScalarValue::Float64(Some(n)),
        (Value::String(s), DataType::Utf8) => ScalarValue::Utf8(Some(s)),
        (Value::Bool(b), DataType::Boolean) => ScalarValue::Boolean(Some(b)),
        (Value::Temporal(value), ty)
            if domain::descriptor(ty)
                .is_some_and(|(name, _)| name.starts_with("cypher_temporal")) =>
        {
            let domain_name = domain::descriptor(ty).unwrap().0;
            if let Some(expected) = domain_name.strip_prefix("cypher_temporal_") {
                let valid = if expected == "datetime_fixed" {
                    matches!(&value, TemporalValue::DateTime(_, None))
                } else {
                    value.kind() == expected
                };
                if !valid {
                    return Err(format!("temporal fixture {value:?} does not fit {ty:?}"));
                }
            }
            let storage = match (domain::descriptor(ty).unwrap().1, &value) {
                (DataType::Date32, TemporalValue::Date(date))
                    if (1..=9999).contains(&date.year()) =>
                {
                    ScalarValue::Date32(Some(date.num_days_from_ce() - 719163))
                }
                (DataType::Utf8, _) => ScalarValue::Utf8(Some(value.encode())),
                _ => return Err(format!("temporal fixture {value:?} does not fit {ty:?}")),
            };
            domain::scalar(domain::descriptor(ty).unwrap().0, storage).map_err(|e| e.to_string())?
        }
        (Value::List(xs), DataType::List(item)) => {
            let values = xs
                .into_iter()
                .map(|x| scalar(x, item.data_type()))
                .collect::<Result<Vec<_>, _>>()?;
            ScalarValue::List(ScalarValue::new_list(&values, item.data_type(), true))
        }
        (v, t) => {
            return Err(format!(
                "fixture value {v:?} cannot be represented losslessly as {t:?}"
            ));
        }
    })
}
fn table(
    name: &str,
    keys: &[&str],
    props: &BTreeMap<String, String>,
    rows: Vec<Vec<Value>>,
) -> Result<TableData, String> {
    if props.keys().any(|p| keys.contains(&p.as_str())) {
        return Err("fixture property conflicts with reserved key column".into());
    }
    let mut fields = keys
        .iter()
        .map(|k| Field::new(*k, DataType::Int64, false))
        .collect::<Vec<_>>();
    for (name, ty) in props {
        fields.push(Field::new(name, data_type(ty)?, true));
    }
    let schema = Arc::new(Schema::new(fields));
    let mut columns = Vec::new();
    for (i, field) in schema.fields().iter().enumerate() {
        let vals = rows
            .iter()
            .map(|r| scalar(r[i].clone(), field.data_type()))
            .collect::<Result<Vec<_>, _>>()?;
        columns.push(if vals.is_empty() {
            arrow::array::new_empty_array(field.data_type())
        } else {
            ScalarValue::iter_to_array(vals).map_err(|e| e.to_string())?
        });
    }
    let batch = RecordBatch::try_new(schema.clone(), columns).map_err(|e| e.to_string())?;
    Ok(TableData {
        name: name.into(),
        schema,
        batches: vec![batch],
    })
}
fn fixture_table_sql(table: &TableData) -> Result<Vec<String>, String> {
    let mut sql = sql::table_setup_sql(SqlDialect::DuckDb, table).map_err(|e| e.to_string())?;
    sql[0] = sql[0]
        .replace("\"__id\" BIGINT", "\"__id\" BIGINT PRIMARY KEY")
        .replace("\"__src\" BIGINT", "\"__src\" BIGINT NOT NULL")
        .replace("\"__dst\" BIGINT", "\"__dst\" BIGINT NOT NULL");
    Ok(sql)
}

pub fn materialize(
    graph: &PropertyGraph,
    d: &Declaration,
) -> Result<(Arc<GraphMapping>, Vec<String>), String> {
    let mut mapping = GraphMapping::new();
    let mut setup = Vec::new();
    let mut names = BTreeSet::new();
    let mut labels = BTreeSet::new();
    for label in graph.labels() {
        if !d.nodes.iter().any(|n| n.label == label) {
            return Err(format!("undeclared fixture node label {label:?}"));
        }
    }
    for n in &d.nodes {
        if !names.insert(n.table.clone()) || !labels.insert(n.label.clone()) {
            return Err(format!("duplicate fixture table {}", n.table));
        }
        let ids = if graph.labels().contains(&n.label) {
            graph.node_ids(&n.label).map_err(|e| e.to_string())?
        } else {
            vec![]
        };
        for key in graph
            .node_property_keys_with_id(&n.label)
            .into_iter()
            .filter(|key| property_key(key))
        {
            if !n.properties.contains_key(&key) {
                return Err(format!("undeclared fixture property {}.{key}", n.label));
            }
        }
        let mut rows = vec![];
        let dynamic_labels = ids.iter().any(|id| {
            let labels = graph.node_labels(&n.label, id.clone());
            labels != vec![n.label.clone()] && !(n.label.is_empty() && labels.is_empty())
        });
        let mut physical_properties = n.properties.clone();
        if dynamic_labels {
            if physical_properties.contains_key("__labels") {
                return Err("fixture property conflicts with logical label column".into());
            }
            physical_properties.insert("__labels".into(), "VARCHAR[]".into());
        }
        for id in ids {
            let labels = graph.node_labels(&n.label, id.clone());
            let mut row = vec![Value::Int(
                id.as_i64().ok_or("non-integer fixture identity")?,
            )];
            for key in physical_properties.keys() {
                row.push(if dynamic_labels && key == "__labels" {
                    Value::List(labels.iter().cloned().map(Value::String).collect())
                } else {
                    graph.node_property(&n.label, id.clone(), key)
                });
            }
            rows.push(row);
        }
        let t = table(&n.table, &["__id"], &physical_properties, rows)?;
        mapping.register_table_schema(&n.table, t.schema.clone());
        let mut node = NodeMapping::table(&n.label, &n.table, "__id");
        if dynamic_labels {
            node = node.labels_column("__labels");
        }
        for key in n.properties.keys() {
            node = node.property(key, key);
        }
        mapping.map_node(node);
        setup.extend(fixture_table_sql(&t)?);
    }
    for rel in graph.rel_types() {
        for id in graph.edge_ids(&rel) {
            let (s, _, t, _) = graph
                .edge_endpoints(&rel, id)
                .ok_or("missing fixture edge endpoints")?;
            if !d
                .edges
                .iter()
                .any(|e| e.rel_type == rel && e.src_label == s && e.dst_label == t)
            {
                return Err(format!("undeclared edge group {rel} {s}->{t}"));
            }
        }
    }
    for e in &d.edges {
        if !labels.contains(&e.src_label) || !labels.contains(&e.dst_label) {
            return Err("relationship endpoints need declared node tables".into());
        }
        if !names.insert(e.table.clone()) {
            return Err(format!("duplicate fixture table {}", e.table));
        }
        for key in graph
            .edge_property_keys(&e.rel_type)
            .into_iter()
            .filter(|key| property_key(key))
        {
            if !e.properties.contains_key(&key) {
                return Err(format!("undeclared fixture property {}.{key}", e.rel_type));
            }
        }
        let mut rows = vec![];
        for id in graph.edge_ids(&e.rel_type) {
            let (src, sid, dst, did) = graph
                .edge_endpoints(&e.rel_type, id.clone())
                .ok_or("missing fixture edge endpoints")?;
            if src != e.src_label || dst != e.dst_label {
                continue;
            }
            let mut row = vec![
                Value::Int(id.as_i64().ok_or("non-integer edge id")?),
                Value::Int(sid.as_i64().ok_or("non-integer source id")?),
                Value::Int(did.as_i64().ok_or("non-integer target id")?),
            ];
            for key in e.properties.keys() {
                row.push(graph.edge_property(&e.rel_type, id.clone(), key));
            }
            rows.push(row);
        }
        let t = table(&e.table, &["__id", "__src", "__dst"], &e.properties, rows)?;
        mapping.register_table_schema(&e.table, t.schema.clone());
        let mut edge = EdgeMapping::table(
            &e.rel_type,
            &e.table,
            "__src",
            "__dst",
            &e.src_label,
            &e.dst_label,
        )
        .with_id("__id");
        for key in e.properties.keys() {
            edge = edge.property(key, key);
        }
        mapping.map_edge_partition(edge);
        setup.extend(fixture_table_sql(&t)?);
    }
    Ok((Arc::new(mapping), setup))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn richer_shapes_load_without_dropping_edges_labels_or_types() {
        for (setup, _expected) in [
            (
                "CREATE (a:A),(b:B),(c:C),(a)-[:R]->(b),(a)-[:R]->(c)",
                "multi-endpoint relationship type",
            ),
            ("CREATE (:A:B)", "logical node labels"),
            ("CREATE (:A {v:1}),(:A {v:'1'})", "lossless union storage"),
        ] {
            let graph = PropertyGraph::new();
            let plan =
                orchiddb::language::cypher::preparation::prepare(setup, &BTreeMap::new(), None)
                    .unwrap();
            orchiddb::ir::rel::runtime::execute(&plan, &graph, None)
                .await
                .unwrap();
            let declaration = discover(&graph).unwrap();
            let (_, sql) = materialize(&graph, &declaration).unwrap();
            let connection = duckdb::Connection::open_in_memory().unwrap();
            for statement in sql {
                connection.execute_batch(&statement).unwrap();
            }
        }
    }

    #[test]
    fn temporal_fixture_transport_preserves_type_precision_and_zone() {
        let cases = [
            ("date", "1910-05-06"),
            ("localtime", "12:31:14.645876123"),
            ("time", "12:31:14.645876123+01:00"),
            ("localdatetime", "1984-11-11T12:31:14.645876123"),
            (
                "datetime",
                "1984-11-11T12:31:14.645876123+01:00[Europe/Stockholm]",
            ),
            ("duration", "P1M-14DT56950S"),
        ];
        for (kind, text) in cases {
            let value = orchiddb::ir::temporal::parse(kind, text).unwrap();
            let original = Value::Temporal(value.clone());
            let declaration = value_type(&original).unwrap().unwrap();
            let ty = data_type(&declaration).unwrap();
            let converted = scalar(original.clone(), &ty).unwrap();
            assert_eq!(converted.data_type(), ty);
            let storage = domain::storage(&converted).unwrap();
            let expected = if kind == "date" {
                assert_eq!(declaration, "CYPHER_DATE");
                assert!(matches!(storage, ScalarValue::Date32(_)));
                text.to_owned()
            } else {
                assert_eq!(
                    declaration,
                    format!("CYPHER_TEMPORAL_{}", kind.to_uppercase())
                );
                let ScalarValue::Utf8(Some(payload)) = storage else {
                    panic!("{storage:?}")
                };
                assert_eq!(TemporalValue::decode(&payload).unwrap(), value);
                payload
            };
            let table = table(
                "temporal_fixture",
                &["__id"],
                &BTreeMap::from([("value".into(), declaration)]),
                vec![
                    vec![Value::Int(0), original],
                    vec![Value::Int(1), Value::Null],
                ],
            )
            .unwrap();
            let setup = fixture_table_sql(&table).unwrap();
            if kind == "date" {
                assert!(setup[0].contains("\"value\" DATE"), "{setup:?}");
            }
            #[cfg(feature = "duckdb")]
            {
                let conn = duckdb::Connection::open_in_memory().unwrap();
                for statement in setup {
                    conn.execute_batch(&statement).unwrap();
                }
                let actual: String = conn
                    .query_row(
                        "SELECT CAST(value AS VARCHAR) FROM temporal_fixture WHERE __id=0",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(actual, expected);
                let missing: bool = conn
                    .query_row(
                        "SELECT value IS NULL FROM temporal_fixture WHERE __id=1",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert!(missing);
            }
        }
    }

    #[test]
    fn cypher_integer_carriers_share_lossless_bigint_storage() {
        for value in [i64::MIN, 0, 1, i64::MAX] {
            assert_eq!(
                value_type(&Value::Long(value)).unwrap(),
                Some("BIGINT".into())
            );
            assert_eq!(
                scalar(Value::Long(value), &DataType::Int64).unwrap(),
                ScalarValue::Int64(Some(value))
            );
        }
        assert_eq!(
            value_type(&Value::List(vec![Value::Int(1), Value::Long(2)])).unwrap(),
            Some("BIGINT[]".into())
        );
        assert!(scalar(Value::Long(9_007_199_254_740_993), &DataType::Float64).is_err());
    }

    #[test]
    fn mixed_temporal_arrays_keep_logical_kinds_and_nulls() {
        let date = orchiddb::ir::temporal::parse("date", "1910-05-06").unwrap();
        let time = orchiddb::ir::temporal::parse("localtime", "12:31:14.645876123").unwrap();
        let value = Value::List(vec![
            Value::Temporal(date.clone()),
            Value::Temporal(time.clone()),
            Value::Null,
        ]);
        assert_eq!(
            value_type(&value).unwrap(),
            Some("CYPHER_TEMPORAL[]".into())
        );
        let ScalarValue::List(array) =
            scalar(value, &data_type("CYPHER_TEMPORAL[]").unwrap()).unwrap()
        else {
            unreachable!()
        };
        let values = array.value(0);
        for (index, expected) in [date, time].into_iter().enumerate() {
            let v = ScalarValue::try_from_array(&values, index).unwrap();
            let ScalarValue::Utf8(Some(payload)) = domain::storage(&v).unwrap() else {
                unreachable!()
            };
            assert_eq!(TemporalValue::decode(&payload).unwrap(), expected);
        }
        assert!(ScalarValue::try_from_array(&values, 2).unwrap().is_null());
    }

    #[tokio::test]
    async fn declared_age_survives_an_empty_fixture_and_populated_rows_are_loaded() {
        let declaration = Declaration {
            nodes: vec![Node {
                label: "Person".into(),
                table: "people".into(),
                properties: BTreeMap::from([("age".into(), "BIGINT".into())]),
            }],
            edges: vec![],
        };
        for populated in [false, true] {
            let graph = PropertyGraph::new();
            if populated {
                let plan = orchiddb::language::cypher::preparation::prepare(
                    "CREATE (:Person {age:20}), (:Person {age:40})",
                    &BTreeMap::new(),
                    None,
                )
                .unwrap();
                orchiddb::ir::rel::runtime::execute(&plan, &graph, None)
                    .await
                    .unwrap();
            }
            let (mapping, setup) = materialize(&graph, &declaration).unwrap();
            assert!(setup[0].contains("\"age\" BIGINT"));
            assert!(!setup.join("\n").contains("__orchiddb_struct_"));
            if populated {
                let inferred = discover(&graph).unwrap();
                assert_eq!(
                    inferred.nodes[0].properties,
                    declaration.nodes[0].properties
                );
            }
            assert_eq!(setup.iter().any(|s| s.starts_with("INSERT")), populated);
            let plan = orchiddb::language::cypher::preparation::prepare(
                "MATCH (person) WITH $age + avg(person.age) - 1000 AS agg RETURN *",
                &BTreeMap::from([("age".into(), Value::Int(38))]),
                None,
            )
            .unwrap();
            let backend =
                orchiddb::ir::rel::RelBackend::with_options(orchiddb::ir::rel::RelBackendOptions {
                    mapping: Some(mapping.clone()),
                    ..Default::default()
                });
            let lowered = backend.lower(&plan, &graph).unwrap();
            let prepared = sql::prepare_with_external(
                &lowered,
                SqlDialect::DuckDb,
                &mapping.physical_table_names(),
            )
            .await
            .unwrap();
            assert!(prepared.query.contains("people"), "{}", prepared.query);
            assert!(!prepared.query.contains("CAST(NULL"), "{}", prepared.query);
            assert!(prepared.tables.is_empty());
            #[cfg(feature = "duckdb")]
            {
                let connection = duckdb::Connection::open_in_memory().unwrap();
                for statement in &setup {
                    connection.execute_batch(statement).unwrap();
                }
                let read_aggregate = || {
                    connection
                        .query_row(&prepared.query, [], |row| row.get::<_, Option<f64>>(0))
                        .unwrap()
                };
                assert_eq!(
                    read_aggregate(),
                    if populated { Some(-932.0) } else { None }
                );
                if !populated {
                    // Reuse SQL compiled while the table was empty. Both new
                    // values and subsequent changes must remain visible.
                    connection
                        .execute_batch(
                            "INSERT INTO people (__id, age) VALUES (1, 20), (2, 40), (3, NULL)",
                        )
                        .unwrap();
                    assert_eq!(read_aggregate(), Some(-932.0));
                    connection
                        .execute_batch("UPDATE people SET age = 50 WHERE __id = 1")
                        .unwrap();
                    assert_eq!(read_aggregate(), Some(-917.0));
                }
            }
        }
    }
    #[test]
    fn typed_loader_rejects_lossy_values_and_undeclared_properties() {
        assert!(scalar(Value::Int(9_007_199_254_740_993), &DataType::Float64).is_err());
        assert!(scalar(Value::String("2".into()), &DataType::Int64).is_err());
        assert_eq!(
            value_type(&Value::List(vec![Value::Int(1), Value::String("1".into())])).unwrap(),
            Some("CYPHER_SCALAR[]".into())
        );
    }
}
