use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::collections::BTreeMap;

use orchiddb::ir::{
    PropertyGraph, Value,
    expr::IrExpr,
    policy::PropertyMissing,
    runtime::{Row, eval},
    value::STRUCT_ORDER_KEY,
};

struct CountingAllocator;
thread_local! {
    static ALLOCATED: Cell<Option<usize>> = const { Cell::new(None) };
}
fn record(bytes: usize) {
    let _ = ALLOCATED.try_with(|total| {
        if let Some(value) = total.get() {
            total.set(Some(value + bytes));
        }
    });
}
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        record(size);
        unsafe { System.realloc(pointer, layout, size) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn property(name: &str, policy: PropertyMissing) -> IrExpr {
    IrExpr::Property {
        binding: "record".into(),
        name: name.into(),
        policy,
    }
}

#[test]
fn property_access_does_not_copy_unselected_values() {
    let graph = PropertyGraph::new();
    let allocated = |size, expression: &IrExpr| {
        let row = Row::new().with(
            "record",
            Value::Map(BTreeMap::from([
                ("Name".into(), Value::Int(7)),
                (
                    "payload".into(),
                    Value::List(vec![Value::String("x".repeat(size))]),
                ),
            ])),
        );
        ALLOCATED.with(|total| total.set(Some(0)));
        let result = eval(expression, &row, &graph);
        let bytes = ALLOCATED.with(|total| total.replace(None).unwrap());
        assert_eq!(result.unwrap(), Value::Int(7));
        bytes
    };
    for name in ["Name", "nAME"] {
        let expression = property(name, PropertyMissing::NullOnMissing);
        assert_eq!(
            allocated(1, &expression),
            allocated(262_144, &expression),
            "reading {name} must not copy an unrelated nested payload"
        );
    }
}

#[test]
fn borrowed_property_access_preserves_field_precedence_and_missing_policy() {
    let graph = PropertyGraph::new();
    let mut row = Row::new().with(
        "record",
        Value::Map(BTreeMap::from([
            ("Name".into(), Value::Int(1)),
            ("NAME".into(), Value::Int(2)),
            ("null".into(), Value::Null),
            (
                STRUCT_ORDER_KEY.into(),
                Value::List(vec![
                    Value::String("Name".into()),
                    Value::String("NAME".into()),
                ]),
            ),
        ])),
    );
    let read = |row: &Row, name, policy| eval(&property(name, policy), row, &graph);
    assert_eq!(
        read(&row, "NAME", PropertyMissing::NullOnMissing).unwrap(),
        Value::Int(2)
    );
    assert_eq!(
        read(&row, "nAME", PropertyMissing::NullOnMissing).unwrap(),
        Value::Int(1)
    );
    for name in ["missing", "null"] {
        assert_eq!(
            read(&row, name, PropertyMissing::NullOnMissing).unwrap(),
            Value::Null
        );
        assert_eq!(
            read(&row, name, PropertyMissing::DropUnproductive).unwrap(),
            Value::Null
        );
        assert!(read(&row, name, PropertyMissing::Error).is_err());
    }
    let Value::Map(map) = row.bindings.get_mut("record").unwrap() else {
        unreachable!()
    };
    map.remove(STRUCT_ORDER_KEY);
    assert_eq!(
        read(&row, "nAME", PropertyMissing::NullOnMissing).unwrap(),
        Value::Int(2)
    );
    assert_eq!(
        read(&Row::new(), "Name", PropertyMissing::NullOnMissing).unwrap(),
        Value::Null
    );
    assert!(read(&Row::new(), "Name", PropertyMissing::Error).is_err());
}
