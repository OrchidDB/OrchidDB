use super::{
    AggKind, BySpec, CallArg, CastTarget, FormatPart, ListOpKind, MapColumn, MathExpr, Pop, SackOp,
    StringOp, TraversalOption,
};
use crate::language::gremlin::semantics::{Direction, GValue, Predicate};

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    V {
        ids: Vec<GValue>,
    },
    E {
        ids: Vec<GValue>,
    },
    HasLabel(Vec<String>),
    HasKey {
        key: String,
    },
    /// `hasKey(k1, k2, ...)`: property-key filter matching any supplied key.
    HasKeyAny(Vec<String>),
    Has {
        key: String,
        predicate: Predicate,
    },
    HasNot {
        key: String,
    },
    ExpandVertex {
        direction: Direction,
        edge_labels: Vec<String>,
    },
    /// `outE`/`inE`/`bothE`: vertex → adjacent edges, *staying on the edge*.
    ExpandEdge {
        direction: Direction,
        edge_labels: Vec<String>,
    },
    /// `outV`/`inV`/`bothV`: edge → endpoint vertex.
    EndpointVertex {
        direction: Direction,
    },
    /// `otherV()`: edge → endpoint vertex opposite the side by which the
    /// current edge was reached.
    OtherVertex,
    Values(Vec<String>),
    /// `id()`: project the current element's id as a scalar.
    Id,
    /// `label()`: project the current element's label as a scalar string.
    Label,
    /// `identity()`: pass-through, no plan change.
    Identity,
    /// `is(predicate)`: filter the current scalar value by predicate.
    Is {
        predicate: Predicate,
    },
    /// `all(P)`: keep list traversers only when every list element matches.
    /// Non-list inputs do not match.
    All {
        predicate: Predicate,
    },
    /// `any(P)`: keep list traversers when at least one list element matches.
    /// Non-list inputs do not match.
    Any {
        predicate: Predicate,
    },
    /// `none(P)`: keep list traversers when no list element matches.
    NonePredicate {
        predicate: Predicate,
    },
    /// `hasId(ids...)`: filter elements whose id is in the given set.
    HasId {
        ids: Vec<GValue>,
    },
    /// `hasId(P)`: filter elements by applying a predicate to the current id.
    HasIdPredicate {
        predicate: Predicate,
    },
    /// `dedup()`: SELECT DISTINCT.
    Dedup,
    /// `dedup("a", "b")` — deduplicate globally by the named path labels
    /// rather than by the current traverser.
    DedupLabels(Vec<String>),
    /// `order()`: order by the current scalar expression (ascending).
    Order,
    /// `range(low, high)`: skip `low` rows, take `high - low`.
    Range {
        low: u64,
        high: u64,
    },
    /// `skip(n)`: drop the first n rows.
    Skip(u64),
    /// `tail(n)`: take the last n rows.
    Tail(u64),
    Limit(u64),
    Count,
    Discard,
    /// `as(label)`: attach a binding label to the current step. Recorded for
    /// downstream `select(label)` lookups; otherwise no plan change.
    As(String),
    /// `select(label)` / `select(Pop, label)` re-projects a previously
    /// labelled element back into the current row. `Pop::Last` picks the
    /// most recent binding (and is the default for the unscoped form);
    /// `Pop::First` picks the earliest. Unbound labels degenerate to
    /// Identity at the planner.
    Select(String, Pop),
    /// `select(Column.keys)` / `select(keys)` and the values equivalent on
    /// a map-shaped traverser.
    SelectColumn(MapColumn),
    /// `select(label1, label2, ...)` — multi-label form. Per traverser,
    /// emits a single Map row keyed by label → bound value. Optional
    /// `by(...)` modulators rotate through the projection list at the
    /// planner (one per label).
    SelectMulti(Vec<String>, Pop),
    /// `asNumber()` / `asString()` / `asBool()` / `asDate()`: cast the
    /// current scalar to the named target type.
    CastScalar(CastTarget),
    /// `dateAdd(unit, amount)`: add a temporal amount to the current date.
    DateAdd {
        unit: String,
        amount: i64,
    },
    /// `dateDiff(date)`: milliseconds between the current date and argument.
    DateDiff(GValue),
    /// `constant(value)`: replace the current scalar with a literal.
    Constant(GValue),
    /// `inject(values...)`: spawn from a literal value list.
    Inject(Vec<GValue>),
    /// `properties(keys...)`: project property objects (one row per matching
    /// (element, key)). Empty `keys` means "all properties".
    Properties(Vec<String>),
    /// `valueMap(keys...)`: project a struct of key-to-value pairs.
    ValueMap(Vec<String>),
    /// `elementMap(keys...)`: like `valueMap` but also includes id/label.
    ElementMap(Vec<String>),
    /// `barrier()`: traverser-barrier marker; compile-time no-op.
    Barrier,
    /// `simplePath()` / `cyclicPath()`: traverser-state filters; compile-time
    /// no-op since we don't track per-traverser paths.
    SimplePath,
    CyclicPath,
    /// Reduction aggregates (`sum`/`min`/`max`/`mean`/`product`) that fold
    /// the entire result set down to one row.
    Aggregate(AggKind),
    /// `group()` / `groupCount()`: GROUP BY the current scalar, then either
    /// collect (group) or count (groupCount).
    Group,
    GroupCount,
    /// `fold()`: collect remaining rows into a single list traverser. We
    /// approximate it as `array_agg(object_value)` returning one row.
    Fold,
    /// `unfold()`: opposite of fold; flatten a list traverser. Approximated
    /// as no-op since our row model is already flat.
    Unfold,
    /// `union(t1, t2, ...)`: for each input traverser, run every sub-
    /// traversal and concatenate results.
    Union(Vec<Vec<Step>>),
    /// `coalesce(t1, t2, ...)`: take the first non-empty sub-traversal per
    /// input. We approximate it as Union for now.
    Coalesce(Vec<Vec<Step>>),
    /// `branch(t).option(k, sub)...` / `choose(t).option(k, sub)...`.
    /// The dispatch traversal `t` is evaluated per input; each option whose
    /// key matches the dispatch value runs against that original input.
    /// `is_choose=true` selects the first matching option (TinkerPop's
    /// `choose` semantics). `is_choose=false` is multi-dispatch (branch).
    BranchOptions {
        dispatch: Vec<Step>,
        options: Vec<TraversalOption>,
        is_choose: bool,
    },
    /// `choose(P, then)` / `choose(P, then, else)` — for each input,
    /// evaluate the predicate against the current scalar; matching inputs
    /// run `then`, non-matching run `else_branch` (or pass through when
    /// no else is supplied). Implemented at the planner as filter+union.
    ChoosePredicate {
        predicate: Predicate,
        then: Vec<Step>,
        else_branch: Option<Vec<Step>>,
    },
    /// `choose(t, then)` / `choose(t, then, else)` — like ChoosePredicate
    /// but the condition is itself a sub-traversal acting as a filter
    /// (it "matches" when running it on the input yields ≥1 row).
    ChooseTraversal {
        condition: Vec<Step>,
        then: Vec<Step>,
        else_branch: Option<Vec<Step>>,
    },
    /// `local(t)`: execute the sub-traversal in a per-traverser scope. We
    /// inline the sub-traversal globally — this is correct for many patterns
    /// where the surrounding aggregation/limit is already global.
    Local(Vec<Step>),
    /// `by(...)` modulator. Lives next to the previous step (Group, Order,
    /// Dedup, ...) and tells it which key/traversal to use. Captured as a
    /// separate step so the planner can lookahead-merge it.
    By(BySpec),
    /// `where(t)` and `filter(t)` (sub-traversal forms): keep input rows
    /// where the sub-traversal yields at least one row.
    WhereTraversal(Vec<Step>),
    /// `not(t)`: keep input rows where the sub-traversal yields nothing.
    NotTraversal(Vec<Step>),
    /// `repeat(t)` — body of a repeat loop. Modulators (`Times`, `Emit`,
    /// `Until`) appear as separate adjacent steps; the planner peephole-merges
    /// them whether they precede or follow this step.
    Repeat(Option<String>, Vec<Step>),
    /// `times(n)` modulator. Captured as its own step so the planner can
    /// peephole-merge it with a neighbouring `Repeat`.
    Times(u64),
    /// `emit()` / `emit(traversal)` modulator on `repeat`. None means
    /// unconditional emission, Some(t) means emit only branches where the
    /// sub-traversal produces ≥1 row.
    Emit(Option<Vec<Step>>),
    /// `until(traversal)` modulator on `repeat`. Branches where the
    /// sub-traversal produces ≥1 row exit the loop and propagate; the rest
    /// continue iterating.
    Until(Vec<Step>),
    /// `coin(p)` — keep each row with probability p. Lowers to a random()
    /// filter at the planner.
    Coin(f64),
    /// String operations applied to the current scalar projection. Each
    /// variant maps to a DataFusion UDF call wrapping the existing scalar
    /// expression.
    StringOp(StringOp),
    /// `aggregate(label)` / `store(label)` — snapshot the current row
    /// stream under the given label so a later `cap(label)` can restore it.
    AggregateAs(String),
    /// `cap(label)` — replace the current row stream with whatever was
    /// snapshotted under `label`.
    Cap(String),
    /// `cap(label1, label2, ...)` — pull multiple named side-effects back
    /// as a map-shaped traverser. Kept distinct from `Cap` so older planner
    /// paths that only understand a single bag can remain conservative.
    CapMulti(Vec<String>),
    /// `sideEffect(t)` — execute t for its side effects, keep current
    /// stream. We approximate as no-op since we don't model side effects.
    SideEffect(Vec<Step>),
    /// `none()` — drop every row (compile to a `WHERE false` filter).
    None,
    /// `project('a', 'b', ...)` — fan each input out to N rows, each tagged
    /// with one of the labels. Following `by(...)` modulators (one per
    /// label) supply the value expression; the planner peephole-merges them.
    Project(Vec<String>),
    /// `match(t1, t2, ...)` — a set of labelled traversal patterns that share
    /// one match environment. Repeated labels are identity constraints.
    Match(Vec<Vec<Step>>),
    /// `loops()` / `loops("name")` — current iteration count of an enclosing
    /// repeat, optionally selected by repeat name.
    Loops(Option<String>),
    /// `path()` — the list of element ids visited along the current branch.
    /// We project a `|`-joined string of every alias.id seen so far.
    Path,
    /// `format(template)` — substitute the current scalar at every
    /// `{N}` / `%s` placeholder. Multi-arg format (different scalars per
    /// placeholder) is approximated with the same scalar everywhere
    /// because we don't track multiple bindings.
    Format(Vec<FormatPart>),
    /// Set / list operation against a literal list. Applied to a folded
    /// list traverser; degenerates to the right-hand list (or a stub) when
    /// the current projection isn't list-typed.
    ListOp(ListOpKind, GValue),
    /// `math(expr)` — arithmetic expression. We only model the simple
    /// `_OP literal` shapes (`_+1`, `_*2`, `_-3`, `_/4`). Anything more
    /// complex falls back to identity at the planner.
    Math(MathExpr),
    /// `hasValue(value, ...)` / `hasValue(P)` — keep elements where ANY
    /// property matches the predicate. We test every property column of
    /// the branch's current label against the predicate via `OR`.
    HasValue(Predicate),
    /// `g.withSack(initial)` / `g.withSack(initial, op)` source-self.
    /// Sets every initial traverser's sack to `initial`; the optional
    /// reducer is recorded for `repeat()` semantics (where merging
    /// children re-folds with the same op).
    WithSack {
        initial: GValue,
        op: Option<SackOp>,
    },
    /// `g.withSideEffect(label, initial)` / `withSideEffect(label, initial, op)`.
    /// Pre-seeds the side-effect bag for `label` with `initial` and, when
    /// `op` is supplied, configures it as a reducer-folded scalar bag —
    /// later `aggregate(label)` writes fold via op against the running
    /// scalar instead of multisetting.
    WithSideEffect {
        label: String,
        initial: GValue,
        op: Option<SackOp>,
    },
    /// `sack()` — read the current sack value as a scalar projection.
    Sack,
    /// `sack(op)` — mutate the per-traverser sack: `sack = op(sack, rhs)`
    /// where `rhs` comes from a following `by(...)` modulator (key or
    /// sub-traversal). With no `by()` the current scalar value is used.
    SackOp(SackOp),
    /// `g.withStrategies(new SubgraphStrategy(vertices: ..., edges: ...))`.
    /// Restricts the visible graph: every V()/out()/in()/both() result
    /// is post-filtered by `vertex_filter`, every edge result (E(), outE(),
    /// inE(), bothE()) by `edge_filter` AND must have both endpoints
    /// passing `vertex_filter` to remain visible.
    WithStrategy {
        vertex_filter: Option<Vec<Step>>,
        edge_filter: Option<Vec<Step>>,
        check_adjacent_vertices: bool,
    },
    /// `g.withStrategies(ProductiveByStrategy)` keeps rows whose `by(...)`
    /// projection is unproductive and surfaces NULL rather than dropping the
    /// traverser. The default Gremlin strategy stack drops those rows.
    WithProductiveByStrategy,
    /// `tree()` / `tree(label)` — collect a path-tree of visited elements.
    /// The optional label, when present, doubles as a side-effect store so
    /// a later `cap(label)` can retrieve it.
    Tree(Option<String>),
    /// `subgraph(label)` — gather every traversed edge into a side-effect
    /// named `label`. The result is a sub-graph view rather than rows.
    Subgraph(String),
    /// `element()` — from a property-object traverser, return the parent
    /// vertex/edge. Planner-side this needs property-object modelling.
    Element,
    /// `propertyMap(keys...)` — like `ValueMap` but the values are property
    /// objects (carry their own keys/labels). Kept distinct so a future
    /// planner can preserve the property-object shape.
    PropertyMap(Vec<String>),
    /// `index()` — emit each list element along with its zero-based index.
    Index,
    /// `fail()` / `fail(message)` — raise an error at runtime. Planner can
    /// short-circuit to an empty result + diagnostic.
    Fail(Option<String>),
    /// `call(name, args...)` — invoke a registered procedure. Planner needs
    /// a procedure registry.
    Call(String, Vec<CallArg>),
    /// Graph algorithms. None are implemented; preserved so the planner can
    /// distinguish them from a generic Identity fallback.
    ShortestPath,
    PageRank,
    PeerPressure,
    ConnectedComponent,
    /// `group(label)` — same semantics as `Group` but additionally stores
    /// the computed map under `label` for later `cap(label)`.
    GroupAs(String),
    /// `groupCount(label)` — like `GroupCount` plus side-effect store under
    /// `label`.
    GroupCountAs(String),
    /// `where("label", P.eq("other"))` / `where("label", P.gt("other"))` —
    /// cross-binding compare. The predicate's right-hand operand is itself
    /// a binding name to dereference, not a literal.
    WhereString {
        label: String,
        predicate: Predicate,
    },
    /// `path().from(label)` / `path().to(label)` — restrict the rendered
    /// path to the slice between the labelled bindings.
    PathFrom(String),
    PathTo(String),
    /// `flatMap(t)` — fan each input out by re-running `t`, flattening the
    /// per-input results. Distinct from `Local` (which keeps per-traverser
    /// scope) and `Map` (which projects 1-to-1).
    FlatMap(Vec<Step>),
    /// `map(t)` — 1-to-1 projection: run `t` per input and replace the
    /// traverser with the (single) result. Different from `Local`/`FlatMap`.
    Map(Vec<Step>),
    /// `sample(n)` — random sample of n traversers (no implicit truncation
    /// to the head, unlike `Limit`).
    Sample(u64),
    /// Scope-local (per-list) variants of common steps. Each operates on
    /// the *current list traverser's elements* rather than on the global
    /// row stream. `tail(Scope.local, n)` etc. The wrapped step is the
    /// global-scope analogue; the planner reuses the same evaluator under
    /// a per-list aggregator.
    LocalScoped(Box<Step>),
    /// `combine(__.traversal)` / `merge(__.traversal)` / etc. — set/list
    /// operation whose right-hand side is the result of a sub-traversal,
    /// not a literal. Captures all of `merge`/`combine`/`intersect`/
    /// `difference`/`disjunct`/`product` traversal forms.
    ListOpTraversal(ListOpKind, Vec<Step>),
    /// `fold(seed, Operator)` — reducer-fold form. The seed is the initial
    /// accumulator; the operator is the binary reducer. Distinct from the
    /// argument-less `Fold` (which collects into a list).
    FoldReduce {
        seed: GValue,
        op: SackOp,
    },
    /// `valueMap(true, ...)` / `.with(WithOptions.tokens, ...)` — variant
    /// that includes selected id/label tokens in the projected map. The
    /// `keys` list is otherwise identical to `ValueMap`.
    ValueMapTokens {
        keys: Vec<String>,
        include_id: bool,
        include_label: bool,
    },
    /// `with(key)` / `with(key, value)` traversal option. Some options are
    /// folded into the preceding step at parse time; the rest are preserved
    /// as compile-friendly no-ops so the AST keeps the user's configuration.
    WithOption {
        key: String,
        value: Option<GValue>,
    },
}
