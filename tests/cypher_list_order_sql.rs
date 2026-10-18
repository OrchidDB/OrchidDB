#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{DuckDbExecutor, execute_lowered_sql}}};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

async fn rows(query: &str) -> Vec<String> {
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered = RelBackend::new().lower(&plan, &PropertyGraph::new()).unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered).await.unwrap_or_else(|e| panic!("{query}: {e}"));
    (0..result.batch.num_rows()).map(|i| result.batch.columns().iter().map(|a| arrow::util::display::array_value_to_string(a,i).unwrap().replace(", ",",")).collect::<Vec<_>>().join("|")).collect()
}

#[tokio::test]
async fn list_order_is_lexicographic_with_nested_nulls_last() {
    assert_eq!(rows("UNWIND [[2,-2],[1,2],[300,0],[1,-20],[2,-2,100],[],[1,null],null] AS xs RETURN xs ORDER BY xs").await,
        vec!["[]","[1,-20]","[1,2]","[1,]","[2,-2]","[2,-2,100]","[300,0]",""]);
    assert_eq!(rows("UNWIND [[[1],null],[[1],[2]],[[1],[]],[[1]]] AS xs RETURN xs ORDER BY xs DESC").await,
        vec!["[[1],]","[[1],[2]]","[[1],[]]","[[1]]"]);
}

#[tokio::test]
async fn composed_list_addition_allows_total_index_reads() {
    assert_eq!(rows("UNWIND [[3,1],[2,5],null] AS xs WITH xs, [xs[1],xs[0]] + xs + [9] AS ys RETURN ys ORDER BY ys").await,
        vec!["[1,3,3,1,9]","[5,2,2,5,9]",""]);
}

#[tokio::test]
async fn primitive_list_equality_preserves_unknown_and_definite_mismatch() {
    assert_eq!(rows("UNWIND [[1,null],[2,null],[],null] AS xs RETURN xs = [1,null], xs <> [1,null]").await,
        vec!["|", "false|true", "false|true", "|"]);
    assert_eq!(rows("UNWIND [[1,2],[1,3],[],null] AS xs RETURN xs = [1,2]").await,
        vec!["true", "false", "false", ""]);
}

#[test]
fn scalar_overflow_inside_list_addition_stays_fenced() {
    let query = "UNWIND [9223372036854775807] AS n WITH n, null AS xs RETURN xs + [n + 1]";
    let plan = CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    assert!(RelBackend::new().lower(&plan, &PropertyGraph::new()).is_err());
}

#[tokio::test]
async fn list_equality_internal_alias_cannot_capture_user_binding() {
    assert_eq!(rows("UNWIND [[1,2],[1,null],[]] AS __w_list_equality_index RETURN __w_list_equality_index = [1,2]").await,
        vec!["true", "", "false"]);
    assert_eq!(rows("UNWIND [[1,2],[1,null],[]] AS __w_list_equality_items_guard RETURN __w_list_equality_items_guard = [1,2]").await,
        vec!["true", "", "false"]);
}

#[tokio::test]
async fn empty_list_equality_uses_zero_row_aggregate_identity() {
    assert_eq!(rows("UNWIND [[1],[],null] AS xs RETURN xs = xs").await,
        vec!["true", "true", ""]);
}

#[tokio::test]
async fn bound_list_membership_uses_typed_null_probes() {
    let mut actual = rows("UNWIND [[],[true],[false],[null],[true,null],null] AS xs RETURN true IN xs, false IN xs, null IN xs").await;
    actual.sort();
    let mut expected = vec!["false|false|false", "true|false|", "false|true|", "||", "true||", "||"];
    expected.sort();
    assert_eq!(actual, expected);
    let mut actual = rows("UNWIND [[],[null],null] AS xs RETURN null IN xs").await;
    actual.sort();
    assert_eq!(actual, vec!["", "", "false"]);
}
