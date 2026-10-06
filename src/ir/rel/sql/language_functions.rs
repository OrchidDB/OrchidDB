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
    // Bind null calls without evaluating kernels. This is much cheaper than
    // enumerating DuckDB's full function catalog on every borrowed connection.
    // A missing function is a binder error; it does not abort a transaction.
    if connection.prepare("SELECT __orchiddb_lang_cypher_key(NULL), __orchiddb_lang_text(NULL, NULL), __orchiddb_lang_texts(NULL, NULL), __orchiddb_lang_int(NULL, NULL), __orchiddb_lang_ints(NULL, NULL), __orchiddb_lang_sparql(NULL, NULL, NULL, NULL, NULL) WHERE false").is_ok() {
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
    add!(Text, "__orchiddb_lang_text");
    add!(Texts, "__orchiddb_lang_texts");
    add!(Int, "__orchiddb_lang_int");
    add!(Ints, "__orchiddb_lang_ints");
    add!(crate::rdf_engine::scalar::SparqlScalar, kernels::SPARQL);
    Ok(())
}
