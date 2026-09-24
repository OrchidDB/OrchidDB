//! Path-shape predicates (simple_path / cyclic_path).
//!
//! Extracted from `interpreter.rs` lines 1951..1962.

use crate::ir::value::Value;

/// A simple path contains no repeated objects, including projected scalar
/// values and edges. Comparing only vertices loses cycles after by().
pub(crate) fn is_simple_path(items: &[Value]) -> bool {
    !items.iter().enumerate().any(|(index, item)| items[..index].contains(item))
}
