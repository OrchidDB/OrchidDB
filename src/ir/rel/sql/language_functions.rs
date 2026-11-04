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

const REGISTRATION_PROBE: &str = "SELECT __orchiddb_cypher_list_compare('gte', [1], [1]), __orchiddb_cypher_collect_distinct(CAST(NULL AS BIGINT[])), __orchiddb_cypher_node_text(NULL, NULL, NULL, NULL), __orchiddb_lang_cypher_key(NULL), __orchiddb_lang_text(NULL, NULL), __orchiddb_lang_texts(NULL, NULL), __orchiddb_lang_int(NULL, NULL), __orchiddb_lang_ints(NULL, NULL) WHERE false";

/// Register the pure Cypher language functions for generated SQL. Applications
/// compiling SQL themselves must also opt in via RelBackendOptions.
/// Registration is idempotent on connections sharing a DuckDB catalog.
pub fn register(connection: &Connection) -> duckdb::Result<()> {
    super::native_language_functions::register(connection)?;
    super::typed_language_functions::register(connection)?;
    register_float(connection)?;
    // Bind null calls without evaluating kernels. This is much cheaper than
    // enumerating DuckDB's full function catalog on every borrowed connection.
    // A missing function is a binder error; it does not abort a transaction.
    if connection.prepare(REGISTRATION_PROBE).is_ok() {
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
#[path = "../../../../tests/unit/extracted/ir__rel__sql__language_functions__float_tests.rs"]
mod float_tests;
