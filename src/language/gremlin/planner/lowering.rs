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
mod path;
mod predicates;
mod procedures;
mod project;
mod property_object;
mod reduce;
mod repeat;
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

    consume_leading_config(&mut steps, &mut lo);
    let remaining = steps.cloned().collect::<Vec<_>>();

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
                check_adjacent_vertices,
            } => {
                if let Some(vf) = vertex_filter {
                    lo.subgraph_vertex_filter = Some(vf.clone());
                }
                if let Some(ef) = edge_filter {
                    lo.subgraph_edge_filter = Some(ef.clone());
                }
                lo.subgraph_check_adjacent_vertices = *check_adjacent_vertices;
                steps.next();
            }
            Step::WithProductiveByStrategy => {
                lo.productive_by = true;
                steps.next();
            }
            Step::WithSack { initial, .. } => {
                lo.sack_initial = Some(initial.clone());
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
