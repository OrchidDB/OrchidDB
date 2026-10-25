//! DuckDB vector adapters for focused Cypher kernels. Values travel as typed
//! arrays; the SQL call contains only the operation's ordinary arguments.
use crate::ir::rel::native_values::typed::{self, Output};
use duckdb::{
    Connection,
    core::{DataChunkHandle, LogicalTypeHandle, LogicalTypeId},
    vscalar::{ScalarFunctionSignature, VScalar},
    vtab::arrow::{WritableVector, write_arrow_array_to_vector},
};
fn carrier() -> LogicalTypeHandle {
    LogicalTypeHandle::struct_type(&[(
        crate::ir::rel::native_values::FIELD,
        LogicalTypeId::Varchar.into(),
    )])
}
macro_rules! adapter {
    ($name:ident, $result:expr) => {
        struct $name;
        impl VScalar for $name {
            type State = &'static str;
            fn special_null_handling() -> bool {
                true
            }
            fn signatures() -> Vec<ScalarFunctionSignature> {
                vec![ScalarFunctionSignature::variadic(
                    LogicalTypeId::Any.into(),
                    $result,
                )]
            }
            unsafe fn invoke(
                operation: &Self::State,
                input: &mut DataChunkHandle,
                output: &mut dyn WritableVector,
            ) -> Result<(), Box<dyn std::error::Error>> {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let arrays = super::language_vectors::arrays(input)?;
                    let result = typed::evaluate(operation, &arrays, input.len())?;
                    write_arrow_array_to_vector(&result, output)
                })) {
                    Ok(result) => result,
                    Err(_) => Err("Typed Cypher helper panicked".into()),
                }
            }
        }
    };
}
adapter!(ValueFunction, carrier());
adapter!(BooleanFunction, LogicalTypeId::Boolean.into());
adapter!(IntegerFunction, LogicalTypeId::Bigint.into());
adapter!(TextFunction, LogicalTypeId::Varchar.into());
adapter!(
    IndicesFunction,
    LogicalTypeHandle::list(&LogicalTypeId::Bigint.into())
);
adapter!(ListFunction, LogicalTypeHandle::list(&carrier()));
pub(super) fn register(connection: &Connection) -> duckdb::Result<()> {
    if connection
        .prepare("SELECT orchid_cypher_pack(NULL) WHERE false")
        .is_ok()
    {
        return Ok(());
    }
    for operation in typed::operations() {
        let name = typed::sql_name(operation);
        match typed::output(operation) {
            Output::Value => connection
                .register_scalar_function_with_state::<ValueFunction>(&name, &operation)?,
            Output::Boolean => connection
                .register_scalar_function_with_state::<BooleanFunction>(&name, &operation)?,
            Output::Integer => connection
                .register_scalar_function_with_state::<IntegerFunction>(&name, &operation)?,
            Output::Text => {
                connection.register_scalar_function_with_state::<TextFunction>(&name, &operation)?
            }
            Output::Indices => connection
                .register_scalar_function_with_state::<IndicesFunction>(&name, &operation)?,
            Output::List => {
                connection.register_scalar_function_with_state::<ListFunction>(&name, &operation)?
            }
        }
    }
    Ok(())
}
