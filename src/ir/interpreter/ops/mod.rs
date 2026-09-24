//! Per-operator interpreter helpers.
//!
//! Each submodule corresponds to one Node variant family in
//! [`crate::ir::plan::Node`]; the pipeline is wired together by
//! [`super::run::run`].

pub(crate) mod aggregate;
pub(crate) mod apply;
pub(crate) mod barrier;
pub(crate) mod choose;
pub(crate) mod coalesce;
pub(crate) mod collect;
pub(crate) mod distinct;
pub(crate) mod expand;
pub(crate) mod join;
pub(crate) mod list_comprehension;
pub(crate) mod mutation;
pub(crate) mod path_pattern;
pub(crate) mod project;
pub(crate) mod quantifier;
pub(crate) mod repeat;
pub(crate) mod select;
pub(crate) mod slice;
pub(crate) mod sort;
pub(crate) mod source;
pub(crate) mod stream;
pub(crate) mod unwind;

pub(crate) mod sample;

pub(crate) mod java_hashmap;
