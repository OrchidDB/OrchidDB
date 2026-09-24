//! Step-by-step lowering of a `Traversal` into Graph IR `Node`s.
//!
//! The lowering keeps Gremlin's `current` traverser model: every
//! intermediate plan exposes a binding called `current` that subsequent
//! steps consume and rewrite. Labelled bindings produced by `as(label)`
//! are preserved alongside the current binding and are reachable via
//! `select(label)` (lowered to `GraphSelect`).
//!
//! The lowering pipeline is split into category modules:
//!
//! - `context`            — `Lowerer` state plus root / child traversal
//!   contracts.
//! - `dispatch`           — the contextful step match that routes each
//!   `Step` to its category handler.
//! - `sources`            — `g.V/E/inject/union(...)` spawns.
//! - `expand`             — `out/in/both`, `outE/inE/bothE`, `outV/inV`.
//! - `filter`             — `has*`, `where*`, `is`, quantifiers,
//!   `simple/cyclic path`, `discard`/`none()`.
//! - `project`            — `values`, `id`, `label`, `identity`,
//!   `constant`.
//! - `select`             — `as`, `select`.
//! - `slice`              — `dedup`, `order`, `range`/`skip`/`tail`/
//!   `limit`/`sample`.
//! - `reduce`             — `count`, `sum/min/max/mean`, `fold`,
//!   `unfold`.
//! - `group`              — `group`, `groupCount`.
//! - `repeat`             — `repeat/times/emit/until`.
//! - `branch`             — `union`, `coalesce`, `choose`, `branch`.
//! - `strings`            — per-element string ops.
//! - `predicates`         — `Predicate → IrExpr`.
//! - `literals`           — `GValue → Lit`/`Value`.
//! - `helpers`            — small label/direction/by/id-filter utilities.
//! - `subgraph_strategy`  — `g.withStrategies(SubgraphStrategy(...))`
//!   post-filters applied at every vertex / edge producer.
//! - `sub_traversal`      — contextful root / correlated child traversal
//!   entry points.
//!
//! Steps that depend on Gremlin features outside the documented Graph IR
//! catalog return `GremlinPlanError::Unsupported` with a message
//! describing the gap; those are handled by Phase 2 work in this
//! directory.

mod branch;
mod casts;
mod context;
mod dispatch;
mod expand;
mod filter;
mod format;
mod group;
mod helpers;
mod list_ops;
mod literals;
mod local_scope;
mod match_step;
mod math;
mod mutations;
mod merge;
mod path;
mod predicates;
mod procedures;
mod project;
mod property_object;
mod reduce;
mod repeat;
mod label_liveness;
mod select;
mod side_effects;
mod slice;
mod sources;
mod strings;
mod sub_traversal;
mod subgraph_strategy;

use crate::ir::plan::{GraphPlan, Node};
use crate::ir::policy::{GraphPlanPolicy, ResultForm};
use crate::language::gremlin::ast::{Step, Traversal};
use crate::language::gremlin::planner::error::GremlinPlanResult;

use context::{CURRENT, Lowerer};
use sub_traversal::lower_source_traversal_with_context;

pub fn lower_traversal(traversal: &Traversal) -> GremlinPlanResult<GraphPlan> {
    let mut lo = Lowerer::new();
    let mut steps = traversal.steps.iter().peekable();

    register_side_effects(&traversal.steps, &mut lo);
    consume_leading_config(&mut steps, &mut lo);
    let remaining = steps.cloned().collect::<Vec<_>>();

    lo.bulk_safe = permits_path_elision(&traversal.steps);
    label_liveness::configure(&traversal.steps, &mut lo);
    let ctx = lo.root_context();
    let node = lo.enter_context(ctx, |lo, ctx| {
        lower_source_traversal_with_context(&remaining, lo, ctx)
    })?;

    let policy = GraphPlanPolicy::gremlin();
    let root = Node::GraphReturn {
        fields: vec![CURRENT.to_string()],
        result_form: ResultForm::TraverserStream,
        input: node.boxed(),
    };
    Ok(GraphPlan::new(policy, root))
}

/// Drain leading source-self configuration steps into the `Lowerer`.
///
/// `WithStrategy` and `WithProductiveByStrategy` are honored at every
/// producer / `by(...)` site below. `WithSack` and `WithSideEffect` are
/// silently consumed for now: the side-effect and sack machinery is its
/// own work-bucket, and dropping the leading step lets the chain at
/// least reach the consumer (`sack()` / `cap(...)` / ...) so the
/// failure surfaces against the *actual* unsupported step.
fn consume_leading_config<'a, I>(steps: &mut std::iter::Peekable<I>, lo: &mut Lowerer)
where
    I: Iterator<Item = &'a Step>,
{
    while let Some(step) = steps.peek() {
        match step {
            Step::WithStrategy {
                vertex_filter,
                edge_filter,
                vertex_property_filter,
                check_adjacent_vertices,
            } => {
                if let Some(vf) = vertex_filter {
                    lo.subgraph_vertex_filter = Some(vf.clone());
                }
                if let Some(ef) = edge_filter {
                    lo.subgraph_edge_filter = Some(ef.clone());
                }
                if let Some(vpf) = vertex_property_filter {
                    lo.subgraph_vertex_property_filter = Some(vpf.clone());
                }
                lo.subgraph_check_adjacent_vertices = *check_adjacent_vertices;
                steps.next();
            }
            Step::WithPartitionWrite { key, value } => {
                lo.partition_write = Some((key.clone(), value.clone()));
                steps.next();
            }
            Step::WithSeedStrategy(seed) => {
                lo.random_seed = Some(*seed);
                steps.next();
            }
            Step::WithProductiveByStrategy => {
                lo.productive_by = true;
                steps.next();
            }
            Step::WithoutPathRetraction => { steps.next(); }
            Step::WithBulk(enabled) => {
                lo.bulk_enabled = *enabled;
                steps.next();
            }
            Step::WithSack { initial, op } => {
                lo.sack_initial = Some(initial.clone());
                lo.sack_merge = *op;
                steps.next();
            }
            Step::WithSideEffect { label, initial, op } => {
                lo.side_effect_seeds.insert(label.clone(), initial.clone());
                if let Some(op) = op {
                    lo.side_effect_reducers
                        .insert(label.clone(), (initial.clone(), *op));
                }
                steps.next();
            }
            Step::WithOption { .. } => {
                steps.next();
            }
            _ => break,
        }
    }
}

/// Prove that no step can observe the complete path. Unknown steps retain it.
/// Labels still participate in traverser identity and are never discarded.
fn permits_path_elision(steps: &[Step]) -> bool {
    let mut steps = steps.iter().peekable();
    while let Some(step) = steps.next() {
        let safe = match step {
            Step::Group | Step::GroupAs(_) | Step::GroupCount | Step::GroupCountAs(_) => {
                // A group can discard incoming path history only when both
                // its key and value traversals depend on the current object.
                // Default fold values remain excluded because order matters.
                let key_safe = if matches!(steps.peek(), Some(Step::By(_))) {
                    let Some(Step::By(key)) = steps.next() else { unreachable!() };
                    key.traversal.as_ref().is_none_or(|body| {
                        body.iter().all(group::current_only_projection)
                    })
                } else {
                    true
                };
                let value_safe = if matches!(step, Step::GroupCount | Step::GroupCountAs(_)) {
                    true
                } else if let Some(Step::By(value)) = steps.next() {
                    value.traversal.as_ref().is_some_and(|body| {
                        group::current_only_reduction(body)
                    })
                } else {
                    false
                };
                key_safe && value_safe
            }
            Step::V { .. } | Step::E { .. } | Step::Inject(_) | Step::ExpandVertex { .. }
            | Step::ExpandEdge { .. } | Step::EndpointVertex { .. } | Step::OtherVertex
            | Step::Has { .. } | Step::HasLabel(_) | Step::HasId { .. } | Step::HasIdPredicate { .. }
            | Step::HasNot { .. } | Step::Identity | Step::Is { .. } | Step::Values(_)
            | Step::Id | Step::Label | Step::As(_) | Step::Select(_, _) | Step::SelectMulti(_, _)
            | Step::Count | Step::Dedup | Step::Times(_) | Step::Loops(_)
            | Step::Limit(_) | Step::Range { .. } | Step::Skip(_) | Step::Tail(_)
            | Step::WithBulk(_) | Step::WithSack { .. } | Step::Sack | Step::SackOp(_)
            | Step::Barrier | Step::NormSackBarrier | Step::Constant(_) | Step::Cap(_) => true,
            Step::Repeat(_, body) | Step::Until(body) | Step::Local(body) => permits_path_elision(body),
            Step::Emit(body) => body.as_ref().is_none_or(|body| permits_path_elision(body)),
            _ => false,
        };
        if !safe { return false; }
    }
    true
}

/// Register shared names before lowering any reader (including readers that
/// precede the writer inside repeat). Registration belongs to the traversal.
fn register_side_effects(steps: &[Step], lo: &mut Lowerer) {
    use crate::language::gremlin::ast::{CallArg, MutationArgument, OptionKey, StringOp};

    fn register_argument(argument: &MutationArgument, lo: &mut Lowerer) {
        if let MutationArgument::Traversal(sub) = argument {
            register_side_effects(sub, lo);
        }
    }

    for step in steps {
        match step {
            Step::AggregateAs(label) | Step::AggregateLocal(label) | Step::Tree(Some(label)) => {
                lo.side_effect_bags.entry(label.clone()).or_insert_with(|| label.clone());
            }
            Step::GroupAs(label) | Step::GroupCountAs(label) => { lo.group_count_side_effects.insert(label.clone()); }
            Step::By(spec) => if let Some(sub) = &spec.traversal { register_side_effects(sub, lo); },
            Step::Union(branches) | Step::Coalesce(branches) | Step::Match(branches) => {
                for sub in branches { register_side_effects(sub, lo); }
            }
            Step::BranchOptions { dispatch, options, .. } => {
                register_side_effects(dispatch, lo);
                for option in options {
                    if let OptionKey::Traversal(key) = &option.key {
                        register_side_effects(key, lo);
                    }
                    register_side_effects(&option.traversal, lo);
                }
            }
            Step::ChoosePredicate { then, else_branch, .. } => {
                register_side_effects(then, lo);
                if let Some(sub) = else_branch { register_side_effects(sub, lo); }
            }
            Step::ChooseTraversal { condition, then, else_branch } => {
                register_side_effects(condition, lo); register_side_effects(then, lo);
                if let Some(sub) = else_branch { register_side_effects(sub, lo); }
            }
            Step::Local(sub) | Step::Map(sub) | Step::FlatMap(sub) | Step::SideEffect(sub)
            | Step::WhereTraversal(sub) | Step::NotTraversal(sub) | Step::Repeat(_, sub)
            | Step::Until(sub) | Step::ListOpTraversal(_, sub) | Step::Emit(Some(sub))
            | Step::PropertyTraversal { traversal: sub, .. }
            | Step::WithOption { traversal: Some(sub), .. }
            | Step::StringOp(StringOp::ConcatTraversal(sub)) => register_side_effects(sub, lo),
            Step::WithStrategy { vertex_filter, edge_filter, vertex_property_filter, .. } => {
                for sub in [vertex_filter, edge_filter, vertex_property_filter].into_iter().flatten() {
                    register_side_effects(sub, lo);
                }
            }
            Step::Call(_, arguments) => {
                for argument in arguments {
                    if let CallArg::Traversal(sub) = argument {
                        register_side_effects(sub, lo);
                    }
                }
            }
            Step::DynamicMerge { criteria, options, .. } => {
                register_argument(criteria, lo);
                for argument in options.values() { register_argument(argument, lo); }
            }
            Step::AddDynamicV { label } => register_argument(label, lo),
            Step::AddDynamicE { label, from, to } => {
                register_argument(label, lo);
                for argument in [from, to].into_iter().flatten() { register_argument(argument, lo); }
            }
            Step::PropertyDynamic { key, value } => {
                register_argument(key, lo);
                register_argument(value, lo);
            }
            Step::LocalScoped(inner) => register_side_effects(std::slice::from_ref(inner.as_ref()), lo),
            _ => {}
        }
    }
}
