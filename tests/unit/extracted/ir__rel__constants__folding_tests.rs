
    use super::*;

    #[test]
    fn engine_calls_are_not_scalar_constants_even_with_null_arguments() {
        for name in [
            "unknown_function",
            "stats",
            "uuid_extract_version",
            "tinker_search",
            "gremlin_cast_string",
            "requested_property_values",
            "properties_list",
            "property_map",
            "value_map",
            "value_map_tokens",
            "element_map",
            "gremlin_id",
        "gremlin_merge_matches",
        ] {
            let call = IrExpr::Call {
                name: name.into(),
                args: vec![IrExpr::Lit(Lit::Null)],
            };
            assert!(!expr_is_constant(&call, &[]), "{name}");
            let nested = IrExpr::Call {
                name: "coalesce".into(),
                args: vec![call, IrExpr::Lit(Lit::Int(1))],
            };
            assert!(!expr_is_constant(&nested, &[]), "nested {name}");
        }
        assert!(constant_foldable_function("UPPER"));
        assert!(constant_foldable_function("tointeger"));
        assert!(!constant_foldable_function("random"));
    }
