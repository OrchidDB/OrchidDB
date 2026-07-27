//! Logical Graph IR plan tree.
//!
//! See `docs/graph_ir_language_examples_v0_2_draft.md` §11 for the operator
//! catalog. Variants are named exactly as the spec prints them so a doc
//! line like `GraphRepeat(...)` maps to `Node::GraphRepeat { ... }` and to
//! the `crate::ir::df::GraphRepeat` extension struct without translation.
//!
//! Every operator family from §11 is present so that plans for every
//! supported language (Cypher, GQL, Gremlin, SPARQL) can be represented
//! faithfully — including operators the runtime does not yet execute.
//! Mutation-shaped Cypher/GQL operators are logical Graph IR nodes as
//! well; physical execution backends can decide whether to use an
//! in-memory overlay, SQL/DuckDB statements, or another store.

use crate::ir::expr::{AggCall, BindingId, IrExpr, Lit};
use crate::ir::policy::{GraphPlanPolicy, MatchMode, OptionalMissing, PathMode, ResultForm};
use crate::ir::value::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Out,
    In,
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindKind {
    Node,
    Edge,
    Scalar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetMode {
    /// Bind a fresh target.
    BindNew,
    /// Replace the Gremlin current object.
    ReplaceCurrent,
    /// Bind a fresh target and label it for `select` (Gremlin `as`).
    ReplaceCurrentAndBindLabel,
    /// Constrain the expansion to land on an already-bound target.
    Existing,
    /// Either bind-new (Cypher) or replace-current (Gremlin).
    BindNewOrReplaceCurrent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LabelExpr {
    /// Match every node/edge.
    Any,
    /// Match nodes carrying *exactly* this label (or one of these labels).
    AnyOf(Vec<String>),
    /// Match nodes that carry *all* of these labels (multi-label).
    AllOf(Vec<String>),
    /// Negation: match nodes that do not carry the given label.
    Not(Box<LabelExpr>),
}

impl LabelExpr {
    pub fn label(name: impl Into<String>) -> Self {
        Self::AnyOf(vec![name.into()])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Length {
    pub min: u32,
    pub max: Option<u32>,
}

impl Length {
    pub const ONE: Self = Self {
        min: 1,
        max: Some(1),
    };

    pub const fn bounded(min: u32, max: u32) -> Self {
        Self {
            min,
            max: Some(max),
        }
    }

    pub const fn unbounded(min: u32) -> Self {
        Self { min, max: None }
    }

    pub fn max_display(&self) -> String {
        self.max
            .map(|max| max.to_string())
            .unwrap_or_else(|| "unbounded".to_string())
    }

    pub fn is_single_hop(&self) -> bool {
        self.min == 1 && self.max == Some(1)
    }

    pub fn is_variable_length(&self) -> bool {
        !self.is_single_hop()
    }

    pub fn sql_expand_shape(&self) -> ExpandSqlShape {
        if self.is_single_hop() {
            ExpandSqlShape::SingleJoin
        } else {
            ExpandSqlShape::Recursive
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpandSqlShape {
    SingleJoin,
    Recursive,
}

/// What an `Expand` writes into a path-binding row, if any. Mirrors the
/// `pathMaterialization` field in §11 / §2.7 / §5.x of the spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMaterialization {
    /// No path is materialized.
    None,
    /// Endpoints only — SPARQL property-path style.
    EndpointsOnly,
    /// Nodes and relationships — Cypher / GQL path values.
    NodesAndRelationships,
    /// Gremlin traverser path with edges and vertices.
    VisitedEdgesAndVertices,
    /// Gremlin vertices-only traverser path.
    VerticesOnly,
}

/// How a step of `GraphExpand` updates an upstream `path` binding inside
/// a `GraphRepeat` body. Mirrors `pathUpdate` in §5.3 / §5.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathUpdate {
    /// No path update.
    None,
    /// Append the new target vertex (vertex-only path history).
    AppendTargetVertex,
    /// Append the traversed edge then the new target vertex.
    AppendEdgeAndTargetVertex,
}

/// Catalog of objects threaded through a `GraphRepeat`'s `path` binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathObjects {
    VerticesOnly,
    VerticesAndEdges,
}

/// Repeat emit policy. Spec §5.3 / §5.5.
#[derive(Debug, Clone, PartialEq)]
pub enum EmitMode {
    /// Default: only emit rows after the loop terminates.
    AfterLoop,
    /// `repeat(...).emit()` — emit body output after every iteration.
    AfterEachIteration,
    /// `repeat(...).emit(P.predicate)` — emit each iteration's body
    /// output for which the row-level predicate evaluates to `true`.
    AfterEachIfPredicate(IrExpr),
    /// `repeat(...).emit(__.traversal)` — emit each iteration's body
    /// output for which the sub-traversal produces ≥1 row when run
    /// against that row as upstream.
    AfterEachIfTraversal(Box<Node>),
}

/// Where a `GraphPathFilter` evaluates its condition. Spec §11 / §5.5.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathFilterScope {
    /// Per-row predicate evaluated on the current path prefix during
    /// loop expansion (e.g. `simple_path()` inside repeat body).
    CurrentPrefix,
    /// Predicate evaluated on the final materialized path.
    FinalPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMode {
    /// Cypher `RETURN x, y` — keep upstream visible scope and add new.
    PreserveVisible,
    /// Cypher `WITH ...` — replace visible scope to exactly the listed
    /// fields.
    ReplaceScope,
    /// Gremlin `values(...)` — replace the `current` traverser binding.
    ReplaceCurrent,
}

/// SPARQL `BIND` and similar expression-eval boundaries can either fail
/// the row or rebind to `Null`/Unbound on expression error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectErrorPolicy {
    /// Default — propagate evaluation errors.
    PropagateError,
    /// SPARQL `BIND` semantics: expression error → variable becomes
    /// unbound (modeled as `Null`).
    UnboundOnExpressionError,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectionItem {
    pub alias: BindingId,
    pub expr: IrExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyKind {
    /// Standard correlated nested-loop join.
    Inner,
    /// Cypher `OPTIONAL MATCH`. Left rows with no right rows are
    /// null-extended on `outputs`.
    Optional,
    /// `EXISTS { ... }` — left row passes iff at least one right row.
    Semi,
    /// `NOT EXISTS { ... }` — left row passes iff zero right rows.
    Anti,
    /// Scalar correlated subquery — right side is required to produce
    /// exactly one row.
    Scalar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinKind {
    Inner,
    LeftOuter,
    RightOuter,
    FullOuter,
    Cross,
}

/// Spec §8.14 SPARQL `UNION` aligns by variable name; Cypher / Gremlin
/// concatenate by position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnionAlign {
    ByPosition,
    ByVariableName,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullsOrder {
    First,
    Last,
    ProviderDefined,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SortKey {
    pub expr: IrExpr,
    pub dir: SortDir,
    pub nulls: NullsOrder,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Slice {
    pub offset: u64,
    pub fetch: Option<u64>,
    pub tail: Option<u64>,
}

impl Slice {
    pub const NONE: Self = Self {
        offset: 0,
        fetch: None,
        tail: None,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistinctMode {
    /// Cypher / GQL row distinct.
    Row,
    /// Gremlin `dedup()` — see `bulk` for the bulk-handling rule.
    Traverser,
    /// SPARQL solution-mapping distinct.
    Solution,
}

/// Bulk handling for `GraphDistinct`. Cypher/GQL rows have no bulk so
/// this is `NotApplicable`; Gremlin `dedup()` resets bulk to one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistinctBulk {
    NotApplicable,
    ResetToOne,
    Preserve,
}

/// Spec §10.4 — barrier bulk policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarrierBulkPolicy {
    /// Preserve incoming bulk and merge equal traversers.
    PreserveAndMerge,
    /// Discard bulk; emit traversers with bulk=1.
    ResetToOne,
    ProviderDefined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoalesceSuccess {
    /// Pick the first arm that yields ≥1 row for the input, take all of its
    /// rows, ignore later arms. Matches Gremlin `coalesce`.
    FirstNonEmpty,
}

/// Per-arm output rename for `GraphCoalesce`. Spec §4.6 prints
/// `armOutputs=[knows->current, created->current, ...]`.
#[derive(Debug, Clone, PartialEq)]
pub struct CoalesceArmOutput {
    pub from: BindingId,
    pub to: BindingId,
}

/// `GraphChoose` selector. Boolean dispatch picks `arms[0]` when true and
/// `arms[1]` when false (binary form). Value dispatch matches a row's
/// computed value against each arm's `key` (switch form, §4.7).
#[derive(Debug, Clone, PartialEq)]
pub enum ChooseSelector {
    /// Boolean condition — `arms` must be exactly `[true_arm, false_arm]`
    /// and arm keys are ignored.
    Boolean(IrExpr),
    /// Value dispatch — each arm's `key` is matched against this value.
    Value(IrExpr),
}

/// One arm of a `GraphChoose` switch.
#[derive(Debug, Clone, PartialEq)]
pub struct ChooseArm {
    /// `Some(value)` for value-dispatch arms; `None` for the boolean form.
    pub key: Option<Value>,
    pub body: Node,
}

/// Behaviour when no arm matches and `default` is `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChooseUnmatched {
    /// Drop the row.
    Drop,
    /// Pass the row through with no arm applied (identity).
    PassThrough,
    /// Raise an error.
    Error,
}

/// Map value produced by `GraphGroupMap`. Gremlin `groupCount()` is a
/// keyed bulk count; `group()` is a keyed aggregate/collection.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupValue {
    CountBulk,
    Aggregate(AggCall),
}

/// `GraphPathPattern` selector — spec §5.6, §5.7, §5.8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSelector {
    /// All matching paths.
    All,
    /// Any single path.
    Any,
    /// `ANY SHORTEST` (k=1) / `ANY K SHORTEST`.
    Shortest { k: u32, ties: PathTies },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathTies {
    Any,
    All,
}

/// One element of a `GraphPathPattern`. Mirrors the `Node(...)` /
/// `Rel(...)` rows printed in spec §5.6.
#[derive(Debug, Clone, PartialEq)]
pub enum PathPart {
    Node {
        bind: BindingId,
        labels: LabelExpr,
    },
    Rel {
        bind: Option<BindingId>,
        types: LabelExpr,
        dir: Direction,
        length: Length,
    },
}

/// `GraphProcedureCall` mode. §9.1 / §9.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcedureMode {
    Read,
    Write,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureArg {
    /// `Some` for keyword-style args (Gremlin `with('key', value)`),
    /// `None` for positional Cypher args.
    pub name: Option<String>,
    pub value: IrExpr,
}

/// Quantifier kind — spec §11 collection nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuantifierKind {
    All,
    Any,
    None,
    Single,
}

// ============================================================
// SPARQL / RDF supporting types (spec §0.6, §5.9, §5.10, §8.x)
// ============================================================

/// One RDF term as it appears in `GraphRdfQuadScan`,
/// `GraphRdfPropertyPath`, `GraphConstructTriples`, `GraphService`, etc.
/// Mirrors the spec's `iri(...)`, `literal(...)`, `?var`, `_:b`
/// renderings.
#[derive(Debug, Clone, PartialEq)]
pub enum RdfTerm {
    /// `?name` — a SPARQL solution variable. Bound by upstream operators
    /// (Quad scans, BIND, …) and read by downstream filters.
    Variable(BindingId),
    /// `iri(...)` — an absolute or prefixed IRI.
    Iri(String),
    /// `literal(value)` — typed RDF literal sharing the scalar shapes
    /// used by `IrExpr::Lit`. For language-tagged or explicitly
    /// datatyped SPARQL literals, prefer `LanguageTagged` / `Typed`.
    Literal(Lit),
    /// `"text"@en` — language-tagged literal.
    LanguageTagged { value: String, lang: String },
    /// `"5"^^xsd:integer` — datatyped literal preserving the lexical form.
    Typed { lexical: String, datatype: String },
    /// `_:b` — blank node. The string is the local label.
    BlankNode(String),
}

/// Which graph in the SPARQL dataset a quad/property-path operator
/// targets. Spec §0.6.
#[derive(Debug, Clone, PartialEq)]
pub enum RdfGraphScope {
    /// SPARQL default graph.
    DefaultGraph,
    /// Active graph at evaluation time (used by SERVICE and by
    /// property paths inside `GRAPH ?g { ... }`).
    ActiveGraph,
    /// `GRAPH iri(:g) { ... }`.
    NamedGraph(RdfTerm),
    /// `GRAPH ?g { ... }`.
    NamedGraphVariable(BindingId),
}

/// SPARQL property-path expression. Spec §5.9 / §5.10 use the form
/// `one_or_more(seq(iri(:knows), iri(:worksWith)))`.
#[derive(Debug, Clone, PartialEq)]
pub enum RdfPathExpr {
    /// `iri(:knows)`.
    Iri(String),
    /// `^p` — inverse path.
    Inverse(Box<RdfPathExpr>),
    /// `seq(p1, p2, …)` — sequence path.
    Sequence(Vec<RdfPathExpr>),
    /// `alt(p1, p2, …)` — alternative path.
    Alternative(Vec<RdfPathExpr>),
    /// `p+` — one or more.
    OneOrMore(Box<RdfPathExpr>),
    /// `p*` — zero or more.
    ZeroOrMore(Box<RdfPathExpr>),
    /// `p?` — zero or one.
    ZeroOrOne(Box<RdfPathExpr>),
    /// `!p` — negated property set.
    Negated(Box<RdfPathExpr>),
}

/// Whether a property path permits zero-length matches (subject =
/// object). Spec §5.10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZeroLengthPolicy {
    /// `*` and `?` paths permit subject = object even if the predicate
    /// would otherwise not match.
    Allowed,
    /// `+` paths require at least one step.
    Disallowed,
}

/// SPARQL `MINUS` compatibility predicate. Spec §8.4 / §8.7.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinusCompatibility {
    /// Standard SPARQL semantics: a left row is removed only when the
    /// right side has a compatible solution mapping that shares at least
    /// one variable with the left row. With no shared variables the
    /// left row is kept.
    SharedVariables,
}

/// One triple of a `GraphConstructTriples` template. Spec §8.12.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstructTriple {
    pub subject: RdfTerm,
    pub predicate: RdfTerm,
    pub object: RdfTerm,
}

/// One node element created by `GraphCreate`.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateNode {
    pub bind: Option<BindingId>,
    pub label: String,
    pub properties: Option<IrExpr>,
}

/// One relationship element created by `GraphCreate`. `src`/`dst` name
/// bindings that are either already in scope or created by the same
/// `GraphCreate` node.
#[derive(Debug, Clone, PartialEq)]
pub struct CreateEdge {
    pub bind: Option<BindingId>,
    pub rel_type: String,
    pub src: BindingId,
    pub dst: BindingId,
    pub properties: Option<IrExpr>,
}

/// One mutation target for `GraphSetProperty`.
#[derive(Debug, Clone, PartialEq)]
pub struct SetPropertyItem {
    pub target: IrExpr,
    /// The property name for [`SetMode::Property`]; empty otherwise.
    pub key: String,
    pub mode: SetMode,
    pub value: IrExpr,
}

/// How a `GraphSetProperty` item applies its value. Cypher spells these
/// `n.k = v`, `n = {…}` and `n += {…}`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetMode {
    /// Assign one property named by `key`.
    Property,
    /// Replace the whole property bag with the map `value`.
    Replace,
    /// Merge the map `value` into the existing property bag.
    Merge,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GraphPlan {
    pub policy: GraphPlanPolicy,
    pub root: Box<Node>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    // -------- output boundary --------
    /// `GraphReturn(fields, resultForm)` — the result-shape boundary.
    GraphReturn {
        fields: Vec<BindingId>,
        result_form: ResultForm,
        input: Box<Node>,
    },
    /// `GraphConstructTriples(template)` — SPARQL `CONSTRUCT` output.
    /// One triple is emitted per input solution mapping for each
    /// triple in the template. Spec §8.12.
    GraphConstructTriples {
        template: Vec<ConstructTriple>,
        input: Box<Node>,
    },
    /// `GraphDescribe(terms)` — SPARQL `DESCRIBE` output. Per
    /// implementation, the engine returns a description of each term
    /// (typically a CBD) computed over the input solution mappings.
    GraphDescribe {
        terms: Vec<RdfTerm>,
        input: Box<Node>,
    },
    /// `GraphAsk(field)` — SPARQL `ASK` output. The single result
    /// row carries a boolean under `field`. Spec §8.11.
    GraphAsk {
        field: BindingId,
        input: Box<Node>,
    },

    // -------- sources --------
    /// `GraphNodeScan(graph, labels|labelsExpr)`.
    GraphNodeScan {
        graph: String,
        binding: BindingId,
        labels: LabelExpr,
    },
    /// `GraphRelScan(graph, types, dir)`.
    GraphRelScan {
        graph: String,
        binding: BindingId,
        types: LabelExpr,
        dir: Direction,
    },
    /// `GraphValues(bindings, rows, hidden)`.
    GraphValues {
        bindings: Vec<BindingId>,
        rows: Vec<Vec<Value>>,
        /// Hidden Gremlin `_bulk` for each row (parallel to `rows`).
        bulk: Option<Vec<u64>>,
    },
    /// `GraphOneRow()`.
    GraphOneRow,
    /// `GraphEmpty()`.
    GraphEmpty,
    /// `GraphCorrelate(bindings)`. Used as the source of an `Apply` right
    /// side; the interpreter materializes one row containing the correlated
    /// bindings.
    GraphCorrelate {
        bindings: Vec<BindingId>,
    },
    /// `GraphRdfQuadScan(dataset, graphScope, subject, predicate, object,
    /// outputs)` — scans RDF triples/quads in a query dataset and graph
    /// scope. Spec §8.x. Terms may be IRIs, literals, blank nodes, or
    /// variables (newly bound or correlated from the outer scope).
    GraphRdfQuadScan {
        dataset: String,
        graph_scope: RdfGraphScope,
        subject: RdfTerm,
        predicate: RdfTerm,
        object: RdfTerm,
        outputs: Vec<BindingId>,
    },

    // -------- pattern --------
    /// `GraphBind(bind, kind, expr)`. When `expr` is `None`, this is a
    /// pure metadata rename of the `current` binding produced by an
    /// upstream scan or expansion.
    GraphBind {
        bind: BindingId,
        kind: BindKind,
        expr: Option<IrExpr>,
        input: Box<Node>,
    },
    /// `GraphExpand(...)` — single-step or variable-length traversal.
    GraphExpand {
        graph: String,
        source: BindingId,
        target: BindingId,
        target_mode: TargetMode,
        target_labels: LabelExpr,
        rel_binding: Option<BindingId>,
        rel_types: LabelExpr,
        dir: Direction,
        length: Length,
        /// Optional traversal history used for relationship uniqueness across
        /// a larger graph pattern. Unlike `path`, this binding is not the
        /// user-visible path value; it only carries visited relationships.
        history: Option<BindingId>,
        /// When `Some`, each output row carries the visited path under this
        /// binding (`Path` value).
        path: Option<BindingId>,
        path_mode: PathMode,
        match_mode: MatchMode,
        path_materialization: PathMaterialization,
        path_update: PathUpdate,
        input: Box<Node>,
    },
    /// `GraphPathPattern(...)` — full property-graph path expression
    /// (Cypher `shortestPath`, GQL `MATCH ... TRAIL`, etc.). Spec §5.6 ff.
    GraphPathPattern {
        graph: String,
        path: BindingId,
        selector: PathSelector,
        path_mode: PathMode,
        match_mode: MatchMode,
        endpoints: Vec<BindingId>,
        parts: Vec<PathPart>,
        path_materialization: PathMaterialization,
        input: Box<Node>,
    },
    /// `GraphRdfPropertyPath(dataset, graphScope, subject, object, path,
    /// pathMaterialization, zeroLength)` — SPARQL property path
    /// (§5.9 / §5.10). Endpoints are typically variables; `path` is a
    /// composed `RdfPathExpr`.
    GraphRdfPropertyPath {
        dataset: String,
        graph_scope: RdfGraphScope,
        subject: RdfTerm,
        object: RdfTerm,
        path: RdfPathExpr,
        path_materialization: PathMaterialization,
        zero_length: ZeroLengthPolicy,
    },
    /// `GraphRepeat(seed, body, ...)`. Modeled as a vertical loop.
    /// `times = Some(N)` means user-requested `times(N)` cap; `None`
    /// means no user-supplied iteration count, in which case
    /// termination comes from `until` (predicate match) or natural
    /// frontier emptiness.
    GraphRepeat {
        loop_name: Option<String>,
        times: Option<u32>,
        emit: EmitMode,
        until: Option<IrExpr>,
        until_traversal: Option<Box<Node>>,
        /// Optional path binding that accumulates visited objects across
        /// iterations.
        path: Option<BindingId>,
        path_objects: PathObjects,
        /// Prefix-emit row-level predicate (e.g. `emit(P).repeat(...)`).
        /// Applied to the seed before any iteration runs.
        prefix_predicate: Option<IrExpr>,
        /// Prefix-emit sub-traversal probe (e.g.
        /// `emit(__.traversal).repeat(...)`). Applied to the seed before
        /// any iteration runs; emit each row whose probe yields ≥1 result.
        prefix_traversal: Option<Box<Node>>,
        seed: Box<Node>,
        body: Box<Node>,
    },
    /// `GraphPathFilter(condition, scope)`.
    GraphPathFilter {
        condition: IrExpr,
        scope: PathFilterScope,
        input: Box<Node>,
    },

    // -------- mutations --------
    /// `GraphCreate(nodes, edges)` — create graph elements once per input
    /// row. All `nodes` are created before any `edges`, so an edge may name
    /// a node bound in the same clause.
    GraphCreate {
        graph: String,
        nodes: Vec<CreateNode>,
        edges: Vec<CreateEdge>,
        input: Box<Node>,
    },
    /// `GraphMerge` — Cypher `MERGE`. Per input row, run `match_arm`; if it
    /// yields no rows, run `create_arm` instead. Both arms are correlated
    /// subplans rooted at `GraphCorrelate(correlation)` and carry their own
    /// `ON MATCH` / `ON CREATE` mutations.
    GraphMerge {
        correlation: Vec<BindingId>,
        outputs: Vec<BindingId>,
        input: Box<Node>,
        match_arm: Box<Node>,
        create_arm: Box<Node>,
    },
    /// `GraphSetProperty(items)` — mutate properties and pass rows through.
    GraphSetProperty {
        items: Vec<SetPropertyItem>,
        input: Box<Node>,
    },
    /// `GraphDelete(targets, detach)` — delete graph elements and pass rows through.
    GraphDelete {
        targets: Vec<IrExpr>,
        detach: bool,
        input: Box<Node>,
    },

    // -------- row algebra --------
    GraphFilter {
        condition: IrExpr,
        input: Box<Node>,
    },
    GraphProject {
        mode: ProjectMode,
        items: Vec<ProjectionItem>,
        error_policy: ProjectErrorPolicy,
        input: Box<Node>,
    },
    /// `GraphCurrentProject(expr=current=...)` — Gremlin replaces the
    /// `current` binding with a derived value, dropping rows where the
    /// expression evaluates to `Null` (unproductive policy).
    GraphCurrentProject {
        expr: IrExpr,
        /// Visible output fields after this projection. Conventionally
        /// `["current"]`; declared explicitly so HEP rules and explain
        /// output match the spec.
        fields: Vec<BindingId>,
        input: Box<Node>,
    },
    GraphAggregate {
        group: Vec<ProjectionItem>,
        aggs: Vec<AggCall>,
        /// Visible output fields = group keys + agg aliases. Stored
        /// explicitly to match the spec's `fields=[...]` rendering.
        fields: Vec<BindingId>,
        input: Box<Node>,
    },
    /// `GraphGroupMap(key, value, output)` — Gremlin map-shaped
    /// `group()` / `groupCount()`. Spec §6.2 / §6.3.
    GraphGroupMap {
        key: IrExpr,
        value: GroupValue,
        output: BindingId,
        input: Box<Node>,
    },
    /// `GraphGroupCountSideEffect(label, key)` — Gremlin
    /// `groupCount(label).by(key)`. Updates the named side-effect map and
    /// passes the input traverser stream through unchanged.
    GraphGroupCountSideEffect {
        label: BindingId,
        key: IrExpr,
        input: Box<Node>,
    },
    /// `GraphCap(labels)` — read named Gremlin side effects back into the
    /// stream. Single-label cap emits the side-effect value as `current`;
    /// multi-label cap emits a map keyed by label.
    GraphCap {
        labels: Vec<BindingId>,
        input: Box<Node>,
    },
    GraphShortestPath {
        source: BindingId,
        target: Option<BindingId>,
        direction: Direction,
        rel_types: LabelExpr,
        max_distance: Option<f64>,
        include_edges: bool,
        output: BindingId,
        all_paths: bool,
        input: Box<Node>,
    },
    GraphDistinct {
        keys: Vec<BindingId>,
        mode: DistinctMode,
        bulk: DistinctBulk,
        input: Box<Node>,
    },
    GraphSort {
        keys: Vec<SortKey>,
        input: Box<Node>,
    },
    GraphSlice {
        slice: Slice,
        input: Box<Node>,
    },
    GraphSliceExpr {
        offset: Option<IrExpr>,
        fetch: Option<IrExpr>,
        input: Box<Node>,
    },
    /// `GraphBarrier` — stream materialization with optional partitioned
    /// order/slice. Spec §6.6, §6.7, §10.4.
    GraphBarrier {
        partition: Vec<BindingId>,
        order: Vec<SortKey>,
        slice: Slice,
        materialize: bool,
        bulk_policy: BarrierBulkPolicy,
        input: Box<Node>,
    },
    GraphJoin {
        kind: JoinKind,
        left: Box<Node>,
        right: Box<Node>,
        /// `None` ⇒ Cartesian product / "true" condition.
        condition: Option<IrExpr>,
    },
    GraphApply {
        kind: ApplyKind,
        correlation: Vec<BindingId>,
        outputs: Vec<BindingId>,
        optional_missing: OptionalMissing,
        left: Box<Node>,
        right: Box<Node>,
    },
    GraphUnion {
        all: bool,
        align: UnionAlign,
        left: Box<Node>,
        right: Box<Node>,
    },
    GraphUnwind {
        input_expr: IrExpr,
        bind: BindingId,
        outer: bool,
        input: Box<Node>,
    },

    // -------- collection / quantification --------
    /// `GraphQuantifier` — `all`/`any`/`none`/`single` collection
    /// predicates. Outputs a row per input with a boolean `output`
    /// binding.
    GraphQuantifier {
        kind: QuantifierKind,
        item_binding: BindingId,
        input_expr: IrExpr,
        predicate: IrExpr,
        output: BindingId,
        input: Box<Node>,
    },
    /// `GraphCollect(value, distinct, order)` — list-shaped collection
    /// projection when not modeled as an aggregate.
    GraphCollect {
        value: IrExpr,
        distinct: bool,
        order: Vec<SortKey>,
        alias: BindingId,
        input: Box<Node>,
    },
    /// `GraphListComprehension(input, item, filter, map)` — Cypher list
    /// comprehension as a node. Spec §11. Most plans inline the
    /// comprehension as `IrExpr::Call("list_comprehension", …)` inside
    /// `GraphProject`; this node form exists for plans that need to
    /// represent a comprehension as a separate planning boundary.
    GraphListComprehension {
        input_expr: IrExpr,
        item: BindingId,
        filter: Option<IrExpr>,
        map_expr: Option<IrExpr>,
        alias: BindingId,
        input: Box<Node>,
    },

    // -------- language-shaped --------
    /// `GraphCoalesce` — Gremlin first-success branch. Per input row, try
    /// arms in order; emit the rows from the first arm that produces ≥1
    /// row (under `success=FirstNonEmpty`).
    GraphCoalesce {
        success: CoalesceSuccess,
        output: BindingId,
        correlation: Vec<BindingId>,
        /// Per-arm rename mapping `arm_output_binding -> output`. Spec
        /// §4.6 prints this as `armOutputs=[knows->current, ...]`.
        arm_outputs: Vec<CoalesceArmOutput>,
        input: Box<Node>,
        arms: Vec<Node>,
    },
    /// `GraphChoose` — boolean (binary form) or value-dispatch (switch
    /// form) branch.
    GraphChoose {
        selector: ChooseSelector,
        output: BindingId,
        correlation: Vec<BindingId>,
        arms: Vec<ChooseArm>,
        default: Option<Box<Node>>,
        unmatched: ChooseUnmatched,
        input: Box<Node>,
    },
    /// `GraphSelect(labels, output)` — Gremlin label re-materialization.
    GraphSelect {
        labels: Vec<BindingId>,
        outputs: Vec<BindingId>,
        input: Box<Node>,
    },
    /// `GraphSparqlMinus(compatible, shared)` — SPARQL `MINUS`
    /// operator. Must survive initial planning per spec §8.4 / §8.7;
    /// rewriting to anti-join is only legal once compatibility analysis
    /// proves equivalence.
    GraphSparqlMinus {
        compatible: MinusCompatibility,
        shared: Vec<BindingId>,
        left: Box<Node>,
        right: Box<Node>,
    },
    /// `GraphService(endpoint, silent, outputs)` — SPARQL federated
    /// pattern. The `input` is the inner pattern that runs against
    /// `endpoint`. Spec §8.13.
    GraphService {
        endpoint: RdfTerm,
        silent: bool,
        outputs: Vec<BindingId>,
        input: Box<Node>,
    },
    /// `GraphProcedureCall(name, args, yields, mode)` — Cypher `CALL`,
    /// Gremlin `g.call(...)`. `input` is `None` for top-level calls and
    /// `Some` for correlated mid-traversal calls.
    GraphProcedureCall {
        name: String,
        args: Vec<ProcedureArg>,
        yields: Vec<BindingId>,
        mode: ProcedureMode,
        input: Option<Box<Node>>,
    },
    /// `GraphExtension(name, inputs, metadata)` — explicit escape hatch
    /// for an operator outside the shared catalog. Spec §11.
    GraphExtension {
        name: String,
        metadata: Vec<(String, Value)>,
        inputs: Vec<Node>,
    },
}

impl Node {
    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

/// Builder helpers that mirror the doc's "explain" style.
impl Node {
    pub fn node_scan(
        graph: impl Into<String>,
        binding: impl Into<BindingId>,
        labels: LabelExpr,
    ) -> Self {
        Self::GraphNodeScan {
            graph: graph.into(),
            binding: binding.into(),
            labels,
        }
    }

    pub fn bind_node(self, binding: impl Into<BindingId>) -> Self {
        Self::GraphBind {
            bind: binding.into(),
            kind: BindKind::Node,
            expr: None,
            input: self.boxed(),
        }
    }

    pub fn filter(self, condition: IrExpr) -> Self {
        Self::GraphFilter {
            condition,
            input: self.boxed(),
        }
    }

    pub fn return_(self, fields: Vec<BindingId>, result_form: ResultForm) -> Self {
        Self::GraphReturn {
            fields,
            result_form,
            input: self.boxed(),
        }
    }
}

impl GraphPlan {
    pub fn new(policy: GraphPlanPolicy, root: Node) -> Self {
        Self {
            policy,
            root: Box::new(root),
        }
    }
}

/// Pretty-printer that mirrors the `EXPLAIN` style used in the design doc.
/// Each operator on its own line with two-space indentation per depth.
pub fn explain(plan: &GraphPlan) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    writeln!(out, "Policy: {:?}", plan.policy).ok();
    writeln!(out).ok();
    write_node(&mut out, &plan.root, 0);
    out
}

fn pad(buf: &mut String, depth: usize) {
    for _ in 0..depth {
        buf.push_str("  ");
    }
}

fn write_node(buf: &mut String, node: &Node, depth: usize) {
    use std::fmt::Write;
    pad(buf, depth);
    match node {
        Node::GraphReturn {
            fields,
            result_form,
            input,
        } => {
            writeln!(
                buf,
                "GraphReturn(fields=[{}], resultForm=[{:?}])",
                fields.join(", "),
                result_form
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphNodeScan {
            graph,
            binding,
            labels,
        } => {
            writeln!(
                buf,
                "GraphNodeScan(graph=[{graph}], bind=[{binding}], labels=[{labels:?}])"
            )
            .ok();
        }
        Node::GraphRelScan {
            graph,
            binding,
            types,
            dir,
        } => {
            writeln!(
                buf,
                "GraphRelScan(graph=[{graph}], bind=[{binding}], types=[{types:?}], dir=[{dir:?}])"
            )
            .ok();
        }
        Node::GraphValues {
            bindings,
            rows,
            bulk,
        } => {
            writeln!(
                buf,
                "GraphValues(bindings=[{}], rows={}, bulk={})",
                bindings.join(", "),
                rows.len(),
                bulk.is_some()
            )
            .ok();
        }
        Node::GraphOneRow => {
            writeln!(buf, "GraphOneRow()").ok();
        }
        Node::GraphEmpty => {
            writeln!(buf, "GraphEmpty()").ok();
        }
        Node::GraphCorrelate { bindings } => {
            writeln!(buf, "GraphCorrelate(bindings=[{}])", bindings.join(", ")).ok();
        }
        Node::GraphBind {
            bind, kind, input, ..
        } => {
            writeln!(buf, "GraphBind(bind=[{bind}], kind=[{kind:?}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphExpand {
            source,
            target,
            target_mode,
            rel_types,
            dir,
            length,
            history,
            path,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphExpand(source=[{source}], target=[{target}], mode=[{target_mode:?}], types=[{rel_types:?}], dir=[{dir:?}], length=[{}..{}], history=[{}], path=[{}])",
                length.min,
                length.max_display(),
                history.as_deref().unwrap_or("-"),
                path.as_deref().unwrap_or("-")
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphPathPattern {
            path,
            selector,
            endpoints,
            parts,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphPathPattern(path=[{path}], selector=[{selector:?}], endpoints=[{}], parts={})",
                endpoints.join(", "),
                parts.len()
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphRepeat {
            times,
            emit,
            seed,
            body,
            ..
        } => {
            let times_str = match times {
                Some(n) => format!("{n}"),
                None => "Unbounded".to_string(),
            };
            writeln!(buf, "GraphRepeat(times=[{times_str}], emit=[{emit:?}])").ok();
            pad(buf, depth + 1);
            writeln!(buf, "seed:").ok();
            write_node(buf, seed, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "body:").ok();
            write_node(buf, body, depth + 2);
        }
        Node::GraphPathFilter { input, scope, .. } => {
            writeln!(buf, "GraphPathFilter(scope=[{scope:?}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCreate {
            graph,
            nodes,
            edges,
            input,
        } => {
            let specs = nodes
                .iter()
                .map(|node| match &node.bind {
                    Some(bind) => format!("{bind}:{}", node.label),
                    None => format!(":{}", node.label),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let edge_specs = edges
                .iter()
                .map(|edge| {
                    let bind = edge.bind.as_deref().unwrap_or("");
                    format!("({})-[{bind}:{}]->({})", edge.src, edge.rel_type, edge.dst)
                })
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                buf,
                "GraphCreate(graph=[{graph}], nodes=[{specs}], edges=[{edge_specs}])"
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphMerge {
            correlation,
            outputs,
            input,
            match_arm,
            create_arm,
        } => {
            writeln!(
                buf,
                "GraphMerge(correlation=[{}], outputs=[{}])",
                correlation.join(", "),
                outputs.join(", ")
            )
            .ok();
            write_node(buf, input, depth + 1);
            write_node(buf, match_arm, depth + 1);
            write_node(buf, create_arm, depth + 1);
        }
        Node::GraphSetProperty { items, input } => {
            let specs = items
                .iter()
                .map(|item| item.key.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(buf, "GraphSetProperty(keys=[{specs}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphDelete {
            targets,
            detach,
            input,
        } => {
            writeln!(
                buf,
                "GraphDelete(targets=[{}], detach=[{detach}])",
                targets.len()
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphFilter { input, .. } => {
            writeln!(buf, "GraphFilter").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphProject {
            mode, items, input, ..
        } => {
            let names = items
                .iter()
                .map(|item| item.alias.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(buf, "GraphProject(mode=[{mode:?}], fields=[{names}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCurrentProject { input, fields, .. } => {
            writeln!(buf, "GraphCurrentProject(fields=[{}])", fields.join(", ")).ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphAggregate {
            group, aggs, input, ..
        } => {
            let group_names = group
                .iter()
                .map(|item| item.alias.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let agg_names = aggs
                .iter()
                .map(|agg| agg.alias.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            writeln!(
                buf,
                "GraphAggregate(group=[{group_names}], aggs=[{agg_names}])"
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphGroupMap { output, input, .. } => {
            writeln!(buf, "GraphGroupMap(output=[{output}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphGroupCountSideEffect { label, input, .. } => {
            writeln!(buf, "GraphGroupCountSideEffect(label=[{label}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCap { labels, input } => {
            writeln!(buf, "GraphCap(labels=[{}])", labels.join(", ")).ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphShortestPath {
            source,
            target,
            direction,
            output,
            all_paths,
            max_distance,
            include_edges,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphShortestPath(source=[{source}], target=[{target:?}], dir=[{direction:?}], output=[{output}], all_paths=[{all_paths}], max_distance=[{max_distance:?}], include_edges=[{include_edges}])"
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphDistinct {
            keys,
            mode,
            bulk,
            input,
        } => {
            writeln!(
                buf,
                "GraphDistinct(keys=[{}], mode=[{mode:?}], bulk=[{bulk:?}])",
                keys.join(", ")
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphSort { input, .. } => {
            writeln!(buf, "GraphSort").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphSlice { slice, input, .. } => {
            writeln!(
                buf,
                "GraphSlice(offset=[{}], fetch=[{:?}], tail=[{:?}])",
                slice.offset, slice.fetch, slice.tail
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphSliceExpr {
            offset,
            fetch,
            input,
        } => {
            writeln!(
                buf,
                "GraphSliceExpr(offset=[{}], fetch=[{}])",
                offset
                    .as_ref()
                    .map(|expr| format!("{expr:?}"))
                    .unwrap_or_else(|| "None".to_string()),
                fetch
                    .as_ref()
                    .map(|expr| format!("{expr:?}"))
                    .unwrap_or_else(|| "None".to_string())
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphBarrier {
            partition,
            slice,
            materialize,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphBarrier(partition=[{}], slice=[off={},fetch={:?},tail={:?}], materialize=[{materialize}])",
                partition.join(", "),
                slice.offset,
                slice.fetch,
                slice.tail
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphJoin {
            kind, left, right, ..
        } => {
            writeln!(buf, "GraphJoin(kind=[{kind:?}])").ok();
            pad(buf, depth + 1);
            writeln!(buf, "left:").ok();
            write_node(buf, left, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "right:").ok();
            write_node(buf, right, depth + 2);
        }
        Node::GraphApply {
            kind,
            correlation,
            outputs,
            left,
            right,
            ..
        } => {
            writeln!(
                buf,
                "GraphApply(kind=[{kind:?}], correlation=[{}], outputs=[{}])",
                correlation.join(", "),
                outputs.join(", ")
            )
            .ok();
            pad(buf, depth + 1);
            writeln!(buf, "left:").ok();
            write_node(buf, left, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "right:").ok();
            write_node(buf, right, depth + 2);
        }
        Node::GraphUnion {
            all,
            align,
            left,
            right,
        } => {
            writeln!(buf, "GraphUnion(all=[{all}], align=[{align:?}])").ok();
            pad(buf, depth + 1);
            writeln!(buf, "left:").ok();
            write_node(buf, left, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "right:").ok();
            write_node(buf, right, depth + 2);
        }
        Node::GraphUnwind {
            bind, outer, input, ..
        } => {
            writeln!(buf, "GraphUnwind(bind=[{bind}], outer=[{outer}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphQuantifier {
            kind,
            output,
            input,
            ..
        } => {
            writeln!(buf, "GraphQuantifier(kind=[{kind:?}], output=[{output}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCollect {
            alias,
            distinct,
            input,
            ..
        } => {
            writeln!(buf, "GraphCollect(alias=[{alias}], distinct=[{distinct}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphCoalesce {
            output,
            arms,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphCoalesce(output=[{output}], arms=[{}])",
                arms.len()
            )
            .ok();
            pad(buf, depth + 1);
            writeln!(buf, "input:").ok();
            write_node(buf, input, depth + 2);
            for (idx, arm) in arms.iter().enumerate() {
                pad(buf, depth + 1);
                writeln!(buf, "arm{idx}:").ok();
                write_node(buf, arm, depth + 2);
            }
        }
        Node::GraphChoose {
            output,
            input,
            arms,
            default,
            unmatched,
            ..
        } => {
            writeln!(
                buf,
                "GraphChoose(output=[{output}], arms=[{}], unmatched=[{unmatched:?}])",
                arms.len()
            )
            .ok();
            pad(buf, depth + 1);
            writeln!(buf, "input:").ok();
            write_node(buf, input, depth + 2);
            for (idx, arm) in arms.iter().enumerate() {
                pad(buf, depth + 1);
                writeln!(buf, "arm{idx}:").ok();
                write_node(buf, &arm.body, depth + 2);
            }
            if let Some(default) = default {
                pad(buf, depth + 1);
                writeln!(buf, "default:").ok();
                write_node(buf, default, depth + 2);
            }
        }
        Node::GraphSelect {
            labels,
            outputs,
            input,
        } => {
            writeln!(
                buf,
                "GraphSelect(labels=[{}], output=[{}])",
                labels.join(", "),
                outputs.join(", ")
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphProcedureCall {
            name,
            yields,
            mode,
            input,
            ..
        } => {
            writeln!(
                buf,
                "GraphProcedureCall(name=[{name}], yields=[{}], mode=[{mode:?}])",
                yields.join(", ")
            )
            .ok();
            if let Some(input) = input {
                write_node(buf, input, depth + 1);
            }
        }
        Node::GraphExtension { name, inputs, .. } => {
            writeln!(
                buf,
                "GraphExtension(name=[{name}], inputs=[{}])",
                inputs.len()
            )
            .ok();
            for input in inputs {
                write_node(buf, input, depth + 1);
            }
        }
        Node::GraphRdfQuadScan {
            dataset,
            graph_scope,
            subject,
            predicate,
            object,
            outputs,
        } => {
            writeln!(
                buf,
                "GraphRdfQuadScan(dataset=[{dataset}], graphScope=[{graph_scope:?}], subject=[{subject:?}], predicate=[{predicate:?}], object=[{object:?}], outputs=[{}])",
                outputs.join(", ")
            )
            .ok();
        }
        Node::GraphRdfPropertyPath {
            dataset,
            graph_scope,
            subject,
            object,
            path,
            path_materialization,
            zero_length,
        } => {
            writeln!(
                buf,
                "GraphRdfPropertyPath(dataset=[{dataset}], graphScope=[{graph_scope:?}], subject=[{subject:?}], object=[{object:?}], path=[{path:?}], pathMaterialization=[{path_materialization:?}], zeroLength=[{zero_length:?}])"
            )
            .ok();
        }
        Node::GraphSparqlMinus {
            compatible,
            shared,
            left,
            right,
        } => {
            writeln!(
                buf,
                "GraphSparqlMinus(compatible=[{compatible:?}], shared=[{}])",
                shared.join(", ")
            )
            .ok();
            pad(buf, depth + 1);
            writeln!(buf, "left:").ok();
            write_node(buf, left, depth + 2);
            pad(buf, depth + 1);
            writeln!(buf, "right:").ok();
            write_node(buf, right, depth + 2);
        }
        Node::GraphService {
            endpoint,
            silent,
            outputs,
            input,
        } => {
            writeln!(
                buf,
                "GraphService(endpoint=[{endpoint:?}], silent=[{silent}], outputs=[{}])",
                outputs.join(", ")
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphConstructTriples { template, input } => {
            writeln!(
                buf,
                "GraphConstructTriples(template=[{} triple{}])",
                template.len(),
                if template.len() == 1 { "" } else { "s" }
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphDescribe { terms, input } => {
            writeln!(
                buf,
                "GraphDescribe(terms=[{} term{}])",
                terms.len(),
                if terms.len() == 1 { "" } else { "s" }
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphAsk { field, input } => {
            writeln!(buf, "GraphAsk(field=[{field}])").ok();
            write_node(buf, input, depth + 1);
        }
        Node::GraphListComprehension {
            item, alias, input, ..
        } => {
            writeln!(
                buf,
                "GraphListComprehension(item=[{item}], alias=[{alias}])"
            )
            .ok();
            write_node(buf, input, depth + 1);
        }
    }
}
