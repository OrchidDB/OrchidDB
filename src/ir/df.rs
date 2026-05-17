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
//!   so a HEP-rewritten plan can be handed straight back to the
//!   interpreter.
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
    CoalesceArmOutput, CoalesceSuccess, ConstructTriple, Direction, DistinctBulk, DistinctMode,
    EmitMode, GraphPlan, GroupValue, JoinKind, LabelExpr, Length, MinusCompatibility, Node,
    PathFilterScope, PathMaterialization, PathObjects, PathPart, PathSelector, PathUpdate,
    ProcedureArg, ProcedureMode, ProjectErrorPolicy, ProjectMode, ProjectionItem, QuantifierKind,
    RdfGraphScope, RdfPathExpr, RdfTerm, Slice, SortKey, TargetMode, UnionAlign, ZeroLengthPolicy,
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

// ============================================================
// Macro: per-operator boilerplate
// ============================================================

macro_rules! ir_extension {
    (
        $(#[$meta:meta])*
        $name:ident { $($field:ident : $ty:ty),* $(,)? }
        rebuild($rs:ident, $rc:ident) $rebuild:block,
    ) => {
        $(#[$meta])*
        #[derive(Clone)]
        pub struct $name {
            $(pub $field : $ty,)*
            pub schema: DFSchemaRef,
            pub inputs: Vec<LogicalPlan>,
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let mut debug = f.debug_struct(stringify!($name));
                $(
                    debug.field(stringify!($field), &self.$field);
                )*
                debug.finish()
            }
        }

        impl PartialEq for $name {
            fn eq(&self, other: &Self) -> bool {
                $(self.$field == other.$field &&)* self.inputs == other.inputs
            }
        }
        impl Eq for $name {}

        impl Hash for $name {
            fn hash<H: Hasher>(&self, state: &mut H) {
                stringify!($name).hash(state);
                self.inputs.len().hash(state);
            }
        }

        impl PartialOrd for $name {
            fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
                Some(self.cmp(other))
            }
        }

        impl Ord for $name {
            fn cmp(&self, _other: &Self) -> Ordering {
                Ordering::Equal
            }
        }

        impl UserDefinedLogicalNodeCore for $name {
            fn name(&self) -> &str {
                stringify!($name)
            }

            fn inputs(&self) -> Vec<&LogicalPlan> {
                self.inputs.iter().collect()
            }

            fn schema(&self) -> &DFSchemaRef {
                &self.schema
            }

            fn expressions(&self) -> Vec<Expr> {
                Vec::new()
            }

            fn fmt_for_explain(&self, f: &mut fmt::Formatter) -> fmt::Result {
                fmt::Debug::fmt(self, f)
            }

            fn with_exprs_and_inputs(
                &self,
                _exprs: Vec<Expr>,
                inputs: Vec<LogicalPlan>,
            ) -> DFResult<Self> {
                Ok(Self { inputs, ..self.clone() })
            }
        }

        impl GraphIrExtension for $name {
            fn rebuild(&self, $rc: Vec<Node>) -> Node {
                let $rs = self;
                $rebuild
            }
        }
    };
}

// ============================================================
// Per-operator extension types
// ============================================================

ir_extension! {
    /// `GraphReturn(fields, resultForm)` — the result-shape boundary.
    GraphReturn {
        fields: Vec<String>,
        result_form: ResultForm,
        plan_policy: Option<GraphPlanPolicy>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphReturn {
            fields: s.fields.clone(),
            result_form: s.result_form,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphNodeScan {
        graph: String,
        binding: String,
        labels: LabelExpr,
    }
    rebuild(s, _c) {
        let _ = _c;
        Node::GraphNodeScan {
            graph: s.graph.clone(),
            binding: s.binding.clone(),
            labels: s.labels.clone(),
        }
    },
}

ir_extension! {
    GraphRelScan {
        graph: String,
        binding: String,
        types: LabelExpr,
        dir: Direction,
    }
    rebuild(s, _c) {
        let _ = _c;
        Node::GraphRelScan {
            graph: s.graph.clone(),
            binding: s.binding.clone(),
            types: s.types.clone(),
            dir: s.dir,
        }
    },
}

ir_extension! {
    GraphValues {
        bindings: Vec<String>,
        rows: Vec<Vec<Value>>,
        bulk: Option<Vec<u64>>,
    }
    rebuild(s, _c) {
        let _ = _c;
        Node::GraphValues {
            bindings: s.bindings.clone(),
            rows: s.rows.clone(),
            bulk: s.bulk.clone(),
        }
    },
}

ir_extension! {
    GraphOneRow {}
    rebuild(_s, _c) {
        let _ = (_s, _c);
        Node::GraphOneRow
    },
}

ir_extension! {
    GraphEmpty {}
    rebuild(_s, _c) {
        let _ = (_s, _c);
        Node::GraphEmpty
    },
}

ir_extension! {
    GraphCorrelate {
        bindings: Vec<String>,
    }
    rebuild(s, _c) {
        let _ = _c;
        Node::GraphCorrelate { bindings: s.bindings.clone() }
    },
}

ir_extension! {
    GraphBind {
        bind: String,
        kind: BindKind,
        expr: Option<IrExpr>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphBind {
            bind: s.bind.clone(),
            kind: s.kind,
            expr: s.expr.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphExpand {
        graph: String,
        source: String,
        target: String,
        target_mode: TargetMode,
        target_labels: LabelExpr,
        rel_binding: Option<String>,
        rel_types: LabelExpr,
        dir: Direction,
        length: Length,
        history: Option<String>,
        path: Option<String>,
        path_mode: PathMode,
        match_mode: MatchMode,
        path_materialization: PathMaterialization,
        path_update: PathUpdate,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphExpand {
            graph: s.graph.clone(),
            source: s.source.clone(),
            target: s.target.clone(),
            target_mode: s.target_mode,
            target_labels: s.target_labels.clone(),
            rel_binding: s.rel_binding.clone(),
            rel_types: s.rel_types.clone(),
            dir: s.dir,
            length: s.length.clone(),
            history: s.history.clone(),
            path: s.path.clone(),
            path_mode: s.path_mode,
            match_mode: s.match_mode,
            path_materialization: s.path_materialization,
            path_update: s.path_update,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphPathPattern {
        graph: String,
        path: String,
        selector: PathSelector,
        path_mode: PathMode,
        match_mode: MatchMode,
        endpoints: Vec<String>,
        parts: Vec<PathPart>,
        path_materialization: PathMaterialization,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphPathPattern {
            graph: s.graph.clone(),
            path: s.path.clone(),
            selector: s.selector.clone(),
            path_mode: s.path_mode,
            match_mode: s.match_mode,
            endpoints: s.endpoints.clone(),
            parts: s.parts.clone(),
            path_materialization: s.path_materialization,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    /// Children are `[seed, body]`.
    GraphRepeat {
        loop_name: Option<String>,
        times: Option<u32>,
        emit: EmitMode,
        until: Option<IrExpr>,
        until_traversal: Option<Box<Node>>,
        path: Option<String>,
        path_objects: PathObjects,
        prefix_predicate: Option<IrExpr>,
        prefix_traversal: Option<Box<Node>>,
    }
    rebuild(s, c) {
        let mut c = c;
        let body = c.pop().unwrap();
        let seed = c.pop().unwrap();
        Node::GraphRepeat {
            loop_name: s.loop_name.clone(),
            times: s.times,
            emit: s.emit.clone(),
            until: s.until.clone(),
            until_traversal: s.until_traversal.clone(),
            path: s.path.clone(),
            path_objects: s.path_objects,
            prefix_predicate: s.prefix_predicate.clone(),
            prefix_traversal: s.prefix_traversal.clone(),
            seed: Box::new(seed),
            body: Box::new(body),
        }
    },
}

ir_extension! {
    GraphPathFilter {
        condition: IrExpr,
        scope: PathFilterScope,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphPathFilter {
            condition: s.condition.clone(),
            scope: s.scope,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphFilter { condition: IrExpr }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphFilter {
            condition: s.condition.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphProject {
        mode: ProjectMode,
        items: Vec<ProjectionItem>,
        error_policy: ProjectErrorPolicy,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphProject {
            mode: s.mode,
            items: s.items.clone(),
            error_policy: s.error_policy,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphCurrentProject {
        expr: IrExpr,
        fields: Vec<String>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphCurrentProject {
            expr: s.expr.clone(),
            fields: s.fields.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphAggregate {
        group: Vec<ProjectionItem>,
        aggs: Vec<AggCall>,
        fields: Vec<String>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphAggregate {
            group: s.group.clone(),
            aggs: s.aggs.clone(),
            fields: s.fields.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphGroupMap {
        key: IrExpr,
        value: GroupValue,
        output: String,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphGroupMap {
            key: s.key.clone(),
            value: s.value.clone(),
            output: s.output.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphGroupCountSideEffect {
        label: String,
        key: IrExpr,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphGroupCountSideEffect {
            label: s.label.clone(),
            key: s.key.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphCap {
        labels: Vec<String>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphCap {
            labels: s.labels.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphShortestPath {
        source: String,
        target: Option<String>,
        direction: Direction,
        rel_types: LabelExpr,
        max_distance: Option<f64>,
        include_edges: bool,
        output: String,
        all_paths: bool,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphShortestPath {
            source: s.source.clone(),
            target: s.target.clone(),
            direction: s.direction,
            rel_types: s.rel_types.clone(),
            max_distance: s.max_distance,
            include_edges: s.include_edges,
            output: s.output.clone(),
            all_paths: s.all_paths,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphDistinct {
        keys: Vec<String>,
        mode: DistinctMode,
        bulk: DistinctBulk,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphDistinct {
            keys: s.keys.clone(),
            mode: s.mode,
            bulk: s.bulk,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphSort { keys: Vec<SortKey> }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphSort {
            keys: s.keys.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphSlice { slice: Slice }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphSlice {
            slice: s.slice.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphSliceExpr {
        offset: Option<IrExpr>,
        fetch: Option<IrExpr>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphSliceExpr {
            offset: s.offset.clone(),
            fetch: s.fetch.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphBarrier {
        partition: Vec<String>,
        order: Vec<SortKey>,
        slice: Slice,
        materialize: bool,
        bulk_policy: BarrierBulkPolicy,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphBarrier {
            partition: s.partition.clone(),
            order: s.order.clone(),
            slice: s.slice.clone(),
            materialize: s.materialize,
            bulk_policy: s.bulk_policy,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphJoin {
        kind: JoinKind,
        condition: Option<IrExpr>,
    }
    rebuild(s, c) {
        let mut c = c;
        let right = c.pop().unwrap();
        let left = c.pop().unwrap();
        Node::GraphJoin {
            kind: s.kind,
            condition: s.condition.clone(),
            left: Box::new(left),
            right: Box::new(right),
        }
    },
}

ir_extension! {
    GraphApply {
        kind: ApplyKind,
        correlation: Vec<String>,
        outputs: Vec<String>,
        optional_missing: OptionalMissing,
    }
    rebuild(s, c) {
        let mut c = c;
        let right = c.pop().unwrap();
        let left = c.pop().unwrap();
        Node::GraphApply {
            kind: s.kind,
            correlation: s.correlation.clone(),
            outputs: s.outputs.clone(),
            optional_missing: s.optional_missing,
            left: Box::new(left),
            right: Box::new(right),
        }
    },
}

ir_extension! {
    GraphUnion {
        all: bool,
        align: UnionAlign,
    }
    rebuild(s, c) {
        let mut c = c;
        let right = c.pop().unwrap();
        let left = c.pop().unwrap();
        Node::GraphUnion {
            all: s.all,
            align: s.align,
            left: Box::new(left),
            right: Box::new(right),
        }
    },
}

ir_extension! {
    GraphUnwind {
        input_expr: IrExpr,
        bind: String,
        outer: bool,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphUnwind {
            input_expr: s.input_expr.clone(),
            bind: s.bind.clone(),
            outer: s.outer,
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphQuantifier {
        kind: QuantifierKind,
        item_binding: String,
        input_expr: IrExpr,
        predicate: IrExpr,
        output: String,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphQuantifier {
            kind: s.kind,
            item_binding: s.item_binding.clone(),
            input_expr: s.input_expr.clone(),
            predicate: s.predicate.clone(),
            output: s.output.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphCollect {
        value: IrExpr,
        distinct: bool,
        order: Vec<SortKey>,
        alias: String,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphCollect {
            value: s.value.clone(),
            distinct: s.distinct,
            order: s.order.clone(),
            alias: s.alias.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    /// Children are `[input, arm0, arm1, ...]`.
    GraphCoalesce {
        success: CoalesceSuccess,
        output: String,
        correlation: Vec<String>,
        arm_outputs: Vec<CoalesceArmOutput>,
    }
    rebuild(s, c) {
        let mut c = c;
        let arms = c.split_off(1);
        let input = c.pop().unwrap();
        Node::GraphCoalesce {
            success: s.success,
            output: s.output.clone(),
            correlation: s.correlation.clone(),
            arm_outputs: s.arm_outputs.clone(),
            input: Box::new(input),
            arms,
        }
    },
}

ir_extension! {
    /// Children are `[input, arm0, arm1, ..., default?]`. The number of
    /// arm children equals `arm_keys.len()`; if `has_default` is true,
    /// a final default child follows the arms.
    GraphChoose {
        selector: ChooseSelector,
        output: String,
        correlation: Vec<String>,
        arm_keys: Vec<Option<Value>>,
        has_default: bool,
        unmatched: ChooseUnmatched,
    }
    rebuild(s, c) {
        let mut c = c;
        let mut iter = c.drain(..);
        let input = iter.next().expect("Choose: missing input child");
        let mut arm_bodies = Vec::with_capacity(s.arm_keys.len());
        for _ in 0..s.arm_keys.len() {
            arm_bodies.push(iter.next().expect("Choose: missing arm body"));
        }
        let default = if s.has_default {
            Some(Box::new(iter.next().expect("Choose: missing default")))
        } else {
            None
        };
        let arms = s
            .arm_keys
            .iter()
            .cloned()
            .zip(arm_bodies.into_iter())
            .map(|(key, body)| ChooseArm { key, body })
            .collect();
        Node::GraphChoose {
            selector: s.selector.clone(),
            output: s.output.clone(),
            correlation: s.correlation.clone(),
            arms,
            default,
            unmatched: s.unmatched,
            input: Box::new(input),
        }
    },
}

ir_extension! {
    GraphSelect {
        labels: Vec<String>,
        outputs: Vec<String>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphSelect {
            labels: s.labels.clone(),
            outputs: s.outputs.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    /// Children are `[input?]` (zero or one, mirroring
    /// `Node::GraphProcedureCall.input`).
    GraphProcedureCall {
        name: String,
        args: Vec<ProcedureArg>,
        yields: Vec<String>,
        mode: ProcedureMode,
        has_input: bool,
    }
    rebuild(s, c) {
        let mut c = c;
        let input = if s.has_input {
            Some(Box::new(c.remove(0)))
        } else {
            None
        };
        Node::GraphProcedureCall {
            name: s.name.clone(),
            args: s.args.clone(),
            yields: s.yields.clone(),
            mode: s.mode,
            input,
        }
    },
}

ir_extension! {
    GraphExtension {
        op_name: String,
        metadata: Vec<(String, Value)>,
    }
    rebuild(s, c) {
        Node::GraphExtension {
            name: s.op_name.clone(),
            metadata: s.metadata.clone(),
            inputs: c,
        }
    },
}

// ============================================================
// SPARQL / RDF extension nodes (spec §5.9, §5.10, §8.x)
// ============================================================

ir_extension! {
    /// `GraphRdfQuadScan(...)` — leaf source. No children.
    GraphRdfQuadScan {
        dataset: String,
        graph_scope: RdfGraphScope,
        subject: RdfTerm,
        predicate: RdfTerm,
        object: RdfTerm,
        outputs: Vec<String>,
    }
    rebuild(s, _c) {
        let _ = _c;
        Node::GraphRdfQuadScan {
            dataset: s.dataset.clone(),
            graph_scope: s.graph_scope.clone(),
            subject: s.subject.clone(),
            predicate: s.predicate.clone(),
            object: s.object.clone(),
            outputs: s.outputs.clone(),
        }
    },
}

ir_extension! {
    /// `GraphRdfPropertyPath(...)` — leaf source.
    GraphRdfPropertyPath {
        dataset: String,
        graph_scope: RdfGraphScope,
        subject: RdfTerm,
        object: RdfTerm,
        path: RdfPathExpr,
        path_materialization: PathMaterialization,
        zero_length: ZeroLengthPolicy,
    }
    rebuild(s, _c) {
        let _ = _c;
        Node::GraphRdfPropertyPath {
            dataset: s.dataset.clone(),
            graph_scope: s.graph_scope.clone(),
            subject: s.subject.clone(),
            object: s.object.clone(),
            path: s.path.clone(),
            path_materialization: s.path_materialization,
            zero_length: s.zero_length,
        }
    },
}

ir_extension! {
    /// Children are `[left, right]`.
    GraphSparqlMinus {
        compatible: MinusCompatibility,
        shared: Vec<String>,
    }
    rebuild(s, c) {
        let mut c = c;
        let right = c.pop().unwrap();
        let left = c.pop().unwrap();
        Node::GraphSparqlMinus {
            compatible: s.compatible,
            shared: s.shared.clone(),
            left: Box::new(left),
            right: Box::new(right),
        }
    },
}

ir_extension! {
    /// Children are `[input]` — the inner pattern that runs against the
    /// remote endpoint.
    GraphService {
        endpoint: RdfTerm,
        silent: bool,
        outputs: Vec<String>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphService {
            endpoint: s.endpoint.clone(),
            silent: s.silent,
            outputs: s.outputs.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    /// SPARQL `CONSTRUCT` output. Single child.
    GraphConstructTriples {
        template: Vec<ConstructTriple>,
        plan_policy: Option<GraphPlanPolicy>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphConstructTriples {
            template: s.template.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    /// SPARQL `DESCRIBE` output. Single child.
    GraphDescribe {
        terms: Vec<RdfTerm>,
        plan_policy: Option<GraphPlanPolicy>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphDescribe {
            terms: s.terms.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    /// SPARQL `ASK` output. Single child.
    GraphAsk {
        field: String,
        plan_policy: Option<GraphPlanPolicy>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphAsk {
            field: s.field.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    /// Cypher list-comprehension as a planning boundary. Single child.
    GraphListComprehension {
        input_expr: IrExpr,
        item: String,
        filter: Option<IrExpr>,
        map_expr: Option<IrExpr>,
        alias: String,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphListComprehension {
            input_expr: s.input_expr.clone(),
            item: s.item.clone(),
            filter: s.filter.clone(),
            map_expr: s.map_expr.clone(),
            alias: s.alias.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

// ============================================================
// IR Node → LogicalPlan
// ============================================================

/// Convert a `GraphPlan` into a DataFusion `LogicalPlan` of nested
/// `Extension` nodes. Each Graph IR operator becomes its own concrete
/// `UserDefinedLogicalNodeCore` so HEP-style rule sets can downcast and
/// rewrite by operator kind.
pub fn to_logical_plan(plan: &GraphPlan) -> DFResult<LogicalPlan> {
    node_to_plan_with_policy(&plan.root, Some(plan.policy.clone()))
}

fn node_to_plan(node: &Node) -> DFResult<LogicalPlan> {
    node_to_plan_with_policy(node, None)
}

fn node_to_plan_with_policy(
    node: &Node,
    plan_policy: Option<GraphPlanPolicy>,
) -> DFResult<LogicalPlan> {
    let schema = build_schema_for_node(node)?;
    let plan: LogicalPlan = match node {
        Node::GraphReturn {
            fields,
            result_form,
            input,
        } => extension(GraphReturn {
            fields: fields.clone(),
            result_form: *result_form,
            plan_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphNodeScan {
            graph,
            binding,
            labels,
        } => extension(GraphNodeScan {
            graph: graph.clone(),
            binding: binding.clone(),
            labels: labels.clone(),
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphRelScan {
            graph,
            binding,
            types,
            dir,
        } => extension(GraphRelScan {
            graph: graph.clone(),
            binding: binding.clone(),
            types: types.clone(),
            dir: *dir,
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphValues {
            bindings,
            rows,
            bulk,
        } => extension(GraphValues {
            bindings: bindings.clone(),
            rows: rows.clone(),
            bulk: bulk.clone(),
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphOneRow => extension(GraphOneRow {
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphEmpty => extension(GraphEmpty {
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphCorrelate { bindings } => extension(GraphCorrelate {
            bindings: bindings.clone(),
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphBind {
            bind,
            kind,
            expr,
            input,
        } => extension(GraphBind {
            bind: bind.clone(),
            kind: *kind,
            expr: expr.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphExpand {
            graph,
            source,
            target,
            target_mode,
            target_labels,
            rel_binding,
            rel_types,
            dir,
            length,
            history,
            path,
            path_mode,
            match_mode,
            path_materialization,
            path_update,
            input,
        } => extension(GraphExpand {
            graph: graph.clone(),
            source: source.clone(),
            target: target.clone(),
            target_mode: *target_mode,
            target_labels: target_labels.clone(),
            rel_binding: rel_binding.clone(),
            rel_types: rel_types.clone(),
            dir: *dir,
            length: length.clone(),
            history: history.clone(),
            path: path.clone(),
            path_mode: *path_mode,
            match_mode: *match_mode,
            path_materialization: *path_materialization,
            path_update: *path_update,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphPathPattern {
            graph,
            path,
            selector,
            path_mode,
            match_mode,
            endpoints,
            parts,
            path_materialization,
            input,
        } => extension(GraphPathPattern {
            graph: graph.clone(),
            path: path.clone(),
            selector: selector.clone(),
            path_mode: *path_mode,
            match_mode: *match_mode,
            endpoints: endpoints.clone(),
            parts: parts.clone(),
            path_materialization: *path_materialization,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphRepeat {
            loop_name,
            times,
            emit,
            until,
            until_traversal,
            path,
            path_objects,
            prefix_predicate,
            prefix_traversal,
            seed,
            body,
        } => extension(GraphRepeat {
            loop_name: loop_name.clone(),
            times: *times,
            emit: emit.clone(),
            until: until.clone(),
            until_traversal: until_traversal.clone(),
            path: path.clone(),
            path_objects: *path_objects,
            prefix_predicate: prefix_predicate.clone(),
            prefix_traversal: prefix_traversal.clone(),
            schema,
            inputs: vec![node_to_plan(seed)?, node_to_plan(body)?],
        }),
        Node::GraphPathFilter {
            condition,
            scope,
            input,
        } => extension(GraphPathFilter {
            condition: condition.clone(),
            scope: *scope,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphFilter { condition, input } => extension(GraphFilter {
            condition: condition.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphProject {
            mode,
            items,
            error_policy,
            input,
        } => extension(GraphProject {
            mode: *mode,
            items: items.clone(),
            error_policy: *error_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphCurrentProject {
            expr,
            fields,
            input,
        } => extension(GraphCurrentProject {
            expr: expr.clone(),
            fields: fields.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphAggregate {
            group,
            aggs,
            fields,
            input,
        } => extension(GraphAggregate {
            group: group.clone(),
            aggs: aggs.clone(),
            fields: fields.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphGroupMap {
            key,
            value,
            output,
            input,
        } => extension(GraphGroupMap {
            key: key.clone(),
            value: value.clone(),
            output: output.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphGroupCountSideEffect { label, key, input } => {
            extension(GraphGroupCountSideEffect {
                label: label.clone(),
                key: key.clone(),
                schema,
                inputs: vec![node_to_plan(input)?],
            })
        }
        Node::GraphCap { labels, input } => extension(GraphCap {
            labels: labels.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphShortestPath {
            source,
            target,
            direction,
            rel_types,
            max_distance,
            include_edges,
            output,
            all_paths,
            input,
        } => extension(GraphShortestPath {
            source: source.clone(),
            target: target.clone(),
            direction: *direction,
            rel_types: rel_types.clone(),
            max_distance: *max_distance,
            include_edges: *include_edges,
            output: output.clone(),
            all_paths: *all_paths,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphDistinct {
            keys,
            mode,
            bulk,
            input,
        } => extension(GraphDistinct {
            keys: keys.clone(),
            mode: *mode,
            bulk: *bulk,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphSort { keys, input } => extension(GraphSort {
            keys: keys.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphSlice { slice, input } => extension(GraphSlice {
            slice: slice.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphSliceExpr {
            offset,
            fetch,
            input,
        } => extension(GraphSliceExpr {
            offset: offset.clone(),
            fetch: fetch.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphBarrier {
            partition,
            order,
            slice,
            materialize,
            bulk_policy,
            input,
        } => extension(GraphBarrier {
            partition: partition.clone(),
            order: order.clone(),
            slice: slice.clone(),
            materialize: *materialize,
            bulk_policy: *bulk_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphJoin {
            kind,
            left,
            right,
            condition,
        } => extension(GraphJoin {
            kind: *kind,
            condition: condition.clone(),
            schema,
            inputs: vec![node_to_plan(left)?, node_to_plan(right)?],
        }),
        Node::GraphApply {
            kind,
            correlation,
            outputs,
            optional_missing,
            left,
            right,
        } => extension(GraphApply {
            kind: *kind,
            correlation: correlation.clone(),
            outputs: outputs.clone(),
            optional_missing: *optional_missing,
            schema,
            inputs: vec![node_to_plan(left)?, node_to_plan(right)?],
        }),
        Node::GraphUnion {
            all,
            align,
            left,
            right,
        } => extension(GraphUnion {
            all: *all,
            align: *align,
            schema,
            inputs: vec![node_to_plan(left)?, node_to_plan(right)?],
        }),
        Node::GraphUnwind {
            input_expr,
            bind,
            outer,
            input,
        } => extension(GraphUnwind {
            input_expr: input_expr.clone(),
            bind: bind.clone(),
            outer: *outer,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphQuantifier {
            kind,
            item_binding,
            input_expr,
            predicate,
            output,
            input,
        } => extension(GraphQuantifier {
            kind: *kind,
            item_binding: item_binding.clone(),
            input_expr: input_expr.clone(),
            predicate: predicate.clone(),
            output: output.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphCollect {
            value,
            distinct,
            order,
            alias,
            input,
        } => extension(GraphCollect {
            value: value.clone(),
            distinct: *distinct,
            order: order.clone(),
            alias: alias.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphCoalesce {
            success,
            output,
            correlation,
            arm_outputs,
            input,
            arms,
        } => {
            let mut children = vec![node_to_plan(input)?];
            for arm in arms {
                children.push(node_to_plan(arm)?);
            }
            extension(GraphCoalesce {
                success: *success,
                output: output.clone(),
                correlation: correlation.clone(),
                arm_outputs: arm_outputs.clone(),
                schema,
                inputs: children,
            })
        }
        Node::GraphChoose {
            selector,
            output,
            correlation,
            arms,
            default,
            unmatched,
            input,
        } => {
            let mut children = vec![node_to_plan(input)?];
            for arm in arms {
                children.push(node_to_plan(&arm.body)?);
            }
            if let Some(default) = default {
                children.push(node_to_plan(default)?);
            }
            extension(GraphChoose {
                selector: selector.clone(),
                output: output.clone(),
                correlation: correlation.clone(),
                arm_keys: arms.iter().map(|a| a.key.clone()).collect(),
                has_default: default.is_some(),
                unmatched: *unmatched,
                schema,
                inputs: children,
            })
        }
        Node::GraphSelect {
            labels,
            outputs,
            input,
        } => extension(GraphSelect {
            labels: labels.clone(),
            outputs: outputs.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphProcedureCall {
            name,
            args,
            yields,
            mode,
            input,
        } => {
            let inputs = match input {
                Some(input) => vec![node_to_plan(input)?],
                None => Vec::new(),
            };
            extension(GraphProcedureCall {
                name: name.clone(),
                args: args.clone(),
                yields: yields.clone(),
                mode: *mode,
                has_input: input.is_some(),
                schema,
                inputs,
            })
        }
        Node::GraphExtension {
            name,
            metadata,
            inputs,
        } => {
            let plan_inputs = inputs
                .iter()
                .map(node_to_plan)
                .collect::<DFResult<Vec<_>>>()?;
            extension(GraphExtension {
                op_name: name.clone(),
                metadata: metadata.clone(),
                schema,
                inputs: plan_inputs,
            })
        }

        // -------- SPARQL / RDF --------
        Node::GraphRdfQuadScan {
            dataset,
            graph_scope,
            subject,
            predicate,
            object,
            outputs,
        } => extension(GraphRdfQuadScan {
            dataset: dataset.clone(),
            graph_scope: graph_scope.clone(),
            subject: subject.clone(),
            predicate: predicate.clone(),
            object: object.clone(),
            outputs: outputs.clone(),
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphRdfPropertyPath {
            dataset,
            graph_scope,
            subject,
            object,
            path,
            path_materialization,
            zero_length,
        } => extension(GraphRdfPropertyPath {
            dataset: dataset.clone(),
            graph_scope: graph_scope.clone(),
            subject: subject.clone(),
            object: object.clone(),
            path: path.clone(),
            path_materialization: *path_materialization,
            zero_length: *zero_length,
            schema,
            inputs: Vec::new(),
        }),
        Node::GraphSparqlMinus {
            compatible,
            shared,
            left,
            right,
        } => extension(GraphSparqlMinus {
            compatible: *compatible,
            shared: shared.clone(),
            schema,
            inputs: vec![node_to_plan(left)?, node_to_plan(right)?],
        }),
        Node::GraphService {
            endpoint,
            silent,
            outputs,
            input,
        } => extension(GraphService {
            endpoint: endpoint.clone(),
            silent: *silent,
            outputs: outputs.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphConstructTriples { template, input } => extension(GraphConstructTriples {
            template: template.clone(),
            plan_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphDescribe { terms, input } => extension(GraphDescribe {
            terms: terms.clone(),
            plan_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphAsk { field, input } => extension(GraphAsk {
            field: field.clone(),
            plan_policy,
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
        Node::GraphListComprehension {
            input_expr,
            item,
            filter,
            map_expr,
            alias,
            input,
        } => extension(GraphListComprehension {
            input_expr: input_expr.clone(),
            item: item.clone(),
            filter: filter.clone(),
            map_expr: map_expr.clone(),
            alias: alias.clone(),
            schema,
            inputs: vec![node_to_plan(input)?],
        }),
    };
    Ok(plan)
}

fn extension<T: UserDefinedLogicalNodeCore + 'static>(node: T) -> LogicalPlan {
    LogicalPlan::Extension(Extension {
        node: Arc::new(node) as Arc<dyn UserDefinedLogicalNode>,
    })
}

// ============================================================
// LogicalPlan → IR Node (round-trip)
// ============================================================

/// Reconstruct a `GraphPlan` from a DataFusion `LogicalPlan` previously
/// produced by [`to_logical_plan`] and possibly rewritten by HEP rules.
/// The plan policy is carried by the root output-boundary extension node.
pub fn from_logical_plan(plan: &LogicalPlan) -> DFResult<GraphPlan> {
    let policy = root_plan_policy(plan).ok_or_else(|| {
        DataFusionError::Plan("expected root GraphIR output node to carry GraphPlanPolicy".into())
    })?;
    let node = plan_to_node(plan)?;
    Ok(GraphPlan::new(policy, node))
}

/// Compatibility helper for callers that need to reconstruct an older
/// extension tree without an embedded root policy.
pub fn from_logical_plan_with_policy(
    fallback_policy: GraphPlanPolicy,
    plan: &LogicalPlan,
) -> DFResult<GraphPlan> {
    let policy = root_plan_policy(plan).unwrap_or(fallback_policy);
    let node = plan_to_node(plan)?;
    Ok(GraphPlan::new(policy, node))
}

fn root_plan_policy(plan: &LogicalPlan) -> Option<GraphPlanPolicy> {
    let LogicalPlan::Extension(ext) = plan else {
        return None;
    };
    let any: &dyn Any = ext.node.as_ref().as_any();
    if let Some(op) = any.downcast_ref::<GraphReturn>() {
        return op.plan_policy.clone();
    }
    if let Some(op) = any.downcast_ref::<GraphConstructTriples>() {
        return op.plan_policy.clone();
    }
    if let Some(op) = any.downcast_ref::<GraphDescribe>() {
        return op.plan_policy.clone();
    }
    if let Some(op) = any.downcast_ref::<GraphAsk>() {
        return op.plan_policy.clone();
    }
    None
}

fn plan_to_node(plan: &LogicalPlan) -> DFResult<Node> {
    let LogicalPlan::Extension(ext) = plan else {
        return Err(DataFusionError::Plan(format!(
            "expected GraphIR Extension node, got {plan:?}"
        )));
    };
    let any: &dyn Any = ext.node.as_ref().as_any();

    // Walk the children up front so each extension's `rebuild` can take
    // owned `Node` values in the right order.
    let mut children: Vec<Node> = Vec::with_capacity(ext.node.inputs().len());
    for child in ext.node.inputs() {
        children.push(plan_to_node(child)?);
    }

    macro_rules! try_op {
        ($($t:ty),* $(,)?) => {
            $(
                if let Some(op) = any.downcast_ref::<$t>() {
                    return Ok(op.rebuild(children));
                }
            )*
        };
    }

    try_op!(
        GraphReturn,
        GraphNodeScan,
        GraphRelScan,
        GraphValues,
        GraphOneRow,
        GraphEmpty,
        GraphCorrelate,
        GraphBind,
        GraphExpand,
        GraphPathPattern,
        GraphRepeat,
        GraphPathFilter,
        GraphFilter,
        GraphProject,
        GraphCurrentProject,
        GraphAggregate,
        GraphGroupMap,
        GraphShortestPath,
        GraphDistinct,
        GraphSort,
        GraphSlice,
        GraphSliceExpr,
        GraphBarrier,
        GraphJoin,
        GraphApply,
        GraphUnion,
        GraphUnwind,
        GraphQuantifier,
        GraphCollect,
        GraphCoalesce,
        GraphChoose,
        GraphSelect,
        GraphProcedureCall,
        GraphExtension,
        // SPARQL / RDF
        GraphRdfQuadScan,
        GraphRdfPropertyPath,
        GraphSparqlMinus,
        GraphService,
        GraphConstructTriples,
        GraphDescribe,
        GraphAsk,
        GraphListComprehension,
    );

    Err(DataFusionError::Plan(format!(
        "unrecognized graph extension node `{}`",
        ext.node.name()
    )))
}

// ============================================================
// Schema computation (per binding)
// ============================================================

fn build_schema_for_node(node: &Node) -> DFResult<DFSchemaRef> {
    build_schema_from_fields(schema_fields_for_node(node))
}

fn build_schema_from_fields(fields: Vec<Field>) -> DFResult<DFSchemaRef> {
    let schema = Schema::new(fields);
    let df_schema = DFSchema::try_from(schema).map_err(DataFusionError::from)?;
    Ok(Arc::new(df_schema))
}

fn binding_field(name: &str) -> Field {
    semantic_field(name, DataType::Utf8, true, "value")
}

fn semantic_field(name: &str, data_type: DataType, nullable: bool, value_type: &str) -> Field {
    Field::new(name, data_type, nullable).with_metadata(HashMap::from([(
        "graph_ir.value_type".to_string(),
        value_type.to_string(),
    )]))
}

fn hidden_field(name: &str, data_type: DataType, value_type: &str) -> Field {
    Field::new(name, data_type, false).with_metadata(HashMap::from([
        ("graph_ir.value_type".to_string(), value_type.to_string()),
        ("graph_ir.hidden".to_string(), "true".to_string()),
    ]))
}

fn field_named(fields: &[Field], name: &str) -> Option<Field> {
    fields
        .iter()
        .find(|field| field.name().as_str() == name)
        .cloned()
}

fn upsert_field(fields: &mut Vec<Field>, field: Field) {
    if let Some(existing) = fields
        .iter_mut()
        .find(|existing| existing.name().as_str() == field.name().as_str())
    {
        *existing = field;
    } else {
        fields.push(field);
    }
}

fn append_missing_fields(fields: &mut Vec<Field>, incoming: Vec<Field>) {
    for field in incoming {
        if !fields
            .iter()
            .any(|existing| existing.name().as_str() == field.name().as_str())
        {
            fields.push(field);
        }
    }
}

fn schema_fields_for_node(node: &Node) -> Vec<Field> {
    match node {
        Node::GraphReturn { fields, input, .. } => {
            let input_fields = schema_fields_for_node(input);
            fields
                .iter()
                .map(|name| field_named(&input_fields, name).unwrap_or_else(|| binding_field(name)))
                .collect()
        }
        Node::GraphNodeScan { binding, .. } => {
            vec![semantic_field(binding, DataType::Utf8, true, "node")]
        }
        Node::GraphRelScan { binding, .. } => {
            vec![semantic_field(binding, DataType::Utf8, true, "edge")]
        }
        Node::GraphValues {
            bindings,
            rows,
            bulk,
        } => {
            let mut fields = bindings
                .iter()
                .enumerate()
                .map(|(idx, binding)| {
                    let values = rows.iter().filter_map(|row| row.get(idx));
                    semantic_field(binding, infer_values_type(values), true, "value")
                })
                .collect::<Vec<_>>();
            if bulk.is_some() {
                fields.push(hidden_field("_bulk", DataType::UInt64, "traverser_bulk"));
            }
            fields
        }
        Node::GraphOneRow | Node::GraphEmpty => Vec::new(),
        Node::GraphCorrelate { bindings } => {
            bindings.iter().map(|name| binding_field(name)).collect()
        }
        Node::GraphBind {
            bind,
            kind,
            expr,
            input,
            ..
        } => {
            let mut fields = schema_fields_for_node(input);
            let field = match expr {
                Some(expr) => semantic_field(bind, infer_expr_type(expr), true, "value"),
                None => {
                    let value_type = match kind {
                        BindKind::Node => "node",
                        BindKind::Edge => "edge",
                        BindKind::Scalar => "value",
                    };
                    semantic_field(bind, DataType::Utf8, true, value_type)
                }
            };
            upsert_field(&mut fields, field);
            fields
        }
        Node::GraphExpand {
            target,
            rel_binding,
            history,
            path,
            input,
            ..
        } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(
                &mut fields,
                semantic_field(target, DataType::Utf8, true, "node"),
            );
            if let Some(binding) = rel_binding {
                upsert_field(
                    &mut fields,
                    semantic_field(binding, DataType::Utf8, true, "edge"),
                );
            }
            if let Some(binding) = path {
                upsert_field(
                    &mut fields,
                    semantic_field(binding, DataType::Utf8, true, "path"),
                );
            }
            if let Some(binding) = history {
                upsert_field(
                    &mut fields,
                    semantic_field(binding, DataType::Utf8, true, "path"),
                );
            }
            fields
        }
        Node::GraphPathPattern {
            path,
            endpoints,
            parts,
            input,
            ..
        } => {
            let mut fields = schema_fields_for_node(input);
            for endpoint in endpoints {
                upsert_field(
                    &mut fields,
                    semantic_field(endpoint, DataType::Utf8, true, "node"),
                );
            }
            for part in parts {
                if let PathPart::Rel {
                    bind: Some(binding),
                    ..
                } = part
                {
                    upsert_field(
                        &mut fields,
                        semantic_field(binding, DataType::Utf8, true, "edge"),
                    );
                }
            }
            upsert_field(
                &mut fields,
                semantic_field(path, DataType::Utf8, true, "path"),
            );
            fields
        }
        Node::GraphFilter { input, .. }
        | Node::GraphSort { input, .. }
        | Node::GraphSlice { input, .. }
        | Node::GraphSliceExpr { input, .. }
        | Node::GraphBarrier { input, .. }
        | Node::GraphPathFilter { input, .. } => schema_fields_for_node(input),
        Node::GraphProject {
            mode, items, input, ..
        } => match mode {
            ProjectMode::ReplaceScope => items
                .iter()
                .map(|item| semantic_field(&item.alias, infer_expr_type(&item.expr), true, "value"))
                .collect(),
            ProjectMode::ReplaceCurrent => {
                let mut fields = schema_fields_for_node(input);
                for item in items {
                    fields.retain(|field| field.name().as_str() != item.alias.as_str());
                    fields.push(semantic_field(
                        &item.alias,
                        infer_expr_type(&item.expr),
                        true,
                        "value",
                    ));
                }
                fields
            }
            ProjectMode::PreserveVisible => {
                let mut fields = schema_fields_for_node(input);
                for item in items {
                    upsert_field(
                        &mut fields,
                        semantic_field(&item.alias, infer_expr_type(&item.expr), true, "value"),
                    );
                }
                fields
            }
        },
        Node::GraphCurrentProject {
            expr,
            fields,
            input,
        } => {
            let mut out = schema_fields_for_node(input);
            for name in fields {
                out.retain(|field| field.name().as_str() != name.as_str());
                out.push(semantic_field(name, infer_expr_type(expr), true, "value"));
            }
            out
        }
        Node::GraphAggregate {
            group,
            aggs,
            fields,
            ..
        } => {
            let mut out: Vec<Field> = group
                .iter()
                .map(|item| semantic_field(&item.alias, infer_expr_type(&item.expr), true, "value"))
                .collect();
            for agg in aggs {
                upsert_field(
                    &mut out,
                    semantic_field(&agg.alias, infer_agg_type(&agg.kind), false, "aggregate"),
                );
            }
            if fields.is_empty() {
                out
            } else {
                fields
                    .iter()
                    .map(|name| field_named(&out, name).unwrap_or_else(|| binding_field(name)))
                    .collect()
            }
        }
        Node::GraphGroupMap { output, .. } => {
            vec![semantic_field(output, DataType::Utf8, true, "map")]
        }
        Node::GraphGroupCountSideEffect { input, .. } => schema_fields_for_node(input),
        Node::GraphCap { labels, .. } if labels.len() == 1 => {
            vec![semantic_field("current", DataType::Utf8, true, "map")]
        }
        Node::GraphCap { .. } => {
            vec![semantic_field("current", DataType::Utf8, true, "map")]
        }
        Node::GraphShortestPath { output, .. } => {
            vec![semantic_field(output, DataType::Utf8, true, "path")]
        }
        Node::GraphDistinct { input, .. } => schema_fields_for_node(input),
        Node::GraphJoin { left, right, .. } | Node::GraphUnion { left, right, .. } => {
            let mut fields = schema_fields_for_node(left);
            append_missing_fields(&mut fields, schema_fields_for_node(right));
            fields
        }
        Node::GraphApply {
            outputs,
            left,
            right,
            ..
        } => {
            let mut fields = schema_fields_for_node(left);
            let right_fields = schema_fields_for_node(right);
            for output in outputs {
                upsert_field(
                    &mut fields,
                    field_named(&right_fields, output).unwrap_or_else(|| binding_field(output)),
                );
            }
            fields
        }
        Node::GraphUnwind { bind, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(&mut fields, binding_field(bind));
            fields
        }
        Node::GraphQuantifier { output, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(
                &mut fields,
                semantic_field(output, DataType::Boolean, false, "bool"),
            );
            fields
        }
        Node::GraphCollect { alias, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(
                &mut fields,
                semantic_field(alias, DataType::Utf8, true, "list"),
            );
            fields
        }
        Node::GraphCoalesce { output, input, .. } | Node::GraphChoose { output, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(&mut fields, binding_field(output));
            fields
        }
        Node::GraphSelect {
            labels,
            outputs,
            input,
        } => {
            let input_fields = schema_fields_for_node(input);
            outputs
                .iter()
                .zip(labels.iter())
                .map(|(output, label)| {
                    field_named(&input_fields, label)
                        .map(|field| {
                            semantic_field(
                                output,
                                field.data_type().clone(),
                                field.is_nullable(),
                                "value",
                            )
                        })
                        .unwrap_or_else(|| binding_field(output))
                })
                .collect()
        }
        Node::GraphRepeat { seed, .. } => schema_fields_for_node(seed),
        Node::GraphProcedureCall { yields, input, .. } => {
            let mut fields = input
                .as_deref()
                .map(schema_fields_for_node)
                .unwrap_or_default();
            for yield_name in yields {
                upsert_field(&mut fields, binding_field(yield_name));
            }
            fields
        }
        Node::GraphExtension { inputs, .. } => {
            let mut fields = Vec::new();
            for input in inputs {
                append_missing_fields(&mut fields, schema_fields_for_node(input));
            }
            fields
        }
        Node::GraphRdfQuadScan { outputs, .. } => outputs
            .iter()
            .map(|name| semantic_field(name, DataType::Utf8, true, "rdf_term"))
            .collect(),
        Node::GraphRdfPropertyPath {
            subject, object, ..
        } => {
            let mut fields = Vec::new();
            if let RdfTerm::Variable(name) = subject {
                upsert_field(
                    &mut fields,
                    semantic_field(name, DataType::Utf8, true, "rdf_term"),
                );
            }
            if let RdfTerm::Variable(name) = object {
                upsert_field(
                    &mut fields,
                    semantic_field(name, DataType::Utf8, true, "rdf_term"),
                );
            }
            fields
        }
        Node::GraphSparqlMinus { left, .. } => schema_fields_for_node(left),
        Node::GraphService { outputs, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            for output in outputs {
                upsert_field(
                    &mut fields,
                    semantic_field(output, DataType::Utf8, true, "rdf_term"),
                );
            }
            fields
        }
        Node::GraphConstructTriples { .. } | Node::GraphDescribe { .. } => {
            vec![semantic_field(
                "_rdf_graph",
                DataType::Utf8,
                false,
                "rdf_graph",
            )]
        }
        Node::GraphAsk { field, .. } => {
            vec![semantic_field(field, DataType::Boolean, false, "bool")]
        }
        Node::GraphListComprehension { alias, input, .. } => {
            let mut fields = schema_fields_for_node(input);
            upsert_field(
                &mut fields,
                semantic_field(alias, DataType::Utf8, true, "list"),
            );
            fields
        }
    }
}

fn infer_values_type<'a>(values: impl Iterator<Item = &'a Value>) -> DataType {
    let mut inferred: Option<DataType> = None;
    for value in values {
        if matches!(value, Value::Null) {
            continue;
        }
        let current = value_to_data_type(value);
        inferred = Some(match inferred {
            None => current,
            Some(existing) if existing == current => existing,
            Some(DataType::Int64) if current == DataType::Float64 => DataType::Float64,
            Some(DataType::Float64) if current == DataType::Int64 => DataType::Float64,
            Some(_) => DataType::Utf8,
        });
    }
    inferred.unwrap_or(DataType::Utf8)
}

fn value_to_data_type(value: &Value) -> DataType {
    match value {
        Value::Bool(_) => DataType::Boolean,
        Value::Int(_) => DataType::Int64,
        Value::Float(_) => DataType::Float64,
        _ => DataType::Utf8,
    }
}

fn infer_expr_type(expr: &IrExpr) -> DataType {
    match expr {
        IrExpr::Lit(lit) => match lit {
            crate::ir::expr::Lit::Null => DataType::Utf8,
            crate::ir::expr::Lit::Bool(_) => DataType::Boolean,
            crate::ir::expr::Lit::Int(_) => DataType::Int64,
            crate::ir::expr::Lit::Float(_) => DataType::Float64,
            crate::ir::expr::Lit::String(_) => DataType::Utf8,
        },
        IrExpr::Binary { op, .. } => match op {
            crate::ir::expr::BinaryOp::Eq
            | crate::ir::expr::BinaryOp::Neq
            | crate::ir::expr::BinaryOp::Lt
            | crate::ir::expr::BinaryOp::Lte
            | crate::ir::expr::BinaryOp::Gt
            | crate::ir::expr::BinaryOp::Gte
            | crate::ir::expr::BinaryOp::And
            | crate::ir::expr::BinaryOp::Or => DataType::Boolean,
            crate::ir::expr::BinaryOp::Add
            | crate::ir::expr::BinaryOp::Sub
            | crate::ir::expr::BinaryOp::Mul
            | crate::ir::expr::BinaryOp::Div => DataType::Float64,
        },
        IrExpr::Not(_)
        | IrExpr::StringPredicate { .. }
        | IrExpr::IsNull(_)
        | IrExpr::IsNotNull(_)
        | IrExpr::IsBound(_)
        | IrExpr::SimplePath(_)
        | IrExpr::HasLabel { .. } => DataType::Boolean,
        IrExpr::List(_)
        | IrExpr::ListReduce { .. }
        | IrExpr::ListTransform { .. }
        | IrExpr::ListFilter { .. }
        | IrExpr::Case { .. }
        | IrExpr::Call { .. } => DataType::Utf8,
        IrExpr::Binding(_) | IrExpr::Property { .. } | IrExpr::Id(_) | IrExpr::Label(_) => {
            DataType::Utf8
        }
    }
}

fn infer_agg_type(kind: &crate::ir::expr::AggKind) -> DataType {
    match kind {
        crate::ir::expr::AggKind::CountRows
        | crate::ir::expr::AggKind::CountBulk
        | crate::ir::expr::AggKind::CountDistinct => DataType::Int64,
        crate::ir::expr::AggKind::Avg
        | crate::ir::expr::AggKind::AvgOrZero
        | crate::ir::expr::AggKind::AvgOrNull
        | crate::ir::expr::AggKind::StDev
        | crate::ir::expr::AggKind::StDevP
        | crate::ir::expr::AggKind::PercentileCont
        | crate::ir::expr::AggKind::PercentileDisc => DataType::Float64,
        crate::ir::expr::AggKind::Sum
        | crate::ir::expr::AggKind::SumOrZero
        | crate::ir::expr::AggKind::Min
        | crate::ir::expr::AggKind::MinOrNull
        | crate::ir::expr::AggKind::Max
        | crate::ir::expr::AggKind::MaxOrNull
        | crate::ir::expr::AggKind::CollectRows
        | crate::ir::expr::AggKind::CollectTraversers => DataType::Utf8,
    }
}
