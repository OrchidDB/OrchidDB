#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, diagnostics::without_constant_folding,
    rel::{RelBackend,sql::{DuckDbExecutor,execute_lowered_sql,unparse,SqlDialect}}};
use orchiddb::language::cypher::{parser::parse_query,planner::CypherPlanner};

async fn run_mode(query:&str, nofold:bool) -> (String,Vec<String>) {
    let work=async {
        let graph=PropertyGraph::new();
        let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered=RelBackend::new().with_language_functions(true).lower(&plan,&graph).unwrap_or_else(|error| panic!("{query}: {error:?}"));
        let sql=unparse(&lowered,SqlDialect::DuckDb).unwrap();
        let result=execute_lowered_sql(&mut DuckDbExecutor::default(),&lowered).await.unwrap();
        let rows=(0..result.batch.num_rows()).map(|row| result.batch.columns().iter()
            .map(|array| arrow::util::display::array_value_to_string(array,row).unwrap())
            .collect::<Vec<_>>().join("|")).collect();
        (sql,rows)
    };
    if nofold { without_constant_folding(work).await } else { work.await }
}
async fn run(query:&str) -> (String,Vec<String>) { run_mode(query,true).await }
#[tokio::test]
async fn scalar_quantifiers_preserve_null_empty_duplicates_and_unknowns_without_folding() {
    let (sql,mut rows)=run("UNWIND [[],[true],[false],[null],[true,null],[false,null],[true,true,null],null,[true]] AS xs RETURN all(x IN xs WHERE x) AS a,any(x IN xs WHERE x) AS b,none(x IN xs WHERE x) AS c,single(x IN xs WHERE x) AS d").await;
    assert!(!sql.contains("row_number"),"{sql}");
    assert!(!sql.contains("GROUP BY"),"{sql}");
    assert!(sql.contains("FILTER (WHERE"),"{sql}");
    assert!(sql.to_lowercase().contains("unnest"),"{sql}");
    rows.sort();
    let mut expected=vec!["true|false|true|false","true|true|false|true","false|false|true|false","|||","|true|false|","false|||","|true|false|false","|||","true|true|false|true"];
    expected.sort();
    assert_eq!(rows,expected);
}
#[tokio::test]
async fn total_modulo_predicate_stays_runtime_sql_and_preserves_shadowing() {
    let (sql,mut rows)=run("UNWIND [[-9223372036854775808,2,null],[],[3,5],[2,4]] AS xs WITH xs,99 AS x RETURN x,single(x IN xs WHERE x % 2 = 0) AS result").await;
    assert!(sql.contains("% 2"),"{sql}");
    assert!(!sql.contains("row_number"),"{sql}");
    rows.sort();
    assert_eq!(rows,vec!["99|false","99|false","99|false","99|false"]);
}
#[tokio::test]
async fn correlated_predicate_uses_compact_sql_and_preserves_duplicate_rows() {
    let (sql,mut rows)=run("UNWIND [1,1,2] AS k WITH k,[1,2,null] AS xs RETURN k,any(x IN xs WHERE x > k) AS result").await;
    assert!(!sql.contains("row_number"),"{sql}");
    rows.sort();
    assert_eq!(rows,vec!["1|true","1|true","2|"]);
}

#[tokio::test]
async fn shadowed_list_and_internal_looking_item_names_keep_outer_correlation() {
    for item in ["xs","__quantifier_input_guard","__quantifier_item"] {
        let query=format!("UNWIND [[1,2],[3],[1,2]] AS xs RETURN any({item} IN xs WHERE {item}=2) AS result");
        let (sql,mut rows)=run(&query).await;
        assert!(!sql.contains("row_number"),"{sql}");
        assert!(!sql.contains("__w_sql_cte_quantifier"),"{sql}");
        rows.sort();
        assert_eq!(rows,vec!["false","true","true"],"{query}");
    }
}

#[tokio::test]
async fn literal_list_keeps_unknown_elements_without_input_guard() {
    let (sql,rows)=run("RETURN any(x IN [false,null] WHERE x) AS result").await;
    assert!(!sql.contains("__quantifier_input_guard"),"{sql}");
    assert!(sql.to_lowercase().contains("unnest"),"{sql}");
    assert!(!sql.contains("IS NULL THEN NULL"),"{sql}");
    assert_eq!(rows,vec![""]);
}
#[tokio::test]
async fn lean_unnest_scope_preserves_nullable_correlated_results_in_both_modes() {
    let query="UNWIND [[1,2],[],[null],null,[3],[1,2]] AS xs RETURN any(xs IN xs WHERE xs=2) AS result";
    for nofold in [true,false] {
        let (sql,mut rows)=run_mode(query,nofold).await;
        assert!(!sql.contains("__quantifier_input_guard"),"{sql}");
        assert!(sql.contains("IS NULL THEN NULL"),"{sql}");
        rows.sort();
        assert_eq!(rows,vec!["","","false","false","true","true"]);
    }
}

#[tokio::test]
async fn runtime_constructor_is_nonnull_even_with_nullable_elements() {
    let (sql,mut rows)=run("UNWIND [true,false,null] AS value RETURN any(x IN [value] WHERE x) AS result").await;
    assert!(!sql.contains("IS NULL THEN NULL"),"{sql}");
    assert!(sql.to_lowercase().contains("unnest"),"{sql}");
    rows.sort();
    assert_eq!(rows,vec!["","false","true"]);
}

#[tokio::test]
async fn correlated_comparisons_preserve_null_empty_and_shadowed_rows() {
    for nofold in [true,false] {
        let (sql,mut rows)=run_mode("UNWIND [1,null,1] AS k UNWIND [[1,null],[],null] AS xs RETURN k,all(x IN xs WHERE x=k),any(x IN xs WHERE x=k),none(x IN xs WHERE x=k),single(x IN xs WHERE x=k)",nofold).await;
        assert!(!sql.contains("row_number"),"{sql}");
        rows.sort();
        let mut expected=vec!["1||true|false|","1|true|false|true|false","1||||","||||","|true|false|true|false","||||","1||true|false|","1|true|false|true|false","1||||"];
        expected.sort();
        assert_eq!(rows,expected,"{sql}");
        let (sql,mut rows)=run_mode("UNWIND [1,2,1] AS __quantifier_item WITH __quantifier_item,[1,2] AS xs RETURN any(xs IN xs WHERE xs=__quantifier_item) AS result",nofold).await;
        assert!(!sql.contains("row_number"),"{sql}");
        rows.sort();
        assert_eq!(rows,vec!["true","true","true"]);
    }
}

#[tokio::test]
async fn outer_only_predicate_uses_cardinality_without_expanding_items() {
    for nofold in [true,false] {
        let (sql,mut rows)=run_mode("UNWIND [1,null,2] AS k UNWIND [[1,null],[],null] AS xs RETURN k,all(x IN xs WHERE k=1),any(x IN xs WHERE k=1),none(x IN xs WHERE k=1),single(x IN xs WHERE k=1)",nofold).await;
        rows.sort();
        assert!(!sql.contains("row_number"),"{sql}");
        assert!(!sql.contains("__quantifier_items"),"{sql}");
        let mut expected=vec!["1|true|true|false|false","1|true|false|true|false","1||||","||||","|true|false|true|false","||||","2|false|false|true|false","2|true|false|true|false","2||||"];
        expected.sort();
        assert_eq!(rows,expected,"{sql}");
    }
}

#[tokio::test]
async fn item_independent_quantifiers_preserve_singletons_and_scale_without_item_work() {
    for nofold in [true,false] {
        let (sql,mut rows)=run_mode("UNWIND [1,null,2,1] AS k WITH k,[null] AS xs RETURN k,all(x IN xs WHERE k=1),any(x IN xs WHERE k=1),none(x IN xs WHERE k=1),single(x IN xs WHERE k=1)",nofold).await;
        rows.sort();
        assert_eq!(rows,vec!["1|true|true|false|true","1|true|true|false|true","2|false|false|true|false","||||"]);
        assert!(!sql.contains("__quantifier_items"),"{sql}");
    }
    let (sql,rows)=run("UNWIND range(1,100000) AS i WITH collect(i) AS xs RETURN all(x IN xs WHERE 1=1),any(x IN xs WHERE 1=1),none(x IN xs WHERE 1=1),single(x IN xs WHERE 1=1)").await;
    assert_eq!(rows,vec!["true|true|false|false"]);
    assert!(!sql.to_lowercase().contains("unnest"),"{sql}");
    assert!(!sql.to_lowercase().contains("count("),"{sql}");
    // collect(i) may retain an ordinal to preserve list order; the quantifier
    // itself must add no per-item expansion or occurrence-number window.
    assert!(!sql.contains("__w_quantifier_row"),"{sql}");
}

#[tokio::test]
async fn nested_list_quantifiers_use_scalar_sql_without_occurrence_identity() {
    for (query, expected) in [
        ("RETURN single(x IN [['abc'], ['abc', 'def']] WHERE none(y IN x WHERE y = 'ghi')) AS result", "false"),
        ("RETURN single(x IN [['abc'], ['abc', 'def']] WHERE single(y IN x WHERE y = 'def')) AS result", "true"),
        ("WITH ['a'][0..0] AS empty RETURN all(x IN [['a'], empty, ['a',null]] WHERE any(y IN x WHERE y='a')) AS result", "false"),
        ("RETURN single(x IN [[false], [true,null]] WHERE all(y IN x WHERE y)) AS result", ""),
        ("WITH ['a'][0..0] AS empty RETURN none(x IN [empty, ['a',null]] WHERE any(y IN x WHERE y='b')) AS result", ""),
        ("WITH [1] AS target RETURN all(x IN [1] WHERE any(y IN [[1],[2]] WHERE y=target)) AS result", "true"),
        ("WITH [1] AS target RETURN all(x IN [1] WHERE any(y IN [[1],[2]] WHERE y>target)) AS result", "true"),
        ("WITH [1,2] AS target RETURN all(x IN [1] WHERE any(y IN [[1,null],[1,1]] WHERE y>target)) AS result", ""),
        ("WITH [1,null] AS target RETURN all(x IN [1] WHERE any(y IN [[1,null],[2,null]] WHERE y=target)) AS result", ""),
        ("RETURN all(x IN [1,2] WHERE any(__quantifier_item IN [3] WHERE __quantifier_item > x)) AS result", "true"),
        ("RETURN all(x IN [1,2] WHERE any(y IN [3] WHERE single(z IN [4] WHERE z > y AND z > x))) AS result", "true"),
        ("RETURN all(x IN [[[1]], [[2]]] WHERE any(y IN x WHERE single(z IN y WHERE z > 0))) AS result", "true"),
    ] {
        let (sql, rows) = run(query).await;
        assert_eq!(rows, vec![expected], "{query}\n{sql}");
        assert!(!sql.to_lowercase().contains("row_number"), "{sql}");
        assert!(!sql.contains("JOIN"), "{sql}");
        assert!(!sql.contains("GROUP BY"), "{sql}");
        assert!(!sql.contains("__w_sql_cte_apply"), "{sql}");
        assert!(sql.to_lowercase().contains("unnest"), "{sql}");
        assert!(sql.len() < 4000, "{} bytes: {sql}", sql.len());
    }
}

#[tokio::test]
async fn nested_scalar_quantifiers_preserve_outer_rows_and_lexical_shadowing() {
    for nofold in [true, false] {
        for (query, mut expected) in [
            ("UNWIND ['1','1','2',null] AS k WITH k,[['1'],['2',null],null] AS xs RETURN k,single(x IN xs WHERE any(y IN x WHERE y=k)) AS result", vec!["1|", "1|", "2|", "|"]),
            ("UNWIND [[['1'],['2']],[['1'],['2']],null] AS xs RETURN single(xs IN xs WHERE none(xs IN xs WHERE xs='2')) AS result", vec!["true", "true", ""]),
            ("UNWIND [1,2,1] AS __quantifier_item RETURN any(x IN [[1],[2]] WHERE any(y IN x WHERE y=__quantifier_item)) AS result", vec!["true", "true", "true"]),
        ] {
            let (sql, mut rows) = run_mode(query, nofold).await;
            rows.sort(); expected.sort();
            assert_eq!(rows, expected, "{query}\n{sql}");
            assert!(!sql.to_lowercase().contains("row_number"), "{sql}");
            assert!(!sql.contains("JOIN"), "{sql}");
        }
    }
}

#[tokio::test]
async fn nested_quantifiers_keep_native_lexical_capture_without_folding() {
    without_constant_folding(async {
        let mut engine=orchiddb::engine::GraphEngine::in_memory().unwrap();
        for (query, expected) in [
            ("RETURN single(x IN [['abc'], ['abc', 'def']] WHERE none(y IN x WHERE y='ghi')) AS result", vec!["false"]),
            ("UNWIND [1,2,1] AS k RETURN all(x IN [k] WHERE any(__quantifier_item IN [3] WHERE __quantifier_item > x)) AS result", vec!["true","true","true"]),
            ("RETURN single(x IN [[false], [true,null]] WHERE all(y IN x WHERE y)) AS result", vec![""]),
            ("WITH rand() AS r RETURN all(x IN [1] WHERE any(y IN [2] WHERE r=r)) AS result", vec!["true"]),
        ] {
            let result=engine.cypher(query).await.unwrap();
            let batch=&result.returned.batch;
            let rows=(0..batch.num_rows()).map(|row|batch.columns().iter()
                .map(|array|arrow::util::display::array_value_to_string(array,row).unwrap())
                .collect::<Vec<_>>().join("|")).collect::<Vec<_>>();
            assert_eq!(rows,expected,"{query}");
        }
    }).await;
}
