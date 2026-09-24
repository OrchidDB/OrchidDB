//! Filter steps: `has*`, `where*`, `is`, `all`, `any`, `none(P)`,
//! `simplePath`, `cyclicPath`, `discard`/`none()`, plus the small
//! `lower_quantifier_filter` helper used by all-of / any-of / none-of.

use super::context::{CURRENT, ChildTraversalKind, Lowerer, PATH, TraversalContext};
use std::iter::Peekable;

use super::helpers::{any_label, consume_by, element_token_filter, filter_by_ids, or_chain};
use super::literals::gvalue_to_expr;
use super::predicates::{predicate_to_expr, predicate_to_expr_with_bindings};
use super::sub_traversal::lower_child_traversal;
use crate::ir::expr::IrExpr;
use crate::ir::plan::{ApplyKind, Node, PathFilterScope, QuantifierKind};
use crate::ir::policy::{OptionalMissing, PropertyMissing};
use crate::language::gremlin::ast::{BySpec, Step};
use crate::language::gremlin::planner::error::GremlinPlanResult;
use crate::language::gremlin::semantics::{GValue, Predicate};

pub(super) fn lower_has_label(input: Node, labels: &[String]) -> GremlinPlanResult<Node> {
    Ok(Node::GraphFilter {
        condition: any_label(CURRENT, labels),
        input: input.boxed(),
    })
}

pub(super) fn lower_has(input: Node, key: &str, predicate: &Predicate) -> GremlinPlanResult<Node> {
    let candidate="__gremlin_has_property_value";
    let values=IrExpr::Call{name:"requested_property_values".into(),args:vec![IrExpr::Binding(CURRENT.into()),IrExpr::List(vec![IrExpr::lit_str(key)])]};
    let matches=IrExpr::ListFilter{list:Box::new(values),item:candidate.into(),predicate:Box::new(predicate_to_expr(IrExpr::Binding(candidate.into()),predicate)?)};
    Ok(Node::GraphFilter{condition:IrExpr::Binary{op:crate::ir::expr::BinaryOp::Gt,lhs:Box::new(IrExpr::Call{name:"size".into(),args:vec![matches]}),rhs:Box::new(IrExpr::lit_int(0))},input:input.boxed()})
}

pub(super) fn lower_has_not(input: Node, key: &str) -> GremlinPlanResult<Node> {
    Ok(Node::GraphFilter {
        condition: property_presence_expr(key, false),
        input: input.boxed(),
    })
}

/// `hasKey(k)` (and the single-arg `has(k)`) keeps rows whose current
/// value carries `k`: either an element with property `k`, or a
/// Property-stream row (the `{key, value}` map produced by
/// `properties()`) whose `"key"` field equals `k`. We OR the two
/// checks so the step works in both contexts.
pub(super) fn lower_has_key(input: Node, key: &str) -> GremlinPlanResult<Node> {
    Ok(Node::GraphFilter {
        condition: has_key_expr(key),
        input: input.boxed(),
    })
}

pub(super) fn lower_has_key_any(input: Node, keys: &[String]) -> GremlinPlanResult<Node> {
    let parts = keys.iter().map(|k| has_key_expr(k)).collect::<Vec<_>>();
    Ok(Node::GraphFilter {
        condition: or_chain(parts),
        input: input.boxed(),
    })
}

pub(super) fn property_presence_expr(key: &str, present: bool) -> IrExpr {
    IrExpr::Binary {
        op: if present { crate::ir::expr::BinaryOp::Gt } else { crate::ir::expr::BinaryOp::Eq },
        lhs: Box::new(IrExpr::Call { name: "size".into(), args: vec![IrExpr::Call {
            name: "requested_property_values".into(),
            args: vec![IrExpr::Binding(CURRENT.into()), IrExpr::List(vec![IrExpr::lit_str(key)])],
        }] }),
        rhs: Box::new(IrExpr::lit_int(0)),
    }
}

fn has_key_expr(key: &str) -> IrExpr {
    let element_match = property_presence_expr(key, true);
    let property_map_match = IrExpr::Binary {
        op: crate::ir::expr::BinaryOp::Eq,
        lhs: Box::new(IrExpr::property(
            CURRENT,
            "key".to_string(),
            PropertyMissing::NullOnMissing,
        )),
        rhs: Box::new(IrExpr::lit_str(key.to_string())),
    };
    IrExpr::Binary {
        op: crate::ir::expr::BinaryOp::Or,
        lhs: Box::new(property_map_match),
        rhs: Box::new(element_match),
    }
}

pub(super) fn lower_has_id(input: Node, ids: &[GValue]) -> Node {
    // Flatten any list-shaped ids and drop nulls; if everything is
    // null/empty after flattening, the filter matches nothing.
    fn flatten(values: &[GValue], out: &mut Vec<GValue>) {
        for v in values {
            match v {
                GValue::List(items) | GValue::Set(items) => flatten(items, out),
                GValue::Null => {}
                other => out.push(other.clone()),
            }
        }
    }
    if ids.is_empty() {
        return input;
    }
    let mut flat = Vec::new();
    flatten(ids, &mut flat);
    if flat.is_empty() {
        // `hasId(null)` / `hasId(P.eq(null))` / `hasId([])` should
        // discard every row.
        return Node::GraphFilter {
            condition: IrExpr::lit_bool(false),
            input: input.boxed(),
        };
    }
    filter_by_ids(input, &flat)
}

pub(super) fn lower_has_id_predicate(
    input: Node,
    predicate: &Predicate,
) -> GremlinPlanResult<Node> {
    if let Predicate::Compare {
        op,
        value: GValue::String(token),
    } = predicate
    {
        if let Some(condition) = element_token_filter(CURRENT, token).or_else(|| Some(IrExpr::Binary {
            op: crate::ir::expr::BinaryOp::Eq,
            lhs: Box::new(IrExpr::Call {name:"cast_string".into(),args:vec![IrExpr::Call {name:"gremlin_id".into(),args:vec![IrExpr::Binding(CURRENT.into())]}]}),
            rhs: Box::new(IrExpr::lit_str(token.clone())),
        })) {
            let condition = match op {
                crate::language::gremlin::semantics::CompareOp::Eq => condition,
                crate::language::gremlin::semantics::CompareOp::Neq => {
                    IrExpr::Not(Box::new(condition))
                }
                _ => predicate_to_expr(
                    IrExpr::Call {
                        name: "gremlin_id".into(),
                        args: vec![IrExpr::Binding(CURRENT.into())],
                    },
                    predicate,
                )?,
            };
            return Ok(Node::GraphFilter {
                condition,
                input: input.boxed(),
            });
        }
    }
    Ok(Node::GraphFilter {
        condition: predicate_to_expr(
                    IrExpr::Call {
                        name: "gremlin_id".into(),
                        args: vec![IrExpr::Binding(CURRENT.into())],
                    },
                    predicate,
                )?,
        input: input.boxed(),
    })
}

/// `hasValue(P)` — keep elements where ANY property matches the
/// predicate. A correct implementation enumerates the catalog row's
/// properties at runtime; for now we delegate to a runtime helper that
/// inspects the bound element's full property bag.
pub(super) fn lower_has_value(input: Node, predicate: &Predicate) -> GremlinPlanResult<Node> {
    use super::predicates::predicate_to_expr;
    // `hasValue(P)` is a Property-stream filter (after `.properties()`),
    // where current is a `{key, value}` map: apply the predicate to
    // `current["value"]`.
    let cond = predicate_to_expr(
        IrExpr::property(CURRENT, "value".to_string(), PropertyMissing::NullOnMissing),
        predicate,
    )?;
    Ok(Node::GraphFilter {
        condition: cond,
        input: input.boxed(),
    })
}

pub(super) fn lower_is<'a, I>(
    input: Node,
    predicate: &Predicate,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let by = consume_by(steps);
    // `where(P.within("x"))` referencing an aggregate side-effect bag:
    // attach the shared bag as a hidden binding so membership sees all
    // values written so far. Correlate the read to this parent without
    // replaying the upstream traversal.
    let mut input = input;
    let mut bag_refs: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::new();
    for label in predicate_string_refs(predicate) {
        if bag_refs.contains_key(&label) || !lo.side_effect_bags.contains_key(&label) {
            continue;
        }
        if let Some(bag_plan) = super::side_effects::lower_side_effect_bag_as_list(
            Node::GraphCorrelate { bindings: vec![] },
            &label,
            lo,
        ) {
            let alias = lo.fresh("bag_ref");
            let right = Node::GraphProject {
                mode: crate::ir::plan::ProjectMode::PreserveVisible,
                items: vec![crate::ir::plan::ProjectionItem {
                    alias: alias.clone(),
                    expr: IrExpr::Binding(CURRENT.into()),
                }],
                error_policy: crate::ir::plan::ProjectErrorPolicy::PropagateError,
                input: bag_plan.boxed(),
            };
            input = Node::GraphApply {
                kind: crate::ir::plan::ApplyKind::Scalar,
                correlation: Vec::new(),
                outputs: vec![alias.clone()],
                optional_missing: crate::ir::policy::OptionalMissing::Null,
                left: input.boxed(),
                right: right.boxed(),
            };
            bag_refs.insert(label, alias);
        }
    }
    let target = binding_by_expr(CURRENT, by.as_ref());
    let condition = predicate_to_expr_with_bindings(target, predicate, &|label| {
        if let Some(alias) = bag_refs.get(label) {
            return Some(IrExpr::Binding(alias.clone()));
        }
        if let Some(by) = by.as_ref() {
            return Some(binding_by_expr(label, Some(by)));
        }
        if let Some(seed) = lo.side_effect_seeds.get(label) {
            return gvalue_to_expr(seed).ok();
        }
        None
    })?;
    Ok(Node::GraphFilter {
        condition,
        input: input.boxed(),
    })
}

/// String values referenced anywhere inside a predicate tree (candidate
/// binding / side-effect-bag names).
fn predicate_string_refs(predicate: &Predicate) -> Vec<String> {
    fn value_refs(value: &GValue, out: &mut Vec<String>) {
        match value {
            GValue::String(s) => out.push(s.clone()),
            GValue::List(items) | GValue::Set(items) => {
                for item in items {
                    value_refs(item, out);
                }
            }
            _ => {}
        }
    }
    fn walk(p: &Predicate, out: &mut Vec<String>) {
        match p {
            Predicate::Compare { value, .. } => value_refs(value, out),
            Predicate::Within(values) | Predicate::Without(values) => {
                for v in values {
                    value_refs(v, out);
                }
            }
            Predicate::And(a, b) | Predicate::Or(a, b) => {
                walk(a, out);
                walk(b, out);
            }
            Predicate::Not(inner) => walk(inner, out),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(predicate, &mut out);
    out
}

pub(super) fn lower_quantifier_filter(
    input: Node,
    kind: QuantifierKind,
    predicate: &Predicate,
    lo: &mut Lowerer,
) -> GremlinPlanResult<Node> {
    let item = lo.fresh("q_item");
    let output = lo.fresh("q_pass");
    let predicate = predicate_to_expr(IrExpr::Binding(item.clone()), predicate)?;
    Ok(Node::GraphFilter {
        condition: IrExpr::Binding(output.clone()),
        input: Node::GraphQuantifier {
            kind,
            item_binding: item,
            input_expr: IrExpr::Binding(CURRENT.into()),
            predicate,
            output,
            input: input.boxed(),
        }
        .boxed(),
    })
}

pub(super) fn lower_simple_path(input: Node) -> Node {
    Node::GraphPathFilter {
        condition: IrExpr::SimplePath(PATH.into()),
        scope: PathFilterScope::FinalPath,
        input: input.boxed(),
    }
}

pub(super) fn lower_cyclic_path(input: Node) -> Node {
    Node::GraphPathFilter {
        condition: IrExpr::Not(Box::new(IrExpr::SimplePath(PATH.into()))),
        scope: PathFilterScope::FinalPath,
        input: input.boxed(),
    }
}

pub(super) fn lower_discard_or_none(input: Node) -> Node {
    Node::GraphFilter {
        condition: IrExpr::lit_bool(false),
        input: input.boxed(),
    }
}

pub(super) fn lower_where_traversal(
    input: Node,
    sub: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    let sub = anchor_where_labels(sub);
    Ok(Node::GraphApply {
        kind: ApplyKind::Semi,
        correlation: vec![CURRENT.into()],
        outputs: Vec::new(),
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: lower_child_traversal(&sub, lo, ctx, ChildTraversalKind::WherePredicate)?.boxed(),
    })
}

/// TinkerPop `where(t)` semantics treat non-leading `as(label)` steps as
/// equality anchors against the outer binding (end-step labels), not as
/// rebindings. Rewrite them to `WhereAnchor` so the lowering emits the
/// equality filter; the leading `as` (start anchor) is handled by
/// `prefer_existing_label_as_current`.
fn anchor_where_labels(sub: &[Step]) -> Vec<Step> {
    sub.iter()
        .enumerate()
        .map(|(idx, step)| match step {
            Step::As(label) if idx > 0 => Step::WhereAnchor(label.clone()),
            other => other.clone(),
        })
        .collect()
}

/// `where("a", P.eq("b"))` / `where("a", P.gt("b"))` — the predicate's
/// "value" side is the *name* of another binding rather than a literal.
/// Replace the literal references inside the predicate tree with
/// `IrExpr::Binding` lookups so the comparison runs against the
/// already-bound row.
pub(super) fn lower_where_string<'a, I>(
    mut input: Node,
    label: &str,
    predicate: &Predicate,
    steps: &mut Peekable<I>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node>
where
    I: Iterator<Item = &'a Step>,
{
    let mut bys = Vec::new();
    while let Some(by) = consume_by(steps) {
        bys.push(by);
    }
    // WherePredicateStep advances its traversal ring once for the start
    // value, then once per predicate leaf value, including nested predicates.
    // Project each operand independently so an unproductive child drops the
    // parent even if another branch of an OR would have matched.
    let labels = std::iter::once(label.to_owned()).chain(predicate_string_refs(predicate));
    let mut projected = Vec::new();
    for (index, label) in labels.enumerate() {
        let by = if bys.is_empty() {
            None
        } else {
            bys.get(index % bys.len())
        };
        let (next, value) = project_where_binding(input, &label, by, lo, ctx)?;
        input = next;
        projected.push(value);
    }
    let target = projected.remove(0);
    let operands = std::cell::RefCell::new(projected.into_iter());
    let condition = where_predicate_expr(
        target,
        predicate,
        &|_| operands.borrow_mut().next(),
        lo.productive_by,
    )?;
    Ok(Node::GraphFilter {
        condition,
        input: input.boxed(),
    })
}

fn project_where_binding(
    input: Node,
    label: &str,
    by: Option<&BySpec>,
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<(Node, IrExpr)> {
    use crate::ir::plan::{ProjectErrorPolicy, ProjectMode, ProjectionItem};
    let shared = lo.side_effect_bags.contains_key(label)
        || lo.group_count_side_effects.contains(label);
    let seeded = lo.side_effect_seeds.contains_key(label);
    if by.is_none() && !shared && !seeded {
        return Ok((input, IrExpr::Binding(label.into())));
    }
    let selected = if shared {
        Node::GraphReadSideEffect {
            label: label.into(),
            input: Node::GraphCorrelate { bindings: vec![] }.boxed(),
        }
    } else if seeded {
        super::side_effects::lower_side_effect_value(
            Node::GraphCorrelate { bindings: vec![] }, label, lo,
        ).expect("known side-effect seed")
    } else {
        Node::GraphProject {
            mode: ProjectMode::ReplaceCurrent,
            items: vec![ProjectionItem {
                alias: CURRENT.into(),
                expr: IrExpr::Binding(label.into()),
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: Node::GraphCorrelate { bindings: vec![label.into()] }.boxed(),
        }
    };
    let (projected, value) = match by {
        Some(by) => super::helpers::apply_by_spec(selected, by, lo, ctx)?,
        None => (selected, IrExpr::Binding(CURRENT.into())),
    };
    let alias = lo.fresh("where_by");
    let right = Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: alias.clone(),
            expr: value,
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: projected.boxed(),
    };
    Ok((
        Node::GraphApply {
            kind: ApplyKind::Inner,
            correlation: vec![label.into()],
            outputs: vec![alias.clone()],
            optional_missing: OptionalMissing::Null,
            left: input.boxed(),
            right: right.boxed(),
        },
        IrExpr::Binding(alias),
    ))
}

fn where_predicate_expr(
    target: IrExpr,
    predicate: &Predicate,
    resolve: &dyn Fn(&str) -> Option<IrExpr>,
    productive_by: bool,
) -> GremlinPlanResult<IrExpr> {
    use crate::ir::expr::BinaryOp;
    use crate::language::gremlin::semantics::CompareOp;
    match predicate {
        Predicate::And(a, b) | Predicate::Or(a, b) => Ok(IrExpr::Binary {
            op: if matches!(predicate, Predicate::And(..)) {
                BinaryOp::And
            } else {
                BinaryOp::Or
            },
            lhs: Box::new(where_predicate_expr(
                target.clone(),
                a,
                resolve,
                productive_by,
            )?),
            rhs: Box::new(where_predicate_expr(target, b, resolve, productive_by)?),
        }),
        Predicate::Not(inner) => Ok(IrExpr::Not(Box::new(where_predicate_expr(
            target,
            inner,
            resolve,
            productive_by,
        )?))),
        Predicate::Compare {
            op,
            value: GValue::String(label),
        } if productive_by && matches!(op, CompareOp::Eq | CompareOp::Neq) => {
            let rhs = resolve(label).unwrap_or_else(|| IrExpr::Binding(label.clone()));
            let both_null = IrExpr::and(vec![
                IrExpr::IsNull(Box::new(target.clone())),
                IrExpr::IsNull(Box::new(rhs.clone())),
            ]);
            let equal = IrExpr::Call {
                name: "gremlin_compare".into(),
                args: vec![IrExpr::lit_str("eq"), target.clone(), rhs.clone()],
            };
            // ProductiveByStrategy retains nulls: two null values compare
            // equal, while a null and a non-null value compare unequal.
            let equal = IrExpr::Binary {
                op: BinaryOp::Or,
                lhs: Box::new(both_null),
                rhs: Box::new(IrExpr::and(vec![
                    IrExpr::IsNotNull(Box::new(target)),
                    IrExpr::IsNotNull(Box::new(rhs)),
                    equal,
                ])),
            };
            Ok(if matches!(op, CompareOp::Neq) {
                IrExpr::Not(Box::new(equal))
            } else {
                equal
            })
        }
        _ => predicate_to_expr_with_bindings(target, predicate, resolve),
    }
}

fn binding_by_expr(binding: &str, by: Option<&BySpec>) -> IrExpr {
    match by.and_then(|spec| spec.key.as_deref()) {
        Some("id") => IrExpr::Id(binding.to_string()),
        Some("label") => IrExpr::Label(binding.to_string()),
        Some(key) => IrExpr::property(binding, key.to_string(), PropertyMissing::NullOnMissing),
        None => IrExpr::Binding(binding.to_string()),
    }
}

pub(super) fn lower_not_traversal(
    input: Node,
    sub: &[Step],
    lo: &mut Lowerer,
    ctx: &TraversalContext,
) -> GremlinPlanResult<Node> {
    // A scalar filter can be negated directly, retaining the undefined
    // comparison result that a row-existence anti-join would discard.
    if let [Step::Is { predicate }] = sub {
        return Ok(Node::GraphFilter {
            condition: IrExpr::Not(Box::new(predicate_to_expr(IrExpr::Binding(CURRENT.into()), predicate)?)),
            input: input.boxed(),
        });
    }
    let sub = anchor_where_labels(sub);
    Ok(Node::GraphApply {
        kind: ApplyKind::Anti,
        correlation: vec![CURRENT.into()],
        outputs: Vec::new(),
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: lower_child_traversal(&sub, lo, ctx, ChildTraversalKind::NotPredicate)?.boxed(),
    })
}
