
    use super::*;

    #[test]
    fn query_function_resolution_preserves_case_aliases_and_namespaces() {
        let mapping = GraphMapping::new();
        let provider = MappingContextProvider::new(&mapping);
        assert_eq!(provider.get_function_meta("ChAr_LeNgTh").unwrap().name(),"character_length");
        assert_eq!(provider.get_aggregate_meta("MeAn").unwrap().name(), "avg");
        assert_eq!(
            provider.get_window_meta("RoW_NuMbEr").unwrap().name(),
            "row_number"
        );
        assert!(provider.get_function_meta("row_number").is_none());
        assert!(provider.get_aggregate_meta("character_length").is_none());
        assert!(provider.get_window_meta("__missing").is_none());
        // Table dependency tracking remains scoped to each mapping query.
        let other = MappingContextProvider::new(&mapping);
        provider.requested.borrow_mut().insert("customers".into());
        assert!(other.requested.borrow().is_empty());
        mapping
            .plan_sql("SELECT ChAr_LeNgTh('hello'), MeAn(3) OVER (), RoW_NuMbEr() OVER ()")
            .unwrap();
        mapping.plan_sql("SELECT sqrt(9.0), char_length('hello')").unwrap();
        mapping.plan_sql("SELECT fn.sqrt(9.0), fn.char_length('hello')").unwrap();
        assert!(provider.get_function_meta("fn.sqrt").is_some());
        assert!(provider.udf_names().contains(&"fn.sqrt".into()));
    }

    #[test]
    fn toml_round_trip() {
        let toml = r#"
# BYOS mapping
[node.Person]
table = "customers"
id = "cust_id"

[node.Person.properties]
name = "full_name"
age = "age"

[node.Vip]
query = "SELECT cust_id, full_name FROM customers WHERE age >= 30"
id = "cust_id"

[node.Vip.properties]
name = "full_name"

[edge.ORDERED]
table = "orders"
src = "cust_id"
dst = "order_id"
src_label = "Person"
dst_label = "Order"
edge_id = "order_id"

[edge.ORDERED.properties]
total = "total"
"#;
        let mapping = GraphMapping::from_toml(toml).expect("parse");
        let person = mapping.node("Person").expect("person");
        assert_eq!(person.source, MappedSource::Table("customers".into()));
        assert_eq!(person.id_column.columns(), &["cust_id"]);
        assert_eq!(person.properties.get("name").unwrap(), "full_name");
        let vip = mapping.node("Vip").expect("vip");
        assert!(matches!(vip.source, MappedSource::Query(_)));
        let ordered = mapping.edge("ORDERED").expect("ordered");
        assert_eq!(ordered.src_label, "Person");
        assert_eq!(
            ordered.id_column.as_ref().map(KeyColumns::columns),
            Some(["order_id".to_string()].as_slice())
        );
        assert_eq!(ordered.properties.get("total").unwrap(), "total");

        let reparsed = GraphMapping::from_toml(&mapping.to_toml()).expect("reparse");
        assert_eq!(reparsed.nodes, mapping.nodes);
        assert_eq!(reparsed.edges, mapping.edges);
    }

    #[test]
    fn toml_rejects_bad_sections_and_values() {
        assert!(GraphMapping::from_toml("[wat.Person]\ntable = \"t\"").is_err());
        assert!(GraphMapping::from_toml("[node.Person]\nid = \"x\"").is_err());
        assert!(
            GraphMapping::from_toml("[node.Person]\ntable = \"t\"\nquery = \"q\"\nid = \"x\"")
                .is_err()
        );
        assert!(GraphMapping::from_toml("[node.Person]\ntable = unquoted\nid = \"x\"").is_err());
    }

    #[test]
    fn query_planning_resolves_registered_tables() {
        use arrow::datatypes::{DataType, Field, Schema};
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "customers",
            Arc::new(Schema::new(vec![
                Field::new("cust_id", DataType::Int64, false),
                Field::new("age", DataType::Int64, true),
            ])),
        );
        let plan = mapping
            .plan_sql("SELECT cust_id FROM customers WHERE age > 30")
            .expect("plan view sql");
        assert!(format!("{}", plan.display_indent()).contains("TableScan: customers"));
        assert!(mapping.plan_sql("SELECT * FROM missing").is_err());
    }
