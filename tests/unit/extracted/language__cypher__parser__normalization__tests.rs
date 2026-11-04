
    use super::normalize_cypher_extensions;

    #[test]
    fn escaped_relationship_types_are_not_list_slices() {
        for query in ["MATCH ()-[:`A:B`]->() RETURN 1", "MATCH ()-[r:`A:B`]->() RETURN r"] {
            assert_eq!(normalize_cypher_extensions(query), query);
        }
    }

    #[test]
    fn escaped_function_identifiers_survive_all_extension_passes() {
        for name in [
            "&&",
            "|",
            "<<",
            "!",
            "contains",
            "count { x }",
            "list_transform(x, x->x)",
            "a[1:2]",
            "x := y",
            "a``&&b",
            "__graph_escaped_identifier_0",
        ] {
            let input = format!("RETURN `{name}`(NULL, NULL)");
            assert_eq!(normalize_cypher_extensions(&input), input);
        }
        let input = "RETURN 'literal `&&`', `a[1:2]`(NULL), 1 & 2";
        let normalized = normalize_cypher_extensions(input);
        assert!(normalized.contains("'literal `&&`'"), "{normalized}");
        assert!(normalized.contains("`a[1:2]`(NULL)"), "{normalized}");
        assert!(normalized.contains("bitwise_and(1, 2)"), "{normalized}");
    }
