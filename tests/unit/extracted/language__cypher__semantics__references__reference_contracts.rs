
    use super::*;
    fn check(text: &str, expected: &[&str]) {
        let query = crate::language::cypher::parse_query(text).unwrap();
        let mut bound = BTreeSet::new();
        let mut found = BTreeSet::new();
        collect_query_references(&query, &mut bound, &mut found);
        assert_eq!(found, expected.iter().map(|name| name.to_string()).collect(), "{text}");
        assert!(bound.is_empty(), "query-local bindings escaped: {text}");
    }
    macro_rules! case {
        ($name:ident, $query:expr, $expected:expr) => {
            #[test] fn $name() { check($query, $expected); }
        };
    }
    case!(create_start, "CREATE (n:P {x:outer}) RETURN n", &["outer"]);
    case!(create_edge, "CREATE (a:P)-[r:R {x:outer}]->(b:P) RETURN r", &["outer"]);
    case!(create_end, "CREATE (a:P)-[:R]->(b:P {x:outer}) RETURN b", &["outer"]);
    case!(merge_create_property, "MERGE (n:P {id:outer}) ON CREATE SET n.x=outer RETURN n", &["outer"]);
    case!(merge_match_map, "MERGE (n:P) ON MATCH SET n={x:outer} RETURN n", &["outer"]);
    case!(merge_create_map, "MERGE (n:P) ON CREATE SET n+={x:outer} RETURN n", &["outer"]);
    case!(merge_labels, "MERGE (n:P) ON MATCH SET n:Seen RETURN outer", &["outer"]);
    case!(set_property, "SET target.x=outer RETURN 1 AS value", &["outer", "target"]);
    case!(set_replace, "SET target={x:outer} RETURN 1 AS value", &["outer", "target"]);
    case!(set_merge, "SET target+={x:outer} RETURN 1 AS value", &["outer", "target"]);
    case!(set_labels, "SET target:Seen RETURN outer", &["outer", "target"]);
    case!(delete, "DELETE target RETURN outer", &["outer", "target"]);
    case!(detach_delete, "DETACH DELETE target RETURN outer", &["outer", "target"]);
    case!(create_scope, "CREATE (n:P {x:outer}) SET n.y=outer DELETE n RETURN outer", &["outer"]);
    case!(procedure_argument, "CALL example.echo(outer) YIELD value AS local RETURN local", &["outer"]);
    case!(procedure_predicate, "CALL db.labels() YIELD label WHERE label=outer RETURN label", &["outer"]);
    case!(call_import, "CALL { WITH outer RETURN outer AS local } RETURN local", &["outer"]);
    case!(with_scope, "UNWIND outer AS x WITH x AS local WHERE local>other RETURN local", &["other", "outer"]);
    case!(projection_order, "UNWIND outer AS x RETURN x AS local ORDER BY local+other", &["other", "outer"]);
    case!(projection_skip, "UNWIND outer AS x RETURN x SKIP offset LIMIT cap", &["cap", "offset", "outer"]);
    case!(standalone_labels, "CALL db.labels()", &[]);
    case!(standalone_types, "CALL db.relationshipTypes()", &[]);
    case!(standalone_keys, "CALL db.propertyKeys()", &[]);
    case!(standalone_unknown, "CALL example.echo(outer)", &["outer"]);
    #[test]
    fn nested_lambda_shadowing_preserves_the_callers_bound_scope() {
        let query = crate::language::cypher::parse_query(
            "RETURN [x IN outer | list_transform([x], x -> x+external)] AS values").unwrap();
        let mut bound: BTreeSet<String> = ["outer", "x"].into_iter().map(String::from).collect();
        let before = bound.clone();
        let mut found = BTreeSet::new();
        collect_query_references(&query, &mut bound, &mut found);
        assert_eq!(bound, before);
        assert_eq!(found, BTreeSet::from(["external".into()]));
    }
    #[test]
    fn local_outputs_are_removed_without_removing_external_inputs() {
        for text in [
            "CREATE (a:P)-[r:R]->(b:P) RETURN a,b,r",
            "MERGE (a:P)-[r:R]->(b:P) RETURN a,b,r",
            "CALL db.labels() YIELD label AS a RETURN a",
            "CALL { RETURN 1 AS a } RETURN a",
        ] {
            let query = crate::language::cypher::parse_query(text).unwrap();
            let mut refs = ["a", "b", "r", "outer"].into_iter().map(String::from).collect();
            remove_query_outputs(&query, &mut refs);
            assert!(refs.contains("outer"));
            assert!(!refs.contains("a"), "{text}");
        }
    }
