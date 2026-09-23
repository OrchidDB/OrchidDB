use new_graph::ir::plan::explain;
use new_graph::language::cypher::parse_query;
use new_graph::language::cypher::planner::lower_query;

#[test]
fn list_comprehension_lowers_to_relational_unwind_and_collect() {
    let query = parse_query("RETURN [x IN [1, 2, 3] WHERE x > 1 | x * 10] AS result")
        .expect("query parses");
    let plan = lower_query(&query).expect("query lowers");
    let plan_text = explain(&plan);

    assert!(plan_text.contains("GraphUnwind"), "{plan_text}");
    assert!(plan_text.contains("GraphCollect"), "{plan_text}");
    assert!(
        !plan_text.contains("GraphListComprehension"),
        "list comprehensions must lower to relational operators: {plan_text}"
    );
}
