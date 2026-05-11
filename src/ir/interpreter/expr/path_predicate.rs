//! Path-shape predicates (simple_path / cyclic_path).
//!
//! Extracted from `interpreter.rs` lines 1951..1962.

use crate::ir::value::Value;

use std::collections::BTreeSet;

pub(crate) fn is_simple_path(items: &[Value]) -> bool {
    let mut seen = BTreeSet::new();
    for item in items {
        if let Value::Node { label, id } = item {
            let key = (label.clone(), *id);
            if !seen.insert(key) {
                return false;
            }
        }
    }
    true
}
