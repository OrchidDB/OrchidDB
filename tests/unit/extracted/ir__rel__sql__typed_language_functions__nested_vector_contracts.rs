
    use super::*;

    #[test]
    fn folded_nested_vectors_preserve_nulls_and_selected_values() {
        let connection = Connection::open_in_memory().unwrap();
        register(&connection).unwrap();
        for element in ["INTEGER", "BIGINT", "DOUBLE", "VARCHAR"] {
            let query = format!("SELECT list_transform(orchid_cypher_quantifier_list(CAST([NULL,NULL] AS {element}[])), lambda x: orchid_cypher_size(x) IS NULL) IS NOT DISTINCT FROM [true,true]");
            assert!(connection.query_row(&query, [], |r| r.get::<_,bool>(0)).unwrap(), "{query}");
        }
        for (source, expected) in [
            ("CAST([[NULL,NULL],NULL,[]] AS INTEGER[][])", "[2,NULL,0]"),
            ("CAST([[],[NULL],[1,NULL,2],NULL] AS INTEGER[][])", "[0,1,3,NULL]"),
            ("list_select(CAST([[NULL,NULL],NULL,[],[1]] AS INTEGER[][]),[4,2,1,4])", "[1,NULL,2,1]"),
            ("CAST([] AS INTEGER[][])", "[]::BIGINT[]"),
            ("CAST(NULL AS INTEGER[][])", "NULL::BIGINT[]"),
        ] {
            let query = format!("SELECT list_transform(orchid_cypher_quantifier_list({source}), lambda x: orchid_cypher_size(x)) IS NOT DISTINCT FROM {expected}");
            assert!(connection.query_row(&query, [], |r| r.get::<_,bool>(0)).unwrap(), "{query}");
        }
        let query = "SELECT list_transform(orchid_cypher_quantifier_list(list_select([1,NULL,3],[3,2,1,3])), lambda x: orchid_cypher_scalar_text(x)) IS NOT DISTINCT FROM ['3',NULL,'1','3']";
        assert!(connection.query_row(query, [], |r| r.get::<_,bool>(0)).unwrap());
    }
