//! Binding-aware rewrites of residual SQL IR nodes. No executable Graph IR is
//! retained. Unknown functions/operators are dependency and effect fences.
use super::*;
use crate::ir::expr::{BinaryOp, IrExpr};
use crate::ir::plan::{ProjectMode, ProjectionItem, TargetMode};
use datafusion::common::tree_node::{Transformed, TreeNode};
use std::collections::BTreeSet;

type Names = BTreeSet<String>;
#[derive(Clone, Debug)]
pub(super) enum Scalar {
    Project(ProjectMode, Vec<ProjectionItem>),
    Filter(IrExpr),
}
#[derive(Clone, Debug, Default)]
pub(super) struct Contract {
    pub reads: Option<Names>,
    pub writes: Names,
    pub replaces: bool,
    pub total: bool,
    pub scalar: Option<Scalar>,
}

/// None means the expression may inspect implicit traverser state. Never infer
/// dependencies from arguments alone for an opaque language function.
fn references(expr: &IrExpr) -> Option<Names> {
    let mut names = Names::new();
    match expr {
        IrExpr::Lit(_) => {}
        IrExpr::Binding(n)
        | IrExpr::Property { binding: n, .. }
        | IrExpr::Id(n)
        | IrExpr::Label(n)
        | IrExpr::HasLabel { binding: n, .. }
        | IrExpr::IsBound(n)
        | IrExpr::SimplePath(n) => {
            names.insert(n.clone());
        }
        IrExpr::Binary { lhs, rhs, .. } => {
            names.extend(references(lhs)?);
            names.extend(references(rhs)?);
        }
        IrExpr::Not(e) | IrExpr::IsNull(e) | IrExpr::IsNotNull(e) => {
            names.extend(references(e)?);
        }
        IrExpr::StringPredicate {
            target, pattern, ..
        } => {
            names.extend(references(target)?);
            names.extend(references(pattern)?);
        }
        IrExpr::List(items) => {
            for e in items {
                names.extend(references(e)?);
            }
        }
        IrExpr::Case { arms, otherwise } => {
            for (a, b) in arms {
                names.extend(references(a)?);
                names.extend(references(b)?);
            }
            if let Some(e) = otherwise {
                names.extend(references(e)?);
            }
        }
        _ => return None,
    }
    Some(names)
}
fn total(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Lit(_)
        | IrExpr::Binding(_)
        | IrExpr::IsBound(_)
        | IrExpr::Id(_)
        | IrExpr::Label(_)
        | IrExpr::HasLabel { .. }
        | IrExpr::SimplePath(_) => true,
        IrExpr::Property { policy, .. } => {
            !matches!(policy, crate::ir::policy::PropertyMissing::Error)
        }
        IrExpr::StringPredicate {
            target, pattern, ..
        } => total(target) && total(pattern),
        IrExpr::List(items) => items.iter().all(total),
        IrExpr::IsNull(e) | IrExpr::IsNotNull(e) => total(e),
        IrExpr::Binary {
            op:
                BinaryOp::Eq
                | BinaryOp::Neq
                | BinaryOp::Lt
                | BinaryOp::Lte
                | BinaryOp::Gt
                | BinaryOp::Gte
                | BinaryOp::And
                | BinaryOp::Or,
            lhs,
            rhs,
        } => total(lhs) && total(rhs),
        _ => false,
    }
}
fn project_contract(mode: ProjectMode, items: &[ProjectionItem]) -> Contract {
    let reads = items.iter().try_fold(Names::new(), |mut names, i| {
        names.extend(references(&i.expr)?);
        Some(names)
    });
    Contract {
        reads,
        writes: items.iter().map(|i| i.alias.clone()).collect(),
        replaces: matches!(mode, ProjectMode::ReplaceScope),
        total: items.iter().all(|i| total(&i.expr)),
        scalar: Some(Scalar::Project(mode, items.to_vec())),
    }
}
pub(super) fn annotate(plan: LogicalPlan, node: &Node) -> LogicalPlan {
    let contract = match node {
        Node::GraphProject { mode, items, .. } => project_contract(*mode, items),
        Node::GraphBind { bind, expr, .. }
            if expr
                .as_ref()
                .is_none_or(|e| matches!(e, IrExpr::Lit(_) | IrExpr::Binding(_))) =>
        {
            project_contract(
                ProjectMode::PreserveVisible,
                &expr
                    .iter()
                    .map(|e| ProjectionItem {
                        alias: bind.clone(),
                        expr: e.clone(),
                    })
                    .collect::<Vec<_>>(),
            )
        }
        Node::GraphFilter { condition, .. } => Contract {
            reads: references(condition),
            total: total(condition),
            scalar: Some(Scalar::Filter(condition.clone())),
            ..Default::default()
        },
        Node::GraphCurrentProject { expr, .. } => Contract {
            reads: references(expr),
            writes: Names::from(["current".into()]),
            total: total(expr),
            ..Default::default()
        },
        Node::GraphUnwind {
            input_expr, bind, ..
        } => Contract {
            reads: references(input_expr),
            writes: Names::from([bind.clone()]),
            total: total(input_expr),
            ..Default::default()
        },
        Node::GraphSort { keys, .. } => Contract {
            reads: keys.iter().try_fold(Names::new(), |mut names, key| {
                names.extend(references(&key.expr)?);
                Some(names)
            }),
            ..Default::default()
        },
        Node::GraphSlice { .. } => Contract {
            reads: Some(Names::new()),
            ..Default::default()
        },
        Node::GraphExpand {
            source,
            target,
            target_mode,
            rel_binding,
            path,
            history,
            length,
            ..
        } => {
            let mut reads = Names::from([source.clone()]);
            if *target_mode == TargetMode::Existing {
                reads.insert(target.clone());
            }
            reads.extend(path.iter().cloned());
            reads.extend(history.iter().cloned());
            let mut writes = Names::new();
            if *target_mode != TargetMode::Existing {
                writes.insert(target.clone());
            }
            writes.extend(rel_binding.iter().cloned());
            writes.extend(path.iter().cloned());
            writes.extend(history.iter().cloned());
            Contract {
                reads: Some(reads),
                writes,
                total: length.min <= length.max.unwrap_or(30),
                ..Default::default()
            }
        }
        _ => return plan,
    };
    let Some(mut k) = as_kernel(&plan).cloned() else {
        return plan;
    };
    k.contract = Some(contract);
    extension(k)
}
fn as_kernel(plan: &LogicalPlan) -> Option<&RowKernel> {
    let LogicalPlan::Extension(e) = plan else {
        return None;
    };
    e.node.as_any().downcast_ref()
}
fn extension(k: RowKernel) -> LogicalPlan {
    LogicalPlan::Extension(Extension { node: Arc::new(k) })
}

/// Projection expressions read the original input row, including when aliases
/// shadow existing bindings. Repeated total expressions share their value;
/// fallible and opaque expressions retain their evaluation count and order.
fn projection(mut k: RowKernel, mode: ProjectMode, items: Vec<ProjectionItem>) -> RowKernel {
    let preserve = !matches!(mode, ProjectMode::ReplaceScope);
    k.contract = Some(project_contract(mode, &items));
    let repeated: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            total(&item.expr)
                .then(|| items[..index].iter().position(|p| p.expr == item.expr))
                .flatten()
        })
        .collect();
    let identities: Vec<_> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            preserve
                && !items[..index].iter().any(|p| p.alias == item.alias)
                && matches!(&item.expr,IrExpr::Binding(n) if n==&item.alias)
        })
        .collect();
    k.kernel = Arc::new(move |mut inputs, state| {
        let rows = inputs.remove(0);
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let mut values: Vec<Option<Value>> = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                if identities[index] && row.bindings.contains_key(&item.alias) {
                    values.push(None);
                    continue;
                }
                let value = match repeated[index] {
                    Some(i) => values[i]
                        .clone()
                        .unwrap_or_else(|| row.get(&items[i].alias)),
                    None => eval(&item.expr, &row, &state.graph)?,
                };
                values.push(Some(value));
            }
            let mut out = if preserve {
                row
            } else {
                Row {
                    bindings: Default::default(),
                    bulk: row.bulk,
                }
            };
            for (item, value) in items.iter().zip(values) {
                if let Some(value) = value {
                    out.bindings.insert(item.alias.clone(), value);
                }
            }
            result.push(out);
        }
        Ok(result)
    });
    k
}
// Composition is deliberately limited to binding/literal maps. Aliases in
// each projection evaluate against its input, not earlier aliases in that map.
fn compose(expr: &IrExpr, items: &[ProjectionItem], replaces: bool) -> Option<IrExpr> {
    match expr {
        IrExpr::Lit(_) => Some(expr.clone()),
        IrExpr::Binding(name) => Some(
            items
                .iter()
                .rev()
                .find(|i| &i.alias == name)
                .map(|i| i.expr.clone())
                .unwrap_or_else(|| {
                    if replaces {
                        IrExpr::Lit(crate::ir::expr::Lit::Null)
                    } else {
                        expr.clone()
                    }
                }),
        ),
        _ => None,
    }
}
fn simplify(plan: LogicalPlan) -> Result<LogicalPlan> {
    Ok(plan
        .transform_up(|plan| {
            let Some(mut k) = as_kernel(&plan).cloned() else {
                return Ok(Transformed::no(plan));
            };
            if let Some(Scalar::Project(mode, items)) =
                k.contract.as_ref().and_then(|c| c.scalar.clone())
            {
                let mut mode = mode;
                let mut items = items;
                if k.inputs.len() == 1 {
                    if let Some(child) = as_kernel(&k.inputs[0]) {
                        if let Some(Scalar::Project(child_mode, child_items)) =
                            child.contract.as_ref().and_then(|c| c.scalar.clone())
                        {
                            if child.inputs.len() == 1
                                && child_items
                                    .iter()
                                    .all(|i| matches!(i.expr, IrExpr::Binding(_) | IrExpr::Lit(_)))
                            {
                                let composed = items
                                    .iter()
                                    .map(|i| {
                                        Some(ProjectionItem {
                                            alias: i.alias.clone(),
                                            expr: compose(
                                                &i.expr,
                                                &child_items,
                                                matches!(child_mode, ProjectMode::ReplaceScope),
                                            )?,
                                        })
                                    })
                                    .collect::<Option<Vec<_>>>();
                                if let Some(composed) = composed {
                                    if matches!(mode, ProjectMode::ReplaceScope) {
                                        items = composed;
                                    } else {
                                        items = child_items;
                                        items.extend(composed);
                                        mode = child_mode;
                                    }
                                    k.inputs = child.inputs.clone();
                                    k.name = "CollapsedProject".into();
                                }
                            }
                        }
                    }
                }
                k = projection(k, mode, items);
                return Ok(Transformed::yes(extension(k)));
            }
            Ok(Transformed::no(plan))
        })?
        .data)
}

/// Move a total predicate across a total read operator that cannot overwrite
/// its dependencies. Moving across expansion retains input and expansion order.
fn push_filters(plan: LogicalPlan) -> Result<LogicalPlan> {
    Ok(plan
        .transform_up(|plan| {
            let Some(mut filter) = as_kernel(&plan).cloned() else {
                return Ok(Transformed::no(plan));
            };
            let Some(c) = &filter.contract else {
                return Ok(Transformed::no(plan));
            };
            if !c.total || !matches!(c.scalar, Some(Scalar::Filter(_))) || filter.inputs.len() != 1
            {
                return Ok(Transformed::no(plan));
            }
            let Some(reads) = c.reads.clone() else {
                return Ok(Transformed::no(plan));
            };
            let mut crossed = Vec::new();
            let mut input = filter.inputs.remove(0);
            while let Some(k) = as_kernel(&input) {
                let Some(c) = &k.contract else {
                    break;
                };
                if !c.total
                    || c.replaces
                    || k.inputs.len() != 1
                    || !reads.is_disjoint(&c.writes)
                    || matches!(c.scalar, Some(Scalar::Filter(_)))
                {
                    break;
                }
                crossed.push(k.clone());
                input = k.inputs[0].clone();
            }
            if crossed.is_empty() {
                return Ok(Transformed::no(plan));
            }
            filter.inputs = vec![input];
            let mut result = extension(filter);
            for mut k in crossed.into_iter().rev() {
                k.inputs = vec![result];
                result = extension(k);
            }
            Ok(Transformed::yes(result))
        })?
        .data)
}

fn required(plan: LogicalPlan, needed: Option<&Names>) -> Result<LogicalPlan> {
    let Some(mut k) = as_kernel(&plan).cloned() else {
        return Ok(plan);
    };
    // Contracts cannot describe arbitrary language callbacks or stateful nodes.
    // Preserve their complete inputs, but downstream may trim their outputs.
    let mut before = None;
    if let Some(c) = &k.contract {
        if let Some(reads) = &c.reads {
            if needed.is_some() || c.replaces {
                let mut names = if c.replaces {
                    Names::new()
                } else {
                    needed.cloned().unwrap_or_default()
                };
                for name in &c.writes {
                    names.remove(name);
                }
                names.extend(reads.iter().cloned());
                before = Some(names);
            }
        }
    }
    for input in &mut k.inputs {
        *input = required(input.clone(), before.as_ref())?;
    }
    if let Some(names) = needed {
        let names = names.clone();
        let operation = k.kernel.clone();
        k.name = format!("Pruned({})", k.name);
        k.kernel = Arc::new(move |inputs, state| {
            let mut rows = operation(inputs, state)?;
            for row in &mut rows {
                row.bindings.retain(|name, _| names.contains(name));
            }
            Ok(rows)
        });
    }
    Ok(extension(k))
}
pub(super) fn optimize(plan: LogicalPlan, needed: Option<&Names>) -> Result<LogicalPlan> {
    let plan = push_filters(simplify(plan)?)?;
    fuse_unary_kernels(required(plan, needed)?)
}

/// Eligible subplans distribute over a frontier and preserve its hidden
/// occurrence column. Reducers, scope replacement and observable calls fence
/// batching, even if a language frontend labels the enclosing node pure.
pub(super) fn batchable(node: &Node) -> bool {
    fn expression(e: &IrExpr) -> bool {
        match e {
            IrExpr::Property { policy, .. } => {
                !matches!(policy, crate::ir::policy::PropertyMissing::Error)
            }
            IrExpr::Call { name, args }
                if matches!(
                    name.as_str(),
                    "path_append" | "select_history_append" | "path_attach_label"
                ) =>
            {
                args.iter().all(expression)
            }
            _ => total(e),
        }
    }
    match node {
        Node::GraphCorrelate { bindings } => bindings != &["__gremlin_group_members"],
        Node::GraphReturn { input, .. } => batchable(input),
        Node::GraphBind { expr, input, .. } => {
            expr.as_ref().is_none_or(expression) && batchable(input)
        }
        Node::GraphProject {
            mode, items, input, ..
        } => {
            !matches!(mode, ProjectMode::ReplaceScope)
                && items.iter().all(|i| expression(&i.expr))
                && batchable(input)
        }
        Node::GraphFilter { condition, input } => total(condition) && batchable(input),
        Node::GraphCurrentProject { expr, input, .. } => expression(expr) && batchable(input),
        Node::GraphUnwind {
            input_expr, input, ..
        } => expression(input_expr) && batchable(input),
        Node::GraphExpand { input, length, .. } => {
            length.min <= length.max.unwrap_or(30) && batchable(input)
        }
        _ => false,
    }
}

/// Reserve all names referenced or assigned by a batchable subplan, including
/// presently-unbound names, so its private occurrence key cannot become visible.
pub(super) fn batch_names(node: &Node) -> Names {
    fn expr(e: &IrExpr, names: &mut Names) {
        if let IrExpr::Call { args, .. } = e {
            for e in args {
                expr(e, names);
            }
        } else if let Some(found) = references(e) {
            names.extend(found);
        }
    }
    let mut names = Names::new();
    match node {
        Node::GraphCorrelate { bindings } => names.extend(bindings.iter().cloned()),
        Node::GraphBind { bind, expr: e, .. } => {
            names.insert(bind.clone());
            if let Some(e) = e {
                expr(e, &mut names);
            }
        }
        Node::GraphProject { items, .. } => {
            for item in items {
                names.insert(item.alias.clone());
                expr(&item.expr, &mut names);
            }
        }
        Node::GraphFilter { condition, .. } => expr(condition, &mut names),
        Node::GraphCurrentProject { expr: e, .. } => {
            names.insert("current".into());
            expr(e, &mut names);
        }
        Node::GraphUnwind {
            input_expr, bind, ..
        } => {
            names.insert(bind.clone());
            expr(input_expr, &mut names);
        }
        Node::GraphExpand {
            source,
            target,
            rel_binding,
            path,
            history,
            ..
        } => {
            names.extend([source.clone(), target.clone()]);
            names.extend(rel_binding.iter().cloned());
            names.extend(path.iter().cloned());
            names.extend(history.iter().cloned());
        }
        _ => {}
    }
    for child in crate::ir::analysis::children(node) {
        names.extend(batch_names(child));
    }
    names
}
