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
    /// Gremlin barrier: optionally normalize sacks after traverser merging.
    Gremlin { normalize_sack: bool },
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
    /// Route each row to every matching arm, then execute each arm once on
    /// its nonempty stream. Input is evaluated once, including side effects.
    Predicates(Vec<IrExpr>),
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
    /// Evaluate a Gremlin value traversal over the complete correlated group.
    Traversal {
        traversal: Box<Node>,
        /// Proven current-only, pure, order-independent reduction.
        bulk_current: bool,
    },
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

/// One RDF term as it appears in `GraphSparqlTriplePattern`,
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
    /// Default graph formed by merging the listed named graphs in a SPARQL
    /// `FROM` clause. Equal triples from different graphs occur only once.
    DatasetDefaultGraph(Vec<String>),
    /// `GRAPH <iri>` within an explicit dataset. Only listed `FROM NAMED`
    /// graphs are visible, even when the source contains other graphs.
    DatasetNamedGraph { iri: String, allowed: Vec<String> },
    /// `GRAPH ?g` within an explicit dataset.
    DatasetNamedGraphVariable {
        variable: BindingId,
        allowed: Vec<String>,
    },
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SampleKind {
    Coin(f64),
    Global(u64),
    Local(u64),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    /// An explicit interpreter/JVM boundary. The JVM receives evaluated typed
    /// arguments; bindings, traverser bulk and hidden state stay in the interpreter.
    /// Trusted scripts may mutate the caller's graph, so this is an effect fence.
    GraphJvm {
        operation: crate::ir::jvm::JvmOperation,
        input: Box<Node>,
    },
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
    /// `GraphSparqlTriplePattern(dataset, graphScope, subject, predicate,
    /// object, outputs)` preserves an unresolved SPARQL triple pattern until
    /// ontology mapping resolves it to property-graph operators. This is a
    /// logical boundary, not an RDF storage adapter.
    GraphSparqlTriplePattern {
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
        /// Check termination on incoming seeds before the first body iteration.
        until_first: bool,
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
    /// Accumulate a named group without consuming or replaying its input.
    GraphGroupSideEffect {
        label: BindingId,
        key: IrExpr,
        value: GroupValue,
        key_input: Box<Node>,
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
    /// Traversal-scoped collection/reducer update. The input is executed once;
    /// registered seed/reducer state survives child traversals and empty streams.
    GraphSideEffect {
        value_input: Box<Node>,
        label: BindingId,
        value: IrExpr,
        seed: Value,
        reducer: String,
        eager: bool,
        input: Box<Node>,
    },
    /// Read a named side effect for each input traverser, preserving its labels.
    GraphReadSideEffect {
        label: BindingId,
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
    /// Stateful Gremlin coin and local/global sampling. The step id survives
    /// correlated-plan cloning, so child invocations share the step's RNG.
    GraphSample {
        kind: SampleKind,
        seed: Option<i64>,
        step_id: String,
        weight: Option<IrExpr>,
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

mod explain;
pub use explain::explain;
