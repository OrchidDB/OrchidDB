//! Cypher traversal and scope contracts.
//!
//! This mirrors Gremlin's lowering model: a central `Lowerer` owns the active
//! traversal stack and lexical row scope. Individual modules lower one syntax
//! family, but they do not own independent context objects.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::expr::BindingId;
pub(crate) use crate::language::cypher::semantics::BindingKind;

#[derive(Debug, Clone, Default)]
pub(crate) struct ScopeFrame {
    pub(crate) visible: BTreeSet<BindingId>,
    pub(crate) nullable: BTreeSet<BindingId>,
    pub(crate) kinds: BTreeMap<BindingId, BindingKind>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TraversalStart {
    Root,
    Correlated,
    Imported,
    IndependentBranch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TraversalOutput {
    Rows,
    Scalar,
    Predicate,
    Mutation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScopeEffect {
    Preserve,
    AddBindings,
    Replace,
    NullableAdd,
    BranchMerge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CypherTraversalContract {
    pub(crate) start: TraversalStart,
    pub(crate) output: TraversalOutput,
    pub(crate) scope_effect: ScopeEffect,
}

impl CypherTraversalContract {
    const fn root_rows() -> Self {
        Self {
            start: TraversalStart::Root,
            output: TraversalOutput::Rows,
            scope_effect: ScopeEffect::Preserve,
        }
    }

    const fn correlated_rows(scope_effect: ScopeEffect) -> Self {
        Self {
            start: TraversalStart::Correlated,
            output: TraversalOutput::Rows,
            scope_effect,
        }
    }

    const fn correlated_scalar(scope_effect: ScopeEffect) -> Self {
        Self {
            start: TraversalStart::Correlated,
            output: TraversalOutput::Scalar,
            scope_effect,
        }
    }

    const fn correlated_predicate() -> Self {
        Self {
            start: TraversalStart::Correlated,
            output: TraversalOutput::Predicate,
            scope_effect: ScopeEffect::Preserve,
        }
    }

    const fn branch_rows() -> Self {
        Self {
            start: TraversalStart::IndependentBranch,
            output: TraversalOutput::Rows,
            scope_effect: ScopeEffect::BranchMerge,
        }
    }

    const fn mutation() -> Self {
        Self {
            start: TraversalStart::Correlated,
            output: TraversalOutput::Mutation,
            scope_effect: ScopeEffect::Preserve,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CypherTraversalKind {
    RootQuery,

    MatchPattern,
    OptionalMatchPattern,
    PatternPart,
    FixedExpand,
    VariableLengthExpand,
    QuantifiedPath,
    ShortestPath,

    WherePredicate,
    ExistsSubquery,
    PatternPredicate,

    WithProjection,
    ReturnProjection,
    Aggregation,

    Unwind,

    CallSubquery,
    ProcedureCall,

    ListComprehension,
    PatternComprehension,
    Quantifier,

    UnionBranch,

    Create,
    Merge,
    Set,
    Delete,
    Remove,
}

impl CypherTraversalKind {
    pub(crate) const fn contract(self) -> CypherTraversalContract {
        match self {
            CypherTraversalKind::RootQuery => CypherTraversalContract::root_rows(),

            CypherTraversalKind::MatchPattern
            | CypherTraversalKind::PatternPart
            | CypherTraversalKind::FixedExpand
            | CypherTraversalKind::VariableLengthExpand
            | CypherTraversalKind::QuantifiedPath
            | CypherTraversalKind::ShortestPath
            | CypherTraversalKind::Unwind
            | CypherTraversalKind::CallSubquery
            | CypherTraversalKind::ProcedureCall => {
                CypherTraversalContract::correlated_rows(ScopeEffect::AddBindings)
            }

            CypherTraversalKind::OptionalMatchPattern => {
                CypherTraversalContract::correlated_rows(ScopeEffect::NullableAdd)
            }

            CypherTraversalKind::WherePredicate
            | CypherTraversalKind::ExistsSubquery
            | CypherTraversalKind::PatternPredicate => {
                CypherTraversalContract::correlated_predicate()
            }

            CypherTraversalKind::WithProjection | CypherTraversalKind::ReturnProjection => {
                CypherTraversalContract::correlated_scalar(ScopeEffect::Replace)
            }

            CypherTraversalKind::Aggregation => {
                CypherTraversalContract::correlated_scalar(ScopeEffect::Replace)
            }

            CypherTraversalKind::ListComprehension
            | CypherTraversalKind::PatternComprehension
            | CypherTraversalKind::Quantifier => {
                CypherTraversalContract::correlated_scalar(ScopeEffect::Preserve)
            }

            CypherTraversalKind::UnionBranch => CypherTraversalContract::branch_rows(),

            CypherTraversalKind::Create
            | CypherTraversalKind::Merge
            | CypherTraversalKind::Set
            | CypherTraversalKind::Delete
            | CypherTraversalKind::Remove => CypherTraversalContract::mutation(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CypherTraversalContext {
    id: u32,
    parent: Option<u32>,
    kind: CypherTraversalKind,
    contract: CypherTraversalContract,
    imports: BTreeSet<BindingId>,
    correlation: BTreeSet<BindingId>,
    produced: BTreeSet<BindingId>,
    nullable: BTreeSet<BindingId>,
}

impl CypherTraversalContext {
    pub(crate) fn root(id: u32) -> Self {
        Self::new(id, None, CypherTraversalKind::RootQuery)
    }

    pub(crate) fn child(
        id: u32,
        parent: &CypherTraversalContext,
        kind: CypherTraversalKind,
    ) -> Self {
        Self::new(id, Some(parent.id), kind)
    }

    fn new(id: u32, parent: Option<u32>, kind: CypherTraversalKind) -> Self {
        Self {
            id,
            parent,
            kind,
            contract: kind.contract(),
            imports: BTreeSet::new(),
            correlation: BTreeSet::new(),
            produced: BTreeSet::new(),
            nullable: BTreeSet::new(),
        }
    }

    pub(crate) fn kind(&self) -> CypherTraversalKind {
        self.kind
    }

    pub(crate) fn contract(&self) -> CypherTraversalContract {
        self.contract
    }

    pub(crate) fn parent_id(&self) -> Option<u32> {
        self.parent
    }

    pub(crate) fn imports(&self) -> &BTreeSet<BindingId> {
        &self.imports
    }

    pub(crate) fn correlation(&self) -> &BTreeSet<BindingId> {
        &self.correlation
    }

    pub(crate) fn produced(&self) -> &BTreeSet<BindingId> {
        &self.produced
    }

    pub(crate) fn nullable(&self) -> &BTreeSet<BindingId> {
        &self.nullable
    }

    pub(crate) fn add_imports<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        self.imports.extend(bindings);
    }

    pub(crate) fn add_correlation<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        self.correlation.extend(bindings);
    }

    pub(crate) fn add_produced<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        self.produced.extend(bindings);
    }

    pub(crate) fn add_nullable<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        self.nullable.extend(bindings);
    }
}
