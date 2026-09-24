//! Arrow table construction and header-to-column mapping.

use arrow::array::{ArrayRef, BooleanArray, Float64Array, Int64Array, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema as ArrowSchema, SchemaRef};
use new_graph::ir::catalog::{EdgeTable, NodeTable};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use super::DatasetError;
use super::files::{header_base, header_type, load_entry_rows};
use super::schema::{ColumnType, CopyEntry, EdgeDef, NodeDef};
use super::temporal::{normalize_date, normalize_interval, normalize_timestamp};
use super::values::{normalize_uuid, parse_bool, parse_i64, parse_typed_value};

pub(super) fn build_node_table(
    root: &Path,
    def: &NodeDef,
    entries: &[&CopyEntry],
    pk_index: &mut HashMap<(String, String), i64>,
) -> Result<NodeTable, DatasetError> {
    let mut columns: Vec<Vec<Option<String>>> = vec![Vec::new(); def.columns.len()];
    for entry in entries {
        let Some(file) = load_entry_rows(root, entry) else {
            continue;
        };
        if let Some(header) = &file.header {
            // Name-mapped path (typed CSV header or Parquet schema):
            // match each schema column against the header by base name;
            // a `:ID(...)`-typed header column with an empty base maps
            // to the schema's primary-key column.
            let mapping: Vec<Option<usize>> = def
                .columns
                .iter()
                .enumerate()
                .map(|(idx, column)| {
                    header
                        .iter()
                        .position(|h| header_base(h).eq_ignore_ascii_case(&column.name))
                        .or_else(|| {
                            (idx == def.pk_index).then(|| {
                                header.iter().position(|h| {
                                    header_base(h).is_empty()
                                        && header_type(h).to_ascii_uppercase().starts_with("ID")
                                })
                            })?
                        })
                })
                .collect();
            for row in file.rows {
                for (idx, mapped) in mapping.iter().enumerate() {
                    let cell = mapped.and_then(|src| row.get(src).cloned().flatten());
                    columns[idx].push(cell);
                }
            }
            continue;
        }
        let mut rows = file.rows;
        let has_header = entry
            .has_header_hint
            .unwrap_or_else(|| infer_header(&rows, def));
        if has_header && !rows.is_empty() {
            rows.remove(0);
        }
        for row in rows {
            let omitted_serial_pk = matches!(def.columns[def.pk_index].ty, ColumnType::Serial)
                && row.len() + 1 == def.columns.len();
            let generated_serial =
                omitted_serial_pk.then(|| columns[def.pk_index].len().to_string());
            for idx in 0..def.columns.len() {
                let cell = if omitted_serial_pk {
                    if idx == def.pk_index {
                        generated_serial.clone()
                    } else {
                        let source_idx = if idx < def.pk_index { idx } else { idx - 1 };
                        row.get(source_idx).cloned().flatten()
                    }
                } else {
                    row.get(idx).cloned().flatten()
                };
                columns[idx].push(cell);
            }
        }
    }

    for (row_index, value) in columns[def.pk_index].iter().enumerate() {
        if let Some(text) = value {
            pk_index.insert(
                (def.label.to_ascii_lowercase(), text.clone()),
                row_index as i64,
            );
        }
    }

    let columns: Vec<(Field, ArrayRef)> = def
        .columns
        .iter()
        .enumerate()
        .map(|(idx, column)| build_column(&column.name, &columns[idx], &column.ty))
        .collect();
    record_node_table(&def.label, columns)
}

pub(super) fn build_edge_table(
    root: &Path,
    def: &EdgeDef,
    entries: &[&CopyEntry],
    pk_index: &HashMap<(String, String), i64>,
) -> Result<EdgeTable, DatasetError> {
    let mut src_ids: Vec<i64> = Vec::new();
    let mut dst_ids: Vec<i64> = Vec::new();
    let mut prop_columns: Vec<Vec<Option<String>>> = vec![Vec::new(); def.properties.len()];

    for entry in entries {
        let Some(file) = load_entry_rows(root, entry) else {
            continue;
        };
        // Column layout: positional (FROM, TO, props…) unless a header
        // names the endpoint columns (`:START_ID(L)` / `:END_ID(L)` or
        // from/to-style names), in which case columns map by name.
        let mut from_col = 0usize;
        let mut to_col = 1usize;
        let mut prop_mapping: Vec<Option<usize>> =
            (0..def.properties.len()).map(|i| Some(2 + i)).collect();
        if let Some(header) = &file.header {
            let find_endpoint = |type_prefix: &str, names: &[&str]| {
                header
                    .iter()
                    .position(|h| header_type(h).to_ascii_uppercase().starts_with(type_prefix))
                    .or_else(|| {
                        header.iter().position(|h| {
                            names
                                .iter()
                                .any(|name| header_base(h).eq_ignore_ascii_case(name))
                        })
                    })
            };
            let from = find_endpoint("START_ID", &["from", "src", "source", "__src_id"]);
            let to = find_endpoint("END_ID", &["to", "dst", "target", "__dst_id"]);
            if let (Some(from), Some(to)) = (from, to) {
                from_col = from;
                to_col = to;
                prop_mapping = def
                    .properties
                    .iter()
                    .map(|prop| {
                        header
                            .iter()
                            .position(|h| header_base(h).eq_ignore_ascii_case(&prop.name))
                    })
                    .collect();
            }
        }
        let mut rows = file.rows;
        if file.header.is_none() {
            let has_header = entry
                .has_header_hint
                .unwrap_or_else(|| infer_edge_header(&rows, def, pk_index));
            if has_header && !rows.is_empty() {
                rows.remove(0);
            }
        }
        for row in rows {
            let from_text = match row.get(from_col).and_then(|c| c.clone()) {
                Some(text) => text,
                None => continue,
            };
            let to_text = match row.get(to_col).and_then(|c| c.clone()) {
                Some(text) => text,
                None => continue,
            };
            let Some(src) = pk_index
                .get(&(def.src_label.to_ascii_lowercase(), from_text))
                .copied()
            else {
                continue;
            };
            let Some(dst) = pk_index
                .get(&(def.dst_label.to_ascii_lowercase(), to_text))
                .copied()
            else {
                continue;
            };
            src_ids.push(src);
            dst_ids.push(dst);
            for (prop_idx, mapped) in prop_mapping.iter().enumerate() {
                let cell = mapped.and_then(|src_col| row.get(src_col).cloned().flatten());
                prop_columns[prop_idx].push(cell);
            }
        }
    }

    let columns: Vec<(Field, ArrayRef)> = def
        .properties
        .iter()
        .enumerate()
        .map(|(idx, column)| build_column(&column.name, &prop_columns[idx], &column.ty))
        .collect();
    record_edge_table(
        &def.rel_type,
        &def.src_label,
        &def.dst_label,
        src_ids,
        dst_ids,
        columns,
    )
}

/// Header inference for node CSVs. We compare the first row against
/// the schema's known column names: if the first cell matches the
/// primary-key column name (case-insensitively), it's a header. As a
/// fallback for INT primary keys, attempt to parse the first cell as
/// `i64`; failure indicates a header.
fn infer_header(rows: &[Vec<Option<String>>], def: &NodeDef) -> bool {
    let Some(first) = rows.first() else {
        return false;
    };
    let pk_name = &def.columns[def.pk_index].name;
    if let Some(cell) = first.first().and_then(|c| c.as_deref()) {
        if cell.eq_ignore_ascii_case(pk_name) {
            return true;
        }
        if matches!(
            &def.columns[def.pk_index].ty,
            ColumnType::Int64 | ColumnType::Serial
        ) && cell.trim().parse::<i64>().is_err()
        {
            return true;
        }
    }
    false
}

/// Header inference for edge CSVs: the first two columns are FROM/TO
/// references. We treat the row as a header iff the first cell isn't
/// parseable as an integer (FROM is virtually always a numeric PK in
/// the corpus) and matches a small set of header tokens.
fn infer_edge_header(
    rows: &[Vec<Option<String>>],
    def: &EdgeDef,
    pk_index: &HashMap<(String, String), i64>,
) -> bool {
    let Some(first) = rows.first() else {
        return false;
    };
    if let (Some(from), Some(to)) = (
        first.first().and_then(|c| c.as_deref()),
        first.get(1).and_then(|c| c.as_deref()),
    ) {
        let from_key = (def.src_label.to_ascii_lowercase(), from.to_string());
        let to_key = (def.dst_label.to_ascii_lowercase(), to.to_string());
        if pk_index.contains_key(&from_key) && pk_index.contains_key(&to_key) {
            return false;
        }
    }
    if let Some(cell) = first.first().and_then(|c| c.as_deref()) {
        let lower = cell.trim().to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "from" | "src" | "source" | "_id" | "__src_id"
        ) {
            return true;
        }
    }
    false
}

fn build_column(name: &str, values: &[Option<String>], ty: &ColumnType) -> (Field, ArrayRef) {
    match ty {
        ColumnType::Int64 | ColumnType::Serial => {
            let parsed: Vec<Option<i64>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(parse_i64))
                .collect();
            (
                Field::new(name, DataType::Int64, true),
                Arc::new(Int64Array::from(parsed)) as ArrayRef,
            )
        }
        ColumnType::Float64 => {
            let parsed: Vec<Option<f64>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(|s| s.trim().parse::<f64>().ok()))
                .collect();
            (
                Field::new(name, DataType::Float64, true),
                Arc::new(Float64Array::from(parsed)) as ArrayRef,
            )
        }
        ColumnType::Bool => {
            let parsed: Vec<Option<bool>> = values
                .iter()
                .map(|cell| cell.as_deref().and_then(parse_bool))
                .collect();
            (
                Field::new(name, DataType::Boolean, true),
                Arc::new(BooleanArray::from(parsed)) as ArrayRef,
            )
        }
        ColumnType::String => {
            let parsed: Vec<Option<&str>> = values.iter().map(|cell| cell.as_deref()).collect();
            let mut field = Field::new(name, DataType::Utf8, true);
            if values
                .iter()
                .flatten()
                .any(|value| value.contains("\\x") || value.contains("\\X"))
            {
                field = field.with_metadata(HashMap::from([(
                    "new_graph.value_type".to_string(),
                    "blob".to_string(),
                )]));
            }
            (field, Arc::new(StringArray::from(parsed)) as ArrayRef)
        }
        ColumnType::Date => {
            // Canonicalize raw `YYYY-M-D` etc. to the zero-padded
            // `YYYY-MM-DD` form Kuzu prints, so result rows can
            // compare via simple string equality.
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| cell.as_deref().map(normalize_date))
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            (
                Field::new(name, DataType::Utf8, true),
                Arc::new(StringArray::from(refs)) as ArrayRef,
            )
        }
        ColumnType::Timestamp => {
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| cell.as_deref().map(normalize_timestamp))
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            let field = Field::new(name, DataType::Utf8, true).with_metadata(HashMap::from([(
                "new_graph.value_type".to_string(),
                "datetime".to_string(),
            )]));
            (field, Arc::new(StringArray::from(refs)) as ArrayRef)
        }
        ColumnType::Uuid => {
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| cell.as_deref().map(normalize_uuid))
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            (
                Field::new(name, DataType::Utf8, true),
                Arc::new(StringArray::from(refs)) as ArrayRef,
            )
        }
        ColumnType::Interval => {
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| cell.as_deref().map(normalize_interval))
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            (
                Field::new(name, DataType::Utf8, true),
                Arc::new(StringArray::from(refs)) as ArrayRef,
            )
        }
        ColumnType::Value(type_text) => {
            let parsed: Vec<Option<String>> = values
                .iter()
                .map(|cell| {
                    cell.as_deref()
                        .and_then(|raw| parse_typed_value(raw, type_text))
                        .map(|value| format!("{value:?}"))
                })
                .collect();
            let refs: Vec<Option<&str>> = parsed.iter().map(|s| s.as_deref()).collect();
            let field = Field::new(name, DataType::Utf8, true).with_metadata(HashMap::from([(
                "new_graph.value_type".to_string(),
                "value".to_string(),
            )]));
            (field, Arc::new(StringArray::from(refs)) as ArrayRef)
        }
    }
}

fn record_node_table(
    label: &str,
    columns: Vec<(Field, ArrayRef)>,
) -> Result<NodeTable, DatasetError> {
    let fields = columns
        .iter()
        .map(|(field, _)| field.clone())
        .collect::<Vec<_>>();
    let arrays = columns
        .into_iter()
        .map(|(_, array)| array)
        .collect::<Vec<_>>();
    let schema: SchemaRef = Arc::new(ArrowSchema::new(fields));
    let batch =
        RecordBatch::try_new(schema, arrays).map_err(|err| DatasetError(err.to_string()))?;
    Ok(NodeTable {
        label: label.to_string(),
        batch,
    })
}

pub(super) fn record_edge_table(
    rel_type: &str,
    src_label: &str,
    dst_label: &str,
    src: Vec<i64>,
    dst: Vec<i64>,
    columns: Vec<(Field, ArrayRef)>,
) -> Result<EdgeTable, DatasetError> {
    let mut fields = vec![
        Field::new("__src_id", DataType::Int64, false),
        Field::new("__dst_id", DataType::Int64, false),
    ];
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(src)) as ArrayRef,
        Arc::new(Int64Array::from(dst)) as ArrayRef,
    ];
    for (field, array) in columns {
        fields.push(field);
        arrays.push(array);
    }
    let schema: SchemaRef = Arc::new(ArrowSchema::new(fields));
    let batch =
        RecordBatch::try_new(schema, arrays).map_err(|err| DatasetError(err.to_string()))?;
    Ok(EdgeTable {
        rel_type: rel_type.to_string(),
        src_label: src_label.to_string(),
        dst_label: dst_label.to_string(),
        batch,
    })
}
