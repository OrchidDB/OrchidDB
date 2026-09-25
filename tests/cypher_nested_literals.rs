use orchiddb::language::cypher::ast::{Clause, Expr};
use orchiddb::language::cypher::parser::{parse_query, parse_syntax};

#[test]
fn upstream_forty_nested_lists_and_maps_parse_on_small_thread_stack() {
    std::thread::Builder::new()
        .stack_size(512 * 1024)
        .spawn(|| {
            for map in [false, true] {
                let mut literal = if map {
                    "{}".to_string()
                } else {
                    "[]".to_string()
                };
                for depth in (1..40).rev() {
                    literal = if map {
                        format!("{{a{depth}: {literal}}}")
                    } else {
                        format!("[{literal}]")
                    };
                }
                let source = format!("RETURN {literal} AS literal");
                let query = parse_query(&source).expect("40 nested literals parse");
                let Clause::Return(ret) = &query.clauses[0] else {
                    panic!("return clause expected")
                };
                let mut expression = &ret.projection.items[0].expr;
                let mut levels = 0;
                loop {
                    levels += 1;
                    expression = match expression {
                        Expr::List(items) if items.len() == 1 => &items[0],
                        Expr::Map(items) if items.len() == 1 => &items[0].1,
                        Expr::List(items) if items.is_empty() => break,
                        Expr::Map(items) if items.is_empty() => break,
                        other => panic!("unexpected nested literal {other:?}"),
                    };
                }
                assert_eq!(levels, 40);
                assert!(!parse_syntax(&source).unwrap().parse_tree.is_empty());
            }
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn excessive_nesting_is_a_recoverable_error_and_quoted_brackets_are_ignored() {
    let source = format!("RETURN {}0{}", "[".repeat(257), "]".repeat(257));
    assert!(
        parse_query(&source)
            .unwrap_err()
            .to_string()
            .contains("nesting")
    );
    let source = format!("RETURN '{}'", "[".repeat(300));
    assert!(parse_query(&source).is_ok());
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn upstream_forty_nested_literal_results_preserve_all_levels() {
    let mut engine = orchiddb::engine::GraphEngine::in_memory().unwrap();
    for map in [false, true] {
        let mut literal = if map {
            "{}".to_string()
        } else {
            "[]".to_string()
        };
        for depth in (1..40).rev() {
            literal = if map {
                format!("{{a{depth}: {literal}}}")
            } else {
                format!("[{literal}]")
            };
        }
        let result = engine
            .cypher(&format!("RETURN {literal} AS literal"))
            .await
            .unwrap();
        assert_eq!(result.returned.batch.num_rows(), 1);
        let schema = result.returned.batch.schema();
        let native: serde_json::Value =
            serde_json::from_str(&schema.metadata()["crabgraph.cypher.typed_rows.v1"]).unwrap();
        let mut value = &native[0][0];
        for level in 0..40 {
            assert_eq!(value["type"], if map { "map" } else { "list" });
            if level == 39 {
                assert!(value["value"].as_array().unwrap().is_empty());
            } else {
                value = if map {
                    &value["value"][0][1]
                } else {
                    &value["value"][0]
                };
            }
        }
    }
}
