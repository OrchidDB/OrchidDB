#![cfg(feature = "duckdb")]

use arrow::datatypes::DataType;
use duckdb::{
    Connection,
    core::{DataChunkHandle, LogicalTypeId},
    vscalar::{ScalarFunctionSignature, VScalar},
    vtab::arrow::WritableVector,
};
use orchiddb::{
    ir::{
        catalog::PropertyGraph,
        functions::{
            DuckDbCatalog, FunctionKind, FunctionRegistry, OperatorTable, is_native_aggregate,
            with_operator_table,
        },
        rel::{
            RelBackend,
            sql::{self, DuckDbExecutor, SqlDialect},
        },
    },
    language::cypher::{parse_query, planner::CypherPlanner},
};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

struct Triple;
impl VScalar for Triple {
    type State = Arc<AtomicUsize>;
    unsafe fn invoke(
        calls: &Self::State,
        input: &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let len = input.len();
        calls.fetch_add(len, Ordering::SeqCst);
        let source = input.flat_vector(0);
        let source = source.as_slice_with_len::<i64>(len);
        let mut target = output.flat_vector();
        let target = target.as_mut_slice::<i64>();
        for row in 0..len {
            target[row] = source[row] * 3;
        }
        Ok(())
    }
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![ScalarFunctionSignature::exact(
            vec![LogicalTypeId::Bigint.into()],
            LogicalTypeId::Bigint.into(),
        )]
    }
}

async fn execute(
    table: Arc<dyn OperatorTable>,
    connection: Connection,
    query: &str,
) -> Vec<String> {
    let graph = PropertyGraph::new();
    let lowered = with_operator_table(table, || {
        let parsed = parse_query(query).unwrap();
        let plan = CypherPlanner::new().plan(&parsed).unwrap();
        RelBackend::new().lower(&plan, &graph).unwrap()
    });
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap();
    let output = sql::execute_prepared(&mut DuckDbExecutor::from_connection(connection), &prepared)
        .unwrap_or_else(|error| panic!("{error}\n{}", prepared.query));
    (0..output.batch.num_rows())
        .map(|row| {
            (0..output.batch.num_columns())
                .map(|col| {
                    arrow::util::display::array_value_to_string(
                        output.batch.column(col).as_ref(),
                        row,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[tokio::test]
async fn catalog_discovers_rust_udf_and_code_alias_without_running_it_during_binding() {
    let connection = Connection::open_in_memory().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    connection
        .register_scalar_function_with_state::<Triple>("engine_triple", &calls)
        .unwrap();
    let catalog =
        Arc::new(DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap());
    let mut registry = FunctionRegistry::new(catalog);
    registry
        .register_mapping("business_triple", "engine_triple", FunctionKind::Scalar)
        .unwrap();
    let graph = PropertyGraph::new();
    let lowered = with_operator_table(Arc::new(registry), || {
        let parsed = parse_query("RETURN business_triple(4), engine_triple(5)").unwrap();
        let plan = CypherPlanner::new().plan(&parsed).unwrap();
        RelBackend::new().lower(&plan, &graph).unwrap()
    });
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "binding must not execute UDF code"
    );
    let prepared = sql::prepare(&lowered, SqlDialect::DuckDb).await.unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "SQL preparation must not execute UDF code"
    );
    let result =
        sql::execute_prepared(&mut DuckDbExecutor::from_connection(connection), &prepared).unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(0).as_ref(), 0).unwrap(),
        "12"
    );
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(1).as_ref(), 0).unwrap(),
        "15"
    );
    assert!(calls.load(Ordering::SeqCst) > 0);
}

#[tokio::test]
async fn macro_and_aggregate_aliases_use_the_selected_catalog() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE MACRO engine_offset(x) AS x + 10")
        .unwrap();
    let catalog =
        Arc::new(DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap());
    let mut registry = FunctionRegistry::new(catalog);
    registry
        .register_mapping("business_offset", "engine_offset", FunctionKind::Scalar)
        .unwrap();
    registry
        .register_mapping("business_product", "product", FunctionKind::Aggregate)
        .unwrap();
    let rows = execute(
        Arc::new(registry),
        connection,
        "UNWIND [1, 2, 2] AS x RETURN business_offset(business_product(DISTINCT x))",
    )
    .await;
    assert_eq!(rows, ["12.0"]);
}

#[tokio::test]
async fn typed_declarations_support_udfs_absent_from_catalog_snapshot() {
    let connection = Connection::open_in_memory().unwrap();
    let catalog =
        Arc::new(DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap());
    // Register after snapshotting the catalog, then supply its types in code.
    connection
        .register_scalar_function::<Triple>("engine_triple")
        .unwrap();
    let mut registry = FunctionRegistry::new(catalog);
    registry
        .register_typed_mapping(
            "typed_triple",
            "engine_triple",
            FunctionKind::Scalar,
            vec![DataType::Int64],
            DataType::Int64,
        )
        .unwrap();
    assert_eq!(
        execute(Arc::new(registry), connection, "RETURN typed_triple(4)").await,
        ["12"]
    );
}

#[test]
fn operator_table_scopes_restore_after_panics_and_do_not_leak_to_threads() {
    let mut registry = FunctionRegistry::new(Arc::new(DuckDbCatalog::new().unwrap()));
    registry
        .register_mapping("scoped_product", "product", FunctionKind::Aggregate)
        .unwrap();
    let registry: Arc<dyn OperatorTable> = Arc::new(registry);
    assert!(!is_native_aggregate("scoped_product"));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_operator_table(registry, || {
            assert!(is_native_aggregate("scoped_product"));
            assert!(
                !std::thread::spawn(|| is_native_aggregate("scoped_product"))
                    .join()
                    .unwrap()
            );
            panic!("exercise scope restoration");
        })
    }));
    assert!(result.is_err());
    assert!(!is_native_aggregate("scoped_product"));
}

#[test]
fn mapping_registration_preserves_language_semantics_and_rejects_wrong_types() {
    let mut registry = FunctionRegistry::new(Arc::new(DuckDbCatalog::new().unwrap()));
    assert!(
        registry
            .register_mapping("sum", "product", FunctionKind::Aggregate)
            .is_err()
    );
    assert!(
        registry
            .register_mapping("substring", "left", FunctionKind::Scalar)
            .is_err()
    );
    assert!(
        registry
            .register_mapping("bad_table", "range", FunctionKind::Table)
            .is_err()
    );
    registry
        .register_typed_mapping(
            "typed_udf",
            "unseen_udf",
            FunctionKind::Scalar,
            vec![DataType::Int64],
            DataType::Int64,
        )
        .unwrap();
    assert!(
        registry
            .bind(
                "typed_udf",
                FunctionKind::Scalar,
                &[datafusion::prelude::lit("bad")],
                &datafusion::common::DFSchema::empty()
            )
            .is_err()
    );
}

#[tokio::test]
async fn mapped_engine_owns_function_scope_across_async_queries() {
    use orchiddb::{ir::rel::mapping::GraphMapping, mapped_engine::MappedGraphEngine};
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(GraphMapping::new()));
    engine
        .execute_sql("CREATE MACRO engine_offset(x) AS x + 10")
        .unwrap();
    let connection = engine.executor_mut().connection().unwrap();
    connection
        .register_scalar_function::<Triple>("engine_triple")
        .unwrap();
    let catalog =
        Arc::new(DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap());
    let mut registry = FunctionRegistry::new(catalog);
    registry
        .register_mapping("business_offset", "engine_offset", FunctionKind::Scalar)
        .unwrap();
    registry
        .register_mapping("business_triple", "engine_triple", FunctionKind::Scalar)
        .unwrap();
    registry
        .register_mapping("business_product", "product", FunctionKind::Aggregate)
        .unwrap();
    engine.set_operator_table(Arc::new(registry)).unwrap();
    assert!(!is_native_aggregate("business_product"));
    let sql = engine
        .explain_cypher("RETURN business_triple(4)")
        .await
        .unwrap();
    assert!(sql.contains("engine_triple"), "{sql}");
    let result = engine.cypher("UNWIND [1, 2, 2] AS x RETURN business_offset(business_product(DISTINCT x)), business_triple(4)").await.unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(0).as_ref(), 0).unwrap(),
        "12.0"
    );
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(1).as_ref(), 0).unwrap(),
        "12"
    );
    assert!(!is_native_aggregate("business_product"));
}

#[tokio::test]
async fn mapped_updates_use_registered_function_mappings() {
    use arrow::datatypes::{Field, Schema};
    use orchiddb::{
        ir::rel::mapping::{GraphMapping, NodeMapping},
        mapped_engine::MappedGraphEngine,
    };
    let mut mapping = GraphMapping::new();
    mapping
        .register_table_schema(
            "udf_values",
            Arc::new(Schema::new(vec![
                Field::new("id", DataType::Int64, false),
                Field::new("value", DataType::Int64, true),
            ])),
        )
        .map_node(NodeMapping::table("P", "udf_values", "id").property("value", "value"));
    let mut engine = MappedGraphEngine::new(DuckDbExecutor::new(), Arc::new(mapping));
    engine.execute_sql("CREATE TABLE udf_values(id BIGINT, value BIGINT); INSERT INTO udf_values VALUES (1, 4); CREATE MACRO engine_offset(x) AS x + 10").unwrap();
    let connection = engine.executor_mut().connection().unwrap();
    let mut registry = FunctionRegistry::new(Arc::new(
        DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap(),
    ));
    registry
        .register_mapping("business_offset", "engine_offset", FunctionKind::Scalar)
        .unwrap();
    engine.set_operator_table(Arc::new(registry)).unwrap();
    assert_eq!(
        engine
            .cypher_update("MATCH (p:P) SET p.value = business_offset(p.value)")
            .await
            .unwrap(),
        1
    );
    let result = engine.cypher("MATCH (p:P) RETURN p.value").await.unwrap();
    assert_eq!(
        arrow::util::display::array_value_to_string(result.batch.column(0).as_ref(), 0).unwrap(),
        "14"
    );
}

#[tokio::test]
async fn mappings_resolve_schema_qualified_targets() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE SCHEMA analytics; CREATE MACRO analytics.score_impl(x) AS x + 5")
        .unwrap();
    let catalog =
        Arc::new(DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap());
    let mut registry = FunctionRegistry::new(catalog);
    registry
        .register_mapping("score", "analytics.score_impl", FunctionKind::Scalar)
        .unwrap();
    assert_eq!(
        execute(
            Arc::new(registry),
            connection,
            "WITH 7 AS x RETURN score(x)"
        )
        .await,
        ["12"]
    );
}

#[tokio::test]
async fn code_mapping_can_classify_an_aggregate_macro() {
    let connection = Connection::open_in_memory().unwrap();
    connection
        .execute_batch("CREATE MACRO sum_impl(x) AS sum(x)")
        .unwrap();
    let catalog =
        Arc::new(DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap());
    let mut registry = FunctionRegistry::new(catalog);
    registry
        .register_mapping("business_sum", "sum_impl", FunctionKind::Aggregate)
        .unwrap();
    assert_eq!(
        execute(
            Arc::new(registry),
            connection,
            "UNWIND [1, 2, 3] AS x RETURN business_sum(x)"
        )
        .await,
        ["6"]
    );
}

struct OverloadedIdentity;
impl VScalar for OverloadedIdentity {
    type State = ();
    unsafe fn invoke(
        _: &(),
        input: &mut DataChunkHandle,
        output: &mut dyn WritableVector,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use duckdb::core::Inserter;
        let len = input.len();
        let source = input.flat_vector(0);
        if source.logical_type().id() == LogicalTypeId::Integer {
            let target = output.flat_vector();
            for row in 0..len {
                target.insert(row, "integer overload");
            }
        } else {
            let values = source.as_slice_with_len::<i64>(len);
            output.flat_vector().copy(&values[..len]);
        }
        Ok(())
    }
    fn signatures() -> Vec<ScalarFunctionSignature> {
        vec![
            ScalarFunctionSignature::exact(
                vec![LogicalTypeId::Integer.into()],
                LogicalTypeId::Varchar.into(),
            ),
            ScalarFunctionSignature::exact(
                vec![LogicalTypeId::Bigint.into()],
                LogicalTypeId::Bigint.into(),
            ),
        ]
    }
}

#[tokio::test]
async fn typed_declarations_pin_literal_and_null_overload_at_execution() {
    let connection = Connection::open_in_memory().unwrap();
    let catalog =
        Arc::new(DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap());
    connection
        .register_scalar_function::<OverloadedIdentity>("engine_overloaded")
        .unwrap();
    // Verify the engine really resolves an uncast integer literal differently.
    let raw: String = connection
        .query_row("SELECT engine_overloaded(4)", [], |row| row.get(0))
        .unwrap();
    assert_eq!(raw, "integer overload");
    let mut registry = FunctionRegistry::new(catalog);
    registry
        .register_typed_mapping(
            "declared_bigint",
            "engine_overloaded",
            FunctionKind::Scalar,
            vec![DataType::Int64],
            DataType::Int64,
        )
        .unwrap();
    assert_eq!(
        execute(
            Arc::new(registry),
            connection,
            "RETURN declared_bigint(4), declared_bigint(NULL)"
        )
        .await,
        ["4|"]
    );
}

#[test]
fn relational_language_functions_cannot_be_silently_overridden_by_mappings() {
    let mut registry = FunctionRegistry::new(Arc::new(DuckDbCatalog::new().unwrap()));
    for name in ["gcd", "acosh", "to_boolean", "regexp_like"] {
        assert!(
            registry
                .register_mapping(name, "lcm", FunctionKind::Scalar)
                .is_err(),
            "{name}"
        );
    }
}

#[tokio::test]
async fn declared_list_arguments_render_native_sql_casts() {
    let connection = Connection::open_in_memory().unwrap();
    let catalog =
        Arc::new(DuckDbCatalog::from_connection(connection.try_clone().unwrap()).unwrap());
    let mut registry = FunctionRegistry::new(catalog);
    registry
        .register_typed_mapping(
            "custom_list_length",
            "len",
            FunctionKind::Scalar,
            vec![DataType::List(Arc::new(arrow::datatypes::Field::new(
                "item",
                DataType::Int64,
                true,
            )))],
            DataType::Int64,
        )
        .unwrap();
    assert_eq!(
        execute(
            Arc::new(registry),
            connection,
            "RETURN custom_list_length([1, 2])"
        )
        .await,
        ["2"]
    );
}
