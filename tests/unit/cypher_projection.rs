//! Scope contracts use independent expected free-variable sets, including shadowing.
use super::*;
use crate::language::cypher::parser::parse_query;

#[test]
fn with_filters_choose_scope_safe_placement_and_reject_hidden_aggregate_inputs() {
    let mut lowerer=Lowerer::new();lowerer.add_visible("source");lowerer.add_visible("other");
    for (query,expected) in [
        ("WITH source AS renamed WHERE renamed > 1 RETURN renamed",0),
        ("WITH source AS renamed WHERE source > 1 RETURN renamed",1),
        ("WITH source AS renamed WHERE EXISTS { RETURN source + renamed AS x } RETURN renamed",2),
    ] {
        let query=parse_query(query).unwrap();
        let Clause::With(clause)=&query.clauses[0] else {panic!()};
        let placement=lower_with_predicate_placement(&lowerer,&clause.projection,clause.predicate.as_ref().unwrap()).unwrap();
        match (expected,placement) {
            (0,WithPredicatePlacement::AfterProjection)|(2,WithPredicatePlacement::MaterializeAliases)=>{},
            (1,WithPredicatePlacement::BeforeProjection(value))=>assert_eq!(value,expr("source > 1")),
            _=>panic!("incorrect predicate placement"),
        }
    }
    for query in [
        "WITH count(source) AS total WHERE source > 1 RETURN total",
        "WITH source AS renamed WHERE missing > 1 RETURN renamed",
        "WITH source AS renamed WHERE EXISTS { RETURN missing AS x } RETURN renamed",
    ] {
        let query=parse_query(query).unwrap();
        let Clause::With(clause)=&query.clauses[0] else {panic!()};
        assert!(lower_with_predicate_placement(&lowerer,&clause.projection,clause.predicate.as_ref().unwrap()).is_err());
    }
    assert_eq!(lowerer.visible_set(),names(&["source","other"]));
}
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
fn typed_negation_keeps_the_explicit_conversion_and_negates_its_value() {
    use crate::ir::{PropertyGraph, value::Value, runtime::{Row,eval}};
    // Explicit unsigned casts use the typed extension's modular negation.
    for (kind,expected) in [("INT8",Value::Byte(-1)),("INT16",Value::Short(-1)),
        ("INT32",Value::Int(-1)),("INT64",Value::Long(-1)),
        ("UINT8",Value::UInt8(255)),("UINT16",Value::UInt16(65535)),
        ("UINT32",Value::UInt32(u32::MAX)),("UINT64",Value::UInt64(u64::MAX))] {
        let input=expr(&format!("-CAST(1,'{kind}')"));
        let result=lower_expr(&mut Lowerer::new(),&input).unwrap();
        let IrExpr::Call{name,args}=&result else {panic!("typed negation missing")};
        assert_eq!(name,"negate");
        assert!(matches!(args.as_slice(),[IrExpr::Call{name,..}] if name.eq_ignore_ascii_case("cast")));
        assert_eq!(eval(&result,&Row::new(),&PropertyGraph::new()).unwrap(),expected,"{kind}");
    }
}

#[test]
fn static_integer_types_change_at_signed_64_and_128_bit_boundaries() {
    for (value,expected) in [
        (i64::MIN.to_string(),"INT64"),(i64::MAX.to_string(),"INT64"),
        ((i128::from(i64::MIN)-1).to_string(),"INT128"),
        ((i128::from(i64::MAX)+1).to_string(),"INT128"),
        (i128::MIN.to_string(),"INT128"),(i128::MAX.to_string(),"INT128"),
        ((i128::MAX as u128+1).to_string(),"UINT128"),(u128::MAX.to_string(),"UINT128"),
    ] {
        assert_eq!(expression::integer_literal_type(&value),expected);
        assert_eq!(expression::static_typeof_expr(&Expr::Literal(Literal::Integer(value))),Some(expected.into()));
    }
}

#[test]
fn literal_negation_preserves_signed_minimum_overflow_and_float_zero_bits() {
    let lower=|literal|lower_expr(&Lowerer::new(),&Expr::Unary {
        op:UnaryOp::Neg,expr:Box::new(Expr::Literal(literal)),
    });
    for (input,expected) in [("0",0),("9223372036854775807",-9223372036854775807),
        ("9223372036854775808",i64::MIN),("-9223372036854775807",i64::MAX)] {
        assert_eq!(lower(Literal::Integer(input.into())).unwrap(),IrExpr::Lit(Lit::Int(expected)));
    }
    for input in ["9223372036854775809","-9223372036854775808","invalid"] {
        assert_eq!(lower(Literal::Integer(input.into())).unwrap_err().classification(),
            Some(("SyntaxError","IntegerOverflow")));
    }
    for value in [0.0_f64,-0.0,1.25,-1.25] {
        let IrExpr::Lit(Lit::Float(result))=lower(Literal::Float(value)).unwrap() else {panic!()};
        assert_eq!(result.to_bits(),(-value).to_bits());
    }
}

#[test]
fn nested_unknown_functions_keep_their_diagnostic_through_scalar_containers() {
    for text in [
        "absent_fn().value", "absent_fn():Person", "keys(absent_fn())",
        "properties(absent_fn())", "labels(absent_fn())", "type(absent_fn())",
        "-CAST(absent_fn(),'INT8')", "NOT absent_fn()", "1+absent_fn()",
        "absent_fn() IS NULL", "absent_fn() IS NOT NULL",
        "CASE WHEN absent_fn() THEN 1 ELSE 2 END",
        "CASE 1 WHEN absent_fn() THEN 1 ELSE 2 END",
        "CASE WHEN true THEN absent_fn() ELSE 2 END",
        "CASE WHEN true THEN 1 ELSE absent_fn() END",
        "[x IN absent_fn() | x]", "[x IN [1] WHERE absent_fn() | x]",
        "[x IN [1] | absent_fn()]", "[1,absent_fn()]", "{x:absent_fn()}",
    ] {
        let error=lower_expr(&Lowerer::new(),&expr(text)).unwrap_err();
        assert_eq!(error.classification(),Some(("SyntaxError","UnknownFunction")),"{text}: {error}");
        assert!(error.to_string().contains("absent_fn"),"{text}: {error}");
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
        "[(a)-->(b) | b]",
    ] {
        assert!(
            lower_expr(&Lowerer::new(), &expr(text))
                .unwrap_err()
                .to_string()
                .contains("without materialization")
        );
    }
    // Pure comprehensions retain lexical scope and can now be lowered inside
    // another lambda without introducing a separate relation.
    assert!(matches!(
        lower_expr(&Lowerer::new(), &expr("[x IN [1] | x]")).unwrap(),
        IrExpr::ListTransform { .. }
    ));
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

#[test]
fn preaggregate_patterns_use_the_existing_scoped_materialization_boundary() {
    for text in ["[(a)-->(b) | b]", "[(a)-->(b) WHERE b.x > 0 | b.x]"] {
        let expression = expr(text);
        assert!(requires_scoped_materialization(&expression));
        let expected = materialize::materialize_expr(&mut Lowerer::new(), Node::GraphOneRow, &expression);
        let actual = materialize::materialize_pre_aggregate_expr(&mut Lowerer::new(), Node::GraphOneRow, &expression);
        assert_eq!(actual, expected, "{text}");
        assert!(actual.is_ok(), "{text}: {actual:?}");
    }
    let expression = expr("[(a)-->(b) | count(*)]");
    assert!(requires_scoped_materialization(&expression));
    let (input, output) = materialize::materialize_pre_aggregate_expr(&mut Lowerer::new(), Node::GraphOneRow, &expression).unwrap();
    assert_eq!(input, Node::GraphOneRow);
    assert_eq!(output, expression);
}

#[test]
fn projection_alias_substitution_preserves_nested_bindings_and_restores_scope() {
    let replacements = BTreeMap::from([
        ("x".to_string(), expr("99")),
        ("acc".to_string(), expr("88")),
        ("source".to_string(), expr("[1,2]")),
        ("outside".to_string(), expr("7")),
    ]);
    for (input, expected) in [
        ("[x IN source WHERE x > outside | x + outside]", "[x IN [1,2] WHERE x > 7 | x + 7]"),
        ("[x IN source | [x IN x | x + outside]]", "[x IN [1,2] | [x IN x | x + 7]]"),
        ("list_transform(source,x -> x + outside)", "list_transform([1,2],x -> x + 7)"),
        ("list_filter(source,x -> x > outside)", "list_filter([1,2],x -> x > 7)"),
        ("list_reduce(source,(acc,x) -> acc + x + outside)", "list_reduce([1,2],(acc,x) -> acc + x + 7)"),
        ("all(x IN source WHERE x > outside)", "all(x IN [1,2] WHERE x > 7)"),
        ("CASE x WHEN outside THEN acc ELSE source END", "CASE 99 WHEN 7 THEN 88 ELSE [1,2] END"),
        ("{a:x,b:outside}", "{a:99,b:7}"),
        ("x STARTS WITH outside", "99 STARTS WITH 7"),
        ("x:Person", "99:Person"),
    ] {
        assert_eq!(aliases::substitute_projection_aliases(&expr(input), &replacements), expr(expected), "{input}");
    }
    for input in [
        "[x IN source WHERE x > outside | x + outside]",
        "list_transform(source,x -> x + outside)",
        "list_filter(source,x -> x > outside)",
        "list_reduce(source,(acc,x) -> acc + x + outside)",
        "all(x IN source WHERE x > outside)",
    ] {
        let mut bound = names(&["x", "acc", "outside"]);
        let before = bound.clone();
        let actual = aliases::substitute_projection_aliases_with_bound(&expr(input), &replacements, &mut bound);
        assert_eq!(bound, before, "scope leaked from {input}");
        assert_eq!(actual, expr(&input.replace("source", "[1,2]")), "{input}");
    }
}

#[test]
fn pattern_alias_substitution_respects_prebound_nodes_relationships_and_paths() {
    let input = expr("[p = (a {k:outside})-[r:R {k:outside}]->(b {k:outside}) WHERE a.k > outside | [a,b,r,p,outside]]");
    let expected = expr("[p = (a {k:7})-[r:R {k:7}]->(b {k:7}) WHERE a.k > 7 | [a,b,r,p,7]]");
    let replacements = BTreeMap::from([
        ("a".to_string(), expr("1")), ("b".to_string(), expr("2")),
        ("r".to_string(), expr("3")), ("p".to_string(), expr("4")),
        ("outside".to_string(), expr("7")),
    ]);
    for initial in [names(&[]), names(&["a", "r", "p"]), names(&["a", "b", "r", "p"])] {
        let mut bound = initial.clone();
        assert_eq!(aliases::substitute_projection_aliases_with_bound(&input, &replacements, &mut bound), expected);
        assert_eq!(bound, initial);
    }
}

#[test]
fn procedure_default_yields_shadow_candidates_in_query_reference_analysis() {
    for (procedure, output) in [
        ("db.labels", "label"), ("db.relationshipTypes", "relationshipType"),
        ("db.propertyKeys", "propertyKey"), ("example.unknown", "value"),
    ] {
        let candidates=names(&[output,"outer"]);
        for text in [format!("CALL {procedure}()"),
            format!("CALL {procedure}() YIELD *")] {
            let query=parse_query(&text).unwrap();
            let mut refs=BTreeSet::new();
            scope::add_candidate_bindings_from_query(&query,&candidates,&mut refs);
            assert!(refs.is_empty(),"{text}: {refs:?}");
            let mut bound=BTreeSet::new();
            let mut free=BTreeSet::new();
            scope::collect_query_body_variable_references(&query,&mut bound,&mut free);
            assert!(bound.contains(output),"missing default yield: {text}");
            assert!(!free.contains(output),"default yield escaped scope: {text}");
        }
    }
}

query_refs!(match_edge_properties, "MATCH (fresh)-[edge:R {value:outer}]->(other) RETURN edge", &["outer"]);
query_refs!(match_target_properties, "MATCH (fresh)-[:R]->(other {value:outer}) RETURN other", &["outer"]);

#[test]
fn static_slice_bounds_fold_safe_integer_arithmetic_and_defer_dynamic_values() {
    for (text,expected) in [("9-4",Some(5)),("3*4",Some(12)),("9/2",Some(4)),
        ("0-1",None),("18446744073709551615+1",None),("18446744073709551615*2",None),
        ("9/0",None),("9%2",None),("-outside",None),("NOT outside",None),
        ("1+outside",None),("outside+1",None)] {
        assert_eq!(literal_u64(&expr(text)).unwrap(),expected,"{text}");
    }
    assert!(literal_u64(&Expr::Literal(Literal::Integer("18446744073709551616".into()))).is_err());
}

#[test]
fn projected_sort_rewrites_null_and_string_predicates_to_aggregate_outputs() {
    let query=parse_query("RETURN count(*) AS total, min(name) AS smallest").unwrap();
    let Clause::Return(clause)=&query.clauses[0] else {panic!()};
    let fields=vec!["total".into(),"smallest".into()];
    for (source,expected) in [("count(*) IS NULL","total IS NULL"),
        ("count(*) IS NOT NULL","total IS NOT NULL"),
        ("min(name) STARTS WITH 'A'","smallest STARTS WITH 'A'"),
        ("'Ada' ENDS WITH min(name)","'Ada' ENDS WITH smallest")] {
        assert_eq!(rewrite_projected_sort_expr(&clause.projection,&fields,&expr(source)),expr(expected),"{source}");
    }
}
