//! Aggregate rewrites must keep iteration locals out of grouping keys.
use super::*;
use crate::language::cypher::{ast::Clause, parser::parse_query};
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
fn mixed_group_expression(text: &str) {
    let mut lowerer = Lowerer::new();
    lowerer
        .lower_query_body(&parse_query("RETURN 7 AS outside").unwrap())
        .unwrap();
    let mut rewrite = AggregateRewrite::default();
    let mut bound = names(&["local"]);
    let rewritten = rewrite_non_aggregate_projection_expr(
        &mut lowerer,
        &expr(text),
        &mut rewrite,
        None,
        &mut bound,
    )
    .unwrap();
    let grouped = ensure_group_key(
        &mut lowerer,
        &mut rewrite,
        &Expr::Variable("outside".into()),
        None,
    )
    .unwrap();
    assert_eq!(
        rewritten,
        expr(&text.replace("outside", &grouped)),
        "{text}"
    );
    assert_eq!(bound, names(&["local"]));
    assert_eq!(rewrite.group.len(), 1, "{text}");
    assert_eq!(
        rewrite.group[0].expr,
        IrExpr::Binding("outside".into()),
        "{text}"
    );
    assert!(rewrite.aggs.is_empty());
}
macro_rules! mixed {
    ($name:ident,$text:expr) => {
        #[test]
        fn $name() {
            mixed_group_expression($text);
        }
    };
}
mixed!(group_binary, "local+outside");
mixed!(group_unary, "-(local+outside)");
mixed!(group_not, "NOT (local=outside)");
mixed!(group_null, "(local+outside) IS NULL");
mixed!(group_not_null, "(local+outside) IS NOT NULL");
mixed!(group_string, "local STARTS WITH outside");
mixed!(group_function, "coalesce(local,outside)");
mixed!(group_list, "[local,outside]");
mixed!(group_map, "{a:local,b:outside}");
mixed!(group_property, "{a:local,b:outside}.a");
mixed!(group_label, "coalesce(local,outside):Person");
mixed!(
    group_case_input,
    "CASE local+outside WHEN 1 THEN 2 ELSE 3 END"
);
mixed!(group_case_when, "CASE WHEN local=outside THEN 2 ELSE 3 END");
mixed!(
    group_case_then,
    "CASE WHEN true THEN local+outside ELSE 3 END"
);
mixed!(
    group_case_else,
    "CASE WHEN false THEN 3 ELSE local+outside END"
);
mixed!(
    group_transform,
    "list_transform([local,outside],x -> x+local)"
);
mixed!(group_filter, "list_filter([local,outside],x -> x>local)");
mixed!(
    group_reduce,
    "list_reduce([local,outside],(a,x) -> a+x+local)"
);
mixed!(group_quantifier, "any(x IN [local,outside] WHERE x>local)");

#[test]
fn grouped_expression_reuse_preserves_explicit_alias() {
    let mut lowerer = Lowerer::new();
    lowerer
        .lower_query_body(&parse_query("RETURN 1 AS outside").unwrap())
        .unwrap();
    let mut rewrite = AggregateRewrite::default();
    let first = ensure_group_key(
        &mut lowerer,
        &mut rewrite,
        &expr("outside+1"),
        Some("grouped"),
    )
    .unwrap();
    let second = ensure_group_key(&mut lowerer, &mut rewrite, &expr("outside+1"), None).unwrap();
    assert_eq!(first, "grouped");
    assert_eq!(second, first);
}
#[test]
fn aggregates_cannot_capture_iteration_local_variables() {
    for text in [
        "sum(local)",
        "count(local)",
        "collect(local+1)",
        "percentileCont(local,0.5)",
    ] {
        let mut lowerer = Lowerer::new();
        let mut rewrite = AggregateRewrite::default();
        let mut bound = names(&["local"]);
        let error = rewrite_aggregate_projection_expr(
            &mut lowerer,
            &expr(text),
            &mut rewrite,
            None,
            &mut bound,
        )
        .unwrap_err();
        assert_eq!(
            error.classification(),
            Some(("SyntaxError", "InvalidAggregation")),
            "{text}: {error}"
        );
    }
}

// The retained direct-to-IR rewrite has a separate contract from AST rewriting.
fn legacy_value(text: &str) -> crate::ir::value::Value {
    use crate::ir::{
        PropertyGraph,
        runtime::{Row, eval},
        value::Value,
    };
    let mut lowerer = Lowerer::new();
    lowerer
        .lower_query_body(&parse_query("RETURN 7 AS outside").unwrap())
        .unwrap();
    let mut rewrite = AggregateRewrite::default();
    let result =
        rewrite_aggregate_projection(&mut lowerer, &expr(text), &mut rewrite, None).unwrap();
    let graph = PropertyGraph::new();
    let input = Row::new().with("outside", Value::Int(7));
    let mut output = Row::new();
    for group in &rewrite.group {
        output.bindings.insert(
            group.alias.clone(),
            eval(&group.expr, &input, &graph).unwrap(),
        );
    }
    for agg in &rewrite.aggs {
        assert_eq!(agg.kind, AggKind::CountRows);
        assert!(agg.arg.is_none());
        output.bindings.insert(agg.alias.clone(), Value::Int(3));
    }
    assert!(!rewrite.aggs.is_empty());
    eval(&result, &output, &graph).unwrap()
}
macro_rules! legacy {
    ($name:ident,$text:expr,$value:expr) => {
        #[test]
        fn $name() {
            assert_eq!(legacy_value($text), $value, "{}", $text);
        }
    };
}
legacy!(legacy_count, "count(*)", crate::ir::value::Value::Int(3));
legacy!(
    legacy_add,
    "outside+count(*)",
    crate::ir::value::Value::Int(10)
);
legacy!(
    legacy_sub,
    "outside-count(*)",
    crate::ir::value::Value::Int(4)
);
legacy!(
    legacy_mul,
    "outside*count(*)",
    crate::ir::value::Value::Int(21)
);
legacy!(legacy_negate, "-count(*)", crate::ir::value::Value::Int(-3));
legacy!(
    legacy_not,
    "NOT (outside=count(*))",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_eq,
    "outside=count(*)",
    crate::ir::value::Value::Bool(false)
);
legacy!(
    legacy_neq,
    "outside<>count(*)",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_lt,
    "outside<count(*)",
    crate::ir::value::Value::Bool(false)
);
legacy!(
    legacy_lte,
    "outside<=count(*)",
    crate::ir::value::Value::Bool(false)
);
legacy!(
    legacy_gt,
    "outside>count(*)",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_gte,
    "outside>=count(*)",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_and,
    "count(*)>0 AND outside>0",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_or,
    "count(*)<0 OR outside>0",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_null,
    "count(*) IS NULL",
    crate::ir::value::Value::Bool(false)
);
legacy!(
    legacy_nonnull,
    "count(*) IS NOT NULL",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_property,
    "{x:count(*)}.x",
    crate::ir::value::Value::Int(3)
);
legacy!(
    legacy_function,
    "coalesce(count(*),outside)",
    crate::ir::value::Value::Int(3)
);
legacy!(
    legacy_simple_case,
    "CASE count(*) WHEN 3 THEN outside ELSE 0 END",
    crate::ir::value::Value::Int(7)
);
legacy!(
    legacy_case_else,
    "CASE count(*) WHEN 2 THEN 0 ELSE outside END",
    crate::ir::value::Value::Int(7)
);
legacy!(
    legacy_case_null,
    "CASE WHEN count(*)=0 THEN outside END",
    crate::ir::value::Value::Null
);
legacy!(
    legacy_starts,
    "toString(count(*)) STARTS WITH '3'",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_ends,
    "toString(count(*)) ENDS WITH '3'",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_contains,
    "toString(count(*)) CONTAINS '3'",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_regex,
    "toString(count(*)) =~ '^3$'",
    crate::ir::value::Value::Bool(true)
);
legacy!(
    legacy_list,
    "[count(*),outside]",
    crate::ir::value::Value::List(vec![
        crate::ir::value::Value::Int(3),
        crate::ir::value::Value::Int(7)
    ])
);

#[test]
fn both_rewriters_preserve_aggregate_identity_arity_distinct_and_requested_alias() {
    for (name, kind, arity) in [
        ("count", AggKind::CountRows, 1),
        ("count_if", AggKind::CountIf, 1),
        ("sum", AggKind::SumOrZero, 1),
        ("avg", AggKind::AvgOrNull, 1),
        ("min", AggKind::MinOrNull, 1),
        ("max", AggKind::MaxOrNull, 1),
        ("stdev", AggKind::StDev, 1),
        ("stdevp", AggKind::StDevP, 1),
        ("percentileCont", AggKind::PercentileCont, 2),
        ("percentileDisc", AggKind::PercentileDisc, 2),
        ("collect", AggKind::CollectNonNull, 1),
        ("bool_and", AggKind::EngineFunction, 1),
    ] {
        for distinct in [false, true] {
            for legacy in [false, true] {
                let args = if arity == 2 {
                    vec![expr("outside"), expr("0.5")]
                } else {
                    vec![expr("outside")]
                };
                let expression = Expr::Function {
                    name: name.into(),
                    distinct,
                    args,
                };
                let mut lowerer = Lowerer::new();
                let mut rewrite = AggregateRewrite::default();
                if legacy {
                    let output = rewrite_aggregate_projection(
                        &mut lowerer,
                        &expression,
                        &mut rewrite,
                        Some("chosen"),
                    )
                    .unwrap();
                    assert_eq!(output, IrExpr::Binding("chosen".into()));
                } else {
                    let output = rewrite_aggregate_projection_expr(
                        &mut lowerer,
                        &expression,
                        &mut rewrite,
                        Some("chosen"),
                        &mut BTreeSet::new(),
                    )
                    .unwrap();
                    assert_eq!(output, Expr::Variable("chosen".into()));
                }
                assert_eq!(rewrite.aggs.len(), 1, "{name}");
                let call = &rewrite.aggs[0];
                assert_eq!(
                    call.kind,
                    if name == "count" && distinct {
                        AggKind::CountDistinct
                    } else {
                        kind
                    }
                );
                assert_eq!(call.alias, "chosen");
                assert_eq!(call.distinct, distinct);
                match kind {
                    AggKind::PercentileCont | AggKind::PercentileDisc => {
                        assert!(matches!(&call.arg,Some(IrExpr::List(items)) if items.len()==2))
                    }
                    AggKind::EngineFunction => assert!(
                        matches!(&call.arg,Some(IrExpr::Call{name:n,args}) if n==name && args.len()==1)
                    ),
                    _ => assert_eq!(call.arg, Some(IrExpr::Binding("outside".into()))),
                }
            }
        }
    }
}
#[test]
fn direct_rewrite_rejects_malformed_and_nested_aggregations() {
    for name in [
        "count",
        "sum",
        "avg",
        "min",
        "max",
        "collect",
        "percentileCont",
        "percentileDisc",
    ] {
        let valid = if name.starts_with("percentile") { 2 } else { 1 };
        for arity in 0..4 {
            if arity == valid {
                continue;
            }
            let expression = Expr::Function {
                name: name.into(),
                distinct: false,
                args: vec![expr("1"); arity],
            };
            let error = rewrite_aggregate_projection(
                &mut Lowerer::new(),
                &expression,
                &mut AggregateRewrite::default(),
                None,
            )
            .unwrap_err();
            assert!(error.to_string().contains("requires exactly"), "{error}");
        }
    }
    for text in [
        "sum(count(*))",
        "avg(sum(outside))",
        "collect(max(outside))",
    ] {
        let error = rewrite_aggregate_projection(
            &mut Lowerer::new(),
            &expr(text),
            &mut AggregateRewrite::default(),
            None,
        )
        .unwrap_err();
        assert_eq!(
            error.classification(),
            Some(("SyntaxError", "NestedAggregation"))
        );
    }
    let expression = Expr::Function {
        name: "coalesce".into(),
        distinct: true,
        args: vec![Expr::CountStar],
    };
    let error = rewrite_aggregate_projection(
        &mut Lowerer::new(),
        &expression,
        &mut AggregateRewrite::default(),
        None,
    )
    .unwrap_err();
    assert!(error.to_string().contains("DISTINCT is only valid"));
    for text in ["[x IN [1] | count(*)]", "all(x IN [1] WHERE count(*)=1)"] {
        assert!(
            rewrite_aggregate_projection(
                &mut Lowerer::new(),
                &expr(text),
                &mut AggregateRewrite::default(),
                None
            )
            .unwrap_err()
            .to_string()
            .contains("unsupported Cypher expression")
        );
    }
}
#[test]
fn legacy_typed_negation_and_label_predicates_retain_their_operations() {
    let mut rewrite = AggregateRewrite::default();
    let ir = rewrite_aggregate_projection(
        &mut Lowerer::new(),
        &expr("-CAST(count(*),'UINT8')"),
        &mut rewrite,
        None,
    )
    .unwrap();
    assert!(matches!(ir,IrExpr::Call{name,args} if name=="negate" && args.len()==1));
    assert_eq!(rewrite.aggs.len(), 1);
    let mut rewrite = AggregateRewrite::default();
    let ir = rewrite_aggregate_projection(
        &mut Lowerer::new(),
        &expr("head(collect(outside)):Person"),
        &mut rewrite,
        None,
    )
    .unwrap();
    assert!(matches!(ir,IrExpr::Call{name,args} if name=="cypher_has_label" && args.len()==2));
    assert_eq!(rewrite.aggs[0].kind, AggKind::CollectNonNull);
}

#[test]
fn aggregate_extraction_preserves_nested_syntax_and_shadowed_scopes() {
    // These are rewrite contracts: collection/pattern locals remain local,
    // while COUNT is extracted into exactly one aggregation slot.
    for text in [
        "-count(*)", "NOT (count(*)=0)", "count(*)+1",
        "{v:count(*)}.v", "head(collect(1)):Person",
        "count(*) IS NULL", "count(*) IS NOT NULL",
        "toString(count(*)) STARTS WITH '1'",
        "'1' ENDS WITH toString(count(*))",
        "CASE count(*) WHEN 0 THEN 1 ELSE 2 END",
        "CASE WHEN count(*)=0 THEN 1 END",
        "CASE WHEN true THEN count(*) ELSE 0 END",
        "CASE WHEN false THEN 0 ELSE count(*) END",
        "[count(*),1]", "{v:count(*)}",
        "[x IN [1,2] WHERE x>0 | x+count(*)]",
        "[x IN [count(*)] | x]",
        "[x IN [1,2] WHERE x<count(*) | x]",
        "list_transform([1,2],x -> x+count(*))",
        "list_filter([1,2],x -> x<count(*))",
        "list_reduce([1,2],(acc,x) -> acc+x+count(*))",
        "all(x IN [1,2] WHERE x<count(*))",
        "[p=(a)-[r:R]->(b) WHERE count(*)>0 | length(p)]",
        "[(a)-[r:R]->(b) | count(*)]",
    ] {
        for already_bound in [vec![], vec!["x"], vec!["acc","x"], vec!["a","r","b","p"]] {
            let mut lowerer = Lowerer::new();
            let mut rewrite = AggregateRewrite::default();
            let original = names(&already_bound);
            let mut bound = original.clone();
            let rewritten = rewrite_aggregate_projection_expr(
                &mut lowerer, &expr(text), &mut rewrite, None, &mut bound,
            ).unwrap_or_else(|error| panic!("{text}: {error}"));
            assert_eq!(bound, original, "scope escaped from {text}");
            assert!(rewrite.group.is_empty(), "local binding grouped in {text}");
            assert_eq!(rewrite.aggs.len(), 1, "{text}");
            let aggregate = &rewrite.aggs[0];
            let source = if text.contains("collect(1)") { "collect(1)" } else { "count(*)" };
            assert_eq!(rewritten, expr(&text.replace(source, &aggregate.alias)), "{text}");
            assert_eq!(aggregate.kind, if source == "collect(1)" { AggKind::CollectNonNull } else { AggKind::CountRows });
        }
    }
}

#[test]
fn ast_aggregate_rewrite_rejects_volatile_and_invalid_calls() {
    for text in ["sum(rand())", "collect(random())"] {
        let error = rewrite_aggregate_projection_expr(
            &mut Lowerer::new(), &expr(text), &mut AggregateRewrite::default(),
            None, &mut BTreeSet::new(),
        ).unwrap_err();
        assert_eq!(error.classification(), Some(("SyntaxError", "NonConstantExpression")), "{text}: {error}");
    }
    for text in ["sum()", "count(1,2)", "percentileCont(1)", "percentileDisc(1,2,3)"] {
        let error = rewrite_aggregate_projection_expr(
            &mut Lowerer::new(), &expr(text), &mut AggregateRewrite::default(),
            None, &mut BTreeSet::new(),
        ).unwrap_err();
        assert!(error.to_string().contains("requires exactly"), "{text}: {error}");
    }
}

mixed!(group_pattern_projection, "[(a)-[r:R]->(b) | local+outside]");
mixed!(group_pattern_predicate, "[p=(a)-[r:R]->(b) WHERE local=outside | length(p)]");
mixed!(group_pattern_start_properties, "[(a {v:local+outside})-[r:R]->(b) | b]");
mixed!(group_pattern_edge_properties, "[(a)-[r:R {v:local+outside}]->(b) | b]");
mixed!(group_pattern_end_properties, "[(a)-[r:R]->(b {v:local+outside}) | b]");

#[test]
fn aggregate_pattern_properties_extract_counts_without_leaking_pattern_bindings() {
    for text in [
        "[(a {v:count(*)})-[r:R]->(b) | b]",
        "[(a)-[r:R {v:count(*)}]->(b) | b]",
        "[(a)-[r:R]->(b {v:count(*)}) | b]",
    ] {
        for already_bound in [vec![], vec!["a", "b", "r"]] {
            let mut lowerer = Lowerer::new();
            let mut rewrite = AggregateRewrite::default();
            let original = names(&already_bound);
            let mut bound = original.clone();
            let rewritten = rewrite_aggregate_projection_expr(
                &mut lowerer, &expr(text), &mut rewrite, None, &mut bound,
            ).unwrap_or_else(|error| panic!("{text}: {error}"));
            assert_eq!(bound, original);
            assert!(rewrite.group.is_empty());
            assert_eq!(rewrite.aggs.len(), 1);
            assert_eq!(rewrite.aggs[0].kind, AggKind::CountRows);
            assert_eq!(rewritten, expr(&text.replace("count(*)", &rewrite.aggs[0].alias)));
        }
    }
}

#[test]
fn malformed_aggregates_preserve_diagnostics_through_nested_expressions() {
    for text in [
        "sum().value", "sum():Person", "-sum()", "sum()+1", "1+sum()",
        "sum() IS NULL", "sum() IS NOT NULL", "sum() STARTS WITH 'a'",
        "'a' ENDS WITH sum()", "coalesce(sum(),1)", "[sum()]", "{value:sum()}",
        "CASE sum() WHEN 1 THEN 2 END", "CASE WHEN sum() THEN 2 END",
        "CASE WHEN true THEN sum() END", "CASE WHEN false THEN 1 ELSE sum() END",
        "[x IN sum() | x]", "[x IN [1] WHERE sum() | x]", "[x IN [1] | sum()]",
        "list_transform([1],x -> sum())", "list_filter([1],x -> sum())",
        "list_reduce([1],(a,x) -> sum())", "all(x IN [1] WHERE sum())",
        "[(a {v:sum()})-[r:R]->(b) | b]",
        "[(a)-[r:R {v:sum()}]->(b) | b]",
        "[(a)-[r:R]->(b {v:sum()}) | b]",
        "[p=(a)-[r:R]->(b) WHERE sum() | b]",
        "[p=(a)-[r:R]->(b) | sum()]",
    ] {
        let error = rewrite_aggregate_projection_expr(
            &mut Lowerer::new(), &expr(text), &mut AggregateRewrite::default(),
            None, &mut BTreeSet::new(),
        ).unwrap_err();
        assert_eq!(error.to_string(), "invalid cypher plan: aggregate function `sum` requires exactly one argument", "{text}");
    }
}

#[test]
fn direct_aggregate_ir_rewrite_retains_nested_call_errors() {
    for text in [
        "sum().value", "sum():Person", "-sum()", "sum()+1", "1+sum()",
        "sum() IS NULL", "sum() IS NOT NULL", "sum() STARTS WITH 'a'",
        "'a' ENDS WITH sum()", "coalesce(sum(),1)", "[sum()]", "{value:sum()}",
        "CASE sum() WHEN 1 THEN 2 END", "CASE WHEN sum() THEN 2 END",
        "CASE WHEN true THEN sum() END", "CASE WHEN false THEN 1 ELSE sum() END",
    ] {
        let error=rewrite_aggregate_projection(&mut Lowerer::new(),&expr(text),
            &mut AggregateRewrite::default(),None).unwrap_err();
        assert!(error.to_string().contains("aggregate function `sum` requires exactly one argument"),"{text}: {error}");
    }
    for text in ["sum(no_such_function(1))", "percentileCont(no_such_function(1),0.5)",
        "percentileCont(1,no_such_function(0.5))"] {
        for ast in [false,true] {
            let mut lowerer=Lowerer::new();let mut rewrite=AggregateRewrite::default();
            let error=if ast {
                rewrite_aggregate_projection_expr(&mut lowerer,&expr(text),&mut rewrite,None,&mut BTreeSet::new()).unwrap_err()
            } else {
                rewrite_aggregate_projection(&mut lowerer,&expr(text),&mut rewrite,None).unwrap_err()
            };
            assert_eq!(error.classification(),Some(("SyntaxError","UnknownFunction")),"{text}: {error}");
        }
    }
}

#[test]
fn mixed_local_grouping_propagates_invalid_outer_expression_errors() {
    for text in [
        "{a:local,b:no_such_function(outside)}.a",
        "coalesce(local,no_such_function(outside)):Person",
        "-(local+no_such_function(outside))",
        "local+no_such_function(outside)", "no_such_function(outside)+local",
        "(local+no_such_function(outside)) IS NULL",
        "(local+no_such_function(outside)) IS NOT NULL",
        "local STARTS WITH no_such_function(outside)",
        "no_such_function(outside) ENDS WITH local",
        "coalesce(local,no_such_function(outside))",
        "[local,no_such_function(outside)]", "{a:local,b:no_such_function(outside)}",
        "CASE local+no_such_function(outside) WHEN 1 THEN 2 END",
        "CASE WHEN local=no_such_function(outside) THEN 2 END",
        "CASE WHEN true THEN local+no_such_function(outside) END",
        "CASE WHEN false THEN 1 ELSE local+no_such_function(outside) END",
    ] {
        let mut lowerer=Lowerer::new();
        lowerer.lower_query_body(&parse_query("RETURN 7 AS outside").unwrap()).unwrap();
        let error=rewrite_non_aggregate_projection_expr(
            &mut lowerer,&expr(text),&mut AggregateRewrite::default(),None,&mut names(&["local"]),
        ).unwrap_err();
        assert_eq!(error.classification(),Some(("SyntaxError","UnknownFunction")),"{text}: {error}");
        assert!(error.to_string().contains("no_such_function"),"{text}: {error}");
    }
}


mixed!(group_comprehension_predicate, "[x IN [local,outside] WHERE x > outside | x + local]");
mixed!(group_comprehension_shadowed_local, "[local IN [local,outside] WHERE local > outside | local + outside]");
