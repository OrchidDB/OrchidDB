//! Concrete Graph IR extension types and their shared boilerplate.

use super::*;

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
    GraphCreate {
        graph: String,
        nodes: Vec<CreateNode>,
        edges: Vec<CreateEdge>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphCreate {
            graph: s.graph.clone(),
            nodes: s.nodes.clone(),
            edges: s.edges.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphSetProperty {
        items: Vec<SetPropertyItem>,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphSetProperty {
            items: s.items.clone(),
            input: Box::new(c.remove(0)),
        }
    },
}

ir_extension! {
    GraphDelete {
        targets: Vec<IrExpr>,
        detach: bool,
    }
    rebuild(s, c) {
        let mut c = c;
        Node::GraphDelete {
            targets: s.targets.clone(),
            detach: s.detach,
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
    /// `GraphSparqlTriplePattern(...)` — unresolved logical leaf. No children.
    GraphSparqlTriplePattern {
        dataset: String,
        graph_scope: RdfGraphScope,
        subject: RdfTerm,
        predicate: RdfTerm,
        object: RdfTerm,
        outputs: Vec<String>,
    }
    rebuild(s, _c) {
        let _ = _c;
        Node::GraphSparqlTriplePattern {
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
