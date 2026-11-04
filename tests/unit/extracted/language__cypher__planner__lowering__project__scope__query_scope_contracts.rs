
    use super::*;

    #[test]
    fn query_scope_walkers_preserve_external_inputs_across_clause_families() {
        let cases: &[(&str, &[&str])] = &[
            ("MATCH (a {v: external})-[r:R {v: external}]->(b {v: external}) WHERE external > 0 RETURN a AS answer", &["external"]),
            ("CREATE (a {v: external})-[r:R {v: external}]->(b {v: external}) SET a.x = external, a = {x: external}, a += {y: external}, a:Seen DELETE b RETURN a AS answer", &["external"]),
            ("MERGE (a {v: external})-[r:R {v: external}]->(b {v: external}) RETURN a AS answer", &["external"]),
            ("MERGE (a:P) ON CREATE SET a.x = external, a += {x: external}, a:Created ON MATCH SET a = {x: external} RETURN a AS answer", &["external"]),
            ("CALL custom.rows(external) YIELD value AS local WHERE local > threshold RETURN local AS answer", &["external", "threshold"]),
            ("UNWIND external AS x WITH x AS local ORDER BY local SKIP offset LIMIT amount RETURN local AS answer", &["external", "offset", "amount"]),
            ("WITH external AS local RETURN local AS answer UNION ALL WITH other AS local RETURN local AS answer", &["external", "other"]),
            ("CALL { WITH external RETURN external AS local } RETURN local AS answer", &["external"]),
        ];
        for (text, expected) in cases {
            let query = crate::language::cypher::parser::parse_query(text).unwrap_or_else(|e|panic!("{text}: {e}"));
            let initial = BTreeSet::new();
            let mut bound = initial.clone();
            let mut refs = BTreeSet::new();
            collect_query_variable_references(&query,&mut bound,&mut refs);
            remove_local_query_bindings(&query,&initial,&mut refs);
            assert_eq!(bound,initial,"nested queries must not modify the caller scope");
            assert_eq!(refs,expected.iter().map(|name|name.to_string()).collect(),"{text}");
        }
    }

    #[test]
    fn implicit_procedure_yields_are_local_and_case_insensitive() {
        for (name,field) in [("db.labels","label"),("DB.RELATIONSHIPTYPES","relationshipType"),("db.propertyKeys","propertyKey"),("custom.rows","value")] {
            let text=format!("CALL {name}() YIELD *");
            let query=crate::language::cypher::parser::parse_query(&text).unwrap();
            let mut refs=BTreeSet::from([field.to_string(),"external".into()]);
            remove_local_query_bindings(&query,&BTreeSet::new(),&mut refs);
            assert_eq!(refs,BTreeSet::from(["external".into()]),"{text}");
        }
    }
