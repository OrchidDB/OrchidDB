//! Arrow table constructors used by callers and fixtures.

use super::*;

// ---------------- builder helpers ----------------

/// Build a node table from columnar Rust data.
pub fn nodes_from_columns(label: impl Into<String>, columns: Vec<(&str, ArrayRef)>) -> NodeTable {
    let label = label.into();
    if columns.is_empty() {
        // Arrow cannot build a batch with zero columns and an unknown
        // row count. Callers with property-less nodes should pass a
        // hidden column carrying the row count; degrade to an empty
        // table instead of panicking when they don't.
        let schema: SchemaRef = Arc::new(Schema::empty());
        let batch = RecordBatch::try_new_with_options(
            schema,
            Vec::new(),
            &arrow::array::RecordBatchOptions::new().with_row_count(Some(0)),
        )
        .expect("empty node batch");
        return NodeTable { label, batch };
    }
    let fields: Vec<Field> = columns
        .iter()
        .map(|(name, array)| Field::new(*name, array.data_type().clone(), true))
        .collect();
    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let arrays: Vec<ArrayRef> = columns.into_iter().map(|(_, a)| a).collect();
    let batch = RecordBatch::try_new(schema, arrays).expect("node batch");
    NodeTable { label, batch }
}

/// Build a node table from columnar Rust data with an explicit row
/// count. Unlike [`nodes_from_columns`], property-less node tables keep
/// their rows (Arrow needs the count when there are zero columns), so
/// scenario initializers can declare `node a:Label` without properties.
pub fn nodes_from_columns_with_count(
    label: impl Into<String>,
    columns: Vec<(&str, ArrayRef)>,
    row_count: usize,
) -> NodeTable {
    let label = label.into();
    if columns.is_empty() {
        let schema: SchemaRef = Arc::new(Schema::empty());
        let batch = RecordBatch::try_new_with_options(
            schema,
            Vec::new(),
            &arrow::array::RecordBatchOptions::new().with_row_count(Some(row_count)),
        )
        .expect("empty node batch with count");
        return NodeTable { label, batch };
    }
    nodes_from_columns(label, columns)
}

/// Build an edge table from `__src_id, __dst_id` plus property columns.
pub fn edges_from_columns(
    rel_type: impl Into<String>,
    src_label: impl Into<String>,
    dst_label: impl Into<String>,
    src: Vec<i64>,
    dst: Vec<i64>,
    extra: Vec<(&str, ArrayRef)>,
) -> EdgeTable {
    assert_eq!(src.len(), dst.len(), "src/dst length mismatch");
    let mut fields = vec![
        Field::new("__src_id", DataType::Int64, false),
        Field::new("__dst_id", DataType::Int64, false),
    ];
    let mut arrays: Vec<ArrayRef> = vec![
        Arc::new(Int64Array::from(src)),
        Arc::new(Int64Array::from(dst)),
    ];
    for (name, array) in extra {
        fields.push(Field::new(name, array.data_type().clone(), true));
        arrays.push(array);
    }
    let schema: SchemaRef = Arc::new(Schema::new(fields));
    let batch = RecordBatch::try_new(schema, arrays).expect("edge batch");
    EdgeTable {
        rel_type: rel_type.into(),
        src_label: src_label.into(),
        dst_label: dst_label.into(),
        batch,
    }
}
