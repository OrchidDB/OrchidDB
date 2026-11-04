
    use super::*;
    use crate::language::cypher::parser::parse_query;

#[test]
fn update_and_pagination_scope_errors_are_reported_before_execution() {
    for source in [
        "MATCH (n) SET missing.value = 1 RETURN n",
        "MATCH (n) SET missing = {value:1} RETURN n",
        "MATCH (n) SET missing += {value:1} RETURN n",
        "MATCH (n) SET missing:Label RETURN n",
        "MATCH (n) RETURN n SKIP missing",
        "MATCH (n) RETURN n LIMIT missing",
        "UNWIND [missing] AS value RETURN value",
        "MATCH (n) RETURN EXISTS { MATCH (x) WHERE missing.value = 1 } AS result",
    ] {
        let error=analyze_error(source);
        assert!(error.contains("missing"),"{source}: {error}");
    }
    for source in [
        "WITH 1 AS p CREATE p=(a)-[:R]->(b) RETURN p",
        "CREATE p=(a)-[:R]->(b), p=(c)-[:R]->(d) RETURN p",
    ] {
        let error=analyze_error(source);
        assert!(error.contains("already exists"),"{source}: {error}");
    }
}

#[test]
fn implicit_procedure_outputs_match_standalone_and_yield_star_contracts() {
    for (name,field) in [("db.labels","label"),("DB.RELATIONSHIPTYPES","relationshipType"),
        ("db.propertyKeys","propertyKey"),("custom.write","value")] {
        let mut query=parse_query(&format!("CALL {name}()")).unwrap();
        let Clause::Call(clause)=&mut query.clauses[0] else {panic!()};
        clause.signature=None;
        clause.yields.clear();
        for standalone in [false,true] {
            clause.standalone=standalone;
            clause.yield_all=!standalone;
            assert_eq!(procedure_yields(clause),(vec![field.to_string()],vec![field.to_string()]));
        }
    }
}

#[test]
fn relationship_order_diagnostics_identify_fixed_and_recursive_bindings() {
    for relationship in ["r:R","r:R*1..2"] {
        let query=parse_query(&format!("MATCH (a)-[{relationship}]->(b) RETURN r ORDER BY r")).unwrap();
        let error=analyze_query(&query).unwrap_err();
        assert!(error.to_string().contains("Order by REL is not supported"),"{error}");
    }
    analyze_query(&parse_query("MATCH (a)-[r:R]->(b) RETURN r.weight ORDER BY r.weight").unwrap()).unwrap();
    analyze_query(&parse_query("MATCH (a)-[r:R*1..2]->(b) RETURN size(r) AS hops ORDER BY hops").unwrap()).unwrap();
}

    fn analyze_outputs(source: &str) -> CypherPlanResult<Vec<String>> {
        let query = parse_query(source).expect("parse query");
        analyze_query(&query).map(|analyzed| analyzed.output_fields)
    }

    fn analyze_error(source: &str) -> String {
        let query = parse_query(source).expect("parse query");
        analyze_query(&query)
            .expect_err("semantic analysis should fail")
            .to_string()
    }

    #[test]
    fn reports_visible_output_fields() {
        let output_fields = analyze_outputs("MATCH (person) RETURN person").expect("analyze");
        assert_eq!(output_fields, vec!["person".to_string()]);
    }

    #[test]
    fn rejects_projection_references_out_of_scope() {
        let err = analyze_error("MATCH (person) RETURN missing");
        assert!(err.contains("Binder exception: Variable missing is not in scope."));
    }

    #[test]
    fn rejects_unwind_rebinding_visible_name() {
        let err = analyze_error("MATCH (person) UNWIND [1] AS person RETURN person");
        assert!(err.contains("Binder exception: Variable person already exists."));
    }

    #[test]
    fn rejects_node_reused_as_relationship() {
        let err = analyze_error("MATCH (r) MATCH ()-[r]-() RETURN r");
        assert!(err.contains("Binder exception: r has data type NODE but REL was expected."));
    }

    #[test]
    fn rejects_relationship_reused_as_node() {
        let err = analyze_error("MATCH ()-[r]-() MATCH (r) RETURN r");
        assert!(err.contains("Binder exception: Cannot bind r as node pattern."));
    }

    #[test]
    fn rejects_value_alias_reused_as_node() {
        let err = analyze_error("WITH 123 AS n MATCH (n) RETURN n");
        assert!(err.contains("Binder exception: Cannot bind n as node pattern."));
    }

    #[test]
    fn same_pattern_node_reuse_takes_precedence_over_relationship() {
        let err = analyze_error("MATCH ()-[r]-(r) RETURN r");
        assert!(err.contains("Binder exception: r has data type NODE but REL was expected."));
    }

    #[test]
    fn rejects_repeated_relationship_name_in_one_match_clause() {
        let err = analyze_error("MATCH ()-[r]->(), ()-[r]->() RETURN r");
        assert!(err.contains(
            "Binder exception: Bind relationship r to relationship with same name is not supported."
        ));
    }

    #[test]
    fn rejects_unwind_of_non_list_literal() {
        let err = analyze_error("UNWIND 1 AS a RETURN a");
        assert!(err.contains("Binder exception: 1 has data type INT64 but LIST was expected."));
    }

    #[test]
    fn rejects_unwind_of_path_binding() {
        let err = analyze_error("MATCH p = ()-[*1..2]->() UNWIND p AS x RETURN x");
        assert!(
            err.contains("Binder exception: p has data type PATH but LIST was expected.")
        );
    }

    #[test]
    fn rejects_rebinding_visible_path_variable() {
        let err = analyze_error("MATCH (p) MATCH p = ()-[]-() RETURN p");
        assert!(err.contains("SyntaxError: VariableAlreadyBound"));
    }

    #[test]
    fn union_requires_matching_output_names() {
        // openCypher Union1/Union2: columns are matched by name, not position.
        let query = parse_query("MATCH (p:person) RETURN p.age UNION ALL MATCH (p1:person) RETURN p1.age").unwrap();
        let err = analyze_query(&query).unwrap_err();
        assert_eq!(err.classification(), Some(("SyntaxError", "DifferentColumnsInUnion")));
        assert_eq!(analyze_outputs("MATCH (p:person) RETURN p.age AS age UNION ALL MATCH (p1:person) RETURN p1.age AS age").unwrap(), vec!["age"]);
    }

    #[test]
    fn rejects_union_arity_mismatch() {
        let query = parse_query("RETURN 1 AS value, 2 AS extra UNION RETURN 1 AS value").unwrap();
        let err = analyze_query(&query).unwrap_err();
        assert_eq!(err.classification(), Some(("SyntaxError", "DifferentColumnsInUnion")));
    }

    #[test]
    fn union_does_not_infer_property_types_from_names() {
        let output = analyze_outputs("MATCH (p:person) RETURN p.fName AS value UNION ALL MATCH (p1:person) RETURN p1.age AS value").unwrap();
        assert_eq!(output, vec!["value"]);
    }

    #[test]
    fn rejects_mixed_union_and_union_all() {
        let err = analyze_error("RETURN 1 AS value UNION ALL RETURN 2 AS value UNION RETURN 3 AS value");
        assert!(err.contains("Union and union all can not be used together."), "{err}");
    }

    #[test]
    fn accepts_with_order_by_without_skip_or_limit() {
        assert!(analyze_outputs("MATCH (a:person) WITH a.age AS k ORDER BY k RETURN k").is_ok());
    }

    #[test]
    fn order_by_checks_known_types_without_guessing_property_types() {
        let err = analyze_error("MATCH (a:person) RETURN a ORDER BY a");
        assert!(err.contains("Order by NODE is not supported."), "{err}");
        assert!(analyze_outputs("MATCH (a:person) RETURN a ORDER BY a.workedHours").is_ok());
    }

    #[test]
    fn rejects_known_invalid_arithmetic_type_pairs() {
        let err = analyze_error("RETURN true + 1");
        assert!(err.contains("+(BOOL,INT64)"), "{err}");
        // Property names do not establish a static type without a schema.
        assert!(analyze_outputs("MATCH (a:person) RETURN a.age + 'hh'").is_ok());
        assert!(analyze_outputs("MATCH (a:person) WHERE id(a) + 1 < id(a) RETURN a").is_ok());
    }

    #[test]
    fn coalesce_requires_arguments_but_allows_mixed_types() {
        let err = analyze_error("RETURN coalesce()");
        assert!(err.contains("COALESCE requires at least one argument"), "{err}");
        assert!(analyze_outputs("RETURN coalesce(1, 'hello')").is_ok());
    }


#[test]
fn direct_ast_anonymous_outputs_get_distinct_stable_semantic_names() {
    let mut query=parse_query("WITH 1 AS n RETURN n,n+1 AS first,n+2 AS second").unwrap();
    let Clause::Return(result)=query.clauses.last_mut().unwrap() else {panic!("return clause")};
    for item in result.projection.items.iter_mut().skip(1) {
        item.alias=None;
        item.explicit_alias=false;
    }
    let original=query.clone();
    for _ in 0..2 {
        let analyzed=analyze_query(&query).unwrap();
        assert_eq!(analyzed.output_fields,["n","__semantic_expr_0","__semantic_expr_1"]);
        assert_eq!(query,original,"analysis must leave the caller's AST intact");
    }
}

#[test]
fn relationship_targets_require_an_exported_node_binding() {
    use crate::language::cypher::parser::parse_query;
    let query=parse_query("MATCH (n:Person) RETURN n AS target").unwrap();
    validate_relationship_target(&query,"target").unwrap();
    assert!(validate_relationship_target(&query,"n").unwrap_err().to_string().contains("relationship target must be a node"));
    for text in ["RETURN 1 AS target","RETURN null AS target","MATCH (n:Person) RETURN [n] AS target"] {
        assert!(validate_relationship_target(&parse_query(text).unwrap(),"target").unwrap_err().to_string().contains("relationship target must be a node"));
    }
}

#[test]
fn unaliased_ast_projection_retains_known_and_unknown_binding_types() {
    use crate::language::cypher::parser::parse_query;
    let mut query=parse_query("RETURN value").unwrap();
    let Clause::Return(clause)=&mut query.clauses[0] else {panic!()};
    clause.projection.items[0].alias=None;
    for kind in [None,Some(BindingKind::Node),Some(BindingKind::Date),Some(BindingKind::Unknown)] {
        let mut scope=SemanticScope::default();
        if let Some(kind)=kind {scope.insert("value",kind);}
        let outputs=SemanticAnalyzer::default().projection_outputs(&clause.projection,&scope);
        assert_eq!(outputs.len(),1);
        assert_eq!(outputs[0].name,"value");
        assert_eq!(outputs[0].kind,kind.unwrap_or(BindingKind::Unknown));
    }
    assert_eq!(BindingKind::Unknown.cypher_type_name(),"ANY");
}
