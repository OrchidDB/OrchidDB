//! Apache DataFusion adapter — one `UserDefinedLogicalNodeCore` per IR
//! operator.
//!
//! Each Graph IR operator (`GraphNodeScan`, `GraphFilter`, `GraphExpand`,
//! `GraphProject`, `GraphAggregate`, `GraphApply`, `GraphChoose`, …) has
//! its own concrete struct that implements
//! [`UserDefinedLogicalNodeCore`]. This is what HEP and other rule-based
//! optimizers need: rules pattern-match by `extension.node.as_any()
//! .downcast_ref::<GraphFilter>()` and rewrite the plan in place.
//!
//! Conversion is bidirectional:
//!
//! - [`to_logical_plan`] turns a `GraphPlan` into a tree of
//!   `LogicalPlan::Extension` nodes.
//! - [`from_logical_plan`] reconstructs a `GraphPlan` from such a tree —
//!   so a HEP-rewritten plan can be handed to relational lowering.
//!
//! Schemas are computed locally per operator from the IR's binding model;
//! every binding becomes a nullable Utf8 field. The IR doesn't carry
//! per-binding type information, so the schema is descriptive only — but
//! it's well-formed enough that `LogicalPlan::display_indent` works and
//! that DataFusion's analyzer / invariants pass don't reject it.

use std::any::Any;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use arrow::datatypes::{DataType, Field, Schema};
use datafusion::common::{DFSchema, DFSchemaRef};
use datafusion::error::{DataFusionError, Result as DFResult};
use datafusion::logical_expr::{
    Expr, Extension, LogicalPlan, UserDefinedLogicalNode, UserDefinedLogicalNodeCore,
};

use crate::ir::expr::{AggCall, IrExpr};
use crate::ir::plan::{
    ApplyKind, BarrierBulkPolicy, BindKind, ChooseArm, ChooseSelector, ChooseUnmatched,
    CoalesceArmOutput, CoalesceSuccess, ConstructTriple, CreateEdge, CreateNode, Direction,
    DistinctBulk, DistinctMode, EmitMode, GraphPlan, GroupValue, JoinKind, LabelExpr, Length,
    MinusCompatibility, Node, PathFilterScope, PathMaterialization, PathObjects, PathPart,
    PathSelector, PathUpdate, ProcedureArg, ProcedureMode, ProjectErrorPolicy, ProjectMode,
    ProjectionItem, QuantifierKind, RdfGraphScope, RdfPathExpr, RdfTerm, SetPropertyItem, Slice,
    SortKey, TargetMode, UnionAlign, ZeroLengthPolicy,
};
use crate::ir::policy::{GraphPlanPolicy, MatchMode, OptionalMissing, PathMode, ResultForm};
use crate::ir::value::Value;

// ============================================================
// Trait for downcastable Graph IR extension nodes
// ============================================================

/// All IR-side extension nodes share this trait so a HEP rule can recover
/// the IR `Node` from a `LogicalPlan::Extension` without caring which
/// concrete struct it came from.
pub trait GraphIrExtension: UserDefinedLogicalNodeCore {
    /// Materialize this extension back into an IR `Node` using the given
    /// children (already converted from `LogicalPlan` to `Node`). The
    /// children must match the order this extension produced via
    /// `inputs()`.
    fn rebuild(&self, children: Vec<Node>) -> Node;
}

/// Try to downcast a `LogicalPlan::Extension` to a specific
/// `GraphIrExtension` type. Convenience for rule code that wants to
/// pattern-match on operator kind.
pub fn downcast_graph_ir<'a, T: GraphIrExtension>(plan: &'a LogicalPlan) -> Option<&'a T> {
    if let LogicalPlan::Extension(ext) = plan {
        let any: &dyn Any = ext.node.as_ref().as_any();
        any.downcast_ref::<T>()
    } else {
        None
    }
}

mod extensions;
mod conversion;
mod schema;

pub use extensions::*;
pub use conversion::{from_logical_plan, from_logical_plan_with_policy, to_logical_plan};
use schema::build_schema_for_node;
