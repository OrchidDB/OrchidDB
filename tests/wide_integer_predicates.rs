//! Cypher integer literals use signed 64-bit semantics. Reject oversized
//! literals during planning before a SQL backend can round them through f64.
#[path = "common/execution.rs"]
mod datafusion_test;

use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::language::cypher::parser::parse_query;
use orchiddb::language::cypher::planner::CypherPlanner;

#[test]
fn wide_integer_predicates_fail_with_integer_overflow() {
    for literal in [
        "9223372036854775808",
        "-9223372036854775809",
        "170141183460469231731687303715884105727",
        "170141183460469231731687303715884105728",
        "340282366920938463463374607431768211455",
        "340282366920938463463374607431768211456",
    ] {
        let query = parse_query(&format!("MATCH (t:test) WHERE t.id = {literal} RETURN t.id")).unwrap();
        let error = CypherPlanner::new().plan(&query).unwrap_err();
        assert_eq!(error.classification(), Some(("SyntaxError", "IntegerOverflow")), "{literal}: {error}");
    }
}

#[test]
fn signed_integer_boundaries_remain_exact() {
    for value in [i64::MIN, i64::MAX, 9007199254740993] {
        let query = parse_query(&format!("RETURN {value} AS value")).unwrap();
        let plan = CypherPlanner::new().plan(&query).unwrap();
        let result = datafusion_test::execute(&plan, &PropertyGraph::new()).unwrap();
        let column = result.batch.column(0).as_any().downcast_ref::<arrow::array::Int64Array>().unwrap();
        assert_eq!(column.value(0), value);
    }
}
