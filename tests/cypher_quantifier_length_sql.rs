#![cfg(feature = "duckdb")]
use orchiddb::ir::{catalog::PropertyGraph, diagnostics::without_constant_folding,
    rel::{RelBackend,sql::{DuckDbExecutor,execute_lowered_sql,unparse,SqlDialect}}};
use orchiddb::language::cypher::{parser::parse_query,planner::CypherPlanner};

async fn run(query:&str) -> (String,Vec<String>) {
    without_constant_folding(async {
        let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
        let lowered=RelBackend::new().with_language_functions(true)
            .lower(&plan,&PropertyGraph::new()).unwrap_or_else(|error|panic!("{query}: {error:?}"));
        let sql=unparse(&lowered,SqlDialect::DuckDb).unwrap();
        let result=execute_lowered_sql(&mut DuckDbExecutor::default(),&lowered).await.unwrap();
        let rows=(0..result.batch.num_rows()).map(|row|result.batch.columns().iter()
            .map(|array|arrow::util::display::array_value_to_string(array,row).unwrap())
            .collect::<Vec<_>>().join("|")).collect();
        (sql,rows)
    }).await
}
fn compact(sql:&str) {
    assert!(!sql.to_lowercase().contains("row_number"),"{sql}");
    assert!(!sql.contains("GROUP BY"),"{sql}");
    assert!(!sql.contains("JOIN"),"{sql}");
    assert!(sql.to_lowercase().contains("unnest"),"{sql}");
}
#[tokio::test]
async fn string_length_is_evaluated_once_in_compact_quantifier_sql() {
    for (query,expected) in [
        ("RETURN single(x IN ['abc','ef','abc'] WHERE size(x)=3) AS result","false"),
        ("RETURN single(x IN ['é','🧭','é'] WHERE size(x)=2) AS result","true"),
        ("RETURN all(x IN ['é','🧭'] WHERE size(x)=1) AS result","true"),
    ] {
        let (sql,rows)=run(query).await;
        compact(&sql);
        assert_eq!(rows,vec![expected],"{sql}");
        // length() is a substring of character_length() as well.
        assert_eq!(sql.to_lowercase().matches("length(").count(),1,"{sql}");
        assert!(sql.len()<1300,"{}: {sql}",sql.len());
    }
}
#[tokio::test]
async fn length_predicates_preserve_null_empty_duplicate_and_correlated_rows() {
    let (sql,mut rows)=run("UNWIND [['abc'],['ab'],['abc',null],['abc','abc',null],[],null,['abc']] AS xs RETURN all(x IN xs WHERE size(x)=3),any(x IN xs WHERE size(x)=3),none(x IN xs WHERE size(x)=3),single(x IN xs WHERE size(x)=3)").await;
    compact(&sql); rows.sort();
    let mut expected=vec!["true|true|false|true","false|false|true|false","|true|false|","|true|false|false","true|false|true|false","|||","true|true|false|true"];
    expected.sort();assert_eq!(rows,expected,"{sql}");
    let (sql,mut rows)=run("UNWIND [1,2,1,null] AS n RETURN n,single(x IN ['a','bb',null] WHERE size(x)=n)").await;
    compact(&sql);rows.sort();assert_eq!(rows,vec!["1|","1|","2|","|"],"{sql}");
}
#[tokio::test]
async fn list_size_predicates_preserve_element_nulls_and_nested_lists() {
    for (query,expected) in [
        ("RETURN single(xs IN [[1],[1,2],[1,null,null]] WHERE size(xs)=2) AS result","true"),
        ("RETURN single(xs IN [[1],null] WHERE size(xs)=1) AS result",""),
        ("RETURN all(xs IN [[[1]],[[1,2]]] WHERE size(xs)=1) AS result","true"),
    ] {
        let (sql,rows)=run(query).await;compact(&sql);
        assert_eq!(rows,vec![expected],"{sql}");
        assert_eq!(sql.to_lowercase().matches("array_length(").count(),1,"{sql}");
    }
}
