//! Type-directed mappings: field names, map values and union variants are part
//! of the Arrow contract, so these templates are specialized at the call site.
use arrow::datatypes::DataType;
use datafusion::{
    common::{DFSchema, DataFusionError, Result, ScalarValue},
    logical_expr::{Expr, ExprSchemable},
};

pub fn handles(name: &str) -> bool {
    matches!(
        name.strip_prefix("fn.").unwrap_or(name),
        "arrow_cast"
            | "version"
            | "arrow_typeof"
            | "arrow_metadata"
            | "digest"
            | "struct"
            | "named_struct"
            | "get_field"
            | "map"
            | "map_keys"
            | "map_values"
            | "map_entries"
            | "map_extract"
            | "arrays_zip"
            | "union_tag"
            | "union_extract"
    )
}
fn error(s: &str) -> DataFusionError {
    DataFusionError::Plan(s.into())
}
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
fn ident(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}
fn text(e: &Expr) -> Result<&str> {
    match e {
        Expr::Literal(
            ScalarValue::Utf8(Some(v))
            | ScalarValue::LargeUtf8(Some(v))
            | ScalarValue::Utf8View(Some(v)),
            _,
        ) => Ok(v),
        _ => Err(error("field name must be a literal string")),
    }
}
fn element(t: &DataType) -> Result<&DataType> {
    match t {
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
            Ok(f.data_type())
        }
        _ => Err(error("expected list")),
    }
}
fn map_fields(t: &DataType) -> Result<&arrow::datatypes::Fields> {
    match t {
        DataType::Map(f, _) => match f.data_type() {
            DataType::Struct(fs) => Ok(fs),
            _ => Err(error("invalid map entries")),
        },
        _ => Err(error("expected map")),
    }
}
/// JSONB stores structured values; primitive leaves are text to retain exact
/// integers, decimals, non-finite floats, signed zero and binary bytes.
pub(crate) fn pg_encode(value: &str, ty: &DataType, level: usize) -> Result<String> {
    if crate::ir::functions::domain::is_json(ty) {
        return Ok(format!("to_jsonb(CAST({value} AS TEXT))"));
    }
    Ok(match ty {
        DataType::Struct(_) | DataType::Map(_, _) | DataType::Union(_, _) => value.into(),
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
            let v = format!("__local{}", 100 + level * 3);
            let i = format!("__local{}", 101 + level * 3);
            let t = format!("__local{}", 102 + level * 3);
            let child = pg_encode(&v, f.data_type(), level + 1)?;
            format!(
                "CASE WHEN {value} IS NULL THEN NULL ELSE (SELECT coalesce(jsonb_agg({child} ORDER BY {i}), '[]'::jsonb) FROM unnest({value}) WITH ORDINALITY AS {t}({v}, {i})) END"
            )
        }
        DataType::Binary | DataType::LargeBinary | DataType::BinaryView => {
            format!("to_jsonb('\\x' || encode({value}, 'hex'))")
        }
        _ => format!("to_jsonb(CAST({value} AS TEXT))"),
    })
}
pub(crate) fn pg_decode(value: &str, ty: &DataType, level: usize) -> Result<String> {
    if crate::ir::functions::domain::is_json(ty) {
        return Ok(format!("CAST(({value} #>> '{{}}') AS JSONB)"));
    }
    Ok(match ty {
        DataType::Struct(_) | DataType::Map(_, _) | DataType::Union(_, _) => {
            format!("nullif({value}, 'null'::jsonb)")
        }
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
            let v = format!("__local{}", 200 + level * 3);
            let i = format!("__local{}", 201 + level * 3);
            let t = format!("__local{}", 202 + level * 3);
            let child = pg_decode(&v, f.data_type(), level + 1)?;
            format!(
                "CASE WHEN {value} IS NULL OR {value} = 'null'::jsonb THEN NULL ELSE ARRAY(SELECT {child} FROM jsonb_array_elements({value}) WITH ORDINALITY AS {t}({v}, {i}) ORDER BY {i}) END"
            )
        }
        _ => format!(
            "CAST(({value} #>> '{{}}') AS {})",
            crate::ir::functions::types::postgres_type(ty)?
        ),
    })
}

pub fn mapping(name: &str, args: &[Expr], schema: &DFSchema, pg: bool) -> Result<String> {
    let name = name.strip_prefix("fn.").unwrap_or(name);
    let arity_ok = match name {
        "version" => args.is_empty(),
        "arrow_typeof" | "map_keys" | "map_values" | "map_entries" | "union_tag" => args.len() == 1,
        "arrow_cast" | "digest" | "map" | "map_extract" | "union_extract" => args.len() == 2,
        "arrow_metadata" => matches!(args.len(), 1 | 2),
        "named_struct" => !args.is_empty() && args.len() % 2 == 0,
        "struct" => !args.is_empty(),
        "get_field" => args.len() >= 2,
        "arrays_zip" => true,
        _ => false,
    };
    if !arity_ok {
        return Err(error("invalid portable function arity"));
    }
    let types = args
        .iter()
        .map(|e| e.get_type(schema))
        .collect::<Result<Vec<_>>>()?;
    let object = |fields: Vec<(String, String)>| -> String {
        if pg {
            fields
                .chunks(50)
                .map(|chunk| {
                    format!(
                        "jsonb_build_object({})",
                        chunk
                            .iter()
                            .flat_map(|(n, v)| [quote(n), v.clone()])
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect::<Vec<_>>()
                .join(" || ")
        } else {
            format!(
                "struct_pack({})",
                fields
                    .iter()
                    .map(|(n, v)| format!("{} := {v}", ident(n)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    };
    Ok(match name {
        "arrow_cast" => {
            use std::str::FromStr;
            let ty = DataType::from_str(text(&args[1])?)?;
            format!(
                "CAST(__arg0 AS {})",
                if pg {
                    crate::ir::functions::postgres_type(&ty)?
                } else {
                    crate::ir::functions::duckdb_type(&ty)?
                }
            )
        }
        "version" => {
            let result = datafusion::functions::core::version().invoke_with_args(
                datafusion::logical_expr::ScalarFunctionArgs {
                    args: vec![],
                    arg_fields: vec![],
                    number_rows: 1,
                    return_field: std::sync::Arc::new(arrow::datatypes::Field::new(
                        "version",
                        DataType::Utf8,
                        true,
                    )),
                    config_options: Default::default(),
                },
            )?;
            match result {
                datafusion::logical_expr::ColumnarValue::Scalar(ScalarValue::Utf8(Some(s))) => {
                    quote(&s)
                }
                _ => return Err(error("native version returned unexpected type")),
            }
        }
        "arrow_typeof" => quote(&types[0].to_string()),
        "arrow_metadata" => {
            let (_, field) = args[0].to_field(schema)?;
            if args.len() == 2 {
                field
                    .metadata()
                    .get(text(&args[1])?)
                    .map_or_else(|| "CAST(NULL AS VARCHAR)".into(), |v| quote(v))
            } else {
                let mut items = field.metadata().iter().collect::<Vec<_>>();
                items.sort_by_key(|(k, _)| *k);
                if pg {
                    let values = items
                        .iter()
                        .map(|(k, v)| {
                            format!(
                                "jsonb_build_object('key', {}, 'value', {})",
                                quote(k),
                                quote(v)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    format!("jsonb_build_array({values})")
                } else {
                    format!(
                        "map(ARRAY[{}]::VARCHAR[], ARRAY[{}]::VARCHAR[])",
                        items
                            .iter()
                            .map(|(k, _)| quote(k))
                            .collect::<Vec<_>>()
                            .join(","),
                        items
                            .iter()
                            .map(|(_, v)| quote(v))
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                }
            }
        }
        "digest" => {
            let alg = text(&args[1])?;
            if alg == "md5" {
                if pg {
                    "decode(md5(__arg0), 'hex')".into()
                } else {
                    "from_hex(md5(__arg0))".into()
                }
            } else {
                super::scalars::mapping(alg, pg)
                    .or_else(|| super::sha2::mapping(alg, pg))
                    .ok_or_else(|| error("unsupported digest algorithm"))?
            }
        }
        "struct" | "named_struct" => {
            let fields = if name == "struct" {
                types
                    .iter()
                    .enumerate()
                    .map(|(i, t)| {
                        Ok((
                            format!("c{i}"),
                            if pg {
                                pg_encode(&format!("__arg{i}"), t, 0)?
                            } else {
                                format!("__arg{i}")
                            },
                        ))
                    })
                    .collect::<Result<_>>()?
            } else {
                (0..args.len())
                    .step_by(2)
                    .map(|i| {
                        Ok((
                            text(&args[i])?.into(),
                            if pg {
                                pg_encode(&format!("__arg{}", i + 1), &types[i + 1], 0)?
                            } else {
                                format!("__arg{}", i + 1)
                            },
                        ))
                    })
                    .collect::<Result<_>>()?
            };
            object(fields)
        }
        "get_field" => {
            let mut value = "__arg0".to_string();
            let mut ty = types[0].clone();
            for arg in args.iter().skip(1) {
                let key = text(arg)?;
                match &ty {
                    DataType::Struct(fs) => {
                        let f = fs
                            .iter()
                            .find(|f| f.name() == key)
                            .ok_or_else(|| error("unknown struct field"))?;
                        value = if pg {
                            pg_decode(&format!("({value} -> {})", quote(key)), f.data_type(), 0)?
                        } else {
                            format!("struct_extract({value}, {})", quote(key))
                        };
                        ty = f.data_type().clone();
                    }
                    DataType::Map(_, _) => {
                        let fs = map_fields(&ty)?;
                        value = if pg {
                            let result = pg_decode("(__local11 -> 'value')", fs[1].data_type(), 0)?;
                            format!(
                                "(SELECT {result} FROM jsonb_array_elements({value}) AS __local10(__local11) WHERE __local11 ->> 'key' = {} LIMIT 1)",
                                quote(key)
                            )
                        } else {
                            format!("map_extract_value({value}, {})", quote(key))
                        };
                        ty = fs[1].data_type().clone();
                    }
                    _ => return Err(error("get_field requires struct or map")),
                }
            }
            value
        }
        "arrays_zip" => {
            if args.is_empty() {
                return Ok(if pg {
                    "ARRAY[]::TEXT[]"
                } else {
                    "[]::VARCHAR[]"
                }
                .into());
            }
            let fields = types
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let v = format!("__local{}[__local20]", i + 1);
                    Ok((
                        format!("c{i}"),
                        if pg {
                            pg_encode(&v, element(t)?, 0)?
                        } else {
                            v
                        },
                    ))
                })
                .collect::<Result<_>>()?;
            let row = object(fields);
            let lens = (0..args.len())
                .map(|i| {
                    format!(
                        "coalesce({}, 0)",
                        if pg {
                            format!("cardinality(__local{})", i + 1)
                        } else {
                            format!("len(__local{})", i + 1)
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            let nulls = (0..args.len())
                .map(|i| format!("__local{} IS NULL", i + 1))
                .collect::<Vec<_>>()
                .join(" AND ");
            let series = if pg {
                format!("generate_series(1, greatest({lens}))")
            } else {
                format!("range(1, greatest({lens}) + 1)")
            };
            super::bind(
                args.len(),
                &format!(
                    "CASE WHEN {nulls} THEN NULL ELSE ARRAY(SELECT {row} FROM {series} AS __local21(__local20) ORDER BY __local20) END"
                ),
            )
        }
        "map" if !pg => "map(__arg0, __arg1)".into(),
        "map" => {
            let k = pg_encode("__local1[__local20]", element(&types[0])?, 0)?;
            let v = pg_encode("__local2[__local20]", element(&types[1])?, 0)?;
            super::bind(
                2,
                &format!(
                    "CASE WHEN __local1 IS NULL THEN NULL WHEN cardinality(__local1) <> cardinality(__local2) OR __local2 IS NULL OR array_position(__local1, NULL) IS NOT NULL OR cardinality(__local1) <> (SELECT count(DISTINCT __local30) FROM unnest(__local1) AS __local31(__local30)) THEN CAST(CAST('invalid map ' || cardinality(__local1) AS BIGINT) AS TEXT)::jsonb ELSE (SELECT coalesce(jsonb_agg(jsonb_build_object('key', {k}, 'value', {v}) ORDER BY __local20), '[]'::jsonb) FROM generate_series(1, cardinality(__local1)) AS __local21(__local20)) END"
                ),
            )
        }
        "map_keys" | "map_values" | "map_entries" | "map_extract" => {
            let fs = map_fields(&types[0])?;
            if !pg {
                if name == "map_extract" {
                    "ARRAY[map_extract_value(__arg0, __arg1)]".into()
                } else {
                    format!("{name}(__arg0)")
                }
            } else {
                let v = match name {
                    "map_entries" => "__local11".into(),
                    "map_keys" => pg_decode("(__local11 -> 'key')", fs[0].data_type(), 0)?,
                    _ => pg_decode("(__local11 -> 'value')", fs[1].data_type(), 0)?,
                };
                if name == "map_extract" {
                    let k = pg_decode("(__local11 -> 'key')", fs[0].data_type(), 0)?;
                    super::bind(
                        2,
                        &format!(
                            "ARRAY[(SELECT {v} FROM jsonb_array_elements(__local1) AS __local10(__local11) WHERE {k} = __local2 LIMIT 1)]"
                        ),
                    )
                } else {
                    super::unary(&format!(
                        "CASE WHEN __local1 IS NULL THEN NULL ELSE ARRAY(SELECT {v} FROM jsonb_array_elements(__local1) WITH ORDINALITY AS __local10(__local11, __local12) ORDER BY __local12) END"
                    ))
                }
            }
        }
        "union_tag" => if pg {
            "__arg0 ->> 'tag'"
        } else {
            "CAST(union_tag(__arg0) AS VARCHAR)"
        }
        .into(),
        "union_extract" => {
            let tag = text(&args[1])?;
            let DataType::Union(fs, _) = &types[0] else {
                return Err(error("expected union"));
            };
            let f = fs
                .iter()
                .find(|(_, f)| f.name() == tag)
                .ok_or_else(|| error("unknown union variant"))?
                .1;
            if pg {
                let v = pg_decode("(__local1 -> 'value')", f.data_type(), 0)?;
                super::unary(&format!(
                    "CASE WHEN __local1 ->> 'tag' = {} THEN {v} ELSE NULL END",
                    quote(tag)
                ))
            } else {
                format!("union_extract(__arg0, {})", quote(tag))
            }
        }
        _ => return Err(error("unknown structured function")),
    })
}
