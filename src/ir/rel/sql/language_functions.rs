//! Connection-local registration; no loadable extension or global switch.
use super::super::language_functions as kernels;
use duckdb::{
    Connection,
    core::{DataChunkHandle, LogicalTypeId},
    vscalar::{ScalarFunctionSignature, VScalar},
    vtab::arrow::{WritableVector, write_arrow_array_to_vector},
};

macro_rules! function {
    ($name:ident, $sql:literal, $result:expr, $args:expr) => {
        struct $name;
        impl VScalar for $name {
            type State = ();
            fn signatures() -> Vec<ScalarFunctionSignature> {
                vec![ScalarFunctionSignature::exact($args, $result)]
            }
            unsafe fn invoke(
                _: &(),
                input: &mut DataChunkHandle,
                output: &mut dyn WritableVector,
            ) -> Result<(), Box<dyn std::error::Error>> {
                // Never unwind through DuckDB's C callback boundary.
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let arrays = super::language_vectors::arrays(input)?;
                    let result = kernels::evaluate($sql, &arrays, input.len())?;
                    write_arrow_array_to_vector(&result, output)
                })) {
                    Ok(result) => result,
                    Err(_) => Err("language function panicked".into()),
                }
            }
        }
    };
}
function!(CypherToFloat, "__orchiddb_cypher_to_float_v1", LogicalTypeId::Double.into(), vec![LogicalTypeId::Varchar.into()]);

// Keep weak session identities so closing a connection cannot authorize a new
// connection that happens to reuse its address. OIDs are re-read each query;
// replacement/rollback and explicit opt-out never reuse a cached capability.
type Witness = (std::sync::Weak<duckdb::InterruptHandle>, String);
fn witnesses() -> &'static std::sync::Mutex<Vec<Witness>> {
    static WITNESSES: std::sync::OnceLock<std::sync::Mutex<Vec<Witness>>> = std::sync::OnceLock::new();
    WITNESSES.get_or_init(Default::default)
}
fn fingerprint(connection: &Connection) -> duckdb::Result<Option<String>> {
    let mut query = connection.prepare("SELECT database_name, schema_name, function_oid::VARCHAR FROM duckdb_functions() WHERE function_name = '__orchiddb_cypher_to_float_v1' AND function_type = 'scalar' AND parameter_types = ['VARCHAR'] AND return_type = 'DOUBLE' AND varargs IS NULL AND NOT has_side_effects AND stability = 'CONSISTENT'")?;
    let rows = query.query_map([], |r| Ok(format!("{}:{}:{}", r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?.collect::<duckdb::Result<Vec<_>>>()?;
    // Multiple visible registrations can change name resolution. Do not guess.
    let count: i64 = connection.query_row("SELECT count(*) FROM duckdb_functions() WHERE function_name = '__orchiddb_cypher_to_float_v1'", [], |r| r.get(0))?;
    Ok(if count == 1 && rows.len() == 1 { rows.into_iter().next() } else { None })
}
/// Read-only capability verification. Unknown same-named functions are never
/// called or trusted. A borrowed/cloned session without an installation witness
/// conservatively uses conformant SQL, even if another session installed it.
pub(crate) fn float_capability(connection: &Connection, enabled: bool) -> duckdb::Result<Option<kernels::FloatHelperCapability>> {
    if !enabled { return Ok(None); }
    let Some(fingerprint) = fingerprint(connection)? else { return Ok(None); };
    let handle = connection.interrupt_handle();
    let mut registry = witnesses().lock().unwrap_or_else(|e| e.into_inner());
    registry.retain(|(weak, _)| weak.strong_count() > 0);
    Ok(registry.iter().any(|(weak, identity)| weak.upgrade().is_some_and(|h| std::sync::Arc::ptr_eq(&h, &handle)) && identity == &fingerprint)
        .then(|| kernels::FloatHelperCapability::verified(format!("duckdb:{:p}:{fingerprint}", std::sync::Arc::as_ptr(&handle)))))
}
fn register_float(connection: &Connection) -> duckdb::Result<()> {
    let exists: bool = connection.query_row("SELECT count(*) > 0 FROM duckdb_functions() WHERE function_name = '__orchiddb_cypher_to_float_v1'", [], |r| r.get(0))?;
    if !exists {
        connection.register_scalar_function::<CypherToFloat>(kernels::CYPHER_TO_FLOAT)?;
        if let Some(identity) = fingerprint(connection)? {
            let mut registry = witnesses().lock().unwrap_or_else(|e| e.into_inner());
            registry.retain(|(weak, _)| weak.strong_count() > 0);
            registry.push((std::sync::Arc::downgrade(&connection.interrupt_handle()), identity));
        }
    }
    Ok(())
}

function!(ListCompare, "__orchiddb_cypher_list_compare", LogicalTypeId::Boolean.into(),
    vec![LogicalTypeId::Varchar.into(), LogicalTypeId::Any.into(), LogicalTypeId::Any.into()]);

struct CollectDistinct;
impl VScalar for CollectDistinct {
    type State = ();
    fn signatures() -> Vec<ScalarFunctionSignature> {
        [LogicalTypeId::Boolean, LogicalTypeId::Tinyint, LogicalTypeId::Smallint,
         LogicalTypeId::Integer, LogicalTypeId::Bigint, LogicalTypeId::UTinyint,
         LogicalTypeId::USmallint, LogicalTypeId::UInteger, LogicalTypeId::UBigint,
         LogicalTypeId::Float, LogicalTypeId::Double, LogicalTypeId::Varchar].into_iter().map(|ty| {
            let ty: duckdb::core::LogicalTypeHandle = ty.into();
            ScalarFunctionSignature::exact(vec![duckdb::core::LogicalTypeHandle::list(&ty)],
                duckdb::core::LogicalTypeHandle::list(&ty))
         }).collect()
    }
    unsafe fn invoke(_: &(), input: &mut DataChunkHandle, output: &mut dyn WritableVector) -> Result<(), Box<dyn std::error::Error>> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let arrays = super::language_vectors::arrays(input)?;
            let result = kernels::evaluate(kernels::COLLECT_DISTINCT, &arrays, input.len())?;
            write_arrow_array_to_vector(&result, output)
        })) { Ok(result) => result, Err(_) => Err("distinct collection helper panicked".into()) }
    }
}
function!(NodeText, "__orchiddb_cypher_node_text", LogicalTypeId::Varchar.into(),
    vec![LogicalTypeId::Varchar.into(), LogicalTypeId::Varchar.into(), LogicalTypeId::Varchar.into(), LogicalTypeId::Any.into()]);
fn arguments() -> Vec<duckdb::core::LogicalTypeHandle> {
    vec![LogicalTypeId::Varchar.into(), LogicalTypeId::Any.into()]
}
function!(
    Key,
    "__orchiddb_lang_cypher_key",
    LogicalTypeId::Blob.into(),
    vec![LogicalTypeId::Any.into()]
);
function!(
    Text,
    "__orchiddb_lang_text",
    LogicalTypeId::Varchar.into(),
    arguments()
);
function!(
    Texts,
    "__orchiddb_lang_texts",
    duckdb::core::LogicalTypeHandle::list(&LogicalTypeId::Varchar.into()),
    arguments()
);
function!(
    Int,
    "__orchiddb_lang_int",
    LogicalTypeId::Integer.into(),
    arguments()
);
function!(
    Ints,
    "__orchiddb_lang_ints",
    duckdb::core::LogicalTypeHandle::list(&LogicalTypeId::Integer.into()),
    arguments()
);

/// Register the pure Cypher, Gremlin and SPARQL language functions. Applications
/// compiling SQL themselves must also opt in via RelBackendOptions.
/// Registration is idempotent on connections sharing a DuckDB catalog.
pub fn register(connection: &Connection) -> duckdb::Result<()> {
    super::native_language_functions::register(connection)?;
    super::typed_language_functions::register(connection)?;
    register_float(connection)?;
    // Bind null calls without evaluating kernels. This is much cheaper than
    // enumerating DuckDB's full function catalog on every borrowed connection.
    // A missing function is a binder error; it does not abort a transaction.
    if connection.prepare("SELECT __orchiddb_cypher_list_compare('gte', [1], [1]), __orchiddb_cypher_collect_distinct(CAST(NULL AS BIGINT[])), __orchiddb_cypher_node_text(NULL, NULL, NULL, NULL), __orchiddb_lang_cypher_key(NULL), __orchiddb_lang_text(NULL, NULL), __orchiddb_lang_texts(NULL, NULL), __orchiddb_lang_int(NULL, NULL), __orchiddb_lang_ints(NULL, NULL), __orchiddb_lang_sparql(NULL, NULL, NULL, NULL, NULL) WHERE false").is_ok() {
        return Ok(());
    }
    fn present(c: &Connection, name: &str) -> duckdb::Result<bool> {
        c.query_row(
            "SELECT count(*) > 0 FROM duckdb_functions() WHERE function_name = ?",
            [name],
            |r| r.get(0),
        )
    }
    macro_rules! add {
        ($ty:ty, $name:expr) => {
            if !present(connection, $name)? {
                connection.register_scalar_function::<$ty>($name)?;
            }
        };
    }
    add!(Key, kernels::KEY);
    add!(NodeText, kernels::NODE_TEXT);
    add!(CollectDistinct, kernels::COLLECT_DISTINCT);
    add!(ListCompare, kernels::LIST_COMPARE);
    add!(Text, "__orchiddb_lang_text");
    add!(Texts, "__orchiddb_lang_texts");
    add!(Int, "__orchiddb_lang_int");
    add!(Ints, "__orchiddb_lang_ints");
    Ok(())
}

#[cfg(test)]
mod float_tests {
    use super::*;

    #[test]
    fn typed_nullable_helper_matches_reference_and_verifies_registration() {
        let connection = Connection::open_in_memory().unwrap();
        assert!(float_capability(&connection, true).unwrap().is_none());
        register(&connection).unwrap();
        let capability = float_capability(&connection, true).unwrap().unwrap();
        assert_eq!(capability.operation(), "cypher.to_float");
        assert_eq!(capability.signature(), "varchar->double");
        assert_eq!(capability.contract_version(), 1);
        assert!(float_capability(&connection, false).unwrap().is_none());
        connection.execute_batch("CREATE TABLE input(raw VARCHAR)").unwrap();
        let inputs = [None, Some(""), Some("foo"), Some("1.25"), Some("-2e2"),
            Some("__1__2__"), Some("\u{a0}1.5\u{3000}"), Some("NaN"), Some("Infinity"),
            Some("1e999"), Some("1e-999"), Some("-0"), Some("1 2"), Some("0x10")];
        for input in inputs { connection.execute("INSERT INTO input VALUES (?)", [input]).unwrap(); }
        let mut query = connection.prepare("SELECT __orchiddb_cypher_to_float_v1(raw) FROM input").unwrap();
        let actual = query.query_map([], |r| r.get::<_, Option<f64>>(0)).unwrap().collect::<duckdb::Result<Vec<_>>>().unwrap();
        let expected: Vec<_> = inputs.into_iter().map(|s| s.and_then(|s| crate::ir::functions::cypher_float::parse_string(s).ok())).collect();
        assert_eq!(actual.iter().map(|v| v.map(f64::to_bits)).collect::<Vec<_>>(), expected.iter().map(|v| v.map(f64::to_bits)).collect::<Vec<_>>());
        // A clone can use the function, but must not reuse another session's
        // witness or cached plan. It safely chooses the helper-free alternative.
        let clone = connection.try_clone().unwrap();
        assert!(float_capability(&clone, true).unwrap().is_none());
        register(&connection).unwrap();
        assert_eq!(float_capability(&connection, true).unwrap().unwrap(), capability);
    }

    #[test]
    fn mismatched_or_untrusted_registration_never_authorizes_helper() {
        for definition in [
            "CREATE MACRO __orchiddb_cypher_to_float_v1(x) AS TRY_CAST(x AS DOUBLE)",
            "CREATE MACRO __orchiddb_cypher_to_float_v1(x) AS 7",
        ] {
            let connection = Connection::open_in_memory().unwrap();
            connection.execute_batch(definition).unwrap();
            register(&connection).unwrap();
            assert!(float_capability(&connection, true).unwrap().is_none());
        }
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("BEGIN").unwrap();
        register(&connection).unwrap();
        assert!(float_capability(&connection, true).unwrap().is_some());
        connection.execute_batch("ROLLBACK").unwrap();
        assert!(float_capability(&connection, true).unwrap().is_none());
    }
    #[tokio::test]
    async fn mapped_helper_and_expansion_share_the_selected_contract() {
        use crate::ir::rel::{RelBackend, RelBackendOptions, mapping::{GraphMapping, NodeMapping}};
        use crate::ir::catalog::PropertyGraph;
        use arrow::datatypes::{Field, Schema, DataType};
        use std::{collections::BTreeMap, sync::Arc};
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        connection.execute_batch("CREATE TABLE conversion_values (id BIGINT, raw VARCHAR)").unwrap();
        let inputs = [None, Some(""), Some("foo"), Some("2.9"), Some("__1__2__"), Some("NaN"), Some("Infinity"), Some("1e999"), Some("1e-999"), Some("\u{a0}1.5\u{3000}")];
        for (id, input) in inputs.iter().enumerate() {
            connection.execute("INSERT INTO conversion_values VALUES (?, ?)", duckdb::params![id as i64, input]).unwrap();
        }
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema("conversion_values", Arc::new(Schema::new(vec![Field::new("id", DataType::Int64, false), Field::new("raw", DataType::Utf8, true)])));
        mapping.map_node(NodeMapping::table("Conversion", "conversion_values", "id").property("raw", "raw").property("id", "id"));
        let plan = crate::language::cypher::preparation::prepare("MATCH (n:Conversion) RETURN toFloat(n.raw) AS x ORDER BY n.id", &BTreeMap::new(), None).unwrap();
        for enabled in [true, false] {
            let mut options = RelBackendOptions::default();
            options.language_functions = enabled;
            options.rule_context.capabilities.language_functions_enabled = enabled;
            // Preserve the verified registration even when disabled: explicit
            // opt-out must win over availability in an already-used session.
            options.rule_context.capabilities.float_helper = float_capability(&connection, true).unwrap();
            options.mapping = Some(Arc::new(mapping.clone()));
            let lowered = RelBackend::with_options(options).lower(&plan, &PropertyGraph::new()).unwrap();
            let prepared = super::super::prepare_with_external(&lowered, super::super::SqlDialect::DuckDb, &mapping.physical_table_names()).await.unwrap();
            assert_eq!(prepared.query.contains(kernels::CYPHER_TO_FLOAT), enabled, "{}", prepared.query);
            let mut query = connection.prepare(&prepared.query).unwrap();
            let values = query.query_map([], |r| r.get::<_, Option<f64>>(0)).unwrap().collect::<duckdb::Result<Vec<_>>>().unwrap();
            let expected: Vec<_> = inputs.into_iter().map(|v| v.and_then(|v| crate::ir::functions::cypher_float::parse_string(v).ok())).collect();
            assert_eq!(values, expected);
            for op in ["=", "<>", "<", "<=", ">", ">="] {
                for shape in 0..3 {
                    let query = match shape {
                        0 => format!("MATCH (n:Conversion) RETURN toFloat(n.raw) {op} 5.0 AS result ORDER BY n.id"),
                        1 => format!("MATCH (n:Conversion) WITH n.id AS id, toFloat(n.raw) AS x RETURN x {op} 5.0 AS result ORDER BY id"),
                        _ => format!("MATCH (n:Conversion) RETURN (CASE WHEN n.id = 1 THEN 0.0 ELSE toFloat(n.raw) END) {op} 5.0 AS result ORDER BY n.id"),
                    };
                    let plan = crate::language::cypher::preparation::prepare(&query, &BTreeMap::new(), None).unwrap();
                    let mut options = RelBackendOptions::default();
                    options.mapping = Some(Arc::new(mapping.clone()));
                    options.rule_context.capabilities.language_functions_enabled = enabled;
                    options.rule_context.capabilities.float_helper = float_capability(&connection, true).unwrap();
                    let lowered = RelBackend::with_options(options).lower(&plan, &PropertyGraph::new()).unwrap();
                    let prepared = super::super::prepare_with_external(&lowered, super::super::SqlDialect::DuckDb, &mapping.physical_table_names()).await.unwrap();
                    assert!(!prepared.query.to_lowercase().contains("isnan"), "{}", prepared.query);
                    if enabled {
                        assert_eq!(prepared.query.matches(kernels::CYPHER_TO_FLOAT).count(), 1, "{}", prepared.query);
                    }
                    let mut statement = connection.prepare(&prepared.query).unwrap();
                    let actual = statement.query_map([], |r| r.get::<_, Option<bool>>(0)).unwrap().collect::<duckdb::Result<Vec<_>>>().unwrap();
                    let expected: Vec<_> = expected.iter().enumerate().map(|(id, v)| {
                        let value = if shape == 2 && id == 1 { Some(0.0) } else { *v };
                        value.map(|v| match op {
                        "=" => v == 5.0, "<>" => v != 5.0, "<" => v < 5.0,
                        "<=" => v <= 5.0, ">" => v > 5.0, ">=" => v >= 5.0, _ => unreachable!(),
                    })}).collect();
                    assert_eq!(actual, expected, "{query}: {}", prepared.query);
                }
            }
        }
    }

    #[test]
    fn cached_helper_plan_is_bound_to_live_enabled_session() {
        use super::super::{PreparedSql, DuckDbExecutor, execute_prepared};
        use arrow::datatypes::{Schema, Field, DataType};
        use std::sync::Arc;
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        let mut prepared = PreparedSql::query_only("SELECT __orchiddb_cypher_to_float_v1('1.25') AS x".into(), Arc::new(Schema::new(vec![Field::new("x", DataType::Float64, true)])));
        prepared.float_helper = float_capability(&connection, true).unwrap();
        let mut executor = DuckDbExecutor::from_connection(connection);
        assert_eq!(execute_prepared(&mut executor, &prepared).unwrap().batch.num_rows(), 1);
        executor.set_language_functions(false).unwrap();
        assert!(execute_prepared(&mut executor, &prepared).unwrap_err().to_string().contains("reprepare"));
        assert!(executor.execute_prepared_arrow(&prepared).unwrap_err().to_string().contains("reprepare"));
        executor.set_language_functions(true).unwrap();
        assert!(execute_prepared(&mut executor, &prepared).is_ok());
        let mut other = DuckDbExecutor::new();
        other.prepare_float_helper_capability().unwrap();
        assert!(execute_prepared(&mut other, &prepared).unwrap_err().to_string().contains("reprepare"));
    }

}
