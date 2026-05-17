use new_graph::language::cypher::ast::{Clause, Expr, Literal};
use new_graph::language::cypher::parser::parse_query;

#[test]
fn parses_node_label_disjunction() {
    let query = parse_query("MATCH (a:organisation|:person) RETURN count(*)").unwrap();
    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    assert_eq!(
        m.patterns[0].element.start.labels,
        vec!["organisation".to_string(), "person".to_string()]
    );
}

#[test]
fn parses_exists_match_pattern_subquery() {
    let query =
        parse_query("MATCH (a:person) WHERE EXISTS { MATCH (a)-[:knows]->(b:person) } RETURN a")
            .unwrap();
    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Exists(exists)) = &m.predicate else {
        panic!("expected EXISTS predicate");
    };
    assert_eq!(exists.patterns.len(), 1);
    assert!(exists.query.is_none());
}

#[test]
fn lowers_cast_as_type_to_cast_function() {
    let query = parse_query("RETURN CAST(42 AS INT64)").unwrap();
    let Clause::Return(ret) = &query.clauses[0] else {
        panic!("expected RETURN clause");
    };
    let Expr::Function { name, args, .. } = &ret.projection.items[0].expr else {
        panic!("expected function expression");
    };
    assert_eq!(name, "cast");
    assert_eq!(args[0], Expr::Literal(Literal::Integer("42".to_string())));
    assert_eq!(args[1], Expr::Literal(Literal::String("INT64".to_string())));
}

#[test]
fn parses_recursive_relationship_filter_syntax() {
    parse_query(
        "MATCH p = (a:person)-[e:knows*2..2 (r, n | WHERE n.fName = 'Dan')]->(b:person) RETURN p",
    )
    .unwrap();
}

#[test]
fn lowers_kuzu_list_transform_lambda_to_list_transform() {
    let query = parse_query("RETURN LIST_TRANSFORM([1,2,3], x->x + 1)").unwrap();
    let Clause::Return(ret) = &query.clauses[0] else {
        panic!("expected RETURN clause");
    };
    let Expr::ListTransform {
        variable,
        collection,
        map,
    } = &ret.projection.items[0].expr
    else {
        panic!("expected list transform");
    };
    assert_eq!(variable, "x");
    assert!(matches!(collection.as_ref(), Expr::List(_)));
    assert!(matches!(map.as_ref(), Expr::Binary { .. }));
}

#[test]
fn lowers_kuzu_list_filter_lambda_to_list_filter() {
    let query = parse_query("RETURN LIST_FILTER([1,2,3], x->x >= 2)").unwrap();
    let Clause::Return(ret) = &query.clauses[0] else {
        panic!("expected RETURN clause");
    };
    let Expr::ListFilter {
        variable,
        collection,
        predicate,
    } = &ret.projection.items[0].expr
    else {
        panic!("expected list filter");
    };
    assert_eq!(variable, "x");
    assert!(matches!(collection.as_ref(), Expr::List(_)));
    assert!(matches!(predicate.as_ref(), Expr::Binary { .. }));
}

#[test]
fn lowers_nested_list_transform_lambdas() {
    let query =
        parse_query("RETURN LIST_TRANSFORM([[1,2],[3,4]], xs -> LIST_TRANSFORM(xs, y -> y * 2))")
            .unwrap();
    let Clause::Return(ret) = &query.clauses[0] else {
        panic!("expected RETURN clause");
    };
    let Expr::ListTransform { variable, map, .. } = &ret.projection.items[0].expr else {
        panic!("expected list transform");
    };
    assert_eq!(variable, "xs");
    let Expr::ListTransform {
        variable: inner_var,
        ..
    } = map.as_ref()
    else {
        panic!("expected nested list transform");
    };
    assert_eq!(inner_var, "y");
}

#[test]
fn lowers_kuzu_list_reduce_lambda_to_scoped_reduce() {
    let query = parse_query("RETURN LIST_REDUCE([1,2,3], (x, y) -> x + y)").unwrap();
    let Clause::Return(ret) = &query.clauses[0] else {
        panic!("expected RETURN clause");
    };
    let Expr::ListReduce {
        accumulator,
        variable,
        collection,
        map,
    } = &ret.projection.items[0].expr
    else {
        panic!("expected list reduce");
    };
    assert_eq!(accumulator, "x");
    assert_eq!(variable, "y");
    assert!(matches!(collection.as_ref(), Expr::List(_)));
    assert!(matches!(map.as_ref(), Expr::Binary { .. }));
}

#[test]
fn normalizes_colon_list_slice_without_touching_relationship_type() {
    parse_query("MATCH (a:person)-[:knows]->(b) RETURN a.fName[:], [1,2,3][1:2]").unwrap();
}
