#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph,diagnostics::without_constant_folding,
    rel::{RelBackend,sql::{DuckDbExecutor,SqlDialect,execute_lowered_sql,unparse}}};
use orchiddb::language::cypher::{parser::parse_query,planner::CypherPlanner};

async fn run(query:&str)->(String,Vec<String>){
    without_constant_folding(async {
        let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered=RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).unwrap();
        let sql=unparse(&lowered,SqlDialect::DuckDb).unwrap();
        let result=execute_lowered_sql(&mut DuckDbExecutor::default(),&lowered).await.unwrap();
        let rows=(0..result.batch.num_rows()).map(|row|result.batch.columns().iter()
            .map(|array|arrow::util::display::array_value_to_string(array,row).unwrap())
            .collect::<Vec<_>>().join("|")).collect();
        (sql,rows)
    }).await
}
#[tokio::test]
async fn bounded_nested_arithmetic_quantifiers_use_scalar_sql(){
    for predicate in ["x+y<5","x%y=1"] {
        let (sql,rows)=run(&format!("WITH [1,2,3,4,5,6,7,8,9] AS list RETURN single(x IN list WHERE single(y IN list WHERE {predicate})) AS result")).await;
        assert!(!sql.contains("row_number"), "{sql}");
        assert!(!sql.contains("GROUP BY"), "{sql}");
        assert!(!sql.contains("MATERIALIZED"), "{sql}");
        assert!(sql.len() < 2500, "{sql}");
        assert_eq!(rows,vec![if predicate=="x+y<5" {"true"} else {"false"}]);
    }
}
#[tokio::test]
async fn occurrence_reduction_preserves_duplicates_nulls_and_shadowing(){
    let (sql,mut rows)=run("UNWIND [7,7] AS x UNWIND [[1,null],[],null] AS xs RETURN x,all(x IN xs WHERE x+0=1) AS a,any(x IN xs WHERE x+0=1) AS b,none(x IN xs WHERE x+0=1) AS c,single(x IN xs WHERE x+0=1) AS d").await;
    // These dynamically rebound list elements have no static arithmetic range.
    assert!(sql.contains("__w_quantifier_row_"), "{sql}");
    rows.sort();
    let mut expected=vec!["7||true|false|","7|true|false|true|false","7||||","7||true|false|","7|true|false|true|false","7||||"];
    expected.sort();assert_eq!(rows,expected);
}
#[tokio::test]
async fn occurrence_reduction_retains_volatile_source_value(){
    without_constant_folding(async {
        let mut engine=orchiddb::engine::GraphEngine::in_memory().unwrap();
        let result=engine.cypher("UNWIND [1,2,3] AS k WITH k,rand() AS r,[1,2] AS xs WITH k,r,all(x IN xs WHERE r+0=r) AS ok RETURN k,ok").await.unwrap();
        let batch=&result.returned.batch;
        let mut rows=(0..batch.num_rows()).map(|row|batch.columns().iter()
            .map(|array|arrow::util::display::array_value_to_string(array,row).unwrap())
            .collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
        rows.sort();assert_eq!(rows,vec!["1|true","2|true","3|true"]);
    }).await;
}

#[tokio::test]
async fn native_execution_keeps_single_stream_occurrence_reduction(){
    let lowered=without_constant_folding(async {
        let query="UNWIND [7,7] AS k UNWIND [[1,null],[],null] AS xs RETURN k,all(x IN xs WHERE x+0=1) AS result";
        let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).unwrap()
    }).await;
    assert!(!lowered.plan.display_indent().to_string().contains("__w_sql_cte_quantifier_counts_"));
    // Native DataFusion execution requires its mandatory expression rewrite
    // pass; disable compiler folding during lowering, not native execution.
    let result=orchiddb::ir::rel::execute_lowered(lowered).await.unwrap();
    let mut rows=(0..result.batch.num_rows()).map(|row|result.batch.columns().iter()
        .map(|array|arrow::util::display::array_value_to_string(array,row).unwrap())
        .collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
    rows.sort();assert_eq!(rows,vec!["7|","7|","7|","7|","7|true","7|true"]);
}

#[tokio::test]
async fn bounded_nested_predicates_keep_effectful_input_on_materialized_path() {
    without_constant_folding(async {
        let query="UNWIND [1,1] AS k WITH k,rand() AS r,[1,2,3] AS xs RETURN k,r,all(x IN xs WHERE any(y IN xs WHERE x+y>0)) AS result";
        let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        assert!(format!("{plan:?}").contains("GraphQuantifier"), "{plan:?}");
        let mut engine=orchiddb::engine::GraphEngine::in_memory().unwrap();
        let result=engine.cypher(query).await.unwrap();
        let batch=&result.returned.batch;
        assert_eq!(batch.num_rows(),2);
        for row in 0..2 {
            assert_eq!(arrow::util::display::array_value_to_string(batch.column(0),row).unwrap(),"1");
            assert_eq!(arrow::util::display::array_value_to_string(batch.column(2),row).unwrap(),"true");
            let random=arrow::util::display::array_value_to_string(batch.column(1),row).unwrap().parse::<f64>().unwrap();
            assert!((0.0..1.0).contains(&random));
        }
    }).await;
}
