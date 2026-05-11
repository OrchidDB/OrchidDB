//! Per-operator interpreter helpers.
//!
//! Each submodule corresponds to one Node variant family in
//! [`crate::ir::plan::Node`]; the pipeline is wired together by
//! [`super::run::run`].

pub(super) mod aggregate;
pub(super) mod apply;
pub(super) mod barrier;
pub(super) mod choose;
pub(super) mod coalesce;
pub(super) mod collect;
pub(super) mod distinct;
pub(super) mod expand;
pub(super) mod join;
pub(super) mod list_comprehension;
pub(super) mod path_pattern;
pub(super) mod project;
pub(super) mod quantifier;
pub(super) mod repeat;
pub(super) mod select;
pub(super) mod slice;
pub(super) mod sort;
pub(super) mod source;
pub(super) mod unwind;
