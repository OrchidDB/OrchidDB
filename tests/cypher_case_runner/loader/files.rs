//! CSV and Parquet fixture row loading.

use std::fs;
use std::path::Path;
use super::schema::CopyEntry;

/// Rows loaded from one `COPY` source file, plus (when the file carries
/// one) a header describing the columns by name. CSVs with Neo4j-style
/// typed headers (`id:ID(Person)`, `:START_ID(Person)`, `name:STRING`)
/// and Parquet files (whose schema names columns) both surface a
/// header; plain positional CSVs surface `None` and keep the legacy
/// schema-order interpretation.
pub(super) struct FileRows {
    pub(super) header: Option<Vec<String>>,
    pub(super) rows: Vec<Vec<Option<String>>>,
}

pub(super) fn load_entry_rows(root: &Path, entry: &CopyEntry) -> Option<FileRows> {
    let path = root.join(&entry.file);
    if entry.file.to_ascii_lowercase().ends_with(".parquet") {
        return read_parquet_rows(&path);
    }
    let raw = fs::read_to_string(&path).ok()?;
    let delim = detect_delimiter(&raw);
    let mut rows = parse_csv_delim(&raw, delim);
    if rows.first().is_some_and(|first| is_typed_header_row(first)) {
        let header = rows
            .remove(0)
            .into_iter()
            .map(|cell| cell.unwrap_or_default())
            .collect();
        return Some(FileRows {
            header: Some(header),
            rows,
        });
    }
    Some(FileRows { header: None, rows })
}

/// Read a Parquet file into string-rendered rows. The loader pipeline is
/// string-based (it re-parses cells per the schema's column types), so
/// every value is rendered via Arrow's display formatting.
fn read_parquet_rows(path: &Path) -> Option<FileRows> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let file = fs::File::open(path).ok()?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)
        .ok()?
        .build()
        .ok()?;
    let mut header: Option<Vec<String>> = None;
    let mut saw_schema = false;
    let mut rows = Vec::new();
    for batch in reader {
        let batch = batch.ok()?;
        if !saw_schema {
            saw_schema = true;
            let names: Vec<String> = batch
                .schema()
                .fields()
                .iter()
                .map(|f| f.name().clone())
                .collect();
            // Kuzu's converted fixtures carry generic `f0`/`f1`/…
            // column names; those are positional, not name-mapped.
            let generic = names.iter().all(|name| {
                let mut chars = name.chars();
                matches!(chars.next(), Some('f' | 'F'))
                    && chars.clone().next().is_some()
                    && chars.all(|c| c.is_ascii_digit())
            });
            if !generic {
                header = Some(names);
            }
        }
        for row in 0..batch.num_rows() {
            let mut cells = Vec::with_capacity(batch.num_columns());
            for col in batch.columns() {
                if col.is_null(row) {
                    cells.push(None);
                } else {
                    cells.push(arrow::util::display::array_value_to_string(col, row).ok());
                }
            }
            rows.push(cells);
        }
    }
    Some(FileRows { header, rows })
}

/// Pick the CSV delimiter from the first line: LDBC-style fixtures are
/// `|`-separated; everything else in the corpus is comma-separated.
fn detect_delimiter(raw: &str) -> char {
    let first = raw.lines().next().unwrap_or("");
    if first.contains('|') { '|' } else { ',' }
}

/// True when a parsed first row looks like a Neo4j-import typed header:
/// at least one cell of the form `name:TYPE`, `:START_ID(Label)`,
/// `:END_ID(Label)` or `id:ID(Label)` where the type token is
/// uppercase-ish.
fn is_typed_header_row(row: &[Option<String>]) -> bool {
    row.iter().any(|cell| {
        let Some(cell) = cell.as_deref() else {
            return false;
        };
        let Some((_, ty)) = cell.split_once(':') else {
            return false;
        };
        let ty = ty.trim();
        !ty.is_empty()
            && ty.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && ty
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '(' | ')' | ' '))
    })
}

/// `id:ID(Person)` → `id`; `:START_ID(Person)` → `` (empty).
pub(super) fn header_base(header: &str) -> &str {
    header.split(':').next().unwrap_or("").trim()
}

/// `id:ID(Person)` → `ID(Person)`; `name` → `` (empty).
pub(super) fn header_type(header: &str) -> &str {
    header
        .split_once(':')
        .map(|(_, ty)| ty.trim())
        .unwrap_or("")
}

// ============================================================
// CSV reader
// ============================================================

/// Minimal CSV reader: configurable delimiter (`,` or `|`), `"`-quoted
/// fields, `""` for an embedded quote, `\` for backslash escapes inside
/// quoted fields. Empty fields between delimiters surface as `None`;
/// quoted empty fields surface as `Some("")`.
fn parse_csv_delim(input: &str, delim: char) -> Vec<Vec<Option<String>>> {
    let mut rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut row: Vec<Option<String>> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut field_was_quoted = false;
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if in_quotes {
            match ch {
                '"' => {
                    if chars.peek() == Some(&'"') {
                        chars.next();
                        field.push('"');
                    } else {
                        in_quotes = false;
                    }
                }
                '\\' => {
                    if let Some(next) = chars.next() {
                        match next {
                            'n' => field.push('\n'),
                            'r' => field.push('\r'),
                            't' => field.push('\t'),
                            '"' => field.push('"'),
                            '\\' => field.push('\\'),
                            other => {
                                field.push('\\');
                                field.push(other);
                            }
                        }
                    }
                }
                _ => field.push(ch),
            }
            continue;
        }
        match ch {
            '"' => {
                in_quotes = true;
                field_was_quoted = true;
            }
            ch if ch == delim => {
                row.push(finalize_field(&mut field, &mut field_was_quoted));
            }
            '\n' => {
                row.push(finalize_field(&mut field, &mut field_was_quoted));
                rows.push(std::mem::take(&mut row));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    continue;
                }
                row.push(finalize_field(&mut field, &mut field_was_quoted));
                rows.push(std::mem::take(&mut row));
            }
            _ => field.push(ch),
        }
    }

    if !field.is_empty() || !row.is_empty() || field_was_quoted {
        row.push(finalize_field(&mut field, &mut field_was_quoted));
        rows.push(row);
    }

    rows
}

fn finalize_field(field: &mut String, was_quoted: &mut bool) -> Option<String> {
    let value = std::mem::take(field);
    let quoted = std::mem::replace(was_quoted, false);
    if quoted {
        Some(value)
    } else if value.is_empty() {
        None
    } else {
        Some(value)
    }
}
