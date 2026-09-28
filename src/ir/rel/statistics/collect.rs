use super::*;
use arrow::datatypes::{DataType, Field, Schema};
use serde_json::Value;
use std::{
    collections::{BTreeSet, VecDeque},
    time::Instant,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionRequest {
    pub id: String,
    pub source: String,
    pub kind: String,
    pub sql: String,
    pub dialect: String,
    pub max_rows: usize,
    pub max_bytes: usize,
    pub timeout_ms: u64,
}
struct Source {
    name: String,
    schema: Schema,
    selected: Vec<usize>,
    rows: Vec<BTreeMap<String, Value>>,
    estimated_rows: Option<f64>,
    estimated_bytes: Option<f64>,
    method: String,
    accepted_bytes: usize,
}
/// A bounded, adaptive acquisition coordinator. No data access occurs here.
pub struct Generator {
    started: Instant,
    metadata: Value,
    dialect: String,
    sources: Vec<Source>,
    queue: VecDeque<(usize, String)>,
    pending: Option<(usize, CollectionRequest)>,
    pub report: CollectionReport,
    elements: usize,
    retained_bytes: usize,
}
fn retained_value(v: &Value) -> usize {
    match v {
        Value::String(s) => 32usize.saturating_add(s.capacity()),
        Value::Array(a) => a.iter().fold(a.capacity().saturating_mul(32), |n, v| {
            n.saturating_add(retained_value(v))
        }),
        Value::Object(m) => m.iter().fold(0usize, |n, (k, v)| {
            n.saturating_add(256 + k.capacity())
                .saturating_add(retained_value(v))
        }),
        _ => 32,
    }
}
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
fn table(s: &str) -> String {
    datafusion::common::TableReference::from(s)
        .to_vec()
        .iter()
        .map(|s| quote(s))
        .collect::<Vec<_>>()
        .join(".")
}
fn preview(expr: &str, dt: &DataType, dialect: &str) -> String {
    match dt {
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
            format!("substring({expr},1,256)")
        }
        DataType::List(f) | DataType::LargeList(f) if dialect == "duckdb" => {
            let child = preview("__statistics_element", f.data_type(), dialect);
            format!("list_transform(list_slice({expr},1,64), __statistics_element -> {child})")
        }
        DataType::Struct(fields) if dialect == "duckdb" => format!(
            "CASE WHEN {expr} IS NULL THEN NULL ELSE struct_pack({}) END",
            fields
                .iter()
                .map(|f| format!(
                    "{} := {}",
                    quote(f.name()),
                    preview(
                        &format!("({expr}).{}", quote(f.name())),
                        f.data_type(),
                        dialect
                    )
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        DataType::Binary | DataType::LargeBinary | DataType::BinaryView => {
            format!("substring(CAST({expr} AS VARCHAR),1,256)")
        }
        DataType::List(_) | DataType::LargeList(_) | DataType::Struct(_) | DataType::Map(_, _) => {
            "NULL".into()
        }
        _ => expr.to_string(),
    }
}
impl Generator {
    pub fn new(request: Value) -> Result<Self, String> {
        let dialect = request["dialect"].as_str().unwrap_or("duckdb").to_string();
        if !["duckdb", "postgres"].contains(&dialect.as_str()) {
            return Err("statistics collection requires a supported dialect".into());
        }
        let mut sources = Vec::new();
        let mut seen = BTreeSet::new();
        for t in request["tables"]
            .as_array()
            .ok_or("statistics generation requires tables")?
        {
            let name = t["name"]
                .as_str()
                .ok_or("statistics source missing name")?
                .to_string();
            if !seen.insert(name.clone()) {
                return Err("duplicate statistics source".into());
            }
            let fields = t["columns"]
                .as_array()
                .ok_or("statistics source missing columns")?
                .iter()
                .map(|c| {
                    Ok(Field::new(
                        c["name"].as_str().ok_or("column name missing")?,
                        crate::compiler::data_type(
                            c["data_type"].as_str().ok_or("column type missing")?,
                        )?,
                        c["nullable"].as_bool().unwrap_or(true),
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let mut priority = BTreeSet::new();
            for kind in ["nodes", "edges"] {
                for m in request[kind].as_array().into_iter().flatten() {
                    for key in ["id", "source", "target"] {
                        priority.extend(key_columns(&m[key]));
                    }
                    for value in m["properties"]
                        .as_object()
                        .into_iter()
                        .flat_map(|v| v.values())
                    {
                        if let Some(c) = value.as_str() {
                            priority.insert(c.to_owned());
                        }
                    }
                }
            }
            for m in request["collection_sources"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(c) = m["column"].as_str() {
                    priority.insert(c.to_owned());
                }
                for value in m["parent_columns"]
                    .as_object()
                    .into_iter()
                    .flat_map(|v| v.values())
                {
                    if let Some(c) = value.as_str() {
                        priority.insert(c.to_owned());
                    }
                }
            }
            for m in request["rdf"].as_array().into_iter().flatten() {
                for term in ["subject", "predicate", "object", "graph"] {
                    priority.extend(key_columns(&m[term]["columns"]));
                    if let Some(c) = m[term]["column"].as_str() {
                        priority.insert(c.to_owned());
                    }
                }
            }
            let mut selected = (0..fields.len()).collect::<Vec<_>>();
            selected.sort_by_key(|i| (!priority.contains(fields[*i].name()), *i));
            selected.retain(|i| {
                let dt = fields[*i].data_type();
                !matches!(dt, DataType::Map(_, _))
                    && (dialect == "duckdb"
                        || !matches!(
                            dt,
                            DataType::List(_) | DataType::LargeList(_) | DataType::Struct(_)
                        ))
            });
            selected.truncate(64);
            sources.push(Source {
                name,
                schema: Schema::new(fields),
                selected,
                rows: Vec::new(),
                estimated_rows: None,
                estimated_bytes: None,
                method: "unavailable".into(),
                accepted_bytes: 0,
            });
        }
        // Reserve sample work for every source. Wide catalogs spend the request
        // budget on samples rather than exhausting it on metadata alone.
        let initial = if sources.len() * 2 <= MAX_REQUESTS {
            "metadata"
        } else {
            "sample"
        };
        let queue = (0..sources.len()).map(|i| (i, initial.into())).collect();
        Ok(Self {
            started: Instant::now(),
            metadata: request,
            dialect,
            sources,
            queue,
            pending: None,
            report: Default::default(),
            elements: 0,
            retained_bytes: 0,
        })
    }
    fn exhausted(&self) -> bool {
        self.started.elapsed().as_millis() >= TIMEOUT_MS as u128
            || self.report.accepted_rows >= MAX_ROWS
            || self.report.accepted_bytes >= MAX_BYTES
            || self.report.requests >= MAX_REQUESTS
            || self.elements >= MAX_ELEMENTS
            || self.retained_bytes >= MAX_RETAINED_BYTES
    }
    pub fn next(&mut self) -> Option<CollectionRequest> {
        if self.started.elapsed().as_millis() >= TIMEOUT_MS as u128 {
            self.report.stopped = "collection deadline reached".into();
            return None;
        }
        if let Some((_, r)) = &self.pending {
            return Some(r.clone());
        }
        if self.exhausted() {
            self.report.stopped = "collection budget reached".into();
            return None;
        }
        let (i, kind) = self.queue.pop_front()?;
        let source_count = self.sources.len().max(1);
        let s = &mut self.sources[i];
        let parts = datafusion::common::TableReference::from(s.name.as_str()).to_vec();
        let name = parts.last().cloned().unwrap_or_default();
        let schema = parts.iter().rev().nth(1).cloned();
        let remaining = MAX_ROWS - self.report.accepted_rows;
        let max_rows = if kind == "metadata" {
            8
        } else {
            remaining.min((MAX_ROWS / source_count).clamp(64, 8192))
        };
        let sql = if kind == "metadata" {
            if self.dialect == "duckdb" {
                let mut q = format!(
                    "SELECT estimated_size AS rows, NULL::BIGINT AS bytes FROM duckdb_tables() WHERE table_name={}",
                    literal(&name)
                );
                q.push_str(&format!(
                    " AND schema_name={}",
                    schema
                        .map(|s| literal(&s))
                        .unwrap_or_else(|| "current_schema()".into())
                ));
                q.push_str(&format!(
                    " AND database_name={}",
                    parts
                        .get(0)
                        .filter(|_| parts.len() == 3)
                        .map(|s| literal(s))
                        .unwrap_or_else(|| "current_database()".into())
                ));
                q
            } else {
                format!(
                    "SELECT CASE WHEN c.reltuples>=0 THEN c.reltuples ELSE NULL END AS rows, c.relpages::BIGINT*8192 AS bytes FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relname={} AND n.nspname={}",
                    literal(&name),
                    schema
                        .map(|s| literal(&s))
                        .unwrap_or_else(|| "current_schema()".into())
                )
            }
        } else {
            let mut cols = Vec::new();
            for f in s.selected.iter().map(|i| s.schema.field(*i)) {
                cols.push(format!(
                    "{} AS {}",
                    preview(&quote(f.name()), f.data_type(), &self.dialect),
                    quote(f.name())
                ));
                if matches!(f.data_type(), DataType::List(_) | DataType::LargeList(_))
                    && self.dialect == "duckdb"
                {
                    cols.push(format!(
                        "array_length({}) AS {}",
                        quote(f.name()),
                        quote(&format!("__statistics_length_{}", f.name()))
                    ));
                }
            }
            if cols.is_empty() {
                cols.push("1 AS __statistics_row".into());
            }
            let sampling =
                if let Some(rows) = s.estimated_rows.filter(|r| *r > max_rows as f64 * 4.0) {
                    let percent = (max_rows as f64 / rows * 100.0).clamp(0.01, 100.0);
                    s.method = "system block sample; capped and potentially truncated".into();
                    if self.dialect == "duckdb" {
                        format!(" USING SAMPLE {percent:.6} PERCENT (system)")
                    } else {
                        format!(" TABLESAMPLE SYSTEM ({percent:.6})")
                    }
                } else {
                    s.method = "bounded prefix; distribution may be biased".into();
                    String::new()
                };
            format!(
                "SELECT {} FROM {}{} LIMIT {}",
                cols.join(","),
                table(&s.name),
                sampling,
                max_rows
            )
        };
        let r = CollectionRequest {
            id: format!("request-{}", self.report.requests),
            source: s.name.clone(),
            kind,
            sql,
            dialect: self.dialect.clone(),
            max_rows,
            max_bytes: (MAX_BYTES - self.report.accepted_bytes).min(1024 * 1024),
            timeout_ms: TIMEOUT_MS.saturating_sub(self.started.elapsed().as_millis() as u64),
        };
        self.report.requests += 1;
        self.pending = Some((i, r.clone()));
        Some(r)
    }
    pub fn submit(
        &mut self,
        id: &str,
        rows: Vec<BTreeMap<String, Value>>,
        error: Option<String>,
        done: bool,
    ) -> Result<(), String> {
        let (i, r) = self
            .pending
            .as_ref()
            .ok_or("no outstanding statistics request")?
            .clone();
        if r.id != id {
            return Err("statistics request id mismatch".into());
        }
        if let Some(error) = error {
            self.report
                .skipped
                .insert(format!("{}:{}", r.source, r.kind), error);
        } else if self.started.elapsed().as_millis() <= TIMEOUT_MS as u128 {
            for row in rows {
                let bytes = serde_json::to_vec(&row).map_err(|e| e.to_string())?.len();
                let retained = row.iter().fold(0usize, |n, (k, v)| {
                    n.saturating_add(256 + k.capacity())
                        .saturating_add(retained_value(v))
                });
                if self.report.accepted_rows >= MAX_ROWS
                    || self.report.accepted_bytes.saturating_add(bytes) > MAX_BYTES
                    || (r.kind != "metadata"
                        && (self.sources[i].rows.len() >= r.max_rows
                            || self.sources[i].accepted_bytes.saturating_add(bytes) > r.max_bytes
                            || self.retained_bytes.saturating_add(retained) > MAX_RETAINED_BYTES))
                {
                    self.report.stopped = "sample budget reached".into();
                    break;
                }
                self.report.accepted_rows += 1;
                self.report.accepted_bytes += bytes;
                if r.kind == "metadata" {
                    let num = |k: &str| {
                        row.get(k)
                            .and_then(|x| {
                                x.as_f64()
                                    .or_else(|| x.as_str().and_then(|s| s.parse().ok()))
                            })
                            .filter(|x| x.is_finite() && *x >= 0.0)
                    };
                    self.sources[i].estimated_rows = num("rows");
                    self.sources[i].estimated_bytes = num("bytes");
                } else {
                    self.sources[i].accepted_bytes += bytes;
                    self.retained_bytes += retained;
                    self.sources[i].rows.push(row);
                }
            }
        }
        if done {
            if r.kind == "metadata" {
                self.queue.push_back((i, "sample".into()));
            } else {
                let s = &mut self.sources[i];
                if !s.method.starts_with("system")
                    && s.rows.len() < r.max_rows
                    && error_is_none(&self.report, &r)
                    && self.report.stopped.is_empty()
                    && self.started.elapsed().as_millis() <= TIMEOUT_MS as u128
                {
                    s.method = "complete bounded read".into();
                }
                if s.method == "complete bounded read" {
                    s.estimated_rows = Some(s.rows.len() as f64);
                }
            }
            self.pending = None;
        }
        Ok(())
    }
    pub fn finish(mut self) -> StatisticsSnapshot {
        self.report.complete = self.queue.is_empty()
            && self.pending.is_none()
            && self.report.skipped.is_empty()
            && self.report.stopped.is_empty();
        if self.report.stopped.is_empty() {
            self.report.stopped = if self.report.complete {
                "finished"
            } else {
                "partial coverage"
            }
            .into();
        }
        self.report.elapsed_ms = self.started.elapsed().as_millis() as u64;
        let mut sources = BTreeMap::new();
        let mut relationships = Vec::new();
        let mut rdf = Vec::new();
        for s in &self.sources {
            if s.selected.len() < s.schema.fields().len() {
                self.report.notes.push(format!(
                    "{}: selected {} of {} columns, prioritizing mapped fields and omitting unsupported nested projections",
                    s.name,
                    s.selected.len(),
                    s.schema.fields().len()
                ));
            }
            let mut columns = BTreeMap::new();
            for f in s.selected.iter().map(|i| s.schema.field(*i)) {
                let vals = s
                    .rows
                    .iter()
                    .map(|r| r.get(f.name()).cloned().unwrap_or(Value::Null))
                    .collect::<Vec<_>>();
                let mut c = summarize(
                    &vals,
                    &format!("{:?}", f.data_type()),
                    s.method == "complete bounded read",
                    &mut self.elements,
                );
                if let Some(list) = &mut c.list {
                    for row in &s.rows {
                        if let Some(n) = row
                            .get(&format!("__statistics_length_{}", f.name()))
                            .and_then(Value::as_u64)
                        {
                            if n > 64 {
                                list.total_lengths += n - 64;
                                list.truncated = true;
                            }
                        }
                    }
                }
                columns.insert(f.name().clone(), c);
            }
            let mut groups = Vec::new();
            let mut keygroups = BTreeSet::new();
            for kind in ["nodes", "edges"] {
                for m in self.metadata[kind]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|m| m["table"].as_str() == Some(&s.name))
                {
                    for name in ["id", "source", "target"] {
                        let keys = key_columns(&m[name]);
                        if !keys.is_empty() {
                            keygroups.insert(keys);
                        }
                    }
                    if kind == "edges" {
                        let src = key_columns(&m["source"]);
                        let dst = key_columns(&m["target"]);
                        let a = group(&s.rows, &src);
                        let b = group(&s.rows, &dst);
                        let mut pair = src.clone();
                        pair.extend(dst.clone());
                        let pairs = group(&s.rows, &pair);
                        keygroups.insert(pair);
                        relationships.push(RelationshipStatistics {
                            name: m["label"].as_str().unwrap_or("").into(),
                            source: s.name.clone(),
                            source_columns: src,
                            target_columns: dst,
                            sampled_edges: s.rows.len() as u64,
                            source_distinct: a.sample_distinct,
                            target_distinct: b.sample_distinct,
                            pair_distinct: pairs.sample_distinct,
                            outgoing_heavy_hitters: a.frequent,
                            incoming_heavy_hitters: b.frequent,
                            method: s.method.clone(),
                        });
                    }
                }
            }
            // Shared sample supplies selected mapped column correlations without more reads.
            let names = columns.keys().take(8).cloned().collect::<Vec<_>>();
            for pair in names.windows(2) {
                keygroups.insert(pair.to_vec());
            }
            for key in keygroups.clone().into_iter().take(4) {
                for property in names.iter().take(4) {
                    if !key.contains(property) {
                        let mut pair = key.clone();
                        pair.push(property.clone());
                        keygroups.insert(pair);
                    }
                }
            }
            for keys in keygroups.into_iter().take(16) {
                groups.push(group(&s.rows, &keys));
            }
            for rule in self.metadata["rdf"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|r| r["table"].as_str() == Some(&s.name))
            {
                let term = |r: &BTreeMap<String, Value>, m: &Value| -> Option<String> {
                    if m["kind"] == "constant" {
                        return Some(m["value"].to_string());
                    }
                    if let Some(c) = m["column"].as_str() {
                        return r.get(c).filter(|v| !v.is_null()).map(Value::to_string);
                    }
                    let cols = key_columns(&m["columns"]);
                    if cols.is_empty() {
                        return None;
                    }
                    let v = cols
                        .iter()
                        .map(|c| r.get(c).filter(|v| !v.is_null()))
                        .collect::<Option<Vec<_>>>()?;
                    Some(serde_json::to_string(&v).ok()?)
                };
                let mut subjects = BTreeSet::new();
                let mut objects = BTreeSet::new();
                let mut statements = BTreeSet::new();
                for row in &s.rows {
                    if let (Some(a), Some(b), Some(p)) = (
                        term(row, &rule["subject"]),
                        term(row, &rule["object"]),
                        term(row, &rule["predicate"]),
                    ) {
                        subjects.insert(a.clone());
                        objects.insert(b.clone());
                        statements.insert((a, p, b));
                    }
                }
                rdf.push(RdfStatistics {
                    source: s.name.clone(),
                    dataset: rule["dataset"].as_str().unwrap_or("default").into(),
                    predicate: rule["predicate"]["value"]
                        .as_str()
                        .unwrap_or("variable")
                        .into(),
                    sampled_statements: statements.len() as u64,
                    sampled_subjects: subjects.len() as u64,
                    sampled_objects: objects.len() as u64,
                    object_kind: rule["object"]["kind"].as_str().unwrap_or("unknown").into(),
                    datatype: rule["object"]["datatype"].as_str().map(str::to_string),
                    language: rule["object"]["language"].as_str().map(str::to_string),
                    method: s.method.clone(),
                });
            }
            let width = columns.values().map(|c| c.average_width).sum::<f64>();
            sources.insert(
                s.name.clone(),
                SourceStatistics {
                    schema: schema_fingerprint(&s.schema),
                    estimated_rows: s.estimated_rows,
                    estimated_bytes: s.estimated_bytes.or_else(|| {
                        s.estimated_rows
                            .filter(|r| *r == 0.0 || !s.rows.is_empty())
                            .map(|r| r * width.max(1.0))
                    }),
                    sample_rows: s.rows.len() as u64,
                    method: s.method.clone(),
                    columns,
                    groups,
                },
            );
        }
        self.report.notes.push("Estimates only; prefix reads can be biased. Text values capped at 256 characters, lists at 64 elements; no sample bound authorizes pruning.".into());
        let mut snapshot = StatisticsSnapshot {
            version: VERSION,
            revision: String::new(),
            mapping: mapping_fingerprint(&self.metadata),
            collected_at: chrono::Utc::now().to_rfc3339(),
            sources,
            relationships,
            rdf,
            report: self.report,
        };
        // Keep useful source costs if rich summaries outgrow the portable budget.
        if serde_json::to_vec(&snapshot)
            .map(|v| v.len())
            .unwrap_or(usize::MAX)
            > MAX_SNAPSHOT_BYTES
        {
            snapshot.report.complete = false;
            snapshot.report.notes.push(
                "Snapshot size budget: detailed distributions omitted where necessary".into(),
            );
            for source in snapshot.sources.values_mut() {
                source.groups.clear();
                for column in source.columns.values_mut() {
                    column.histogram.clear();
                    column.frequent.truncate(8);
                    if let Some(list) = &mut column.list {
                        list.parents_containing.clear();
                    }
                }
            }
            while serde_json::to_vec(&snapshot)
                .map(|v| v.len())
                .unwrap_or(usize::MAX)
                > MAX_SNAPSHOT_BYTES
            {
                if snapshot.sources.pop_last().is_none() {
                    snapshot.relationships.clear();
                    snapshot.rdf.clear();
                    snapshot.report.skipped.clear();
                    break;
                }
            }
        }
        snapshot.revision = digest(&snapshot);
        snapshot
    }
}
fn error_is_none(report: &CollectionReport, r: &CollectionRequest) -> bool {
    !report
        .skipped
        .contains_key(&format!("{}:{}", r.source, r.kind))
}
pub(crate) fn key_columns(v: &Value) -> Vec<String> {
    v.as_str()
        .map(|s| vec![s.to_string()])
        .or_else(|| {
            v.as_array().map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
        })
        .unwrap_or_default()
}
fn group(rows: &[BTreeMap<String, Value>], columns: &[String]) -> GroupStatistics {
    let mut counts = BTreeMap::new();
    for row in rows {
        let vals = columns
            .iter()
            .map(|c| row.get(c).unwrap_or(&Value::Null).to_string())
            .collect::<Vec<_>>();
        *counts.entry(vals).or_insert(0u64) += 1;
    }
    let distinct = counts.len() as u64;
    let mut frequent = counts.into_iter().collect::<Vec<_>>();
    frequent.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    frequent.truncate(32);
    GroupStatistics {
        columns: columns.to_vec(),
        observations: rows.len() as u64,
        sample_distinct: distinct,
        frequent,
    }
}
fn summarize(values: &[Value], dt: &str, complete: bool, elements: &mut usize) -> ColumnStatistics {
    let mut c = ColumnStatistics {
        data_type: dt.into(),
        observations: values.len() as u64,
        ..Default::default()
    };
    let mut freq = BTreeMap::new();
    let mut widths = 0usize;
    for v in values {
        if v.is_null() {
            c.nulls += 1;
        } else {
            let s = v.to_string();
            widths += s.len();
            *freq.entry(s).or_insert(0u64) += 1;
        }
    }
    c.sample_distinct = freq.len() as u64;
    c.estimated_distinct = complete.then_some(c.sample_distinct as f64);
    c.average_width = widths as f64 / values.len().max(1) as f64;
    let mut sorted = freq.keys().cloned().collect::<Vec<_>>();
    sorted.sort_by(|a, b| numeric_cmp(a, b));
    c.minimum = sorted.first().cloned();
    c.maximum = sorted.last().cloned();
    let observations = freq.values().sum::<u64>();
    if observations > 0 {
        let buckets = observations.min(64);
        let mut cumulative = 0u64;
        let mut next = 0u64;
        for value in &sorted {
            cumulative += freq[value];
            while next < buckets && next * observations / buckets < cumulative {
                c.histogram.push(value.clone());
                next += 1;
            }
        }
    }
    c.frequent = freq.into_iter().collect();
    c.frequent.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    c.frequent.truncate(32);
    if values.iter().any(Value::is_array) {
        let mut list = ListStatistics {
            parents: values.len() as u64,
            null_lists: c.nulls,
            ..Default::default()
        };
        let mut fields: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        for v in values {
            if let Some(a) = v.as_array() {
                list.lengths.push(a.len() as u64);
                list.total_lengths += a.len() as u64;
                if a.is_empty() {
                    list.empty_lists += 1;
                }
                let mut present = BTreeSet::new();
                for item in a {
                    if *elements >= MAX_ELEMENTS {
                        list.truncated = true;
                        break;
                    }
                    *elements += 1;
                    list.observed_elements += 1;
                    if let Some(o) = item.as_object() {
                        for (k, v) in o {
                            fields.entry(k.clone()).or_default().push(v.clone());
                            present.insert(format!("{k}:{}", v));
                        }
                    } else {
                        fields
                            .entry("$element".into())
                            .or_default()
                            .push(item.clone());
                        present.insert(item.to_string());
                    }
                }
                for item in present {
                    if list.parents_containing.len() < 128
                        || list.parents_containing.contains_key(&item)
                    {
                        *list.parents_containing.entry(item).or_insert(0) += 1;
                    }
                }
            }
        }
        for (k, v) in fields {
            list.elements.insert(
                k,
                summarize(&v, "element", complete && !list.truncated, elements),
            );
        }
        list.lengths.sort_unstable();
        if list.lengths.len() > 64 {
            list.lengths = (0..64)
                .map(|i| list.lengths[i * list.lengths.len() / 64])
                .collect();
        }
        c.list = Some(list);
        c.frequent.clear();
        c.histogram.clear();
        c.minimum = None;
        c.maximum = None;
    }
    c
}
pub(crate) fn numeric_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(a), Ok(b)) => a.total_cmp(&b),
        _ => a.cmp(b),
    }
}
