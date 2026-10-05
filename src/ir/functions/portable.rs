//! The complete native scalar catalog, with explicit portable SQL capabilities.
//!
//! `fn.<name>` avoids collisions with Cypher/SPARQL semantics and backend
//! catalogs. Aliases resolve to the same UDF. Unmapped calls still have a typed
//! native implementation; SQL-only compilation rejects them rather than guessing.
mod arrays;
mod audit;
pub use audit::audit_note;
mod graphemes;
mod regex;
mod sha2;
mod unicode;
pub(crate) use unicode::overlay as native_overlay;
mod scalars;
pub(crate) mod structured;
mod support;
pub use support::{literal_issue, validate_call};

use super::logical::{LogicalFunction, SqlFunctionMapping};
use datafusion::logical_expr::ScalarUDF;
use std::{
    collections::BTreeMap,
    sync::{Arc, LazyLock},
};

#[derive(Debug, Clone)]
pub struct ScalarCapability {
    pub name: String,
    pub aliases: Vec<String>,
    /// Schema/query-state resolution or cast lowering rather than a runtime SQL UDF.
    pub preparation: bool,
    pub volatility: datafusion::logical_expr::Volatility,
    pub duckdb: bool,
    pub postgres: bool,
    /// Reviewed scope or the concrete reason SQL execution remains native.
    pub duckdb_note: &'static str,
    pub postgres_note: &'static str,
}

struct Catalog {
    functions: BTreeMap<String, Arc<ScalarUDF>>,
    capabilities: Vec<ScalarCapability>,
}

static CATALOG: LazyLock<Catalog> = LazyLock::new(|| {
    let mut functions = BTreeMap::new();
    let mut capabilities = Vec::new();
    for native in datafusion::functions::all_default_functions()
        .into_iter()
        .chain(datafusion::functions_nested::all_default_nested_functions())
    {
        let name = format!("fn.{}", native.name());
        let aliases = native
            .aliases()
            .iter()
            .map(|a| format!("fn.{a}"))
            .collect::<Vec<_>>();
        let sql = ["duckdb", "postgres"]
            .into_iter()
            .filter_map(|dialect| {
                mapping(native.name(), dialect).map(|value| {
                    (
                        dialect.to_owned(),
                        SqlFunctionMapping {
                            value,
                            ordering: None,
                        },
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        let mut definition = LogicalFunction::new(&name, native.clone(), sql);
        definition.portable_builtin = true;
        for dialect in ["duckdb", "postgres"] {
            for arity in 0..=5 {
                if let Some(value) = overload(native.name(), dialect, arity) {
                    definition.sql_overloads.insert(
                        (dialect.into(), arity),
                        SqlFunctionMapping {
                            value,
                            ordering: None,
                        },
                    );
                }
            }
        }
        capabilities.push(ScalarCapability {
            name: name.clone(),
            preparation: resolves_in_preparation(native.name()),
            duckdb_note: audit_note(native.name(), "duckdb"),
            postgres_note: audit_note(native.name(), "postgres"),
            aliases: aliases.clone(),
            volatility: native.signature().volatility,
            duckdb: structured::handles(native.name())
                || resolves_in_preparation(native.name())
                || definition.sql.contains_key("duckdb")
                || definition.sql_overloads.keys().any(|(d, _)| d == "duckdb"),
            postgres: structured::handles(native.name())
                || resolves_in_preparation(native.name())
                || definition.sql.contains_key("postgres")
                || definition
                    .sql_overloads
                    .keys()
                    .any(|(d, _)| d == "postgres"),
        });
        let udf = definition.into_udf();
        for alias in std::iter::once(name).chain(aliases) {
            // Match the ordinary native catalog's first-match alias precedence.
            functions.entry(alias).or_insert_with(|| udf.clone());
        }
    }
    capabilities.sort_by(|a, b| a.name.cmp(&b.name));
    Catalog {
        functions,
        capabilities,
    }
});

pub fn function(name: &str) -> Option<Arc<ScalarUDF>> {
    // Do not initialize the catalog on ordinary engine function lookups.
    if !name
        .get(..3)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("fn."))
    {
        return None;
    }
    CATALOG.functions.get(&name.to_ascii_lowercase()).cloned()
}

/// Each registered scalar has native execution. Engine flags indicate at least
/// one supported SQL call or compilation lowering; validate each call separately.
pub fn capabilities() -> &'static [ScalarCapability] {
    &CATALOG.capabilities
}

fn unary(body: &str) -> String {
    // Bind once: templates must not duplicate volatile argument expressions.
    format!("(SELECT {body} FROM (SELECT __arg0 AS __local1 OFFSET 0) AS __local0)")
}

fn mapping(name: &str, dialect: &str) -> Option<String> {
    let pg = dialect == "postgres";
    let unary = |body: &str| bind_unary(body, pg);
    let double = if pg { "DOUBLE PRECISION" } else { "DOUBLE" };
    let nan = format!("CAST('NaN' AS {double})");
    let inf = format!("CAST('Infinity' AS {double})");
    let negative_inf = format!("CAST('-Infinity' AS {double})");
    let is_nan = if pg {
        format!("__local1 = {nan}")
    } else {
        "isnan(__local1)".into()
    };
    Some(match name {
        "abs" | "ceil" | "floor" | "atan" | "cbrt" | "asinh" | "tanh" => format!("{name}(__arg0)"),
        "atan2" => "atan2(__arg0, __arg1)".into(),
        "pi" => "pi()".into(),
        "isnan" => unary(&is_nan),
        "iszero" => "__arg0 = 0".into(),
        "signum" => unary(&format!("CASE WHEN {is_nan} THEN __local1 ELSE sign(__local1) END")),
        "sqrt" => unary(&format!("CASE WHEN __local1 < 0 THEN {nan} ELSE sqrt(__local1) END")),
        "ln" | "log2" | "log10" => {
            let value = match name { "log2" => "ln(__local1) / ln(2.0)", "log10" => if pg { "log(__local1)" } else { "log10(__local1)" }, _ => "ln(__local1)" };
            unary(&format!("CASE WHEN __local1 < 0 THEN {nan} WHEN __local1 = 0 THEN {negative_inf} ELSE {value} END"))
        }
        "acos" | "asin" => unary(&format!("CASE WHEN __local1 < -1 OR __local1 > 1 THEN {nan} ELSE {name}(__local1) END")),
        "acosh" => unary(&format!("CASE WHEN __local1 < 1 THEN {nan} ELSE acosh(__local1) END")),
        "atanh" => unary(&format!("CASE WHEN __local1 < -1 OR __local1 > 1 THEN {nan} WHEN __local1 = -1 THEN {negative_inf} WHEN __local1 = 1 THEN {inf} ELSE atanh(__local1) END")),
        "sin" | "cos" | "tan" => unary(&format!("CASE WHEN __local1 IN ({inf}, {negative_inf}) THEN {nan} ELSE {name}(__local1) END")),
        "nanvl" if !pg => "list_transform(ARRAY[ARRAY[__arg0, __arg1]], lambda __local0: CASE WHEN isnan(__local0[1]) THEN __local0[2] ELSE __local0[1] END)[1]".into(),
        "nanvl" => format!("(SELECT CASE WHEN {is_nan} THEN __local2 ELSE __local1 END FROM (SELECT __arg0 AS __local1, __arg1 AS __local2 OFFSET 0) AS __local0)"),
        "contains" => "strpos(__arg0, __arg1) > 0".into(),
        "starts_with" => "starts_with(__arg0, __arg1)".into(),
        "ends_with" if !pg => "ends_with(__arg0, __arg1)".into(),
        "ends_with" => "(SELECT right(__local1, length(__local2)) = __local2 FROM (SELECT __arg0 AS __local1, __arg1 AS __local2 OFFSET 0) AS __local0)".into(),
        "character_length" => "CAST(length(__arg0) AS BIGINT)".into(),
        "octet_length" if pg => "CAST(octet_length(__arg0) AS BIGINT)".into(),
        "octet_length" => "CAST(octet_length(encode(__arg0)) AS BIGINT)".into(),
        "bit_length" => "CAST(bit_length(__arg0) AS BIGINT)".into(),
        "strpos" => "CAST(strpos(__arg0, __arg1) AS BIGINT)".into(),
        "replace" => "replace(__arg0, __arg1, __arg2)".into(),
        "reverse" if pg => "reverse(__arg0)".into(),
        "reverse" => "array_to_string(list_reverse(string_split(__arg0, '')), '')".into(),
        "ascii" => unary(if pg { "CASE WHEN __local1 = '' THEN 0 ELSE ascii(__local1) END" } else { "CASE WHEN __local1 = '' THEN 0 ELSE unicode(__local1) END" }),
        "coalesce" | "concat" | "concat_ws" => format!("{name}(__args)"),
        "nullif" => "nullif(__arg0, __arg1)".into(),
        "nvl" => "coalesce(__arg0, __arg1)".into(),
        "md5" => "md5(__arg0)".into(),
        "uuid" => if pg { "CAST(gen_random_uuid() AS TEXT)" } else { "CAST(uuid() AS VARCHAR)" }.into(),
        _ => return scalars::mapping(name, pg).or_else(|| arrays::mapping(name, pg)).or_else(|| unicode::mapping(name, pg)).or_else(|| graphemes::mapping(name, pg)).or_else(|| sha2::mapping(name, pg)),
    })
}

fn overload(name: &str, dialect: &str, arity: usize) -> Option<String> {
    Some(match (name, arity) {
        ("concat", 0) => "''".into(),
        ("btrim" | "ltrim" | "rtrim", 1 | 2) => {
            let name = if name == "btrim" && dialect == "duckdb" {
                "trim"
            } else {
                name
            };
            format!(
                "{name}({})",
                (0..arity)
                    .map(|i| format!("__arg{i}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        ("log", 1) => mapping("log10", dialect)?,
        ("log", 2) => {
            let double = if dialect == "postgres" {
                "DOUBLE PRECISION"
            } else {
                "DOUBLE"
            };
            let nan = format!("CAST('NaN' AS {double})");
            let inf = format!("CAST('Infinity' AS {double})");
            let logarithm = |x: &str| {
                format!("CASE WHEN {x} < 0 THEN {nan} WHEN {x} = 0 THEN -{inf} ELSE ln({x}) END")
            };
            bind(
                2,
                &format!(
                    "(SELECT CASE WHEN __local3 = 0 THEN CASE WHEN __local4 = 0 THEN {nan} WHEN __local4 < 0 THEN -{inf} ELSE __local4 * {inf} END ELSE __local4 / __local3 END FROM (SELECT {} AS __local3, {} AS __local4 OFFSET 0) AS __local5)",
                    logarithm("__local1"),
                    logarithm("__local2")
                ),
            )
        }

        _ => {
            return graphemes::overload(name, dialect == "postgres", arity)
                .or_else(|| scalars::overload(name, dialect == "postgres", arity))
                .or_else(|| arrays::overload(name, dialect == "postgres", arity))
                .or_else(|| regex::overload(name, dialect == "postgres", arity))
                .or_else(|| unicode::overload(name, dialect == "postgres", arity));
        }
    })
}

fn bind(n: usize, body: &str) -> String {
    let args = (0..n)
        .map(|i| format!("__arg{i} AS __local{}", i + 1))
        .collect::<Vec<_>>()
        .join(", ");
    format!("(SELECT {body} FROM (SELECT {args} OFFSET 0) AS __local0)")
}

fn bind_unary(body: &str, pg: bool) -> String {
    if pg {
        unary(body)
    } else {
        // A correlated scalar subquery can decorrelate by argument equality,
        // which merges +0 and -0. A singleton-list lambda binds once per row
        // while preserving the float's actual bits.
        format!("list_transform(ARRAY[__arg0], lambda __local1: {body})[1]")
    }
}

/// These functions inspect Arrow schema/query state and disappear before SQL
/// execution; mapping them to database introspection would change their meaning.
pub fn resolves_in_preparation(name: &str) -> bool {
    matches!(
        name.strip_prefix("fn.").unwrap_or(name),
        "arrow_cast"
            | "arrow_typeof"
            | "arrow_metadata"
            | "version"
            | "current_date"
            | "current_time"
            | "now"
    )
}
