#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, rel::{RelBackend, sql::{self, DuckDbExecutor, SqlDialect}}};
use orchiddb::language::cypher::{parser::parse_query,planner::CypherPlanner};

async fn execute(query: &str) -> (String,Vec<Vec<String>>) {
    let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
    let lowered=RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).unwrap();
    let sql=sql::unparse(&lowered,SqlDialect::DuckDb).unwrap();
    let result=sql::execute_lowered_sql(&mut DuckDbExecutor::default(),&lowered).await.unwrap();
    let rows=(0..result.batch.num_rows()).map(|row|result.batch.columns().iter().map(|col|arrow::util::display::array_value_to_string(col,row).unwrap()).collect()).collect();
    (sql,rows)
}
#[tokio::test]
async fn boolean_xor_uses_inequality_and_keeps_all_nine_truth_values() {
    let (sql,rows)=execute("UNWIND [true,false,null] AS a UNWIND [true,false,null] AS b RETURN a,b,a XOR b AS result").await;
    assert!(sql.contains("<>"),"{sql}");
    assert_eq!(rows.len(),9);
    for row in rows {
        let expected=if row[0].is_empty() || row[1].is_empty() { "" } else if row[0]!=row[1] { "true" } else { "false" };
        assert_eq!(row[2],expected,"{row:?}");
    }
}
#[tokio::test]
async fn xor_does_not_duplicate_a_volatile_operand() {
    let (sql,rows)=execute("RETURN (random() IS NULL) XOR false AS result").await;
    assert_eq!(sql.to_lowercase().replace('"', "").matches("random()").count(),1,"{sql}");
    assert_eq!(rows.len(),1);
    assert!(matches!(rows[0][0].as_str(),"true"|"false"));
}
#[tokio::test]
async fn xor_membership_precedence_survives_collection_and_quantifiers() {
    let query="UNWIND [true, false, null] AS a UNWIND [true, false, null] AS b UNWIND [[], [true], [false], [null], [true, false], [true, false, null]] AS c WITH collect((a XOR b IN c) = (a XOR (b IN c))) AS eq, collect((a XOR b IN c) <> ((a XOR b) IN c)) AS neq RETURN all(x IN eq WHERE x) AND any(x IN neq WHERE x) AS result";
    let (sql,rows)=execute(query).await;
    assert_eq!(rows,vec![vec!["true".to_string()]],"{sql}");
    assert!(!sql.contains("\"a\" AND"),"{sql}");
}
