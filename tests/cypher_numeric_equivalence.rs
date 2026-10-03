//! Cypher value equivalence must survive native grouping and DISTINCT.
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::ir::rel::runtime::execute;
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

async fn query_rows(query: &str) -> Vec<Vec<String>> {
    let graph = PropertyGraph::new();
    let ast = parse_query(query).unwrap();
    let plan = CypherPlanner::new().plan(&ast).unwrap();
    let (result, _) = execute(&plan, &graph, None).await.unwrap();
    (0..result.batch.num_rows())
        .map(|row| {
            (0..result.batch.num_columns())
                .map(|column| {
                    arrow::util::display::array_value_to_string(result.batch.column(column), row)
                        .unwrap()
                })
                .collect()
        })
        .collect()
}

fn run(query: &str) -> Vec<Vec<String>> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(query_rows(query))
}

#[test]
fn distinct_aggregates_use_numeric_equivalence() {
    assert_eq!(
        run("UNWIND [1, 1.0] AS x RETURN count(DISTINCT x)"),
        vec![vec![String::from("1")]]
    );
    assert_eq!(
        run("UNWIND [[1], [1.0]] AS x RETURN count(DISTINCT x)"),
        vec![vec![String::from("1")]]
    );
    assert_eq!(
        run("UNWIND [0, -0.0] AS x RETURN count(DISTINCT x)"),
        vec![vec![String::from("1")]]
    );
    assert_eq!(
        run("UNWIND [[1], [1.0]] AS x RETURN size(collect(DISTINCT x))"),
        vec![vec![String::from("1")]]
    );
    assert_eq!(
        run("UNWIND [1, 1.0] AS x WITH DISTINCT x RETURN count(x)"),
        vec![vec![String::from("1")]]
    );
}

#[test]
fn grouping_uses_numeric_equivalence() {
    let rows = run("UNWIND [1, 1.0] AS x RETURN x, count(*)");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0][1], "2");
}

#[test]
fn union_distinct_uses_numeric_equivalence() {
    assert_eq!(run("RETURN 1 AS n UNION RETURN 1.0 AS n").len(), 1);
}
