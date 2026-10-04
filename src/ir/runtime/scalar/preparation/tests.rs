use super::*;
use crate::ir::runtime::scalar::{CastMode, cast_conversion::cast_value, eval_call};
use crate::ir::{PropertyGraph, Value};

#[test]
fn preparation_reuses_metadata_and_never_caches_results() {
    let cache = Arc::new(ScalarPreparation::default());
    let graph = PropertyGraph::new();
    with_preparation(&cache, || {
        assert!(Arc::ptr_eq(&regex("^a").unwrap(), &regex("^a").unwrap()));
        assert!(regex("[").is_err());
        assert!(regex("[").is_err());
        assert_eq!(cache.regexes.len(), 2);
        assert!(Arc::ptr_eq(&call("LCASE"), &call("LCASE")));
        assert_eq!(call("LCASE").canonical, "lower");
        assert!(call("LCASE").known);
        for (input, output) in [("HELLO", "hello"), ("WORLD", "world")] {
            assert_eq!(
                eval_call("LCASE", vec![Value::String(input.into())], &graph).unwrap(),
                Value::String(output.into())
            );
        }
        assert!(Arc::ptr_eq(&cast("INT64[]"), &cast("INT64[]")));
        assert_eq!(cache.calls.len(), 1);
        assert_eq!(
            eval_call(
                "regexp_full_match",
                vec![Value::Null, Value::String("[".into())],
                &graph
            )
            .unwrap(),
            Value::Null
        );
    });
    assert!(ACTIVE.with(|active| active.borrow().is_none()));
    let other = Arc::new(ScalarPreparation::default());
    with_preparation(&other, || {
        regex("^b").unwrap();
    });
    assert_eq!(other.regexes.len(), 1);
    assert_eq!(cache.regexes.len(), 2);
}

#[test]
fn preparation_restores_nested_scope_after_unwind_and_moves_between_threads() {
    let outer = Arc::new(ScalarPreparation::default());
    let inner = Arc::new(ScalarPreparation::default());
    with_preparation(&outer, || {
        let pattern = regex("outer").unwrap();
        assert!(
            std::panic::catch_unwind(|| with_preparation(&inner, || {
                regex("inner").unwrap();
                panic!("query failed");
            }))
            .is_err()
        );
        assert!(Arc::ptr_eq(&pattern, &regex("outer").unwrap()));
        let shared = outer.clone();
        let from_worker =
            std::thread::spawn(move || with_preparation(&shared, || regex("outer").unwrap()))
                .join()
                .unwrap();
        assert!(Arc::ptr_eq(&pattern, &from_worker));
    });
    assert!(ACTIVE.with(|active| active.borrow().is_none()));
    assert_eq!(outer.regexes.len(), 1);
    assert_eq!(inner.regexes.len(), 1);
}

#[test]
fn prepared_casts_preserve_nested_types_nulls_and_error_modes() {
    let cache = Arc::new(ScalarPreparation::default());
    with_preparation(&cache, || {
        let strict = CastMode::ExplicitStrict;
        let input = Value::String("[[1, 2], [3, null]]".into());
        let expected = Value::List(vec![
            Value::List(vec![Value::Long(1), Value::Long(2)]),
            Value::List(vec![Value::Long(3), Value::Null]),
        ]);
        for _ in 0..3 {
            assert_eq!(cast_value(&input, "INT64[][]", strict).unwrap(), expected);
        }
        assert_eq!(
            cache.casts.len(),
            3,
            "one descriptor per distinct nested type"
        );
        assert_eq!(
            cast_value(&Value::Null, "invalid target", strict).unwrap(),
            Value::Null
        );
        assert_eq!(
            cache.casts.len(),
            3,
            "null must not prepare or validate its target"
        );
        let overflow = Value::Int(300);
        assert!(cast_value(&overflow, "INT8", strict).is_err());
        assert_eq!(
            cast_value(&overflow, "INT8", CastMode::TryOrLenient).unwrap(),
            Value::Null
        );
        assert!(cast_value(&Value::List(vec![Value::Int(1)]), "INT64[2]", strict).is_err());
        let Value::Map(record) = cast_value(
            &Value::String("{a: 1, b: [2, 3]}".into()),
            "STRUCT(a INT64, b INT64[])",
            strict,
        )
        .unwrap() else {
            panic!("struct")
        };
        assert_eq!(record["a"], Value::Long(1));
        assert_eq!(
            record["b"],
            Value::List(vec![Value::Long(2), Value::Long(3)])
        );
        let Value::Map(union) =
            cast_value(&Value::Long(7), "UNION(n INT64, s STRING)", strict).unwrap()
        else {
            panic!("union")
        };
        assert_eq!(union["__tag"], Value::String("n".into()));
        assert_eq!(union["__value"], Value::Long(7));
    });
}

#[test]
fn shared_duration_pattern_preserves_components_and_invalid_input() {
    use crate::ir::temporal::parse;
    for _ in 0..3 {
        assert_eq!(
            parse("duration", "PT60S").unwrap(),
            parse("duration", "PT1M").unwrap()
        );
        assert_eq!(
            parse("duration", "PT1.5S").unwrap(),
            parse("duration", "PT1,5S").unwrap()
        );
        assert!(parse("duration", "Pinvalid").is_err());
    }
}
