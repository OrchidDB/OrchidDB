#![cfg(feature = "duckdb")]
use orchiddb::ir::{
    catalog::PropertyGraph,
    rel::{
        RelBackend,
        sql::{DuckDbExecutor, execute_lowered_sql},
    },
};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

async fn sql_rows(query: &str) -> Vec<String> {
    let plan = CypherPlanner::new()
        .plan(&parse_query(query).unwrap())
        .unwrap();
    let lowered = RelBackend::new()
        .with_language_functions(true)
        .lower(&plan, &PropertyGraph::new())
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    let result = execute_lowered_sql(&mut DuckDbExecutor::default(), &lowered)
        .await
        .unwrap_or_else(|e| panic!("{query}: {e}"));
    (0..result.batch.num_rows())
        .map(|row| {
            result
                .batch
                .columns()
                .iter()
                .map(|column| arrow::util::display::array_value_to_string(column, row).unwrap())
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[tokio::test]
async fn literal_maps_scalarize_without_value_transport() {
    for (query, expected) in [
        (
            "WITH {existing:42,notMissing:null} AS m RETURN m.missing,m.notMissing,m.existing",
            "||42",
        ),
        (
            "WITH {name:'lower',Name:'upper',NULL:3,null:4} AS m RETURN m.name,m.Name,m.nAMe,m.`NULL`,m.`null`",
            "lower|upper||3|4",
        ),
        (
            "WITH {} AS m RETURN m.missing IS NULL,m.missing IS NOT NULL",
            "true|false",
        ),
        (
            "WITH {name:{name2:'baz'}} AS nestedMap RETURN nestedMap.name.name2",
            "baz",
        ),
        ("WITH {x:7} AS m WITH m AS renamed RETURN renamed.x", "7"),
        (
            "WITH {x:null} AS m RETURN 'x' IN keys(m),'missing' IN keys(m)",
            "true|false",
        ),
        ("WITH {x:42} AS m,null AS k RETURN m[k]", ""),
        (
            "WITH [123,{x:42}] AS l RETURN (l[1]).x,(l[1]).missing",
            "42|",
        ),
        ("WITH null AS m RETURN keys(m),keys(null)", "|"),
        ("WITH {x:42} AS m RETURN [m IN [1,2] | m]", "[1, 2]"),
    ] {
        assert_eq!(sql_rows(query).await, [expected], "{query}");
    }
}

#[tokio::test]
async fn map_scalarization_preserves_outer_row_occurrences() {
    assert_eq!(
        sql_rows("UNWIND [1,1,2] AS x WITH {v:42} AS m,x RETURN m.v,x").await,
        ["42|1", "42|1", "42|2"]
    );
    assert!(
        sql_rows("UNWIND [] AS x WITH {v:42} AS m RETURN m.v")
            .await
            .is_empty()
    );
}

#[test]
fn map_scalarization_keeps_unproved_map_evaluation_native() {
    for query in [
        "WITH {x:42,unused:rand()} AS m RETURN m.x",
        "WITH {x:42,unused:1/0} AS m RETURN m.x",
        "RETURN {x:42}",
    ] {
        let plan = CypherPlanner::new()
            .plan(&parse_query(query).unwrap())
            .unwrap();
        assert!(
            RelBackend::new()
                .lower(&plan, &PropertyGraph::new())
                .is_err(),
            "{query}"
        );
    }
}

#[tokio::test]
async fn literal_collections_fold_after_conversion_without_coercing_inputs() {
    for (query, expected) in [
        ("WITH [2,2.9] AS xs RETURN [x IN xs | toInteger(x)]", "[2, 2]"),
        ("WITH [2,2.9,'1.7'] AS xs RETURN [x IN xs | toInteger(x)]", "[2, 2, 1]"),
        ("WITH ['2','2.9','foo'] AS xs RETURN [x IN xs | toInteger(x)]", "[2, 2, ]"),
        ("WITH [3.4,3] AS xs RETURN [x IN xs | toFloat(x)]", "[3.4, 3.0]"),
        ("WITH [3.4,3,'5'] AS xs RETURN [x IN xs | toFloat(x)]", "[3.4, 3.0, 5.0]"),
        ("WITH ['1','2','foo'] AS xs RETURN [x IN xs | toFloat(x)]", "[1.0, 2.0, ]"),
        ("WITH [1,2,3] AS xs RETURN [x IN xs | toString(x)]", "[1, 2, 3]"),
        ("WITH [1,'a'] AS xs WITH xs AS ys RETURN ys[1]", "a"),
        ("WITH [[1,2,3]] AS xs RETURN 3 IN xs[0]", "true"),
        ("WITH [1,2,3] AS xs RETURN 3 IN xs[0..1]", "false"),
        ("WITH [1,'a'] AS xs RETURN [xs IN [3,4] | xs]", "[3, 4]"),
    ] {
        assert_eq!(sql_rows(query).await, [expected], "{query}");
    }
    assert_eq!(sql_rows("UNWIND [1,1,2] AS k WITH [1,'a'] AS xs,k RETURN xs[1],k").await,
        ["a|1", "a|1", "a|2"]);
    assert!(sql_rows("UNWIND [] AS k WITH [1,'a'] AS xs RETURN xs[1]").await.is_empty());
}

#[tokio::test]
async fn closed_heterogeneous_quantifiers_use_cypher_size_semantics() {
    for (query, expected) in [
        ("RETURN none(x IN [[1,2,3],['a']] WHERE size(x)=3)", "false"),
        ("RETURN single(x IN [[1,2,3],['a']] WHERE size(x)=3)", "true"),
        ("RETURN single(x IN [[1,2,3],['a'],[1,2,3]] WHERE size(x)=3)", "false"),
        ("RETURN any(x IN [['a'],[1,2,3]] WHERE size(x)=3)", "true"),
        ("RETURN all(x IN [['a'],[1,2,3]] WHERE size(x)=3)", "false"),
        ("RETURN all(x IN [[1],null] WHERE size(x)=1)", ""),
        ("RETURN all(x IN [] WHERE size(x)=1)", "true"),
    ] {
        assert_eq!(sql_rows(query).await, [expected], "{query}");
    }
}


#[tokio::test]
async fn nested_constructor_projection_uses_runtime_binding_without_folding() {
    orchiddb::ir::diagnostics::without_constant_folding(async {
        assert_eq!(sql_rows("UNWIND [7,7,9] AS v WITH {outer:{inner:v}} AS m RETURN m.outer.inner AS result").await,
            ["7","7","9"]);
        assert_eq!(sql_rows("WITH {outer:{inner:'baz'}} AS m RETURN m.outer.inner AS result").await,["baz"]);
    }).await;
}

#[tokio::test]
async fn nofold_does_not_misrepresent_maps_as_list_strings() {
    orchiddb::ir::diagnostics::without_constant_folding(async {
        for query in ["RETURN [{}] AS result", "RETURN [{a:1},{a:2}] AS result"] {
            let plan=CypherPlanner::new().plan(&parse_query(query).unwrap()).unwrap();
            assert!(RelBackend::new().with_language_functions(true).lower(&plan,&PropertyGraph::new()).is_err(),"{query}");
        }
        assert_eq!(sql_rows("RETURN ['Map({})'] AS result").await,["[Map({})]"]);
    }).await;
}
