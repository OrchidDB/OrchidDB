use super::{
    collect::collect_op,
    project::{current_project_op, project_op},
    unwind::unwind_op,
};
use crate::ir::{
    PropertyGraph, Value,
    expr::IrExpr,
    plan::{NullsOrder, ProjectMode, ProjectionItem, SortDir, SortKey},
    policy::PropertyMissing,
    runtime::Row,
};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    collections::BTreeMap,
};

// Count this test thread only. Net bytes include releases of inputs created
// before measurement, so peak measures growth above the input's footprint.
#[derive(Clone, Copy, Default, Debug)]
struct Allocations {
    allocated: usize,
    net: i64,
    peak: i64,
}
thread_local! {
    static COUNTER: Cell<Option<Allocations>> = const { Cell::new(None) };
}
fn record(allocated: usize, released: usize) {
    let _ = COUNTER.try_with(|counter| {
        if let Some(mut value) = counter.get() {
            value.allocated += allocated;
            value.net += allocated as i64 - released as i64;
            value.peak = value.peak.max(value.net);
            counter.set(Some(value));
        }
    });
}
struct Allocator;
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size(), 0);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size(), 0);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size, layout.size());
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record(0, layout.size());
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
fn measure<T>(run: impl FnOnce() -> T) -> (T, Allocations) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            COUNTER.with(|counter| counter.set(None));
        }
    }
    let _reset = Reset;
    COUNTER.with(|counter| counter.set(Some(Allocations::default())));
    let result = run();
    (
        result,
        COUNTER.with(|counter| counter.replace(None).unwrap()),
    )
}
fn binding(name: &str) -> IrExpr {
    IrExpr::Binding(name.into())
}
fn string_pointer(row: &Row, name: &str) -> *const u8 {
    let Value::String(value) = &row.bindings[name] else {
        panic!("expected string")
    };
    value.as_ptr()
}
fn sort_key() -> SortKey {
    SortKey {
        expr: binding("key"),
        dir: SortDir::Asc,
        nulls: NullsOrder::Last,
    }
}

#[test]
fn projection_moves_payload_and_evaluates_aliases_in_original_scope() {
    let graph = PropertyGraph::new();
    let items = [
        ProjectionItem {
            alias: "x".into(),
            expr: IrExpr::lit_int(2),
        },
        ProjectionItem {
            alias: "observed".into(),
            expr: binding("x"),
        },
        ProjectionItem {
            alias: "x".into(),
            expr: IrExpr::lit_int(3),
        },
    ];
    for mode in [
        ProjectMode::PreserveVisible,
        ProjectMode::ReplaceCurrent,
        ProjectMode::ReplaceScope,
    ] {
        let mut row = Row::new()
            .with("payload", Value::String("x".repeat(65_536)))
            .with("x", Value::Int(1));
        row.bulk = 7;
        let pointer = string_pointer(&row, "payload");
        let (result, memory) = measure(|| project_op(mode, &items, vec![row], &graph).unwrap());
        assert_eq!(result[0].get("x"), Value::Int(3));
        assert_eq!(result[0].get("observed"), Value::Int(1));
        assert_eq!(result[0].bulk, 7);
        if matches!(mode, ProjectMode::ReplaceScope) {
            assert!(!result[0].bindings.contains_key("payload"));
        } else {
            assert_eq!(string_pointer(&result[0], "payload"), pointer);
        }
        assert!(
            memory.allocated < 65_536,
            "projection copied payload: {memory:?}"
        );
    }
}

#[test]
fn current_projection_copies_selected_value_once_and_preserves_productive_null() {
    let graph = PropertyGraph::new();
    let expression = IrExpr::Property {
        binding: "current".into(),
        name: "value".into(),
        policy: PropertyMissing::DropUnproductive,
    };
    for typed in [false, true] {
        let value = Value::String("x".repeat(65_536));
        let owner = if typed {
            Value::TypedMap(vec![(Value::String("value".into()), value)])
        } else {
            Value::Map(BTreeMap::from([("value".into(), value)]))
        };
        let row = Row::new().with("current", owner);
        let (result, memory) =
            measure(|| current_project_op(&expression, vec![row], &graph).unwrap());
        assert!(
            matches!(&result[0].bindings["current"], Value::String(value) if value.len() == 65_536)
        );
        assert!(
            memory.allocated < 2 * 65_536,
            "selected value copied twice: {memory:?}"
        );
    }
    for owner in [
        Value::Map(BTreeMap::new()),
        Value::Map(BTreeMap::from([("value".into(), Value::Null)])),
        Value::TypedMap(vec![]),
    ] {
        let rows = current_project_op(&expression, vec![Row::new().with("current", owner)], &graph)
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("current"), Value::Null);
    }
}

#[test]
fn unwind_does_not_copy_overwritten_payload_and_moves_single_output_rows() {
    let graph = PropertyGraph::new();
    let entry = Value::Map(BTreeMap::from([
        ("key".into(), Value::String("a".into())),
        ("value".into(), Value::Int(1)),
    ]));
    let cases = [
        (
            Value::List(vec![Value::Int(1), Value::Int(2)]),
            false,
            vec![Value::Int(1), Value::Int(2)],
        ),
        (
            Value::BulkSet(vec![Value::Int(1), Value::Int(1)]),
            false,
            vec![Value::Int(1), Value::Int(1)],
        ),
        (
            Value::Set(vec![Value::Int(2), Value::Int(1)]),
            false,
            vec![Value::Int(2), Value::Int(1)],
        ),
        (
            Value::Map(BTreeMap::from([("a".into(), Value::Int(1))])),
            false,
            vec![entry.clone()],
        ),
        (
            Value::TypedMap(vec![(Value::String("a".into()), Value::Int(1))]),
            false,
            vec![entry],
        ),
        (Value::Null, true, vec![Value::Null]),
        (Value::Null, false, vec![]),
        (Value::List(vec![]), true, vec![Value::Null]),
        (Value::List(vec![]), false, vec![]),
        (Value::Set(vec![]), true, vec![Value::Null]),
        (Value::Map(BTreeMap::new()), true, vec![]),
        (Value::TypedMap(vec![]), true, vec![]),
        (Value::Int(1), false, vec![Value::Int(1)]),
    ];
    for (value, outer, expected) in cases {
        let mut row = Row::new()
            .with("input", value)
            .with("current", Value::String("x".repeat(65_536)))
            .with("retained", Value::String("keep".into()));
        row.bulk = 3;
        let pointer = string_pointer(&row, "retained");
        let expression = binding("input");
        let (result, memory) =
            measure(|| unwind_op(&expression, "current", outer, vec![row], &graph).unwrap());
        assert_eq!(
            result
                .iter()
                .map(|row| row.get("current"))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(
            result
                .iter()
                .all(|row| row.bulk == 3 && row.get("retained") == Value::String("keep".into()))
        );
        if result.len() == 1 {
            assert_eq!(string_pointer(&result[0], "retained"), pointer);
        }
        assert!(
            memory.allocated < 65_536,
            "unwind copied overwritten payload: {memory:?}"
        );
    }
    let row = Row::new().with("current", Value::List(vec![Value::Int(1), Value::Int(2)]));
    let result = unwind_op(&binding("current"), "current", false, vec![row], &graph).unwrap();
    assert_eq!(
        result
            .iter()
            .map(|row| row.get("current"))
            .collect::<Vec<_>>(),
        vec![Value::Int(1), Value::Int(2)]
    );
}

#[test]
fn collect_releases_input_payloads_while_building_results() {
    let graph = PropertyGraph::new();
    let expression = binding("value");
    for ordered in [false, true] {
        let rows = (0..64)
            .map(|i| {
                Row::new()
                    .with("value", Value::String("x".repeat(8_192)))
                    .with("key", Value::Int(i))
            })
            .collect();
        let order = if ordered { vec![sort_key()] } else { vec![] };
        let (result, memory) =
            measure(|| collect_op(&expression, false, &order, "result", rows, &graph).unwrap());
        let Value::List(values) = &result[0].bindings["result"] else {
            panic!("expected list")
        };
        assert_eq!(values.len(), 64);
        assert!(
            memory.peak < 64 * 8_192 / 2,
            "collect retained inputs alongside results: {memory:?}"
        );
        eprintln!("collect ordered={ordered}: {memory:?}");
    }
}

#[test]
fn collect_preserves_stable_order_distinctness_nulls_and_empty_input() {
    let graph = PropertyGraph::new();
    let expression = binding("value");
    let rows = [
        (Value::Int(2), 1),
        (Value::Int(1), 1),
        (Value::Int(2), 0),
        (Value::Null, 2),
    ]
    .into_iter()
    .map(|(value, key)| Row::new().with("value", value).with("key", Value::Int(key)))
    .collect::<Vec<_>>();
    for ordered in [false, true] {
        for distinct in [false, true] {
            let order = if ordered { vec![sort_key()] } else { vec![] };
            let result = collect_op(
                &expression,
                distinct,
                &order,
                "result",
                rows.clone(),
                &graph,
            )
            .unwrap();
            let expected = match (ordered, distinct) {
                (_, true) => vec![Value::Int(2), Value::Int(1), Value::Null],
                (true, false) => vec![Value::Int(2), Value::Int(2), Value::Int(1), Value::Null],
                (false, false) => vec![Value::Int(2), Value::Int(1), Value::Int(2), Value::Null],
            };
            assert_eq!(result[0].get("result"), Value::List(expected));
            assert_eq!(
                collect_op(&expression, distinct, &order, "result", vec![], &graph).unwrap()[0]
                    .get("result"),
                Value::List(vec![])
            );
        }
    }
}
