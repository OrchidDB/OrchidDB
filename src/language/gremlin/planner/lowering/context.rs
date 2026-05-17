//! Per-traversal compilation state and scope contracts.
//!
//! `Lowerer` is the only mutable state threaded through every helper in
//! the lowering pipeline. It mints synthetic binding names (so chains
//! like `g.V().out().out()` don't collide on `current`) and carries the
//! Gremlin-strategy flags consumed by `lower_traversal`'s leading-config
//! preamble.
//!
//! `TraversalContext` is deliberately separate from `Lowerer`: it describes
//! *where* in Gremlin semantics a traversal is being lowered. Root traversals
//! must start from a source (`g.V`, `g.E`, `inject`, source `union`), while
//! anonymous child traversals start from the parent traverser's `current`
//! binding through `GraphCorrelate`. The child kind records the contract
//! expected by the parent operator, so later semantic work can target the
//! right boundary instead of guessing from local syntax.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::{SackOp, Step};
use crate::language::gremlin::semantics::GValue;

pub(super) const CURRENT: &str = "current";
pub(super) const PATH: &str = "__path";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TraversalStart {
    Source,
    CorrelatedCurrent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TraversalOutput {
    TraverserStream,
    ExistencePredicate,
    Scalar,
    SideEffect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct TraversalContract {
    pub(super) start: TraversalStart,
    pub(super) output: TraversalOutput,
}

impl TraversalContract {
    const fn source_stream() -> Self {
        Self {
            start: TraversalStart::Source,
            output: TraversalOutput::TraverserStream,
        }
    }

    const fn correlated_stream() -> Self {
        Self {
            start: TraversalStart::CorrelatedCurrent,
            output: TraversalOutput::TraverserStream,
        }
    }

    const fn correlated_predicate() -> Self {
        Self {
            start: TraversalStart::CorrelatedCurrent,
            output: TraversalOutput::ExistencePredicate,
        }
    }

    const fn correlated_scalar() -> Self {
        Self {
            start: TraversalStart::CorrelatedCurrent,
            output: TraversalOutput::Scalar,
        }
    }

    const fn correlated_side_effect() -> Self {
        Self {
            start: TraversalStart::CorrelatedCurrent,
            output: TraversalOutput::SideEffect,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChildTraversalKind {
    SourceUnionArm,
    WherePredicate,
    NotPredicate,
    ChooseCondition,
    BranchDispatch,
    BranchArm,
    CoalesceArm,
    UnionArm,
    Local,
    Map,
    FlatMap,
    SideEffect,
    RepeatBody,
    RepeatEmitPredicate,
    RepeatUntilPredicate,
    ByModulator,
    ListRhs,
    StringRhs,
    MatchPattern,
    SubgraphFilter,
}

impl ChildTraversalKind {
    pub(super) const fn contract(self) -> TraversalContract {
        match self {
            ChildTraversalKind::SourceUnionArm => TraversalContract::source_stream(),
            ChildTraversalKind::WherePredicate
            | ChildTraversalKind::NotPredicate
            | ChildTraversalKind::ChooseCondition
            | ChildTraversalKind::RepeatEmitPredicate
            | ChildTraversalKind::RepeatUntilPredicate
            | ChildTraversalKind::SubgraphFilter => TraversalContract::correlated_predicate(),
            ChildTraversalKind::ByModulator
            | ChildTraversalKind::BranchDispatch
            | ChildTraversalKind::ListRhs
            | ChildTraversalKind::StringRhs => TraversalContract::correlated_scalar(),
            ChildTraversalKind::SideEffect => TraversalContract::correlated_side_effect(),
            ChildTraversalKind::BranchArm
            | ChildTraversalKind::CoalesceArm
            | ChildTraversalKind::UnionArm
            | ChildTraversalKind::Local
            | ChildTraversalKind::Map
            | ChildTraversalKind::FlatMap
            | ChildTraversalKind::RepeatBody
            | ChildTraversalKind::MatchPattern => TraversalContract::correlated_stream(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TraversalScopeKind {
    Root,
    Child(ChildTraversalKind),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TraversalContext {
    id: u32,
    parent: Option<u32>,
    scope: TraversalScopeKind,
    contract: TraversalContract,
}

impl TraversalContext {
    fn root(id: u32) -> Self {
        Self {
            id,
            parent: None,
            scope: TraversalScopeKind::Root,
            contract: TraversalContract::source_stream(),
        }
    }

    fn child(id: u32, parent: &TraversalContext, kind: ChildTraversalKind) -> Self {
        Self {
            id,
            parent: Some(parent.id),
            scope: TraversalScopeKind::Child(kind),
            contract: kind.contract(),
        }
    }

    pub(super) fn contract(&self) -> TraversalContract {
        self.contract
    }

    pub(super) fn scope(&self) -> TraversalScopeKind {
        self.scope
    }

    pub(super) fn parent_id(&self) -> Option<u32> {
        self.parent
    }

    pub(super) fn current_binding(&self) -> &'static str {
        CURRENT
    }

    pub(super) fn path_binding(&self) -> &'static str {
        PATH
    }
}

pub(super) struct Lowerer {
    next_id: u32,
    next_scope_id: u32,
    scope_stack: Vec<TraversalContext>,
    /// SubgraphStrategy `vertices: __.<sub>` filter. Each vertex-producing
    /// step (V scan, ExpandVertex) is post-filtered by this sub-traversal.
    pub(super) subgraph_vertex_filter: Option<Vec<Step>>,
    /// SubgraphStrategy `edges: __.<sub>` filter, applied to edge-producing
    /// steps (E scan, ExpandEdge).
    pub(super) subgraph_edge_filter: Option<Vec<Step>>,
    /// SubgraphStrategy `vertexProperties: __.<sub>` filter. The current
    /// row model does not expose vertex properties as first-class elements,
    /// so projectors use this as a focused value/properties visibility hook.
    pub(super) subgraph_vertex_property_filter: Option<Vec<Step>>,
    /// SubgraphStrategy `checkAdjacentVertices`; when false, visible edges
    /// are not rejected just because one endpoint fails the vertex filter.
    pub(super) subgraph_check_adjacent_vertices: bool,
    /// ProductiveByStrategy: when set, an unproductive `by(...)` projection
    /// surfaces NULL instead of dropping the row.
    pub(super) productive_by: bool,
    /// Root `withSack(initial)` value. The runtime carries it as a hidden
    /// per-row binding so `sack()` and `sack(op).by(...)` can stay
    /// traverser-local instead of global.
    pub(super) sack_initial: Option<GValue>,
    /// Hidden per-row projections written by side-effect steps such as
    /// `aggregate("x").by(...)`. `cap("x")` folds the recorded binding
    /// when the straight-line writer is visible.
    pub(super) side_effect_bags: BTreeMap<String, String>,
    /// Root `withSideEffect(label, seed)` values. These are global traversal
    /// entries and can be read by `select(label)` without a row-local writer.
    pub(super) side_effect_seeds: BTreeMap<String, GValue>,
    /// Reducer-style side effects configured by
    /// `withSideEffect(label, seed, op)`.
    pub(super) side_effect_reducers: BTreeMap<String, (GValue, SackOp)>,
    /// Labels written by real map-valued `groupCount(label)` side effects.
    /// These are read by `cap(label)` through the interpreter side channel.
    pub(super) group_count_side_effects: BTreeSet<String>,
    /// Recursion guard: when we are *evaluating* a subgraph filter
    /// sub-traversal we must not re-apply the strategy — otherwise the
    /// filter's own scans would each spawn another copy of the filter,
    /// recursively forever.
    pub(super) in_subgraph_filter_eval: bool,
}

impl Lowerer {
    pub(super) fn new() -> Self {
        Self {
            next_id: 0,
            next_scope_id: 0,
            scope_stack: Vec::new(),
            subgraph_vertex_filter: None,
            subgraph_edge_filter: None,
            subgraph_vertex_property_filter: None,
            subgraph_check_adjacent_vertices: true,
            productive_by: false,
            sack_initial: None,
            side_effect_bags: BTreeMap::new(),
            side_effect_seeds: BTreeMap::new(),
            side_effect_reducers: BTreeMap::new(),
            group_count_side_effects: BTreeSet::new(),
            in_subgraph_filter_eval: false,
        }
    }

    pub(super) fn fresh(&mut self, prefix: &str) -> String {
        let id = self.next_id;
        self.next_id += 1;
        format!("{prefix}_{id}")
    }

    pub(super) fn root_context(&mut self) -> TraversalContext {
        let id = self.fresh_scope_id();
        TraversalContext::root(id)
    }

    pub(super) fn child_context(
        &mut self,
        parent: &TraversalContext,
        kind: ChildTraversalKind,
    ) -> TraversalContext {
        let id = self.fresh_scope_id();
        TraversalContext::child(id, parent, kind)
    }

    pub(super) fn enter_context<T, F>(&mut self, ctx: TraversalContext, f: F) -> T
    where
        F: FnOnce(&mut Lowerer, &TraversalContext) -> T,
    {
        self.scope_stack.push(ctx);
        let active = self
            .scope_stack
            .last()
            .cloned()
            .expect("context just pushed");
        let _parent = active.parent_id();
        let out = f(self, &active);
        let popped = self.scope_stack.pop();
        debug_assert_eq!(popped.as_ref(), Some(&active));
        out
    }

    /// Property-missing policy used by every `by(...)` projection site.
    /// `ProductiveByStrategy` flips this from drop-the-row to keep-with-NULL.
    pub(super) fn by_property_missing(&self) -> PropertyMissing {
        if self.productive_by {
            PropertyMissing::NullOnMissing
        } else {
            PropertyMissing::DropUnproductive
        }
    }

    fn fresh_scope_id(&mut self) -> u32 {
        let id = self.next_scope_id;
        self.next_scope_id += 1;
        id
    }
}
