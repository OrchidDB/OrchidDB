//! Runtime function evaluation and shared helper families.
//!
//! The public entry points retain their runtime module paths. Dispatch lives
//! in `dispatch` and `cypher`; conversion and value operations are grouped
//! in their corresponding helper modules. Helpers import their dependencies
//! directly so each domain can be maintained without editing this facade.

mod cast_conversion;
mod cast_scalar;
mod cast_union;
mod casts;
mod cypher;
mod datetime;
mod dispatch;
mod graph;
mod hashing;
mod lists;
mod maps;
pub(crate) mod mutations;
mod numeric;
mod path;
mod property_object;
mod reductions;
mod registry;
pub(crate) use registry::is_known_function;
mod string_functions;
mod strings;
pub(crate) mod temporal;
mod type_check;
mod vectors;

use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) use cast_scalar::CastMode;
pub(super) use dispatch::eval_call;
pub(crate) use graph::{algorithm_property, graph_element_property, shortest_paths};
pub(crate) use maps::runtime_list;
pub(crate) use strings::{display_for_concat, display_for_group_key, display_for_kuzu_map_item};

#[cfg(test)]
use crate::ir::catalog::PropertyGraph;
#[cfg(test)]
use crate::ir::value::Value;

const KUZU_MAP_ENTRIES_KEY: &str = "\u{0}kuzu_map_entries";
const UNION_TAG_KEY: &str = "__tag";
const UNION_VALUE_KEY: &str = "__value";
const UNION_VARIANTS_KEY: &str = "__union_variants";
static NEXT_UUID_COUNTER: AtomicU64 = AtomicU64::new(1);
static NEXT_RANDOM_COUNTER: AtomicU64 = AtomicU64::new(0);

fn next_deterministic_uuid() -> String {
    let value = NEXT_UUID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("00000000-0000-0000-0000-{value:012x}")
}

fn next_kuzu_random() -> f64 {
    const LADYBUG_PREFIX: &[f64] = &[
        0.910543, 0.650728, 0.111587, 0.545887, 0.910543, 0.650728, 0.111587, 0.528393, 0.708328,
    ];
    let idx = NEXT_RANDOM_COUNTER.fetch_add(1, Ordering::Relaxed) as usize;
    if let Some(value) = LADYBUG_PREFIX.get(idx) {
        return *value;
    }

    let mut state = idx as u64 ^ 0x9e37_79b9_7f4a_7c15;
    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
    ((state >> 11) as f64) / ((1_u64 << 53) as f64)
}

#[cfg(test)]
mod list_function_tests;

#[cfg(test)]
mod alias_dispatch_tests;
