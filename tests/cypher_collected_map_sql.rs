#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

#[tokio::test]
async fn collected_map_field_keeps_nulls_order_and_empty_groups() {
    for (query, expected) in [
        ("UNWIND [null,2] AS x WITH head(collect({v:x})) AS m RETURN m.v", ""),
        ("UNWIND [2,null] AS x WITH head(collect({v:x})) AS m RETURN m.v", "2"),
        ("UNWIND [2,null,1] AS x WITH x ORDER BY x DESC WITH head(collect({v:x})) AS m RETURN m.v", ""),
        ("UNWIND [2,null,1] AS x WITH x ORDER BY x WITH head(collect({v:x})) AS m RETURN m.v", "1"),
        ("UNWIND [] AS x WITH head(collect({v:x})) AS m RETURN m.v", ""),
        ("UNWIND [null,null] AS x WITH head(collect({v:x})) AS m RETURN m.v", ""),
        ("UNWIND [1,2] AS x WITH head(collect({v:x,unused:42})) AS m RETURN m.missing", ""),
        ("UNWIND [1,2] AS x WITH head(collect({v:x})) AS m RETURN m.v AS a,m.v AS b", "1|1"),
    ] {
        let graph = PropertyGraph::new();
        let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph)
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await
            .unwrap_or_else(|e| panic!("{query}: {e}"));
        assert_eq!(result.batch.num_rows(), 1, "{query}");
        let row = result.batch.columns().iter().map(|array|
            arrow::util::display::array_value_to_string(array, 0).unwrap()).collect::<Vec<_>>().join("|");
        assert_eq!(row, expected, "{query}");
    }
}

#[tokio::test]
async fn collect_nested_primitive_values_preserves_null_elements() {
    let graph = PropertyGraph::new();
    let query = "UNWIND [null,2] AS x RETURN collect([x])";
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().with_language_functions(true).lower(&plan, &graph).unwrap();
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap();
    use arrow::array::{Array, ListArray, Int64Array};
    let outer = result.batch.column(0).as_any().downcast_ref::<ListArray>().unwrap();
    let elements = outer.value(0);
    let elements = elements.as_any().downcast_ref::<ListArray>().unwrap();
    assert_eq!(elements.len(), 2);
    assert!(!elements.is_null(0));
    let first = elements.value(0);
    assert_eq!(first.len(), 1);
    assert!(first.is_null(0));
    let second = arrow::compute::cast(elements.value(1).as_ref(), &arrow::datatypes::DataType::Int64).unwrap();
    assert_eq!(second.as_any().downcast_ref::<Int64Array>().unwrap().value(0), 2);
}

#[test]
fn collected_map_rewrite_does_not_discard_effects_or_escaping_maps() {
    let graph = PropertyGraph::new();
    for query in [
        "UNWIND [1,2] AS x WITH head(collect({v:x,unused:rand()})) AS m RETURN m.v",
        "UNWIND [1,2] AS x WITH head(collect({v:x})) AS m RETURN m",
        "UNWIND [1,2] AS x WITH head(collect({v:x,other:x})) AS m RETURN m.v,m.other",
        "UNWIND [1,2] AS x WITH head(collect(DISTINCT {v:x})) AS m RETURN m.v",
        "UNWIND [[1],[null]] AS x RETURN collect(DISTINCT x)",
    ] {
        let Ok(plan) = CypherPlanner::new().plan(&parse_query(query).unwrap()) else { continue; };
        assert!(RelBackend::new().with_language_functions(true).lower(&plan, &graph).is_err(), "{query}");
    }
}
