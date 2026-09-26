#[path = "common/execution.rs"]
mod datafusion_test;
use orchiddb::ir::catalog::PropertyGraph;
use crate::datafusion_test::execute;
use orchiddb::language::cypher::parse_query;
use orchiddb::language::cypher::planner::lower_query;

#[test]
fn list_comprehension_preserves_filter_mapping_nulls_and_local_scope() {
    for (source, expected) in [
        (
            "WITH [x IN [1,2,3] WHERE x > 1 | x * 10] AS result RETURN result[0],result[1],size(result)",
            vec!["20", "30", "2"],
        ),
        (
            "WITH [x IN [null,1,2] WHERE x > 1 | x] AS result RETURN size(result),result[0]",
            vec!["1", "2"],
        ),
        (
            "WITH 10 AS x RETURN [x IN [1,2] | x * 2][1],x",
            vec!["4", "10"],
        ),
    ] {
        let query = parse_query(source).expect("query parses");
        let plan = lower_query(&query).expect("query lowers");
        let result = execute(&plan, &PropertyGraph::new()).expect("query executes");
        assert_eq!(result.batch.num_rows(), 1, "{source}");
        let actual = result
            .batch
            .columns()
            .iter()
            .map(|column| arrow::util::display::array_value_to_string(column, 0).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "{source}");
    }
}
