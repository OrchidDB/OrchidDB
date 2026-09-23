use std::collections::BTreeMap;

use new_graph::ir::value::Value;
use new_graph::language::cypher::ast::{Clause, Expr, Literal};
use new_graph::language::cypher::parameters::bind_parameters;
use new_graph::language::cypher::parser::parse_query;

fn params(entries: &[(&str, Value)]) -> BTreeMap<String, Value> {
    entries
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect()
}

fn string(value: &str) -> Value {
    Value::String(value.to_string())
}

fn first_return_expr(query: &new_graph::language::cypher::ast::Query) -> &Expr {
    let Clause::Return(ret) = &query.clauses[0] else {
        panic!("expected RETURN clause");
    };
    &ret.projection.items[0].expr
}

#[test]
fn binds_injection_string_as_literal() {
    let mut query = parse_query("MATCH (n) WHERE n.name = $name RETURN n").unwrap();
    let payload = "Robert'); DROP TABLE users;--";
    bind_parameters(&mut query, &params(&[("name", string(payload))])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Binary { lhs, rhs, .. }) = &m.predicate else {
        panic!("expected binary predicate");
    };
    assert!(matches!(**lhs, Expr::Property { .. }));
    assert_eq!(**rhs, Expr::Literal(Literal::String(payload.to_string())));
    assert!(!matches!(&**rhs, Expr::Parameter(_)));
}

#[test]
fn binds_null_parameter() {
    let mut query = parse_query("MATCH (n) WHERE n.age = $age RETURN n").unwrap();
    bind_parameters(&mut query, &params(&[("age", Value::Null)])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Binary { rhs, .. }) = &m.predicate else {
        panic!("expected binary predicate");
    };
    assert_eq!(**rhs, Expr::Literal(Literal::Null));
}

#[test]
fn binds_bool_parameter() {
    let mut query = parse_query("MATCH (n) WHERE n.active = $active RETURN n").unwrap();
    bind_parameters(&mut query, &params(&[("active", Value::Bool(true))])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Binary { rhs, .. }) = &m.predicate else {
        panic!("expected binary predicate");
    };
    assert_eq!(**rhs, Expr::Literal(Literal::Bool(true)));
}

#[test]
fn binds_integer_parameter() {
    let mut query = parse_query("MATCH (n) WHERE n.age = $age RETURN n").unwrap();
    bind_parameters(&mut query, &params(&[("age", Value::Int(42))])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Binary { rhs, .. }) = &m.predicate else {
        panic!("expected binary predicate");
    };
    assert_eq!(**rhs, Expr::Literal(Literal::Integer("42".to_string())));
}

#[test]
fn binds_float_parameter() {
    let mut query = parse_query("MATCH (n) WHERE n.score > $score RETURN n").unwrap();
    bind_parameters(&mut query, &params(&[("score", Value::Float(3.5))])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Binary { rhs, .. }) = &m.predicate else {
        panic!("expected binary predicate");
    };
    assert_eq!(**rhs, Expr::Literal(Literal::Float(3.5)));
}

#[test]
fn binds_list_parameter() {
    let mut query = parse_query("MATCH (n) WHERE n.id IN $ids RETURN n").unwrap();
    let ids = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
    bind_parameters(&mut query, &params(&[("ids", ids.clone())])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Function { name, args, .. }) = &m.predicate else {
        panic!("expected IN function");
    };
    assert_eq!(name, "in");
    let Expr::List(items) = &args[1] else {
        panic!("expected list literal");
    };
    assert_eq!(items.len(), 3);
    assert_eq!(items[0], Expr::Literal(Literal::Integer("1".to_string())));
}

#[test]
fn binds_map_parameter() {
    let mut query = parse_query("MATCH (n) WHERE n.props = $props RETURN n").unwrap();
    let mut map = BTreeMap::new();
    map.insert("name".to_string(), string("Alice"));
    map.insert("age".to_string(), Value::Int(30));
    bind_parameters(&mut query, &params(&[("props", Value::Map(map))])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Binary { rhs, .. }) = &m.predicate else {
        panic!("expected binary predicate");
    };
    let Expr::Map(entries) = &**rhs else {
        panic!("expected map literal");
    };
    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries.iter().find(|(k, _)| k == "name").map(|(_, v)| v),
        Some(&Expr::Literal(Literal::String("Alice".to_string())))
    );
    assert_eq!(
        entries.iter().find(|(k, _)| k == "age").map(|(_, v)| v),
        Some(&Expr::Literal(Literal::Integer("30".to_string())))
    );
}

#[test]
fn fails_on_missing_parameter() {
    let mut query = parse_query("MATCH (n) WHERE n.name = $name RETURN n").unwrap();
    let err = bind_parameters(&mut query, &BTreeMap::new()).unwrap_err();
    assert!(err.contains("missing value for parameter `$name`"), "{err}");
}

#[test]
fn fails_on_unsupported_parameter_value() {
    let mut query = parse_query("MATCH (n) WHERE n.other = $other RETURN n").unwrap();
    let err = bind_parameters(
        &mut query,
        &params(&[(
            "other",
            Value::Node {
                label: "person".to_string(),
                id: 1,
            },
        )]),
    )
    .unwrap_err();
    assert!(
        err.contains("unsupported parameter value of type `node`"),
        "{err}"
    );
}

#[test]
fn binds_repeated_parameter() {
    let mut query =
        parse_query("MATCH (a) WHERE a.x = $v MATCH (b) WHERE b.y = $v RETURN a, b").unwrap();
    bind_parameters(&mut query, &params(&[("v", string("shared"))])).unwrap();

    let Clause::Match(first) = &query.clauses[0] else {
        panic!("expected first MATCH");
    };
    let Clause::Match(second) = &query.clauses[1] else {
        panic!("expected second MATCH");
    };
    for m in [first, second] {
        let Some(Expr::Binary { rhs, .. }) = &m.predicate else {
            panic!("expected binary predicate");
        };
        assert_eq!(**rhs, Expr::Literal(Literal::String("shared".to_string())));
    }
}

#[test]
fn binds_limit_and_skip_parameters() {
    let mut query = parse_query("MATCH (n) RETURN n SKIP $offset LIMIT $maximum").unwrap();
    bind_parameters(
        &mut query,
        &params(&[("offset", Value::Int(10)), ("maximum", Value::Int(25))]),
    )
    .unwrap();

    let Clause::Return(ret) = &query.clauses[1] else {
        panic!("expected RETURN clause");
    };
    assert_eq!(
        ret.projection.skip,
        Some(Expr::Literal(Literal::Integer("10".to_string())))
    );
    assert_eq!(
        ret.projection.limit,
        Some(Expr::Literal(Literal::Integer("25".to_string())))
    );
}

#[test]
fn binds_pattern_property_parameter() {
    let mut query = parse_query("MATCH (n:person {name: $name}) RETURN n").unwrap();
    bind_parameters(&mut query, &params(&[("name", string("Alice"))])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let properties = m.patterns[0].element.start.properties.as_ref().unwrap();
    assert_eq!(
        *properties,
        Expr::Map(vec![(
            "name".into(),
            Expr::Literal(Literal::String("Alice".to_string()))
        )])
    );
}

#[test]
fn binds_set_mutation_parameter() {
    let mut query = parse_query("MATCH (n) SET n.name = $name RETURN n").unwrap();
    bind_parameters(&mut query, &params(&[("name", string("Bob"))])).unwrap();

    let Clause::Set(set) = &query.clauses[1] else {
        panic!("expected SET clause");
    };
    let new_graph::language::cypher::ast::SetItem::Property { value, .. } = &set.items[0] else {
        panic!("expected property set item");
    };
    assert_eq!(*value, Expr::Literal(Literal::String("Bob".to_string())));
}

#[test]
fn binds_unwind_parameter() {
    let mut query = parse_query("UNWIND $items AS x RETURN x").unwrap();
    bind_parameters(
        &mut query,
        &params(&[("items", Value::List(vec![Value::Int(1), Value::Int(2)]))]),
    )
    .unwrap();

    let Clause::Unwind(unwind) = &query.clauses[0] else {
        panic!("expected UNWIND clause");
    };
    assert!(matches!(unwind.expr, Expr::List(_)));
}

#[test]
fn binds_nested_exists_subquery_parameter() {
    let mut query =
        parse_query("MATCH (n) WHERE EXISTS { MATCH (n)-[:r]->(m) WHERE m.x = $x } RETURN n")
            .unwrap();
    bind_parameters(&mut query, &params(&[("x", Value::Int(7))])).unwrap();

    let Clause::Match(m) = &query.clauses[0] else {
        panic!("expected MATCH clause");
    };
    let Some(Expr::Exists(exists)) = &m.predicate else {
        panic!("expected EXISTS predicate");
    };
    let Some(Expr::Binary { rhs, .. }) = exists.predicate.as_deref() else {
        panic!("expected EXISTS pattern predicate");
    };
    assert_eq!(**rhs, Expr::Literal(Literal::Integer("7".to_string())));
}

#[test]
fn binds_union_parameters() {
    let mut query = parse_query("RETURN $a AS x UNION RETURN $b AS x").unwrap();
    bind_parameters(
        &mut query,
        &params(&[("a", Value::Int(1)), ("b", Value::Int(2))]),
    )
    .unwrap();

    assert_eq!(
        *first_return_expr(&query),
        Expr::Literal(Literal::Integer("1".to_string()))
    );
    let branch = &query.unions[0];
    assert_eq!(
        *first_return_expr(&branch.query),
        Expr::Literal(Literal::Integer("2".to_string()))
    );
}

#[test]
fn binding_failure_preserves_the_parsed_query() {
    let mut query = parse_query("RETURN $present, $missing").unwrap();
    let original = query.clone();
    assert!(bind_parameters(&mut query, &params(&[("present", Value::Int(1))])).is_err());
    assert_eq!(query, original);
}

#[test]
fn exact_decimals_and_nonfinite_floats_are_not_silently_approximated() {
    let mut query = parse_query("RETURN $value").unwrap();
    for value in [
        Value::Float(f64::INFINITY),
        Value::Float(f64::NAN),
        Value::BigDecimal("12345678901234567890.123456789".parse().unwrap()),
    ] {
        assert!(bind_parameters(&mut query, &params(&[("value", value)])).is_err());
    }
}
