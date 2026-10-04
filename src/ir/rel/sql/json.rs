//! SQL implementations of typed JSON operations. Each operation may produce a
//! complete scalar subquery; adapters are not restricted to function renaming.
use super::{SqlDialect, SqlError, SqlResult};
use datafusion::sql::sqlparser::ast;

fn unsupported(message: impl Into<String>) -> SqlError {
    SqlError::Unsupported(message.into())
}
fn expression(sql: &str, args: &[ast::Expr], dialect: SqlDialect) -> SqlResult<ast::Expr> {
    super::functions::portable_template(sql, args, dialect)
}
fn string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
fn literal(expr: &ast::Expr) -> Option<&str> {
    match expr {
        ast::Expr::Value(value) => match &value.value {
            ast::Value::SingleQuotedString(s) => Some(s),
            _ => None,
        },
        ast::Expr::Nested(inner) | ast::Expr::Cast { expr: inner, .. } => literal(inner),
        _ => None,
    }
}
use crate::ir::functions::json::path::Step;
fn path(expr: &ast::Expr) -> SqlResult<Vec<Step>> {
    let source =
        literal(expr).ok_or_else(|| unsupported("dynamic JSON paths require native evaluation"))?;
    let steps = crate::ir::functions::json::path::parse(source)?;
    if !crate::ir::functions::json::path::singular(&steps) {
        return Err(unsupported(
            "non-definite JSON paths require native evaluation",
        ));
    }
    Ok(steps)
}
pub(super) fn access(
    document: &str,
    path_expr: &ast::Expr,
    dialect: SqlDialect,
) -> SqlResult<String> {
    access_steps(document, &path(path_expr)?, dialect)
}
pub(super) fn access_steps(
    document: &str,
    steps: &[Step],
    dialect: SqlDialect,
) -> SqlResult<String> {
    let mut document = document.to_owned();
    for step in steps.iter().cloned() {
        let (key, index, pointer) = match step {
            Step::Field(key) => (Some(key), None, false),
            Step::Pointer(key) => {
                let index = pointer_index(&key);
                (Some(key), index, true)
            }
            Step::Index(index) => (None, Some(index), false),
            _ => unreachable!(),
        };
        document = match dialect {
            SqlDialect::Postgres => {
                let mut clauses = vec![];
                if let Some(key) = &key {
                    clauses.push(format!(
                        "WHEN jsonb_typeof(d)='object' THEN d -> {}",
                        string(key)
                    ));
                }
                if let Some(index) = index {
                    if let Ok(index) = i32::try_from(index) {
                        clauses.push(format!("WHEN jsonb_typeof(d)='array' THEN d -> {index}"));
                    }
                }
                if clauses.is_empty() {
                    "CAST(NULL AS JSONB)".into()
                } else {
                    format!(
                        "(SELECT CASE {} ELSE CAST(NULL AS JSONB) END FROM (SELECT {document} AS d) AS __local6)",
                        clauses.join(" ")
                    )
                }
            }
            SqlDialect::DuckDb => {
                let mut conditions = vec![];
                if let Some(key) = &key {
                    conditions.push(format!("(json_type(d)='OBJECT' AND e.key={})", string(key)));
                }
                if let Some(index) = index {
                    let index = if index < 0 {
                        format!("CAST(json_array_length(d) AS BIGINT) + ({index})")
                    } else {
                        index.to_string()
                    };
                    conditions.push(format!(
                        "(json_type(d)='ARRAY' AND e.key=CAST(({index}) AS VARCHAR))"
                    ));
                }
                if !pointer && key.is_some() {
                    conditions.truncate(1);
                }
                format!(
                    "(SELECT e.value FROM (SELECT {document} AS d) AS __local6 CROSS JOIN LATERAL json_each(d) AS e WHERE {} ORDER BY e.id DESC LIMIT 1)",
                    conditions.join(" OR ")
                )
            }
            _ => {
                return Err(unsupported(
                    "JSON path lowering requires an engine implementation",
                ));
            }
        };
    }
    Ok(document)
}
/// Normalize duplicate members using their final occurrence. SQL constructs
/// row intervals from JSON parse order, then emits surviving tree tokens. All
/// work remains in the owning SQL island, including correlated documents.
pub(super) fn canonical_sql(document: &str, dialect: SqlDialect) -> SqlResult<String> {
    match dialect {
        SqlDialect::DuckDb => Ok(duck_tree(document, 0)),
        SqlDialect::Postgres => Ok(document.into()),
        _ => Err(unsupported("JSON canonicalization requires engine support")),
    }
}
// mode 0: JSON value; mode 1: typed containment encoding; mode 2: equality
// encoding (array indices become object keys so order and multiplicity matter).
fn duck_tree(document: &str, mode: u8) -> String {
    let number = "(SELECT CASE WHEN trim(digits,'0')='' THEN '0e0' ELSE sign || rtrim(ltrim(digits,'0'),'0') || 'e' || CAST(power - fraction + length(digits)-length(rtrim(digits,'0')) AS VARCHAR) END FROM (SELECT CASE WHEN left(raw,1)='-' THEN '-' ELSE '' END AS sign, replace(replace(split_part(lower(raw),'e',1),'-',''),'.','') AS digits, CASE WHEN contains(lower(raw),'e') THEN TRY_CAST(split_part(lower(raw),'e',2) AS BIGINT) ELSE 0 END AS power, length(split_part(split_part(raw,'e',1),'.',2)) AS fraction FROM (SELECT lower(CAST(r.value AS VARCHAR)) AS raw) AS __local7) AS __local6)";
    let scalar = if mode == 0 {
        "CAST(r.value AS VARCHAR)".to_owned()
    } else {
        format!(
            "CASE r.type WHEN 'NULL' THEN '{{\"z' || r.depth || '\":null}}' WHEN 'BOOLEAN' THEN '{{\"b' || r.depth || '\":' || CAST(r.value AS VARCHAR) || '}}' WHEN 'VARCHAR' THEN '{{\"s' || r.depth || '\":' || CAST(r.value AS VARCHAR) || '}}' ELSE '{{\"n' || r.depth || '\":' || CAST(to_json({number}) AS VARCHAR) || '}}' END"
        )
    };
    let (object_open, array_open, object_close, array_close) = if mode == 0 {
        ("'{'", "'['", "}", "]")
    } else if mode == 2 {
        (
            "'{\"o' || r.depth || '\":{'",
            "'{\"a' || r.depth || '\":{'",
            "}}",
            "}}",
        )
    } else {
        (
            "'{\"o' || r.depth || '\":{'",
            "'{\"a' || r.depth || '\":['",
            "}}",
            "]}",
        )
    };
    let offset = if mode == 3 { 1 } else { 0 };
    let keyed = if mode == 2 {
        "p.type IN ('OBJECT','ARRAY')"
    } else {
        "p.type='OBJECT'"
    };
    format!(
        r#"(WITH __local0 AS (SELECT id,parent,key,type,value FROM json_tree({document})),
__local1 AS (SELECT r.id,r.parent,r.key,r.type,r.value,coalesce(min(n.id) FILTER (WHERE n.id>r.id AND coalesce(CAST(n.parent AS BIGINT),-1)<r.id),max(n.id)+1) ending FROM __local0 r CROSS JOIN __local0 n GROUP BY r.id,r.parent,r.key,r.type,r.value),
__local2 AS (SELECT r.id,r.ending FROM __local1 r JOIN __local0 p ON p.id=r.parent WHERE p.type='OBJECT' QUALIFY row_number() OVER (PARTITION BY r.parent,r.key ORDER BY r.id DESC)>1),
__local3 AS (SELECT r.id,r.parent,r.key,r.type,r.value,r.ending,{offset}+count(ancestor.id) AS depth,row_number() OVER (PARTITION BY r.parent ORDER BY r.id) AS ordinal FROM __local1 r LEFT JOIN __local2 d ON r.id>=d.id AND r.id<d.ending LEFT JOIN __local1 ancestor ON ancestor.id<r.id AND ancestor.ending>r.id WHERE d.id IS NULL GROUP BY r.id,r.parent,r.key,r.type,r.value,r.ending),
__local4 AS (SELECT r.id AS position,1 AS priority,r.id,CASE WHEN r.ordinal>1 THEN ',' ELSE '' END || CASE WHEN {keyed} THEN CAST(to_json(r.key) AS VARCHAR)||':' ELSE '' END || CASE r.type WHEN 'OBJECT' THEN {object_open} WHEN 'ARRAY' THEN {array_open} ELSE {scalar} END AS token FROM __local3 r LEFT JOIN __local3 p ON p.id=r.parent
UNION ALL SELECT ending,0,id,CASE type WHEN 'OBJECT' THEN '{object_close}' ELSE '{array_close}' END FROM __local3 WHERE type IN ('OBJECT','ARRAY'))
SELECT CAST(string_agg(token,'' ORDER BY position,priority,id DESC) AS JSON) FROM __local4)"#
    )
}
fn strict_valid(document: &str) -> String {
    // DuckDB accepts JavaScript NaN/Infinity and trailing commas. Ignore
    // quoted strings before rejecting those extensions of the JSON grammar.
    format!(
        r#"(json_valid({document}) AND NOT regexp_matches(regexp_replace({document}, '"([^"\\]|\\.)*"', '0', 'g'), 'NaN|Infinity|,[[:space:]]*[}}\]]'))"#
    )
}
fn arity(name: &str, args: &[ast::Expr], min: usize, max: usize) -> SqlResult<()> {
    if args.len() < min || args.len() > max {
        Err(unsupported(format!("invalid {name} argument count")))
    } else {
        Ok(())
    }
}
pub(crate) fn lower_function(
    function: &ast::Function,
    dialect: SqlDialect,
) -> SqlResult<Option<ast::Expr>> {
    let rendered_name = function.name.to_string();
    let name = rendered_name.trim_matches('"');
    let object = match name {
        "__orchiddb_json_array_agg" => false,
        "__orchiddb_json_object_agg" => true,
        _ => return Ok(None),
    };
    let mut aggregate = function.clone();
    if dialect == SqlDialect::Postgres {
        aggregate.name = ast::ObjectName::from(vec![ast::Ident::new(if object {
            "jsonb_object_agg"
        } else {
            "jsonb_agg"
        })]);
        return Ok(Some(ast::Expr::Function(aggregate)));
    }
    let ast::FunctionArguments::List(arguments) = &mut aggregate.args else {
        return Err(unsupported("JSON aggregate requires positional arguments"));
    };
    let args = arguments
        .args
        .iter()
        .map(|arg| match arg {
            ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(value)) => Ok(value.clone()),
            _ => Err(unsupported("JSON aggregate requires positional arguments")),
        })
        .collect::<SqlResult<Vec<_>>>()?;
    arity(
        name,
        &args,
        if object { 2 } else { 1 },
        if object { 2 } else { 1 },
    )?;
    aggregate.name = ast::ObjectName::from(vec![ast::Ident::new("list")]);
    if object {
        let entry = expression(
            "struct_pack(key := CASE WHEN __arg0 IS NULL THEN error('json.object_agg key cannot be SQL NULL') ELSE __arg0 END, value := coalesce(to_json(__arg1), CAST('null' AS JSON)))",
            &args,
            dialect,
        )?;
        arguments.args = vec![ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(entry))];
        expression("list_extract(list_transform([__arg0], lambda __local0: to_json(map_from_entries(list_filter(__local0, lambda __local1, __local2: __local2 = len(__local0) - list_position(list_transform(list_reverse(__local0), lambda __local3: __local3.key), __local1.key) + 1)))), 1)",&[ast::Expr::Function(aggregate)],dialect).map(Some)
    } else {
        expression(
            "to_json(__arg0)",
            &[ast::Expr::Function(aggregate)],
            dialect,
        )
        .map(Some)
    }
}
pub(crate) fn lower(
    name: &str,
    args: &[ast::Expr],
    dialect: SqlDialect,
) -> SqlResult<Option<ast::Expr>> {
    let Some(name) = name.strip_prefix("__orchiddb_json_") else {
        return Ok(None);
    };
    let pg = dialect == SqlDialect::Postgres;
    let json_type = if pg { "JSONB" } else { "JSON" };
    let kind = |doc: &str| {
        if pg {
            format!("jsonb_typeof({doc})")
        } else {
            format!(
                "CASE json_type({doc}) WHEN 'NULL' THEN 'null' WHEN 'BOOLEAN' THEN 'boolean' WHEN 'VARCHAR' THEN 'string' WHEN 'ARRAY' THEN 'array' WHEN 'OBJECT' THEN 'object' ELSE CASE WHEN {doc} IS NULL THEN NULL ELSE 'number' END END"
            )
        }
    };
    let query = |args: &[ast::Expr]| {
        if args.len() > 1 {
            access("__arg0", &args[1], dialect)
        } else {
            Ok("__arg0".into())
        }
    };
    let sql = match name {
        "parse" => {
            arity(name, args, 1, 1)?;
            if pg {
                format!("CAST(__arg0 AS {json_type})")
            } else {
                duck_tree(
                    &format!(
                        "CASE WHEN __arg0 IS NULL THEN CAST(NULL AS JSON) WHEN {} THEN CAST(__arg0 AS JSON) ELSE CAST(error('invalid strict JSON text') AS JSON) END",
                        strict_valid("__arg0")
                    ),
                    0,
                )
            }
        }
        "stringify" => {
            arity(name, args, 1, 1)?;
            format!("CAST({} AS VARCHAR)", canonical_sql("__arg0", dialect)?)
        }
        "valid" => {
            arity(name, args, 1, 1)?;
            if pg {
                return Err(unsupported(
                    "PostgreSQL 14 cannot safely validate arbitrary JSON text without an installed helper; use the native JSON operation",
                ));
            }
            strict_valid("__arg0")
        }
        "query" => {
            arity(name, args, 2, 2)?;
            query(args)?
        }
        "exists" => {
            arity(name, args, 2, 2)?;
            format!(
                "CASE WHEN __arg0 IS NULL THEN NULL ELSE {} IS NOT NULL END",
                query(args)?
            )
        }
        "value" => {
            arity(name, args, 2, 3)?;
            let selected = query(args)?;
            let text = if pg {
                format!("(j #>> '{{}}')")
            } else {
                "json_extract_string(j, '$')".into()
            };
            let mut target = None;
            let mut required_kind = None;
            if args.len() == 3 {
                let ty = literal(&args[2])
                    .ok_or_else(|| unsupported("json.value requires a literal output type"))?;
                let ty = crate::ir::functions::json::scalar_type(ty)?;
                if crate::ir::functions::domain::is_json(&ty) {
                    return expression(&format!("(SELECT CASE WHEN j IS NULL OR {} IN ('null','array','object') THEN CAST(NULL AS {json_type}) ELSE j END FROM (SELECT {selected} AS j) AS __local0)",kind("j")),args,dialect).map(Some);
                }
                if ty.is_nested() {
                    return Err(unsupported("json.value output must be scalar"));
                }
                if ty.is_numeric() {
                    required_kind = Some("number");
                } else if ty == arrow::datatypes::DataType::Boolean {
                    required_kind = Some("boolean");
                }
                target = Some(ty);
            }
            let valid = required_kind
                .map(|k| format!("{} = '{}'", kind("j"), k))
                .unwrap_or_else(|| "true".into());
            let valid = if target.as_ref().is_some_and(|ty| ty.is_integer()) {
                let pattern = if pg {
                    format!("({text}) ~ '^-?[0-9]+$'")
                } else {
                    format!("regexp_full_match({text}, '-?[0-9]+')")
                };
                format!("({valid}) AND ({pattern})")
            } else {
                valid
            };
            let valid = if target.as_ref().is_some_and(|ty| {
                matches!(
                    ty,
                    arrow::datatypes::DataType::Float32 | arrow::datatypes::DataType::Float64
                )
            }) {
                let sqltype = dialect.sql_type(target.as_ref().unwrap())?;
                let finite = if pg {
                    format!(
                        "CAST({text} AS {sqltype}) NOT IN (CAST('Infinity' AS {sqltype}),CAST('-Infinity' AS {sqltype}),CAST('NaN' AS {sqltype}))"
                    )
                } else {
                    format!("isfinite(CAST({text} AS {sqltype}))")
                };
                format!("({valid}) AND ({finite})")
            } else {
                valid
            };
            let value = if let Some(ty) = &target {
                format!("CAST({text} AS {})", dialect.sql_type(ty)?)
            } else {
                text
            };
            let invalid = if pg {
                format!(
                    "CAST('invalid JSON scalar conversion: ' || CAST(j AS TEXT) AS {})",
                    target
                        .as_ref()
                        .map(|ty| dialect.sql_type(ty))
                        .transpose()?
                        .unwrap_or_else(|| "TEXT".into())
                )
            } else {
                "error('invalid JSON scalar conversion')".into()
            };
            format!(
                "(SELECT CASE WHEN j IS NULL OR {} IN ('null','array','object') THEN NULL WHEN {valid} THEN {value} ELSE {invalid} END FROM (SELECT {selected} AS j) AS __local0)",
                kind("j")
            )
        }
        "type" => {
            arity(name, args, 1, 2)?;
            kind(&query(args)?)
        }
        "array_length" => {
            arity(name, args, 1, 2)?;
            let selected = query(args)?;
            if pg {
                format!("CAST(jsonb_array_length({selected}) AS BIGINT)")
            } else {
                format!(
                    "(SELECT CASE WHEN j IS NULL THEN NULL WHEN json_type(j)='ARRAY' THEN json_array_length(j) ELSE error('json.array_length requires an array') END FROM (SELECT {selected} AS j) AS __local0)"
                )
            }
        }
        "keys" => {
            arity(name, args, 1, 2)?;
            let selected = query(args)?;
            if pg {
                format!(
                    "(SELECT CASE WHEN j IS NULL THEN CAST(NULL AS TEXT[]) ELSE ARRAY(SELECT k FROM jsonb_object_keys(j) AS k ORDER BY k COLLATE \"C\") END FROM (SELECT {selected} AS j) AS __local0)"
                )
            } else {
                format!(
                    "(SELECT CASE WHEN j IS NULL THEN CAST(NULL AS VARCHAR[]) WHEN json_type(j)='OBJECT' THEN list_sort(list_distinct(json_keys(j))) ELSE error('json.keys requires an object') END FROM (SELECT {selected} AS j) AS __local0)"
                )
            }
        }
        "array" => format!(
            "{}({})",
            if pg {
                "jsonb_build_array"
            } else {
                "json_array"
            },
            (0..args.len())
                .map(|i| format!("__arg{i}"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        "object" => {
            if args.len() % 2 != 0 {
                return Err(unsupported("json.object requires key/value pairs"));
            }
            let result = format!(
                "{}({})",
                if pg {
                    "jsonb_build_object"
                } else {
                    "json_object"
                },
                (0..args.len())
                    .map(|i| format!("__arg{i}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            if pg {
                result
            } else {
                let null_keys = (0..args.len())
                    .step_by(2)
                    .map(|i| format!("__arg{i} IS NULL"))
                    .collect::<Vec<_>>()
                    .join(" OR ");
                let result = duck_tree(&result, 0);
                if null_keys.is_empty() {
                    result
                } else {
                    format!(
                        "CASE WHEN {null_keys} THEN CAST(error('json.object key cannot be SQL NULL') AS JSON) ELSE {result} END"
                    )
                }
            }
        }
        "contains" | "equals" => {
            arity(name, args, 2, 2)?;
            if pg {
                if name == "contains" {
                    "(__arg0 @> __arg1)".into()
                } else {
                    "(__arg0 = __arg1)".into()
                }
            } else {
                let mode = if name == "equals" { 2 } else { 1 };
                let left = duck_tree("__arg0", mode);
                let right = duck_tree("__arg1", mode);
                let root_primitive = duck_tree("__arg1", 3);
                let comparison = if name == "equals" {
                    "json_contains(json_array(a),json_array(b)) AND json_contains(json_array(b),json_array(a))".to_owned()
                } else {
                    format!(
                        "CASE WHEN json_type(__arg0)='ARRAY' AND json_type(__arg1) NOT IN ('ARRAY','OBJECT') THEN json_contains(json_extract(a,'$.a0'), json_array({root_primitive})) ELSE json_contains(json_array(a),json_array(b)) END"
                    )
                };
                format!(
                    "(SELECT CASE WHEN __arg0 IS NULL OR __arg1 IS NULL THEN NULL ELSE {comparison} END FROM (SELECT {left} AS a,{right} AS b) AS __local5)"
                )
            }
        }
        "merge_patch" if pg => {
            arity(name, args, 2, 2)?;
            "(WITH RECURSIVE __local0(path,value) AS (SELECT CAST(ARRAY[] AS TEXT[]), __arg1 UNION ALL SELECT n.path || e.key, e.value FROM __local0 n CROSS JOIN LATERAL jsonb_each(CASE WHEN jsonb_typeof(n.value)='object' THEN n.value ELSE CAST('{}' AS JSONB) END) e), __local1 AS (SELECT path,value,row_number() OVER (ORDER BY cardinality(path),path) AS ordinal FROM __local0), __local2(ordinal,document) AS (SELECT CAST(0 AS BIGINT),__arg0 UNION ALL SELECT o.ordinal,CASE WHEN cardinality(o.path)=0 THEN CASE WHEN jsonb_typeof(o.value)='object' THEN CASE WHEN jsonb_typeof(s.document)='object' THEN s.document ELSE CAST('{}' AS JSONB) END ELSE o.value END WHEN o.value=CAST('null' AS JSONB) THEN s.document #- o.path WHEN jsonb_typeof(o.value)='object' THEN CASE WHEN jsonb_typeof(s.document #> o.path)='object' THEN s.document ELSE jsonb_set(s.document,o.path,CAST('{}' AS JSONB),true) END ELSE jsonb_set(s.document,o.path,o.value,true) END FROM __local2 s JOIN __local1 o ON o.ordinal=s.ordinal+1) SELECT CASE WHEN __arg0 IS NULL OR __arg1 IS NULL THEN CAST(NULL AS JSONB) ELSE document END FROM __local2 ORDER BY ordinal DESC LIMIT 1)".into()
        }
        "merge_patch" if !pg => {
            arity(name, args, 2, 2)?;
            format!(
                "CASE WHEN __arg0 IS NULL OR __arg1 IS NULL THEN NULL ELSE json_merge_patch({}, {}) END",
                duck_tree("__arg0", 0),
                duck_tree("__arg1", 0)
            )
        }
        "set" | "insert" | "replace" if !pg => {
            arity(name, args, 3, 3)?;
            let steps = path(&args[1])?;
            let replacement = "coalesce(to_json(__arg2),CAST('null' AS JSON))";
            let document = canonical_sql("__arg0", dialect)?;
            duck_mutate(&document, &steps, replacement, name, &mut 8)
        }
        "remove" if !pg => {
            arity(name, args, 2, usize::MAX)?;
            let mut document = canonical_sql("__arg0", dialect)?;
            for arg in &args[1..] {
                let steps = path(arg)?;
                if steps.is_empty() {
                    return Err(unsupported("cannot remove the JSON document root"));
                }
                document = duck_mutate(&document, &steps, "CAST('null' AS JSON)", name, &mut 8);
            }
            document
        }
        "transform" => return super::json_transform::lower(args, dialect).map(Some),
        "set" | "replace" | "insert" if pg => {
            arity(name, args, 3, 3)?;
            let segments = path(&args[1])?;
            let replacement = "(jsonb_build_array(__arg2) -> 0)";
            if segments.is_empty() {
                if name == "insert" {
                    "__arg0".into()
                } else {
                    format!(
                        "CASE WHEN __arg0 IS NULL THEN CAST(NULL AS JSONB) ELSE {replacement} END"
                    )
                }
            } else {
                let fields = pg_path(&segments);
                let parent = access_steps("__arg0", &segments[..segments.len() - 1], dialect)?;
                let valid = mutation_container(&parent, segments.last().unwrap());
                let create = if name == "replace" { "false" } else { "true" };
                let mutation = if name == "insert" {
                    let selected = access_steps("__arg0", &segments, dialect)?;
                    format!(
                        "CASE WHEN jsonb_typeof({parent})='object' AND {selected} IS NOT NULL THEN __arg0 ELSE jsonb_insert(__arg0, {fields}, {replacement}, false) END"
                    )
                } else {
                    format!("jsonb_set(__arg0,{fields},{replacement},{create})")
                };
                format!("CASE WHEN {valid} THEN {mutation} ELSE __arg0 END")
            }
        }
        "remove" if pg => {
            arity(name, args, 2, usize::MAX)?;
            let mut value = "__arg0".to_owned();
            for arg in &args[1..] {
                let segments = path(arg)?;
                if segments.is_empty() {
                    return Err(unsupported("cannot remove the JSON document root"));
                }
                let selected = access_steps("d", &segments, dialect)?;
                let fields = pg_path(&segments);
                value = format!(
                    "(SELECT CASE WHEN {selected} IS NOT NULL THEN d #- {fields} ELSE d END FROM (SELECT {value} AS d) AS __local5)"
                );
            }
            value
        }
        _ => {
            return Err(unsupported(format!(
                "json.{name} has no semantics-preserving {} SQL lowering; use a native JSON region",
                dialect.name()
            )));
        }
    };
    expression(&sql, args, dialect).map(Some)
}

// Rebuild only containers along a definite path. Other members remain JSON
// values, so arbitrary nested documents and SQL-null replacement values retain
// their domain semantics without an engine-specific mutation extension.
fn duck_mutate(
    document: &str,
    steps: &[Step],
    replacement: &str,
    operation: &str,
    next: &mut usize,
) -> String {
    if steps.is_empty() {
        return if operation == "insert" {
            document.into()
        } else {
            format!("CASE WHEN {document} IS NULL THEN CAST(NULL AS JSON) ELSE {replacement} END")
        };
    }
    let alias = format!("__local{}", *next);
    *next += 1;
    let entries = format!("__local{}", *next);
    *next += 1;
    let d = format!("{alias}.d");
    let leaf = steps.len() == 1;
    let (key, index) = match &steps[0] {
        Step::Field(key) => (Some(key.clone()), None),
        Step::Pointer(key) => (Some(key.clone()), pointer_index(key)),
        Step::Index(index) => (None, Some(*index)),
        _ => unreachable!(),
    };
    let mut branches = vec![];
    if let Some(key) = key {
        let selected = format!("{entries}.key={}", string(&key));
        let old = format!("{entries}.value");
        let new = if leaf {
            replacement.into()
        } else {
            duck_mutate(&old, &steps[1..], replacement, operation, next)
        };
        let value = if leaf && matches!(operation, "insert" | "remove") {
            old.clone()
        } else {
            format!("CASE WHEN {selected} THEN {new} ELSE {old} END")
        };
        let filter = if leaf && operation == "remove" {
            format!("WHERE NOT ({selected})")
        } else {
            "".into()
        };
        let extra = if leaf && matches!(operation, "set" | "insert") {
            format!(
                " UNION ALL SELECT {} AS key,{replacement} AS value WHERE NOT EXISTS (SELECT 1 FROM json_each({d}) AS {entries} WHERE {selected})",
                string(&key)
            )
        } else {
            "".into()
        };
        let aggregate_alias = format!("__local{}", *next);
        *next += 1;
        let object = format!(
            "(SELECT CAST('{{' || coalesce(string_agg(CAST(to_json(key) AS VARCHAR)||':'||CAST(value AS VARCHAR),','),'') || '}}' AS JSON) FROM (SELECT {entries}.key,{value} AS value FROM json_each({d}) AS {entries} {filter}{extra}) AS {aggregate_alias})"
        );
        branches.push(format!("WHEN json_type({d})='OBJECT' THEN {object}"));
    }
    if let Some(index) = index {
        let length = format!("CAST(json_array_length({d}) AS BIGINT)");
        let index = if index < 0 {
            format!("{length}+({index})")
        } else {
            index.to_string()
        };
        let selected = format!("CAST({entries}.key AS BIGINT)=({index})");
        let old = format!("{entries}.value");
        let new = if leaf {
            replacement.into()
        } else {
            duck_mutate(&old, &steps[1..], replacement, operation, next)
        };
        let value = if leaf && matches!(operation, "insert" | "remove") {
            old.clone()
        } else {
            format!("CASE WHEN {selected} THEN {new} ELSE {old} END")
        };
        let filter = if leaf && operation == "remove" {
            format!("WHERE NOT ({selected})")
        } else {
            "".into()
        };
        let extra = if leaf && matches!(operation, "set" | "insert") {
            let condition = if operation == "set" {
                format!("WHERE ({index})<0 OR ({index})>={length}")
            } else {
                "".into()
            };
            format!(
                " UNION ALL SELECT 2*greatest(0,least({length},({index}))) AS ordinal,{replacement} AS value {condition}"
            )
        } else {
            "".into()
        };
        let aggregate_alias = format!("__local{}", *next);
        *next += 1;
        let array = format!(
            "(SELECT CAST('[' || coalesce(string_agg(CAST(value AS VARCHAR),',' ORDER BY ordinal),'') || ']' AS JSON) FROM (SELECT 2*CAST({entries}.key AS BIGINT)+1 AS ordinal,{value} AS value FROM json_each({d}) AS {entries} {filter}{extra}) AS {aggregate_alias})"
        );
        branches.push(format!("WHEN json_type({d})='ARRAY' THEN {array}"));
    }
    format!(
        "(SELECT CASE {} ELSE {d} END FROM (SELECT {document} AS d) AS {alias})",
        branches.join(" ")
    )
}
fn pointer_index(key: &str) -> Option<i64> {
    if key == "0" || (!key.starts_with('0') && key.bytes().all(|b| b.is_ascii_digit())) {
        key.parse().ok()
    } else {
        None
    }
}
fn pg_path(steps: &[Step]) -> String {
    let fields = steps
        .iter()
        .map(|step| {
            string(&match step {
                Step::Field(key) | Step::Pointer(key) => key.clone(),
                Step::Index(index) => index
                    .clamp(&(i32::MIN as i64), &(i32::MAX as i64))
                    .to_string(),
                _ => unreachable!(),
            })
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("CAST(ARRAY[{fields}] AS TEXT[])")
}
fn mutation_container(parent: &str, step: &Step) -> String {
    match step {
        Step::Field(_) => format!("jsonb_typeof({parent})='object'"),
        Step::Index(_) => format!("jsonb_typeof({parent})='array'"),
        Step::Pointer(key) if pointer_index(key).is_some() => {
            format!("jsonb_typeof({parent}) IN ('object','array')")
        }
        Step::Pointer(_) => format!("jsonb_typeof({parent})='object'"),
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use datafusion::sql::sqlparser::parser::Parser;
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    fn expr(sql: &str, dialect: SqlDialect) -> ast::Expr {
        Parser::new(dialect.parser_dialect().as_ref())
            .try_with_sql(sql)
            .unwrap()
            .parse_expr()
            .unwrap()
    }
    fn render(name: &str, args: &[&str], dialect: SqlDialect) -> String {
        let args = args.iter().map(|s| expr(s, dialect)).collect::<Vec<_>>();
        lower(&format!("__orchiddb_json_{name}"), &args, dialect)
            .unwrap()
            .unwrap()
            .to_string()
    }
    fn execute(sql: &str, pg: bool) -> Result<String, String> {
        let mut command = if pg {
            let mut c = Command::new("psql");
            c.arg(std::env::var("GRAPH_PG_URL").unwrap_or_else(|_| "dbname=postgres".into()));
            c.args(["-X", "-A", "-t", "-v", "ON_ERROR_STOP=1"]);
            c
        } else {
            let mut c = Command::new(
                std::env::var("ORCHIDDB_DUCKDB_BIN").unwrap_or_else(|_| "duckdb".into()),
            );
            c.args(["-list", "-noheader"]);
            c
        };
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(sql.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().into())
        } else {
            Err(String::from_utf8_lossy(&output.stderr).into())
        }
    }
    #[test]
    fn render_json_families() {
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            for (name, args) in [
                ("parse", vec!["text_column"]),
                ("query", vec!["doc", "'$.a[0]'"]),
                ("value", vec!["doc", "'$.a'", "'int64'"]),
                ("type", vec!["doc"]),
                ("keys", vec!["doc"]),
                ("array_length", vec!["doc"]),
                ("object", vec!["'a'", "1"]),
                ("array", vec!["1", "NULL"]),
                ("equals", vec!["doc", "other"]),
                ("contains", vec!["doc", "other"]),
            ] {
                let sql = render(name, &args, dialect);
                assert!(!sql.contains("__arg"), "{sql}");
            }
        }
    }
    #[test]
    #[ignore = "requires PostgreSQL and DuckDB 1.5.2 CLI"]
    fn live_json_scalar_semantics() {
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let pg = dialect == SqlDialect::Postgres;
            let ty = if pg { "JSONB" } else { "JSON" };
            let cases = [
                ("[1]", "1", true, false),
                ("[[1]]", "[1]", false, false),
                ("{\"a\":[1]}", "{\"a\":1}", false, false),
                ("{\"x\":{\"a\":1}}", "{\"a\":1}", false, false),
                ("{\"a\":1,\"a\":2}", "{\"a\":2}", true, true),
                ("{\"a\":{\"x\":0},\"a\":2}", "{\"a\":2}", true, true),
                ("[1,2]", "[2,1]", true, false),
                ("[1,1]", "[1]", true, false),
                ("1", "1.0", true, true),
                ("9007199254740993", "9007199254740992", false, false),
                ("{\"b\":2,\"a\":1}", "{\"a\":1,\"b\":2}", true, true),
            ];
            for (left, right, contains, equals) in cases {
                for (name, expected) in [("contains", contains), ("equals", equals)] {
                    let expression = render(name, &["left_doc", "right_doc"], dialect);
                    let sql = format!(
                        "SELECT CASE WHEN {expression} THEN 'yes' ELSE 'no' END FROM (SELECT CAST({} AS {ty}) left_doc,CAST({} AS {ty}) right_doc) docs;",
                        string(left),
                        string(right)
                    );
                    assert_eq!(
                        execute(&sql, pg)
                            .unwrap_or_else(|e| panic!("{dialect:?} {name} {left} {right}: {e}")),
                        if expected { "yes" } else { "no" },
                        "{dialect:?} {name} {left} {right}"
                    );
                }
            }
            for (doc, path, expected) in [
                ("{\"a\":1,\"a\":2}", "$.a", "2"),
                ("[7]", "$['0']", "missing"),
                ("{\"0\":8}", "$[0]", "missing"),
                ("[7]", "/0", "7"),
                ("{\"0\":8}", "/0", "8"),
            ] {
                let expression = render("value", &["doc", &string(path)], dialect);
                let sql = format!(
                    "SELECT coalesce({expression},'missing') FROM (SELECT CAST({} AS {ty}) doc) docs;",
                    string(doc)
                );
                assert_eq!(
                    execute(&sql, pg).unwrap(),
                    expected,
                    "{dialect:?} {doc} {path}"
                );
            }
            for doc in ["{\"n\":\"7\"}", "{\"n\":1.5}"] {
                let expression = render("value", &["doc", "'$.n'", "'int64'"], dialect);
                let sql = format!(
                    "SELECT {expression} FROM (SELECT CAST({} AS {ty}) doc) docs;",
                    string(doc)
                );
                assert!(execute(&sql, pg).is_err(), "{dialect:?} must reject {doc}");
            }
        }
    }
    #[test]
    #[ignore = "requires DuckDB 1.5.2 CLI"]
    fn live_duck_strict_parse() {
        for text in ["NaN", "Infinity", "-Infinity", "[1,]", "{\"x\":1,}"] {
            let valid = render("valid", &["txt"], SqlDialect::DuckDb);
            assert_eq!(
                execute(
                    &format!("SELECT {valid} FROM (SELECT {} txt) s;", string(text)),
                    false
                )
                .unwrap(),
                "false"
            );
            let parse = render("parse", &["txt"], SqlDialect::DuckDb);
            assert!(
                execute(
                    &format!("SELECT {parse} FROM (SELECT {} txt) s;", string(text)),
                    false
                )
                .is_err()
            );
        }
        let valid = render("valid", &["txt"], SqlDialect::DuckDb);
        assert_eq!(
            execute(
                &format!("SELECT {valid} FROM (SELECT '\"NaN,]Infinity\"' txt) s;"),
                false
            )
            .unwrap(),
            "true"
        );
    }
    #[test]
    #[ignore = "requires PostgreSQL and DuckDB 1.5.2 CLI"]
    fn live_json_aggregates_and_mutations() {
        for dialect in [SqlDialect::DuckDb, SqlDialect::Postgres] {
            let pg = dialect == SqlDialect::Postgres;
            for (call, expected) in [
                (
                    "__orchiddb_json_array_agg(v ORDER BY i DESC) FILTER (WHERE i>0)",
                    serde_json::json!([2, null, 1]),
                ),
                (
                    "__orchiddb_json_object_agg(k,v ORDER BY i) FILTER (WHERE i>0)",
                    serde_json::json!({"a":2,"b":null}),
                ),
                (
                    "__orchiddb_json_array_agg(DISTINCT v)",
                    serde_json::json!([1, 2, null]),
                ),
            ] {
                let ast::Expr::Function(call) = expr(call, dialect) else {
                    panic!()
                };
                let lowered = lower_function(&call, dialect).unwrap().unwrap();
                assert!(lowered.to_string().contains(if call.filter.is_some() {
                    "FILTER"
                } else {
                    "DISTINCT"
                }));
                let sql = format!(
                    "SELECT {lowered} FROM (VALUES (1,'a',1),(2,'b',CAST(NULL AS INTEGER)),(3,'a',2)) t(i,k,v);"
                );
                let actual: serde_json::Value =
                    serde_json::from_str(&execute(&sql, pg).unwrap()).unwrap();
                if call.to_string().contains("DISTINCT") {
                    assert_eq!(actual.as_array().unwrap().len(), 3);
                    assert!(
                        actual
                            .as_array()
                            .unwrap()
                            .contains(&serde_json::Value::Null)
                    );
                } else {
                    assert_eq!(actual, expected, "{dialect:?}");
                }
                let empty = format!("SELECT {lowered} FROM (SELECT 1 i,'a' k,1 v WHERE false) t;");
                assert_eq!(execute(&empty, pg).unwrap(), if pg { "" } else { "NULL" });
            }
            let ty = if pg { "JSONB" } else { "JSON" };
            for (left, right, expected) in [
                (
                    "{\"a\":{\"b\":1,\"c\":2},\"x\":1}",
                    "{\"a\":{\"b\":null,\"d\":3},\"x\":[]}",
                    serde_json::json!({"a":{"c":2,"d":3},"x":[]}),
                ),
                ("[1]", "{\"a\":{\"b\":2}}", serde_json::json!({"a":{"b":2}})),
                ("{}", "null", serde_json::Value::Null),
            ] {
                let lowered = render("merge_patch", &["a", "b"], dialect);
                let sql = format!(
                    "SELECT {lowered} FROM (SELECT CAST({} AS {ty}) a,CAST({} AS {ty}) b) t;",
                    string(left),
                    string(right)
                );
                assert_eq!(
                    serde_json::from_str::<serde_json::Value>(&execute(&sql, pg).unwrap()).unwrap(),
                    expected,
                    "{dialect:?}"
                );
            }
            {
                for (op, doc, path, value, expected) in [
                    (
                        "set",
                        "{\"a\":1}",
                        "$.a",
                        "'hello'",
                        serde_json::json!({"a":"hello"}),
                    ),
                    ("set", "[1]", "$['0']", "2", serde_json::json!([1])),
                    (
                        "insert",
                        "{\"a\":1}",
                        "$.a",
                        "2",
                        serde_json::json!({"a":1}),
                    ),
                    (
                        "replace",
                        "{\"a\":1}",
                        "$.b",
                        "2",
                        serde_json::json!({"a":1}),
                    ),
                    ("set", "[1]", "$[9]", "2", serde_json::json!([1, 2])),
                ] {
                    let lowered = render(op, &["doc", &string(path), value], dialect);
                    let sql = format!(
                        "SELECT {lowered} FROM (SELECT CAST({} AS {ty}) doc) t;",
                        string(doc)
                    );
                    assert_eq!(
                        serde_json::from_str::<serde_json::Value>(&execute(&sql, pg).unwrap())
                            .unwrap(),
                        expected,
                        "{op} {doc} {path}"
                    );
                }
            }
        }
    }
}
