use orchiddb::ir::{
    catalog::PropertyGraph,
    rel::{
        RelBackend,
        sql::{SqlDialect, unparse},
    },
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

fn lower(query: &str) -> orchiddb::ir::rel::LoweredPlan {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    RelBackend::new()
        .with_language_functions(true)
        .lower(&plan, &PropertyGraph::new())
        .unwrap()
}

#[test]
fn compact_literal_unwind() {
    let plan = lower("WITH [1,2,3] AS list UNWIND list AS x RETURN *");
    let sql = unparse(&plan, SqlDialect::DuckDb).unwrap();
    println!("{sql}");
    assert!(sql.contains("VALUES"), "{sql}");
    assert!(
        !sql.contains("__w_sql_cte") && !sql.contains("__graph_rel"),
        "{sql}"
    );
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn literal_expansions_preserve_values_and_multiplicity() {
    use orchiddb::ir::rel::sql::{DuckDbExecutor, execute_lowered_sql};
    for (query, expected) in [
        (
            "WITH [1,2,3] AS list UNWIND list AS x RETURN *",
            vec![
                vec!["[1, 2, 3]", "1"],
                vec!["[1, 2, 3]", "2"],
                vec!["[1, 2, 3]", "3"],
            ],
        ),
        (
            "WITH [1,1,null] AS list UNWIND list AS x RETURN x",
            vec![vec!["1"], vec!["1"], vec![""]],
        ),
        ("WITH [] AS list UNWIND list AS x RETURN *", vec![]),
        (
            "WITH [[1,2],[3]] AS list UNWIND list AS x RETURN x",
            vec![vec!["[1, 2]"], vec!["[3]"]],
        ),
        (
            "WITH [1,2] AS list UNWIND list AS item RETURN item AS list",
            vec![vec!["1"], vec!["2"]],
        ),
        (
            "WITH [1,2] AS list UNWIND list AS x RETURN x AS y, x AS z",
            vec![vec!["1", "1"], vec!["2", "2"]],
        ),
    ] {
        let lowered = lower(query);
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        assert!(!sql.contains("__w_sql_cte"), "{query}: {sql}");
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
        let actual: Vec<Vec<String>> = (0..result.batch.num_rows())
            .map(|row| {
                result
                    .batch
                    .columns()
                    .iter()
                    .map(|c| arrow::util::display::array_value_to_string(c, row).unwrap())
                    .collect()
            })
            .collect();
        assert_eq!(actual, expected, "{query}: {sql}");
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn consumers_keep_scope_boundaries() {
    use orchiddb::ir::rel::sql::{DuckDbExecutor, execute_lowered_sql};
    for (query, rows) in [
        (
            "WITH [1,2,3] AS list UNWIND list AS x RETURN sum(x) AS total",
            1,
        ),
        (
            "WITH [1,2,3] AS list UNWIND list AS x WITH x WHERE x > 1 RETURN x",
            2,
        ),
        (
            "WITH [1,2,3] AS list UNWIND list AS x RETURN abs(x) AS x",
            3,
        ),
        ("WITH [1,2] AS xs, [3,4] AS ys UNWIND xs AS x UNWIND ys AS y RETURN x,y", 4),
        ("WITH [1,2,3] AS list UNWIND list AS x RETURN list", 3),
    ] {
        let lowered = lower(query);
        let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
        assert!(sql.contains("__w_sql_cte"), "{query}: {sql}");
        let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
            .await
            .unwrap_or_else(|e| panic!("{query}: {e}\n{sql}"));
        assert_eq!(result.batch.num_rows(), rows, "{query}: {sql}");
    }
}

#[test]
fn singleton_empty_relation_needs_no_seed_table() {
    use datafusion::common::tree_node::{Transformed, TreeNode};
    use datafusion::logical_expr::{LogicalPlan, LogicalPlanBuilder};
    let mut lowered = lower("WITH [1,2,3] AS list UNWIND list AS x RETURN *");
    lowered.plan = lowered.plan.transform_up(|node| {
        if matches!(&node, LogicalPlan::TableScan(s) if s.table_name.table().starts_with("__graph_rel_one_row_")) {
            Ok(Transformed::yes(LogicalPlanBuilder::empty(true).build()?))
        } else { Ok(Transformed::no(node)) }
    }).unwrap().data;
    let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
    assert!(
        sql.contains("VALUES") && !sql.contains("__graph_rel"),
        "{sql}"
    );
}

#[test]
fn internal_table_name_does_not_prove_singleton() {
    use datafusion::common::tree_node::{Transformed, TreeNode};
    use datafusion::datasource::{MemTable, provider_as_source};
    use datafusion::logical_expr::LogicalPlan;
    use std::sync::Arc;
    let mut lowered = lower("WITH [1,2,3] AS list UNWIND list AS x RETURN *");
    lowered.plan = lowered
        .plan
        .transform_up(|node| {
            if let LogicalPlan::TableScan(mut scan) = node {
                scan.source = provider_as_source(Arc::new(MemTable::try_new(
                    scan.source.schema(),
                    vec![vec![]],
                )?));
                Ok(Transformed::yes(LogicalPlan::TableScan(scan)))
            } else {
                Ok(Transformed::no(node))
            }
        })
        .unwrap()
        .data;
    let sql = unparse(&lowered, SqlDialect::DuckDb).unwrap();
    assert!(sql.contains("__graph_rel_one_row"), "{sql}");
}
