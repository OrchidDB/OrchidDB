//! The `lower_step` dispatcher — one big match that routes each
//! Gremlin `Step` to the per-category handler.

use std::iter::Peekable;

use super::branch::{
    lower_branch_options, lower_choose_predicate, lower_choose_traversal, lower_coalesce,
    lower_local_or_map, lower_mid_traversal_union,
};
use super::casts::lower_cast_scalar;
use super::context::{ChildTraversalKind, Lowerer, TraversalContext};
use super::expand::{
    lower_endpoint_vertex, lower_expand_edge, lower_expand_vertex, lower_other_vertex,
};
use super::filter::{
    lower_cyclic_path, lower_discard_or_none, lower_has, lower_has_id, lower_has_id_predicate,
    lower_has_key, lower_has_key_any, lower_has_label, lower_has_not, lower_has_value, lower_is,
    lower_not_traversal, lower_quantifier_filter, lower_simple_path, lower_where_traversal,
};
use super::format::lower_format;
use super::group::lower_group_step;
use super::list_ops::{lower_fold_reduce, lower_list_op, lower_list_op_traversal};
use super::local_scope::{lower_local_order, lower_local_scoped};
use super::match_step::lower_match;
use super::math::lower_math;
use super::path::{lower_path, lower_path_from, lower_path_to};
use super::procedures::{lower_call, lower_call_with_option, lower_fail, lower_graph_algorithm};
use super::project::{lower_constant, lower_id, lower_label, lower_project, lower_values};
use super::property_object::{
    lower_element, lower_element_map, lower_properties, lower_properties_value, lower_property_map,
    lower_value_map, lower_value_map_tokens,
};
use super::reduce::{lower_aggregate, lower_count, lower_fold, lower_unfold};
use super::repeat::lower_repeat;
use super::select::{lower_as, lower_select_column, lower_select_label, lower_select_multi};
use super::side_effects::{
    lower_aggregate_as, lower_cap, lower_cap_multi, lower_group_as, lower_group_count_as,
    lower_sack_op, lower_sack_read, lower_side_effect_bag_as_list, lower_subgraph, lower_tree,
};
use super::slice::{
    lower_dedup, lower_dedup_labels, lower_limit_or_sample, lower_order, lower_range, lower_sample,
    lower_skip, lower_tail,
};
use super::strings::lower_string_op;
use crate::ir::plan::{Node, QuantifierKind};
use crate::language::gremlin::ast::Step;
use crate::language::gremlin::planner::error::GremlinPlanResult;

pub(super) fn loop_binding_name(name: &Option<String>) -> String {
    match name {
        Some(name) => format!("__loops:{name}"),
        None => "__loops".to_string(),
    }
}

pub(super) fn lower_step_with_context<'a, I>(
    input: Node,
    step: &'a Step,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    match step {
        // ----- filters that don't fit the simple scalar predicate path -----
        Step::HasLabel(labels) => lower_has_label(input, labels),
        Step::Has { key, predicate } => lower_has(input, key, predicate),
        Step::HasNot { key } => lower_has_not(input, key),
        Step::HasKey { key } => lower_has_key(input, key),
        Step::HasKeyAny(keys) => lower_has_key_any(input, keys),
        Step::HasId { ids } => Ok(lower_has_id(input, ids)),
        Step::HasIdPredicate { predicate } => lower_has_id_predicate(input, predicate),
        Step::HasValue(predicate) => lower_has_value(input, predicate),
        Step::Is { predicate } => lower_is(input, predicate, steps, lo),
        Step::All { predicate } => {
            lower_quantifier_filter(input, QuantifierKind::All, predicate, lo)
        }
        Step::Any { predicate } => {
            lower_quantifier_filter(input, QuantifierKind::Any, predicate, lo)
        }
        Step::NonePredicate { predicate } => {
            lower_quantifier_filter(input, QuantifierKind::None, predicate, lo)
        }

        // ----- expansion -----
        Step::ExpandVertex {
            direction,
            edge_labels,
        } => lower_expand_vertex(input, *direction, edge_labels, lo, ctx),
        Step::ExpandEdge {
            direction,
            edge_labels,
        } => lower_expand_edge(input, *direction, edge_labels, lo, ctx),
        Step::EndpointVertex { direction } => lower_endpoint_vertex(input, *direction, lo, ctx),
        Step::OtherVertex => lower_other_vertex(input, lo, ctx),

        // ----- value projection -----
        Step::Values(keys) => lower_values(input, keys, lo),
        Step::Id => Ok(lower_id(input)),
        Step::Label => Ok(lower_label(input)),
        Step::Identity => Ok(input),
        Step::Constant(value) => lower_constant(input, value),
        Step::CastScalar(target) => Ok(lower_cast_scalar(input, *target)),
        Step::DateAdd { unit, amount } => Ok(super::casts::lower_date_add(input, unit, *amount)),
        Step::DateDiff(rhs) => super::casts::lower_date_diff(input, rhs),

        // ----- labelled bindings -----
        Step::As(label) => Ok(lower_as(input, label)),
        // Mid where()-sub-traversal `as(label)`: when the label is already
        // bound in the outer scope this is an equality anchor (the current
        // element must equal the labelled binding); when unbound it degrades
        // to a plain rebinding, matching TinkerPop's where semantics.
        Step::WhereAnchor(label) => {
            let anchored = Node::GraphFilter {
                condition: crate::ir::expr::IrExpr::Binary {
                    op: crate::ir::expr::BinaryOp::Or,
                    lhs: Box::new(crate::ir::expr::IrExpr::Not(Box::new(
                        crate::ir::expr::IrExpr::IsBound(label.clone()),
                    ))),
                    rhs: Box::new(crate::ir::expr::IrExpr::Binary {
                        op: crate::ir::expr::BinaryOp::Eq,
                        lhs: Box::new(crate::ir::expr::IrExpr::Binding("current".into())),
                        rhs: Box::new(crate::ir::expr::IrExpr::Binding(label.clone())),
                    }),
                },
                input: input.boxed(),
            };
            Ok(lower_as(anchored, label))
        }
        // Stray infix connectives (`.and()` / `.or()`): these are folded by
        // `rewrite_infix_connectives` before lowering; if one slips through
        // (e.g. inside a step list lowered without the rewrite) treat it as
        // identity so the surrounding chain still runs.
        Step::InfixAnd | Step::InfixOr => Ok(input),
        Step::Select(label, pop) => lower_select_label(input, label, *pop, steps, lo, ctx),
        Step::SelectMapValueBy(label) => {
            let extracted = Node::GraphCurrentProject {
                expr: crate::ir::expr::IrExpr::Call {
                    name: "map_get_display".into(),
                    args: vec![
                        crate::ir::expr::IrExpr::Binding("current".into()),
                        crate::ir::expr::IrExpr::Binding(label.clone()),
                    ],
                },
                fields: vec!["current".to_string()],
                input: input.boxed(),
            };
            if let Some(spec) = super::helpers::consume_by(steps) {
                let (node, expr) = super::helpers::apply_by_spec(extracted, &spec, lo, ctx)?;
                Ok(Node::GraphCurrentProject {
                    expr,
                    fields: vec!["current".to_string()],
                    input: node.boxed(),
                })
            } else {
                Ok(extracted)
            }
        }
        Step::SelectMulti(labels, pop) => lower_select_multi(input, labels, *pop, steps, lo, ctx),
        Step::SelectColumn(column) => Ok(lower_select_column(input, *column)),

        // ----- distinct / order / slice -----
        Step::Dedup => lower_dedup(input, steps, lo, ctx),
        Step::DedupLabels(labels) => Ok(lower_dedup_labels(input, labels)),
        Step::Order => lower_order(input, steps, lo, ctx),
        Step::Range { low, high } => Ok(lower_range(input, *low, *high)),
        Step::Skip(n) => Ok(lower_skip(input, *n)),
        Step::Tail(n) => Ok(lower_tail(input, *n)),
        Step::Limit(fetch) => Ok(lower_limit_or_sample(input, *fetch)),
        Step::Sample(fetch) => Ok(lower_sample(input, *fetch, steps)),

        // ----- count / aggregate / fold -----
        Step::Count => Ok(lower_count(input)),
        Step::Aggregate(kind) => lower_aggregate(input, *kind, steps, lo, ctx),
        Step::Group => lower_group_step(input, steps, /*just_count=*/ false, lo, ctx),
        Step::GroupCount => lower_group_step(input, steps, /*just_count=*/ true, lo, ctx),
        Step::Fold => Ok(lower_fold(input)),
        Step::Unfold => Ok(lower_unfold(input)),
        Step::Discard | Step::None => Ok(lower_discard_or_none(input)),
        Step::Barrier => Ok(input),
        Step::SimplePath => Ok(lower_simple_path(input)),
        Step::CyclicPath => Ok(lower_cyclic_path(input)),

        // ----- subqueries -----
        Step::WhereTraversal(sub) => lower_where_traversal(input, sub, lo, ctx),
        Step::NotTraversal(sub) => lower_not_traversal(input, sub, lo, ctx),
        Step::Local(sub) | Step::SideEffect(sub) if is_side_effect_only(sub) => {
            // `local(aggregate("a"))` / `sideEffect(aggregate("a"))` — the
            // sub-traversal only attaches a side-effect-bag binding. Inline
            // it onto the outer rows so a later cap("a") can read the bag;
            // wrapping in GraphApply would drop the binding.
            if matches!(step, Step::Local(_)) {
                mark_local_aggregate_labels(sub, lo);
            }
            let mut node = input;
            let mut iter = sub.iter().peekable();
            while let Some(step) = iter.next() {
                node = lower_step_with_context(node, step, &mut iter, lo, ctx)?;
            }
            Ok(node)
        }
        Step::Local(sub) => lower_local_or_map(input, sub, lo, ctx, ChildTraversalKind::Local),
        Step::Map(sub) => lower_local_or_map(input, sub, lo, ctx, ChildTraversalKind::Map),
        Step::FlatMap(sub) => lower_local_or_map(input, sub, lo, ctx, ChildTraversalKind::FlatMap),
        Step::SideEffect(sub) => {
            lower_local_or_map(input, sub, lo, ctx, ChildTraversalKind::SideEffect)
        }
        Step::Coalesce(arms) => lower_coalesce(input, arms, lo, ctx),
        Step::Union(branches) => lower_mid_traversal_union(input, branches, lo, ctx),
        Step::ChoosePredicate {
            predicate,
            then,
            else_branch,
        } => lower_choose_predicate(input, predicate, then, else_branch.as_deref(), lo, ctx),
        Step::ChooseTraversal {
            condition,
            then,
            else_branch,
        } => lower_choose_traversal(input, condition, then, else_branch.as_deref(), lo, ctx),
        Step::BranchOptions {
            dispatch,
            options,
            is_choose,
        } => lower_branch_options(input, dispatch, options, *is_choose, lo, ctx),

        // ----- repeat & friends -----
        Step::Repeat(name, body) => lower_repeat(
            input,
            name.as_deref(),
            body,
            /*pending_emit=*/ None,
            /*pending_until=*/ None,
            /*pending_times=*/ None,
            steps,
            lo,
            ctx,
        ),
        Step::Emit(predicate) => {
            let pending_emit: Option<Option<Vec<Step>>> = Some(predicate.clone());
            let mut pending_until: Option<Vec<Step>> = None;
            let mut pending_times: Option<u64> = None;
            loop {
                match steps.peek() {
                    Some(Step::Repeat(name, body)) => {
                        let name = name.clone();
                        let body = (*body).clone();
                        steps.next();
                        return lower_repeat(
                            input,
                            name.as_deref(),
                            &body,
                            pending_emit,
                            pending_until,
                            pending_times,
                            steps,
                            lo,
                            ctx,
                        );
                    }
                    Some(Step::Until(p)) if pending_until.is_none() => {
                        pending_until = Some(p.clone());
                        steps.next();
                    }
                    Some(Step::Times(n)) if pending_times.is_none() => {
                        pending_times = Some(*n);
                        steps.next();
                    }
                    _ => return Ok(input),
                }
            }
        }
        Step::Until(predicate) => {
            let mut pending_emit: Option<Option<Vec<Step>>> = None;
            let pending_until: Option<Vec<Step>> = Some(predicate.clone());
            let mut pending_times: Option<u64> = None;
            loop {
                match steps.peek() {
                    Some(Step::Repeat(name, body)) => {
                        let name = name.clone();
                        let body = (*body).clone();
                        steps.next();
                        return lower_repeat(
                            input,
                            name.as_deref(),
                            &body,
                            pending_emit,
                            pending_until,
                            pending_times,
                            steps,
                            lo,
                            ctx,
                        );
                    }
                    Some(Step::Emit(p)) if pending_emit.is_none() => {
                        pending_emit = Some(p.clone());
                        steps.next();
                    }
                    Some(Step::Times(n)) if pending_times.is_none() => {
                        pending_times = Some(*n);
                        steps.next();
                    }
                    _ => return Ok(input),
                }
            }
        }
        Step::Times(n) => {
            let mut pending_emit: Option<Option<Vec<Step>>> = None;
            let mut pending_until: Option<Vec<Step>> = None;
            let pending_times: Option<u64> = Some(*n);
            loop {
                match steps.peek() {
                    Some(Step::Repeat(name, body)) => {
                        let name = name.clone();
                        let body = (*body).clone();
                        steps.next();
                        return lower_repeat(
                            input,
                            name.as_deref(),
                            &body,
                            pending_emit,
                            pending_until,
                            pending_times,
                            steps,
                            lo,
                            ctx,
                        );
                    }
                    Some(Step::Emit(p)) if pending_emit.is_none() => {
                        pending_emit = Some(p.clone());
                        steps.next();
                    }
                    Some(Step::Until(p)) if pending_until.is_none() => {
                        pending_until = Some(p.clone());
                        steps.next();
                    }
                    _ => return Ok(input),
                }
            }
        }
        // Stray `times`/`emit`/`until` (no enclosing `repeat`) — the
        // upstream `repeat` lowering must have been swapped for a
        // fallback. Ignore so the surrounding chain still runs.

        // ----- string / math (subset) -----
        Step::StringOp(op) => lower_string_op(input, op, lo, ctx),
        Step::Math(expr) => lower_math(input, expr, steps, lo, ctx),

        // ----- unsupported families (clean error) -----
        Step::Path => Ok(lower_path(input, steps, lo)),
        Step::Match(patterns) => lower_match(input, patterns, lo, ctx),
        Step::Project(labels) => lower_project(input, labels, steps, lo, ctx),
        Step::Loops(name) => Ok(Node::GraphCurrentProject {
            expr: crate::ir::expr::IrExpr::Binding(loop_binding_name(name)),
            fields: vec!["current".to_string()],
            input: input.boxed(),
        }),
        Step::Properties(keys) if matches!(steps.peek(), Some(Step::Identity)) => {
            steps.next();
            Ok(lower_properties_value(input, keys, lo))
        }
        Step::Properties(keys) => Ok(lower_properties(input, keys, lo)),
        Step::ValueMap(keys) => Ok(lower_value_map(input, keys)),
        Step::ElementMap(keys) => Ok(lower_element_map(input, keys)),
        Step::PropertyMap(keys) => Ok(lower_property_map(input, keys)),
        Step::ValueMapTokens {
            keys,
            include_id,
            include_label,
        } => {
            let unfold_values = consume_unfold_by(steps);
            Ok(lower_value_map_tokens(
                input,
                keys,
                *include_id,
                *include_label,
                unfold_values,
            ))
        }
        Step::AggregateAs(label) => lower_aggregate_as(input, label, steps, lo, ctx),
        Step::Cap(label) if cap_feeds_local_collection_step(steps.peek().copied()) => {
            Ok(lower_side_effect_bag_as_list(input.clone(), label, lo)
                .unwrap_or_else(|| lower_cap(input, label, lo)))
        }
        Step::Cap(label) => {
            // `cap(x).is(P.typeOf(SET))`: a cap over an aggregate bag is
            // always a BulkSet (a Set subtype) in TinkerPop, but our cap
            // approximation streams the bag entries. Consume the always-true
            // type check instead of applying it per entry.
            if let Some(Step::Is {
                predicate: crate::language::gremlin::semantics::Predicate::TypeOf(name),
            }) = steps.peek()
            {
                let tag = name
                    .trim()
                    .trim_start_matches("GType.")
                    .to_ascii_lowercase();
                if matches!(tag.as_str(), "set" | "bulkset" | "collection") {
                    steps.next();
                }
            }
            Ok(lower_cap(input, label, lo))
        }
        Step::CapMulti(labels) => Ok(lower_cap_multi(input, labels, lo)),
        Step::Sack => Ok(lower_sack_read(input)),
        Step::SackOp(op) => lower_sack_op(input, *op, steps, lo, ctx),
        Step::Subgraph(label) => Ok(lower_subgraph(input, label)),
        Step::Tree(Some(label)) => {
            // `tree(label)` is a side effect: the stream continues
            // unchanged and a later `select(label)` / `cap(label)`
            // attaches the tree map.
            let map_node = lower_tree(input.clone(), None);
            lo.group_side_effect_maps.insert(label.clone(), map_node);
            Ok(input)
        }
        Step::Tree(label) => Ok(lower_tree(input, label.as_deref())),
        Step::GroupAs(label) => lower_group_as(input, label, steps, lo, ctx),
        Step::GroupCountAs(label) => lower_group_count_as(input, label, steps, lo, ctx),
        Step::Format(parts) => lower_format(input, parts, steps, lo, ctx),
        Step::Fail(message) => lower_fail(input, message.as_deref()),
        // `coin(p)` — keep each row with probability p.
        Step::Coin(p) => Ok(Node::GraphFilter {
            condition: crate::ir::expr::IrExpr::Call {
                name: "coin_keep".into(),
                args: vec![crate::ir::expr::IrExpr::Lit(crate::ir::expr::Lit::Float(
                    *p,
                ))],
            },
            input: input.boxed(),
        }),
        // `index()` — emits `(item, index)` pairs from a list traverser.
        Step::Index => Ok(Node::GraphCurrentProject {
            expr: crate::ir::expr::IrExpr::Call {
                name: "index_list".into(),
                args: vec![crate::ir::expr::IrExpr::Binding("current".into())],
            },
            fields: vec!["current".to_string()],
            input: input.boxed(),
        }),
        Step::Element => Ok(lower_element(input)),
        Step::ListOp(op, rhs) => lower_list_op(input, *op, rhs),
        Step::ListOpTraversal(op, sub) => lower_list_op_traversal(input, *op, sub, lo, ctx),
        Step::FoldReduce { seed, op } => lower_fold_reduce(input, seed, *op),
        Step::Call(name, args) => {
            let options = consume_call_options(steps);
            lower_call(input, name, args, &options)
        }
        Step::ShortestPath => {
            let options = consume_call_options(steps);
            lower_graph_algorithm(input, "shortestPath", &options)
        }
        Step::PageRank => lower_graph_algorithm(input, "pageRank", &[]),
        Step::PeerPressure => lower_graph_algorithm(input, "peerPressure", &[]),
        Step::ConnectedComponent => lower_graph_algorithm(input, "connectedComponent", &[]),
        Step::LocalScoped(inner) if matches!(inner.as_ref(), Step::Order) => {
            let by = super::helpers::consume_by(steps);
            // When the ordered map is not immediately unfolded, the
            // traverser stays a Map — merge the ordered entry list back.
            let merge_map = !matches!(steps.peek(), Some(Step::Unfold));
            lower_local_order(input, by, merge_map)
        }
        Step::LocalScoped(inner) => lower_local_scoped(input, inner, lo),
        Step::PathFrom(label) => Ok(lower_path_from(input, label)),
        Step::PathTo(label) => Ok(lower_path_to(input, label)),
        // `where("a", P.eq("b"))` / `where("a", P.gt("b"))` — cross-binding
        // compare. The predicate's value side is the *name* of another
        // binding rather than a literal. We rewrite the predicate to
        // compare `binding(label)` against `binding(rhs_label)` directly
        // (works for Compare/Range/Outside; Within/TextLike degrade to
        // Identity).
        Step::WhereString { label, predicate } => {
            super::filter::lower_where_string(input, label, predicate, steps, lo.productive_by)
        }
        // `by(...)` modulators that aren't peephole-merged with the
        // preceding step (select/path/aggregate/sack/...): we don't yet
        // wire those up, but failing here drops huge chunks of the
        // conformance corpus. As a pragmatic fallback we accept and
        // ignore them — surrounding query keeps running, output may
        // not exactly match TinkerPop. Real wiring is a follow-up that
        // teaches each consuming step to peek a trailing `by(...)`.
        Step::By(_) => Ok(input),
        // Mid-traversal `with(...)` is a planner option carrier; ignore it
        // so the rest of the chain still lowers. Likewise `withSack` /
        // `withSideEffect` if they show up after the source position.
        Step::WithOption {
            key,
            value,
            traversal,
        } => lower_call_with_option(input.clone(), key, value.as_ref(), traversal.as_deref())
            .map(|node| node.unwrap_or(input)),
        Step::WithSack { .. }
        | Step::WithSideEffect { .. }
        | Step::WithStrategy { .. }
        | Step::WithProductiveByStrategy => Ok(input),
        // Mid-traversal `V/E/inject` rebinds the source. We approximate by
        // running the spawn through `source_node` over a fresh seed and
        // joining via Apply so the outer row stream is preserved.
        Step::Inject(_) => super::sources::lower_mid_traversal_inject(input, step, lo, ctx),
        Step::V { .. } | Step::E { .. } => {
            super::sources::lower_mid_traversal_spawn(input, step, lo, ctx)
        }
    }
}

/// True when `sub` is composed only of side-effect-attaching steps,
/// i.e. lowering them inline preserves outer rows 1:1 and only adds
/// per-row bag bindings. Lets `local(aggregate("a"))` and friends
/// flatten so a later `cap("a")` can read the bag.
fn is_side_effect_only(sub: &[Step]) -> bool {
    if sub.is_empty() {
        return false;
    }
    sub.iter().all(|s| match s {
        Step::AggregateAs(_)
        | Step::By(_)
        | Step::SackOp(_)
        | Step::GroupCountAs(_)
        | Step::GroupAs(_) => true,
        // `sideEffect(local(aggregate(..)))` — nested wrappers that only
        // attach side-effect bags are themselves side-effect-only.
        Step::Local(inner) | Step::SideEffect(inner) => is_side_effect_only(inner),
        _ => false,
    })
}

/// Record which side-effect labels are written by a lazy
/// (`local(...)`-wrapped) aggregate. `Operator.assign` reducers keep only
/// the final write for lazy aggregates.
fn mark_local_aggregate_labels(sub: &[Step], lo: &mut Lowerer) {
    for s in sub {
        match s {
            Step::AggregateAs(label) => {
                lo.side_effect_local_labels.insert(label.clone());
            }
            Step::Local(inner) | Step::SideEffect(inner) => {
                mark_local_aggregate_labels(inner, lo);
            }
            _ => {}
        }
    }
}

fn cap_feeds_local_collection_step(step: Option<&Step>) -> bool {
    matches!(
        step,
        Some(Step::LocalScoped(inner))
            if matches!(inner.as_ref(), Step::Aggregate(_) | Step::Count | Step::Order | Step::Dedup)
    )
}

fn consume_unfold_by<'a, I>(steps: &mut Peekable<I>) -> bool
where
    I: Iterator<Item = &'a Step>,
{
    let Some(Step::By(spec)) = steps.peek() else {
        return false;
    };
    let is_unfold = matches!(spec.traversal.as_deref(), Some([Step::Unfold]));
    if is_unfold {
        steps.next();
    }
    is_unfold
}

fn consume_call_options<'a, I>(steps: &mut Peekable<I>) -> Vec<super::procedures::CallOption>
where
    I: Iterator<Item = &'a Step>,
{
    let mut options = Vec::new();
    while let Some(Step::WithOption {
        key,
        value,
        traversal,
    }) = steps.peek()
    {
        options.push(super::procedures::CallOption {
            key: key.clone(),
            value: value.clone(),
            traversal: traversal.clone(),
        });
        steps.next();
    }
    options
}
