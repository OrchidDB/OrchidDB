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

#[cfg(test)]
mod nested_vector_contracts {
    use super::*;

    #[test]
    fn folded_nested_vectors_preserve_nulls_and_selected_values() {
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        for element in ["INTEGER", "BIGINT", "DOUBLE", "VARCHAR"] {
            let query = format!("SELECT list_transform(orchid_cypher_quantifier_list(CAST([NULL,NULL] AS {element}[])), lambda x: orchid_cypher_size(x) IS NULL) IS NOT DISTINCT FROM [true,true]");
            assert!(connection.query_row(&query, [], |r| r.get::<_,bool>(0)).unwrap(), "{query}");
        }
        for (source, expected) in [
            ("CAST([[NULL,NULL],NULL,[]] AS INTEGER[][])", "[2,NULL,0]"),
            ("CAST([[],[NULL],[1,NULL,2],NULL] AS INTEGER[][])", "[0,1,3,NULL]"),
            ("list_select(CAST([[NULL,NULL],NULL,[],[1]] AS INTEGER[][]),[4,2,1,4])", "[1,NULL,2,1]"),
            ("CAST([] AS INTEGER[][])", "[]::BIGINT[]"),
            ("CAST(NULL AS INTEGER[][])", "NULL::BIGINT[]"),
        ] {
            let query = format!("SELECT list_transform(orchid_cypher_quantifier_list({source}), lambda x: orchid_cypher_size(x)) IS NOT DISTINCT FROM {expected}");
            assert!(connection.query_row(&query, [], |r| r.get::<_,bool>(0)).unwrap(), "{query}");
        }
        let query = "SELECT list_transform(orchid_cypher_quantifier_list(list_select([1,NULL,3],[3,2,1,3])), lambda x: orchid_cypher_scalar_text(x)) IS NOT DISTINCT FROM ['3',NULL,'1','3']";
        assert!(connection.query_row(query, [], |r| r.get::<_,bool>(0)).unwrap());
    }
}
