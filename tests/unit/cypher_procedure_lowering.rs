use super::*;
use crate::language::cypher::ast::ProcedureYieldItem;

fn call(name: &str) -> ProcedureCallClause {
    ProcedureCallClause {
        name: name.into(),
        args: vec![],
        yields: vec![],
        yield_all: false,
        implicit_arguments: false,
        signature: None,
        predicate: None,
        standalone: true,
    }
}

#[test]
fn direct_procedure_lowering_preserves_default_outputs_and_input_cardinality() {
    for (name, field, mode) in [
        ("db.labels", "label", ProcedureMode::Read),
        (
            "DB.RELATIONSHIPTYPES",
            "relationshipType",
            ProcedureMode::Read,
        ),
        ("db.propertyKeys", "propertyKey", ProcedureMode::Read),
        ("custom.write", "value", ProcedureMode::Write),
    ] {
        for standalone in [false, true] {
            let mut clause = call(name);
            clause.standalone = standalone;
            clause.yield_all = !standalone;
            let mut lowerer = Lowerer::new();
            let node = lower_call(&mut lowerer, Node::GraphOneRow, &clause).unwrap();
            let Node::GraphProcedureCall {
                yields,
                input,
                mode: actual,
                ..
            } = node
            else {
                panic!()
            };
            assert_eq!(yields, vec![field.to_string()]);
            assert_eq!(actual, mode);
            assert_eq!(input.is_none(), standalone);
            assert!(lowerer.visible_set().contains(field));
        }
    }
}

#[test]
fn procedure_yield_errors_do_not_mutate_the_callers_scope() {
    for failure in ["implicit_read", "duplicate", "rebound"] {
        let mut lowerer = Lowerer::new();
        lowerer.replace_scope(vec!["outside".to_string()]);
        let scope = lowerer.visible_set();
        let mut clause = call("db.labels");
        clause.standalone = false;
        let expected = match failure {
            "duplicate" => {
                clause.yields = vec![
                    ProcedureYieldItem {
                        field: "label".into(),
                        alias: "result".into()
                    };
                    2
                ];
                "duplicate output variables: result"
            }
            "rebound" => {
                clause.yields = vec![ProcedureYieldItem {
                    field: "label".into(),
                    alias: "outside".into(),
                }];
                "rebind variables already in scope: outside"
            }
            _ => "requires explicit YIELD",
        };
        let error = lower_call(&mut lowerer, Node::GraphOneRow, &clause).unwrap_err();
        assert!(error.to_string().contains(expected), "{error}");
        assert_eq!(lowerer.visible_set(), scope);
    }
}

#[test]
fn embedded_write_without_yield_preserves_its_input_and_has_no_outputs() {
    let mut clause = call("custom.write");
    clause.standalone = false;
    let mut lowerer = Lowerer::new();
    lowerer.replace_scope(vec!["outside".to_string()]);
    let node = lower_call(&mut lowerer, Node::GraphOneRow, &clause).unwrap();
    let Node::GraphProcedureCall {
        yields,
        input,
        mode,
        ..
    } = node
    else {
        panic!()
    };
    assert!(yields.is_empty());
    assert_eq!(mode, ProcedureMode::Write);
    assert!(matches!(input.as_deref(), Some(Node::GraphOneRow)));
    assert_eq!(lowerer.visible_fields(), vec!["outside".to_string()]);
}

#[test]
fn failed_clause_lowering_restores_scope_for_the_next_query() {
    use crate::language::cypher::parser::parse_query;
    for (query, expected) in [
        (
            "UNWIND [1] AS x UNWIND [2] AS x RETURN x",
            "Variable x already exists",
        ),
        ("CREATE p=(a), p=(b) RETURN p", "Variable p already exists"),
        (
            "MATCH p=(a) CREATE p=(b) RETURN p",
            "Variable p already exists",
        ),
        (
            "SET missing = {x:1} RETURN 1",
            "Variable missing is not in scope",
        ),
        (
            "SET missing += {x:1} RETURN 1",
            "Variable missing is not in scope",
        ),
    ] {
        let mut lowerer = Lowerer::new();
        lowerer.replace_scope(vec!["outside".to_string()]);
        let scope = lowerer.visible_set();
        let error = lowerer
            .lower_query_body(&parse_query(query).unwrap())
            .unwrap_err();
        assert!(error.to_string().contains(expected), "{query}: {error}");
        assert_eq!(lowerer.visible_set(), scope, "{query}");
        lowerer
            .lower_query_body(&parse_query("RETURN outside AS result").unwrap())
            .unwrap();
        assert_eq!(lowerer.visible_fields(), vec!["result".to_string()]);
    }
}

#[test]
fn create_relationship_rebinding_is_rejected_before_constructing_an_edge() {
    use crate::language::cypher::parser::parse_query;
    for query in ["CREATE (a)-[a:R]->(b) RETURN a",
        "CREATE (a)-[r:R]->(b)-[r:R]->(c) RETURN r",
        "MATCH (a)-[r:R]->(b) CREATE (a)-[r:R]->(b) RETURN r"] {
        let mut lowerer=Lowerer::new();lowerer.replace_scope(vec!["outside".into()]);
        let error=lowerer.lower_query_body(&parse_query(query).unwrap()).unwrap_err();
        assert_eq!(error.classification(),Some(("SyntaxError","VariableAlreadyBound")),"{query}: {error}");
        assert!(error.to_string().contains("CREATE cannot reuse a bound relationship"),"{query}: {error}");
        assert_eq!(lowerer.visible_fields(),vec!["outside".to_string()]);
    }
}
