//! Inverse term mappings turn RDF effects into existing physical-row mutations.
use super::results::RdfTermValue as Value;
use crate::ir::rel::{
    rdf::RdfDatasetMapping,
    rdf_mapping::{RdfMapping, RdfTermMapping as Term, rdf_datatype},
    sql::mutation::{MappedMutation, RowCondition},
};
use std::collections::BTreeMap;
type Values = BTreeMap<String, Option<String>>;
type Result<T> = std::result::Result<T, String>;
fn invert(
    term: &Term,
    value: &Value,
    schema: &arrow::datatypes::Schema,
    values: &mut Values,
) -> Result<bool> {
    let mut put = |column: &str, value: String| -> Result<bool> {
        if let Some(previous) = values.insert(column.into(), Some(value.clone())) {
            if previous != Some(value) {
                return Ok(false);
            }
        }
        Ok(true)
    };
    match (term, value) {
        (Term::Iri { column }, Value::Iri(value)) => put(column, value.clone()),
        (Term::Template { prefix, columns }, Value::Iri(value))
        | (
            Term::Blank {
                scope: prefix,
                columns,
            },
            Value::BlankNode(value),
        ) => {
            let Some(tail) = value.strip_prefix(prefix) else {
                return Ok(false);
            };
            let parts = tail.split('/').collect::<Vec<_>>();
            if parts.len() != columns.len() {
                return Ok(false);
            }
            for (column, part) in columns.iter().zip(parts) {
                if part.len() % 2 != 0 || !part.is_ascii() {
                    return Ok(false);
                }
                let bytes = (0..part.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&part[i..i + 2], 16))
                    .collect::<std::result::Result<Vec<_>, _>>();
                let Ok(bytes) = bytes else { return Ok(false) };
                if bytes.iter().map(|b| format!("{b:02x}")).collect::<String>() != part {
                    return Ok(false);
                }
                let ty = schema
                    .field_with_name(column)
                    .map_err(|e| e.to_string())?
                    .data_type();
                use arrow::datatypes::DataType;
                if matches!(
                    ty,
                    DataType::Binary
                        | DataType::LargeBinary
                        | DataType::BinaryView
                        | DataType::FixedSizeBinary(_)
                ) {
                    if !put(
                        column,
                        bytes.iter().map(|b| format!("\\x{b:02X}")).collect(),
                    )? {
                        return Ok(false);
                    }
                    continue;
                }
                let Ok(text) = String::from_utf8(bytes) else {
                    return Ok(false);
                };
                let canonical = match ty {
                    DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                        text.parse::<i64>().ok().map(|v| v.to_string())
                    }
                    DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                        text.parse::<u64>().ok().map(|v| v.to_string())
                    }
                    DataType::Boolean => text.parse::<bool>().ok().map(|v| v.to_string()),
                    _ => Some(text.clone()),
                };
                if canonical.as_ref() != Some(&text) {
                    return Ok(false);
                }
                if !put(column, text)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        (
            Term::Literal {
                column,
                datatype,
                language,
                language_column,
            },
            Value::Literal {
                lexical,
                datatype: actual,
                language: lang,
            },
        ) => {
            let expected = match datatype {
                Some(d) => d.clone(),
                None => rdf_datatype(
                    schema
                        .field_with_name(column)
                        .map_err(|e| e.to_string())?
                        .data_type(),
                )
                .map_err(|e| e.to_string())?,
            };
            if language_column.is_none()
                && language.as_ref().map(|s| s.to_ascii_lowercase())
                    != lang.as_ref().map(|s| s.to_ascii_lowercase())
            {
                return Ok(false);
            }
            if actual
                != if lang.is_some() {
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
                } else {
                    &expected
                }
            {
                return Ok(false);
            }
            use arrow::datatypes::DataType;
            let kind = schema
                .field_with_name(column)
                .map_err(|e| e.to_string())?
                .data_type();
            let canonical = match kind {
                DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
                    lexical.parse::<i64>().ok().map(|v| v.to_string())
                }
                DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => {
                    lexical.parse::<u64>().ok().map(|v| v.to_string())
                }
                DataType::Boolean => lexical.parse::<bool>().ok().map(|v| v.to_string()),
                _ => Some(lexical.clone()),
            };
            if canonical.as_ref() != Some(lexical) {
                return Err(format!(
                    "RDF lexical form {lexical:?} cannot round-trip through {column}; map a lexical text column to preserve it"
                ));
            }
            if !put(column, lexical.clone())? {
                return Ok(false);
            }
            if let Some(column) = language_column {
                values.insert(
                    column.clone(),
                    lang.as_ref().map(|s| s.to_ascii_lowercase()),
                );
            }
            Ok(true)
        }
        (
            Term::Constant {
                value,
                datatype: None,
                language: None,
            },
            Value::Iri(actual),
        ) => Ok(value == actual),
        (
            Term::Constant {
                value,
                datatype,
                language,
            },
            Value::Literal {
                lexical,
                datatype: actual,
                language: lang,
            },
        ) => Ok(value == lexical
            && language.as_ref().map(|s| s.to_ascii_lowercase())
                == lang.as_ref().map(|s| s.to_ascii_lowercase())
            && (language.is_some() || datatype.as_ref() == Some(actual))),
        _ => Ok(false),
    }
}
pub(crate) fn inverse(
    rule: &RdfMapping,
    graph: &Option<String>,
    triple: &[Value; 3],
    mapping: &RdfDatasetMapping,
) -> Result<Option<Values>> {
    let tables = mapping.registered_tables();
    let schema = tables
        .get(&rule.table)
        .ok_or_else(|| format!("Unknown RDF table {}", rule.table))?
        .schema();
    let mut values = Values::new();
    match (&rule.graph, graph) {
        (None, None) => (),
        (Some(term), Some(g)) => {
            if !invert(term, &Value::Iri(g.clone()), &schema, &mut values)? {
                return Ok(None);
            }
        }
        _ => return Ok(None),
    }
    for (term, value) in [&rule.subject, &rule.predicate, &rule.object]
        .into_iter()
        .zip(triple)
    {
        if !invert(term, value, &schema, &mut values)? {
            return Ok(None);
        }
    }
    Ok(Some(values))
}
pub(crate) fn effects(
    rule: &RdfMapping,
    values: Values,
    insert: bool,
) -> Result<Vec<MappedMutation>> {
    if !rule.writable {
        return Err(format!("Mapped RDF rule for {} is read-only", rule.table));
    }
    if rule.key.is_empty() {
        return Err("Writable RDF mappings require a complete row key".into());
    }
    let key = rule
        .key
        .iter()
        .map(|c| {
            values
                .get(c)
                .filter(|v| v.is_some())
                .cloned()
                .map(|v| (c.clone(), v))
                .ok_or_else(|| format!("RDF statement does not determine key column {c}"))
        })
        .collect::<Result<Vec<_>>>()?;
    if insert {
        Ok(vec![MappedMutation::Upsert {
            table: rule.table.clone(),
            key,
            values: values
                .into_iter()
                .filter(|(c, _)| !rule.key.contains(c))
                .collect(),
        }])
    } else {
        let mut columns = rule
            .object
            .columns()
            .into_iter()
            .filter(|c| !rule.key.iter().any(|k| k == c))
            .map(str::to_string)
            .collect::<Vec<_>>();
        if columns.is_empty() && !rule.object.columns().is_empty() {
            columns = rule
                .subject
                .columns()
                .into_iter()
                .filter(|c| !rule.key.iter().any(|k| k == c))
                .map(str::to_string)
                .collect();
        }
        let conditions = values
            .into_iter()
            .map(|(c, v)| RowCondition::Equal(c, v))
            .collect();
        Ok(vec![if columns.is_empty() {
            MappedMutation::Delete {
                table: rule.table.clone(),
                conditions,
            }
        } else {
            MappedMutation::Clear {
                table: rule.table.clone(),
                columns,
                conditions,
            }
        }])
    }
}
pub(crate) fn coalesce(effects: Vec<MappedMutation>) -> Result<Vec<MappedMutation>> {
    let mut output = Vec::new();
    let mut positions = BTreeMap::new();
    for effect in effects {
        if let MappedMutation::Upsert { table, key, values } = effect {
            let identity = (table.clone(), key.clone());
            if let Some(&index) = positions.get(&identity) {
                let MappedMutation::Upsert {
                    values: existing, ..
                } = &mut output[index]
                else {
                    unreachable!()
                };
                for (column, value) in values {
                    if let Some((_, old)) = existing.iter().find(|(c, _)| c == &column) {
                        if old != &value {
                            return Err(format!("Conflicting RDF values for {table}.{column}"));
                        }
                    } else {
                        existing.push((column, value));
                    }
                }
            } else {
                positions.insert(identity, output.len());
                output.push(MappedMutation::Upsert { table, key, values });
            }
        } else {
            positions.clear();
            output.push(effect);
        }
    }
    // DELETE/INSERT is one row transition: do not transiently null required columns.
    for i in 0..output.len() {
        let MappedMutation::Clear {
            table,
            columns,
            conditions,
        } = &output[i]
        else {
            continue;
        };
        let replacement = output[i + 1..].iter().find_map(|effect| {
            let MappedMutation::Upsert {
                table: target,
                key,
                values,
            } = effect
            else {
                return None;
            };
            if target != table
                || !key.iter().all(|(k, v)| {
                    conditions
                        .iter()
                        .any(|c| matches!(c,RowCondition::Equal(c,w) if c==k && w==v))
                })
            {
                return None;
            }
            let assignments = columns
                .iter()
                .map(|c| values.iter().find(|(k, _)| k == c).cloned())
                .collect::<Option<Vec<_>>>()?;
            Some(MappedMutation::Assign {
                table: table.clone(),
                values: assignments,
                conditions: conditions.clone(),
            })
        });
        if let Some(effect) = replacement {
            output[i] = effect;
        }
    }
    Ok(output)
}

/// Order physical row effects using the database's declared FK dependencies.
/// Clears/deletes precede insertions; parent inserts precede child inserts.
pub(crate) fn order(
    effects: &mut [MappedMutation],
    dependencies: &[(String, String)],
) -> Result<()> {
    let mut ranks = BTreeMap::<String, usize>::new();
    for _ in 0..=dependencies.len() {
        let mut changed = false;
        for (child, parent) in dependencies {
            if child == parent {
                continue;
            }
            let rank = ranks.get(parent).copied().unwrap_or(0) + 1;
            let old = ranks.entry(child.clone()).or_default();
            if *old < rank {
                *old = rank;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    effects.sort_by_key(|effect| {
        let (table, remove) = match effect {
            MappedMutation::Upsert { table, .. }
            | MappedMutation::InsertAbsent { table, .. }
            | MappedMutation::Assign { table, .. } => (table, false),
            MappedMutation::Delete { table, .. } | MappedMutation::Clear { table, .. } => {
                (table, true)
            }
        };
        let name = datafusion::common::TableReference::from(table.as_str());
        let rank = ranks.get(name.table()).copied().unwrap_or(0);
        (!remove, if remove { usize::MAX - rank } else { rank })
    });
    Ok(())
}
