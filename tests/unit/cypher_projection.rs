//! Scope contracts use independent expected free-variable sets, including shadowing.
use super::*;
use crate::language::cypher::parser::parse_query;
fn expr(text: &str) -> Expr {
    let query = parse_query(&format!("RETURN {text} AS result")).unwrap();
    let Clause::Return(clause) = &query.clauses[0] else {
        panic!()
    };
    clause.projection.items[0].expr.clone()
}
fn names(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|s| s.to_string()).collect()
}
fn refs(text: &str, expected: &[&str]) {
    assert_eq!(
        scope::free_variable_names(&expr(text)),
        names(expected),
        "{text}"
    );
}
macro_rules! free {
    ($name:ident,$text:expr,$expected:expr) => {
        #[test]
        fn $name() {
            refs($text, $expected);
        }
    };
}
free!(variable, "outside", &["outside"]);
free!(property, "outside.name", &["outside"]);
free!(labels, "outside:Person", &["outside"]);
free!(unary, "-outside", &["outside"]);
free!(binary, "left + right", &["left", "right"]);
free!(null_test, "outside IS NULL", &["outside"]);
free!(nonnull_test, "outside IS NOT NULL", &["outside"]);
free!(
    string_predicate,
    "left STARTS WITH right",
    &["left", "right"]
);
free!(function, "coalesce(left,right)", &["left", "right"]);
free!(
    case_all,
    "CASE base WHEN condition THEN yes ELSE no END",
    &["base", "condition", "yes", "no"]
);
free!(list, "[a,b,a]", &["a", "b"]);
free!(map, "{key:a,other:b}", &["a", "b"]);
free!(
    comprehension,
    "[x IN source WHERE x > threshold | x + offset]",
    &["source", "threshold", "offset"]
);
free!(comprehension_collection_shadow, "[x IN x | x]", &["x"]);
free!(
    nested_shadow,
    "[x IN source | [x IN x | x + outside]]",
    &["source", "outside"]
);
free!(
    reduce,
    "list_reduce(source,(acc,x) -> acc + x + outside)",
    &["source", "outside"]
);
free!(
    reduce_shadow,
    "list_reduce(acc,(acc,x) -> acc + x)",
    &["acc"]
);
free!(
    transform,
    "list_transform(source,x -> x + outside)",
    &["source", "outside"]
);
free!(
    filter,
    "list_filter(source,x -> x > outside)",
    &["source", "outside"]
);
free!(
    all,
    "all(x IN source WHERE x > outside)",
    &["source", "outside"]
);
free!(
    any,
    "any(x IN source WHERE x > outside)",
    &["source", "outside"]
);
free!(
    none,
    "none(x IN source WHERE x > outside)",
    &["source", "outside"]
);
free!(
    single,
    "single(x IN source WHERE x > outside)",
    &["source", "outside"]
);
free!(parameter_and_literal, "[$parameter,1,null,true]", &[]);

#[test]
fn local_scope_restores_shadowed_names_after_nested_expression() {
    let expression = expr("[[x IN source | x], x]");
    let mut bound = names(&["x"]);
    let mut found = BTreeSet::new();
    scope::collect_free_variables(&expression, &mut bound, &mut found);
    assert_eq!(found, names(&["source"]));
    assert_eq!(bound, names(&["x"]));
    assert_eq!(
        scope::free_variable_names(&expression),
        names(&["source", "x"])
    );
}
macro_rules! candidate {
    ($name:ident,$text:expr,$expected:expr) => {
        #[test]
        fn $name() {
            let expression = expr($text);
            let candidates = names(&["outer", "n", "r", "p"]);
            assert_eq!(
                scope::expression_candidate_refs(&expression, &candidates),
                names($expected),
                "{}",
                $text
            );
        }
    };
}
candidate!(exists_node_reuse, "EXISTS { MATCH (n) RETURN n }", &["n"]);
candidate!(
    exists_property,
    "EXISTS { MATCH (fresh {value:outer}) RETURN fresh }",
    &["outer"]
);
candidate!(
    exists_predicate,
    "EXISTS { MATCH (fresh) WHERE fresh.id=outer RETURN fresh }",
    &["outer"]
);
candidate!(
    exists_projection,
    "EXISTS { MATCH (fresh) RETURN outer }",
    &["outer"]
);
candidate!(
    exists_unwind,
    "EXISTS { UNWIND outer AS fresh RETURN fresh }",
    &["outer"]
);
candidate!(
    exists_local_shadow,
    "EXISTS { WITH 1 AS outer RETURN outer }",
    &[]
);
candidate!(
    exists_order,
    "EXISTS { MATCH (fresh) RETURN fresh ORDER BY outer }",
    &["outer"]
);
candidate!(
    exists_limit,
    "EXISTS { MATCH (fresh) RETURN fresh LIMIT outer }",
    &["outer"]
);
candidate!(
    exists_skip,
    "EXISTS { MATCH (fresh) RETURN fresh SKIP outer }",
    &["outer"]
);
candidate!(pattern_outer_node, "[(n)-->(fresh) | fresh]", &["n"]);
candidate!(pattern_outer_edge, "[(fresh)-[r]->(other) | other]", &["r"]);
candidate!(
    pattern_outer_property,
    "[(fresh {id:outer})-->(other) | other]",
    &["outer"]
);
candidate!(
    pattern_outer_filter,
    "[(fresh)-->(other) WHERE other.id=outer | other]",
    &["outer"]
);
candidate!(
    pattern_outer_projection,
    "[(fresh)-->(other) | outer]",
    &["outer"]
);

#[test]
fn scope_validation_reports_only_missing_names_in_stable_order() {
    let expression = expr("[z,visible,a,z]");
    let error =
        scope::validate_expression_refs(&expression, &names(&["visible"]), "RETURN").unwrap_err();
    assert_eq!(
        error.classification(),
        Some(("SyntaxError", "UndefinedVariable"))
    );
    assert!(error.to_string().ends_with("a, z"), "{error}");
    scope::validate_expression_refs(&expression, &names(&["visible", "a", "z"]), "RETURN").unwrap();
}
#[test]
fn sort_and_local_scope_preserve_correlated_pattern_bindings() {
    let expression = expr("[(n)-->(local) | local]");
    assert_eq!(
        scope::free_variable_names_for_sort(&expression, &["n".into()], &[]),
        names(&["n"])
    );
    assert_eq!(
        scope::free_variable_names_for_local_scope(&expression, &names(&["n"])),
        names(&["n"])
    );
}

macro_rules! query_refs {
    ($name:ident,$text:expr,$expected:expr) => {
        #[test]
        fn $name() {
            let query = parse_query($text).unwrap();
            let mut found = BTreeSet::new();
            scope::add_candidate_bindings_from_query(
                &query,
                &names(&["outer", "n", "r", "p"]),
                &mut found,
            );
            assert_eq!(found, names($expected), "{}", $text);
        }
    };
}
query_refs!(
    create_node_properties,
    "CREATE (fresh {value:outer}) RETURN fresh",
    &["outer"]
);
query_refs!(
    create_edge_properties,
    "CREATE (fresh)-[edge:R {value:outer}]->(other) RETURN edge",
    &["outer"]
);
query_refs!(
    create_target_properties,
    "CREATE (fresh)-[:R]->(other {value:outer}) RETURN other",
    &["outer"]
);
query_refs!(
    merge_on_create,
    "MERGE (fresh {id:1}) ON CREATE SET fresh.value=outer RETURN fresh",
    &["outer"]
);
query_refs!(
    merge_on_match,
    "MERGE (fresh {id:1}) ON MATCH SET fresh.value=outer RETURN fresh",
    &["outer"]
);
query_refs!(set_property, "SET n.value=outer RETURN n", &["n", "outer"]);
query_refs!(set_replace, "SET n=outer RETURN n", &["n", "outer"]);
query_refs!(set_merge, "SET n+=outer RETURN n", &["n", "outer"]);
query_refs!(set_labels, "SET n:Person RETURN n", &["n"]);
query_refs!(remove_property, "REMOVE n.value RETURN n", &["n"]);
query_refs!(remove_labels, "REMOVE n:Person RETURN n", &["n"]);
query_refs!(delete_expression, "DELETE n RETURN outer", &["n", "outer"]);
query_refs!(
    with_filter,
    "WITH outer AS local WHERE local>0 RETURN local",
    &["outer"]
);
query_refs!(
    call_argument,
    "CALL example.echo(outer) YIELD value AS local RETURN local",
    &["outer"]
);
query_refs!(
    call_predicate,
    "CALL example.echo(1) YIELD value AS local WHERE local>outer RETURN local",
    &["outer"]
);
query_refs!(
    union_scope,
    "RETURN outer AS result UNION ALL RETURN n AS result",
    &["outer", "n"]
);

#[test]
fn query_local_names_are_removed_without_erasing_outer_candidates() {
    for text in [
        "UNWIND [1] AS local RETURN local",
        "CREATE (local) RETURN local",
        "MERGE (local) RETURN local",
        "CALL example.echo(1) YIELD value AS local RETURN local",
        "WITH 1 AS local RETURN local",
    ] {
        let query = parse_query(text).unwrap();
        let mut refs = names(&["local", "outer"]);
        scope::remove_local_query_bindings(&query, &names(&["outer"]), &mut refs);
        assert_eq!(refs, names(&["outer"]), "{text}");
    }
}
#[test]
fn implicit_procedure_yields_are_local_and_case_insensitive() {
    for (name, output) in [
        ("DB.LABELS", "label"),
        ("db.relationshipTypes", "relationshipType"),
        ("db.propertyKeys", "propertyKey"),
        ("example.unknown", "value"),
    ] {
        assert_eq!(scope::default_query_procedure_yields(name), vec![output]);
    }
}

#[test]
fn quantified_result_ir_matches_three_valued_truth_tables() {
    use crate::ir::{
        catalog::PropertyGraph,
        runtime::{Row, eval},
        value::Value,
    };
    let graph = PropertyGraph::new();
    for len in 0..=6u32 {
        for code in 0..3usize.pow(len) {
            let mut remaining = code;
            let mut values = Vec::new();
            for _ in 0..len {
                values.push(match remaining % 3 {
                    0 => Some(false),
                    1 => Some(true),
                    _ => None,
                });
                remaining /= 3;
            }
            let row = Row::new()
                .with("total", Value::Int(values.len() as i64))
                .with(
                    "known",
                    Value::Int(values.iter().filter(|v| v.is_some()).count() as i64),
                )
                .with(
                    "true",
                    Value::Int(values.iter().filter(|v| **v == Some(true)).count() as i64),
                );
            for kind in [
                QuantifierKind::All,
                QuantifierKind::Any,
                QuantifierKind::None,
                QuantifierKind::Single,
            ] {
                let any_true = values.contains(&Some(true));
                let any_false = values.contains(&Some(false));
                let unknown = values.contains(&None);
                let answer = match kind {
                    QuantifierKind::All => {
                        if any_false {
                            Some(false)
                        } else if unknown {
                            None
                        } else {
                            Some(true)
                        }
                    }
                    QuantifierKind::Any => {
                        if any_true {
                            Some(true)
                        } else if unknown {
                            None
                        } else {
                            Some(false)
                        }
                    }
                    QuantifierKind::None => {
                        if any_true {
                            Some(false)
                        } else if unknown {
                            None
                        } else {
                            Some(true)
                        }
                    }
                    QuantifierKind::Single => {
                        if values.iter().filter(|v| **v == Some(true)).count() > 1 {
                            Some(false)
                        } else if unknown {
                            None
                        } else {
                            Some(any_true)
                        }
                    }
                };
                let expression =
                    materialize::quantifier_result_expr(kind, "total", "known", "true");
                assert_eq!(
                    eval(&expression, &row, &graph).unwrap(),
                    answer.map(Value::Bool).unwrap_or(Value::Null),
                    "{kind:?}: {values:?}"
                );
            }
        }
    }
}
#[test]
fn nested_scope_detection_visits_every_expression_position() {
    for text in [
        "-EXISTS {MATCH (n) RETURN n}",
        "EXISTS {MATCH (n) RETURN n} IS NULL",
        "1+EXISTS {MATCH (n) RETURN n}",
        "1+2*EXISTS {MATCH (n) RETURN n}",
        "coalesce(1,EXISTS {MATCH (n) RETURN n})",
        "[1,EXISTS {MATCH (n) RETURN n}]",
        "{a:1,b:EXISTS {MATCH (n) RETURN n}}",
        "CASE EXISTS {MATCH (n) RETURN n} WHEN true THEN 1 END",
        "CASE WHEN EXISTS {MATCH (n) RETURN n} THEN 1 END",
        "CASE WHEN true THEN EXISTS {MATCH (n) RETURN n} END",
        "CASE WHEN false THEN 1 ELSE EXISTS {MATCH (n) RETURN n} END",
        "list_transform([1],x -> EXISTS {MATCH (n) RETURN n})",
        "list_filter([1],x -> EXISTS {MATCH (n) RETURN n})",
        "list_reduce([1],(a,x) -> EXISTS {MATCH (n) RETURN n})",
    ] {
        assert!(
            materialize::requires_scoped_materialization(&expr(text)),
            "{text}"
        );
    }
    for text in [
        "1",
        "x+2",
        "coalesce(x,1)",
        "CASE WHEN true THEN 1 ELSE 2 END",
        "list_transform([1],x -> x+1)",
    ] {
        assert!(
            !materialize::requires_scoped_materialization(&expr(text)),
            "{text}"
        );
    }
}

#[test]
fn scalar_type_inference_preserves_nested_structural_types() {
    for (text, expected) in [
        ("null", "NULL"),
        ("true", "BOOL"),
        ("1", "INT64"),
        ("1.5", "DOUBLE"),
        ("'x'", "STRING"),
        ("[]", "ANY[]"),
        ("[1,2]", "INT64[]"),
        ("[1,2.5]", "DOUBLE[]"),
        ("[2.5,1]", "DOUBLE[]"),
        ("[null,1]", "INT64[]"),
        ("[1,null]", "INT64[]"),
        ("[outside]", "ANY[]"),
        ("[[1],[2]]", "INT64[][]"),
    ] {
        assert_eq!(
            expression::static_typeof_expr(&expr(text)).as_deref(),
            Some(expected),
            "{text}"
        );
    }
    assert_eq!(expression::static_typeof_expr(&expr("outside")), None);
}
#[test]
fn scalar_binary_mapping_keeps_all_operator_identities() {
    for (ast, ir) in [
        (BinaryOp::Or, IrBinaryOp::Or),
        (BinaryOp::And, IrBinaryOp::And),
        (BinaryOp::Eq, IrBinaryOp::Eq),
        (BinaryOp::Neq, IrBinaryOp::Neq),
        (BinaryOp::Lt, IrBinaryOp::Lt),
        (BinaryOp::Lte, IrBinaryOp::Lte),
        (BinaryOp::Gt, IrBinaryOp::Gt),
        (BinaryOp::Gte, IrBinaryOp::Gte),
        (BinaryOp::Add, IrBinaryOp::Add),
        (BinaryOp::Sub, IrBinaryOp::Sub),
        (BinaryOp::Mul, IrBinaryOp::Mul),
        (BinaryOp::Div, IrBinaryOp::Div),
    ] {
        assert_eq!(expression::lower_binary_op(ast), ir);
    }
}
#[test]
fn aggregate_diagnostics_render_nested_calls_and_property_names() {
    for (text, expected) in [
        ("count(*)", "COUNT_STAR()"),
        ("outside", "outside"),
        ("outside.value", "outside.value"),
        ("sum(count(outside.value))", "SUM(COUNT(outside.value))"),
        ("1+2", "..."),
    ] {
        assert_eq!(expression::render_kuzu_expr(&expr(text)), expected);
    }
}
#[test]
fn typed_negation_distinguishes_explicit_integer_casts_from_plain_arithmetic() {
    for name in [
        "cast",
        "CAST",
        "to_int8",
        "to_uint64",
        "toInt8",
        "toInt16",
        "toInt32",
        "toInt64",
        "toUint8",
        "toUint16",
        "toUint32",
        "toUint64",
    ] {
        let e = Expr::Function {
            name: name.into(),
            distinct: false,
            args: vec![expr("outside")],
        };
        assert!(expression::is_typed_negate_operand(&e), "{name}");
    }
    for text in [
        "outside",
        "1",
        "outside+1",
        "abs(outside)",
        "toFloat(outside)",
    ] {
        assert!(!expression::is_typed_negate_operand(&expr(text)), "{text}");
    }
}
#[test]
fn static_type_inference_covers_cast_map_union_and_domain_constructors() {
    for (text, expected) in [
        ("{a:1,b:outside}", "STRUCT(a INT64, b ANY)"),
        ("CAST(1,'uint8')", "UINT8"),
        ("date('2024-01-01')", "DATE"),
        ("timestamp('2024-01-01')", "TIMESTAMP"),
        ("interval('1 day')", "INTERVAL"),
        ("blob('x')", "BLOB"),
        ("gen_random_uuid()", "UUID"),
        ("map(['a'],[1])", "MAP(STRING, INT64)"),
        ("map(outside,outside)", "MAP(ANY, ANY)"),
        ("union_value('number',1)", "UNION(number INT64)"),
        ("union_value({number:1})", "UNION(number INT64)"),
        ("union_value('unknown',outside)", "UNION(unknown ANY)"),
    ] {
        assert_eq!(
            expression::static_typeof_expr(&expr(text)).as_deref(),
            Some(expected),
            "{text}"
        );
    }
    for text in [
        "CAST(1,outside)",
        "union_value()",
        "union_value({})",
        "union_value(1,2,3)",
    ] {
        assert_eq!(expression::static_typeof_expr(&expr(text)), None, "{text}");
    }
}
#[test]
fn scalar_specializations_propagate_nested_aggregate_errors() {
    for name in [
        "date",
        "localtime",
        "time",
        "localdatetime",
        "datetime",
        "duration",
        "date.truncate",
        "coalesce",
        "range",
        "toBoolean",
        "toInteger",
        "toFloat",
        "toString",
        "id",
        "labels",
        "type",
        "nodes",
        "relationships",
        "label",
        "abs",
    ] {
        let e = Expr::Function {
            name: name.into(),
            distinct: false,
            args: vec![Expr::CountStar],
        };
        let error = lower_expr(&Lowerer::new(), &e).unwrap_err();
        assert_eq!(
            error.classification(),
            Some(("SyntaxError", "InvalidAggregation")),
            "{name}: {error}"
        );
    }
}
#[test]
fn scalar_lowering_rejects_out_of_range_literals_and_unmaterialized_scopes() {
    for text in [
        "9223372036854775808",
        "-9223372036854775809",
        "not-an-integer",
    ] {
        let e = Expr::Literal(Literal::Integer(text.into()));
        assert_eq!(
            lower_expr(&Lowerer::new(), &e)
                .unwrap_err()
                .classification(),
            Some(("SyntaxError", "IntegerOverflow"))
        );
    }
    for text in [
        "EXISTS {MATCH (n) RETURN n}",
        "[x IN [1] | x]",
        "[(a)-->(b) | b]",
    ] {
        assert!(
            lower_expr(&Lowerer::new(), &expr(text))
                .unwrap_err()
                .to_string()
                .contains("without materialization")
        );
    }
    let e = Expr::Function {
        name: "abs".into(),
        distinct: true,
        args: vec![expr("1")],
    };
    assert!(
        lower_expr(&Lowerer::new(), &e)
            .unwrap_err()
            .to_string()
            .contains("DISTINCT is only valid")
    );
}
#[test]
fn regex_predicate_uses_regex_call_without_scalar_operator_conversion() {
    let expression = expression::lower_string_predicate_expr(
        crate::language::cypher::ast::StringPredicateOp::Regex,
        IrExpr::Lit(Lit::String("abc".into())),
        IrExpr::Lit(Lit::String("^a".into())),
    );
    assert!(matches!(expression,IrExpr::Call{name,args} if name=="regex_match" && args.len()==2));
}
