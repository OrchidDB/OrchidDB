//! Scalarize immutable collection constructors before relational projection lowering.
//!
//! This deliberately is not a map transport format. Only literal constructors
//! and already bound values are substituted, so discarded fields cannot throw or observe an evaluation
//! count. Values that escape as maps retain the existing typed-value boundary.
use super::*;

fn literal_tree(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Lit(_) => true,
        IrExpr::List(items) => items.iter().all(literal_tree),
        IrExpr::Call { name, args } if matches!(name.as_str(), "make_map" | "map") => {
            args.len() % 2 == 0
                && args.chunks_exact(2).all(|pair| {
                    matches!(pair[0], IrExpr::Lit(Lit::String(_))) && literal_tree(&pair[1])
                })
        }
        _ => false,
    }
}

// Reading an already bound value is pure. Keep every referenced source binding
// available when substituting the constructor across a projection boundary.
fn constructor_bindings(expr: &IrExpr, bindings: &mut BTreeSet<String>) -> bool {
    match expr {
        IrExpr::Binding(name) => { bindings.insert(name.clone()); true }
        IrExpr::Lit(_) => true,
        IrExpr::List(items) => items.iter().all(|item| constructor_bindings(item, bindings)),
        IrExpr::Call { name, args } if matches!(name.as_str(), "make_map" | "map") => {
            args.len() % 2 == 0 && args.chunks_exact(2).all(|pair|
                matches!(pair[0], IrExpr::Lit(Lit::String(_))) && constructor_bindings(&pair[1], bindings))
        }
        _ => false,
    }
}

// Resolve a literal index before trying to materialize a heterogeneous list.
// All other constructor entries have already been proved evaluation-free.
fn select_constructor(expr: IrExpr) -> IrExpr {
    if let IrExpr::Call { name, args } = &expr
        && name == "cypher_subscript" && args.len() == 2
        && let IrExpr::List(items) = &args[0]
        && constructor_bindings(&args[0], &mut BTreeSet::new())
        && let IrExpr::Lit(Lit::Int(index)) = &args[1]
    {
        let index = if *index < 0 { items.len() as i128 + *index as i128 } else { *index as i128 };
        return usize::try_from(index).ok().and_then(|i| items.get(i)).cloned().unwrap_or(IrExpr::Lit(Lit::Null));
    }
    if let IrExpr::Call { name, args } = &expr
        && matches!(name.as_str(), "cypher_subscript" | "property") && args.len() == 2
        && let IrExpr::Call { name: constructor, args: entries } = &args[0]
        && matches!(constructor.as_str(), "make_map" | "map")
        && constructor_bindings(&args[0], &mut BTreeSet::new())
        && let IrExpr::Lit(Lit::String(key)) = &args[1]
    {
        // Duplicate map keys use the last entry, matching map construction.
        return entries.chunks_exact(2).rev()
            .find(|pair| matches!(&pair[0], IrExpr::Lit(Lit::String(k)) if k == key))
            .map(|pair| pair[1].clone()).unwrap_or(IrExpr::Lit(Lit::Null));
    }
    expr
}

fn contains_collection(expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Call { name, .. } if matches!(name.as_str(), "make_map" | "map") => true,
        IrExpr::List(_) => true,
        _ => false,
    }
}

fn substitute(expr: &IrExpr, values: &BTreeMap<String, IrExpr>) -> Option<IrExpr> {
    let mut result = expr.clone();
    match &mut result {
        IrExpr::Binding(name) => return Some(values.get(name).cloned().unwrap_or(result)),
        IrExpr::Property { binding, name, .. } if values.contains_key(binding) => {
            return Some(select_constructor(IrExpr::Call {
                name: "cypher_subscript".into(),
                args: vec![values[binding].clone(), IrExpr::lit_str(name.clone())],
            }));
        }
        IrExpr::Binary { lhs, rhs, .. } => {
            **lhs = substitute(lhs, values)?;
            **rhs = substitute(rhs, values)?;
        }
        IrExpr::Not(inner) | IrExpr::IsNull(inner) | IrExpr::IsNotNull(inner) => {
            **inner = substitute(inner, values)?
        }
        IrExpr::StringPredicate {
            target, pattern, ..
        } => {
            **target = substitute(target, values)?;
            **pattern = substitute(pattern, values)?;
        }
        IrExpr::List(items) | IrExpr::Call { args: items, .. } => {
            for item in items {
                *item = substitute(item, values)?;
            }
        }
        IrExpr::Case { arms, otherwise } => {
            for (condition, value) in arms {
                *condition = substitute(condition, values)?;
                *value = substitute(value, values)?;
            }
            if let Some(value) = otherwise {
                **value = substitute(value, values)?;
            }
        }
        IrExpr::ListTransform { list, item, map } => {
            **list = substitute(list, values)?;
            let mut free = BTreeSet::new();
            for value in values.values() { constructor_bindings(value, &mut free); }
            if free.contains(item) { return None; }
            let mut scope = values.clone();
            scope.remove(item);
            **map = substitute(map, &scope)?;
        }
        IrExpr::ListFilter {
            list,
            item,
            predicate,
        } | IrExpr::Quantifier { list, item, predicate, .. } => {
            **list = substitute(list, values)?;
            let mut free = BTreeSet::new();
            for value in values.values() { constructor_bindings(value, &mut free); }
            if free.contains(item) { return None; }
            let mut scope = values.clone();
            scope.remove(item);
            **predicate = substitute(predicate, &scope)?;
        }
        IrExpr::ListReduce {
            collection,
            accumulator,
            item,
            map,
        } => {
            **collection = substitute(collection, values)?;
            let mut free = BTreeSet::new();
            for value in values.values() { constructor_bindings(value, &mut free); }
            if free.contains(item) { return None; }
            let mut scope = values.clone();
            scope.remove(item);
            if free.contains(accumulator) { return None; }
            scope.remove(accumulator);
            **map = substitute(map, &scope)?;
        }
        IrExpr::Id(name)
        | IrExpr::Label(name)
        | IrExpr::IsBound(name)
        | IrExpr::SimplePath(name) => {
            if values.contains_key(name) {
                return None;
            }
        }
        IrExpr::HasLabel { binding, .. } if values.contains_key(binding) => return None,
        _ => {}
    }
    Some(select_constructor(result))
}

/// Return a rewritten projection chain only when a literal collection can be removed.
/// Stop at every non-projection operator: joins, optional null extension,
/// aggregation, DISTINCT and correlation are not transparent binding scopes.
pub(super) fn scalarize_projection(
    mode: ProjectMode,
    items: &[ProjectionItem],
    input: &Node,
) -> Option<(Vec<ProjectionItem>, Node)> {
    if mode != ProjectMode::ReplaceScope {
        return None;
    }
    let Node::GraphProject {
        mode: input_mode,
        items: input_items,
        error_policy,
        input: source,
    } = input
    else {
        return None;
    };
    if *input_mode != ProjectMode::ReplaceScope {
        return None;
    }
    let (inner_items, source, recursive_change) =
        match scalarize_projection(*input_mode, input_items, source) {
            Some((items, source)) => (items, source, true),
            None => (input_items.clone(), source.as_ref().clone(), false),
        };
    let values: BTreeMap<_, _> = inner_items
        .iter()
        .filter(|item| literal_tree(&item.expr) || (contains_collection(&item.expr) && constructor_bindings(&item.expr, &mut BTreeSet::new())))
        .map(|item| (item.alias.clone(), item.expr.clone()))
        .collect();
    if !values.values().any(contains_collection) {
        return recursive_change.then(|| {
            (
                items.to_vec(),
                Node::GraphProject {
                    mode: *input_mode,
                    items: inner_items,
                    error_policy: *error_policy,
                    input: Box::new(source),
                },
            )
        });
    }
    let rewritten = items
        .iter()
        .map(|item| {
            Some(ProjectionItem {
                alias: item.alias.clone(),
                expr: substitute(&item.expr, &values)?,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    let mut needed = BTreeSet::new();
    for value in values.values() { constructor_bindings(value, &mut needed); }
    // A source name shadowed by this projection cannot be reintroduced under
    // the same name: that would silently change the outer expression's scope.
    if needed.iter().any(|name| inner_items.iter().any(|item|
        item.alias == *name && item.expr != IrExpr::Binding(name.clone()))) {
        return None;
    }
    let mut remaining = inner_items
        .into_iter()
        .filter(|item| !values.contains_key(&item.alias))
        .collect::<Vec<_>>();
    for name in needed {
        if !remaining.iter().any(|item| item.alias == name) {
            remaining.push(ProjectionItem { alias: name.clone(), expr: IrExpr::Binding(name) });
        }
    }
    let input = if remaining.is_empty() {
        source
    } else {
        Node::GraphProject {
            mode: *input_mode,
            items: remaining,
            error_policy: *error_policy,
            input: Box::new(source),
        }
    };
    Some((rewritten, input))
}

/// Eliminate a collected map that is observed only through one field of its
/// first element. A singleton list carries the field through COLLECT: unlike
/// collecting the scalar directly, `[NULL]` is not discarded. Both rewrites
/// use ordinary IR expressions, so no private function or value encoding is
/// introduced. Evaluation-free constructor entries are required because the
/// unused map fields are removed.
pub(super) fn scalarize_collected_map_projection(
    mode: ProjectMode,
    items: &[ProjectionItem],
    input: &Node,
) -> Option<(Vec<ProjectionItem>, Node)> {
    if mode != ProjectMode::ReplaceScope { return None; }
    let Node::GraphProject { mode: inner_mode, items: inner, error_policy, input: aggregate } = input else { return None; };
    if *inner_mode != ProjectMode::ReplaceScope { return None; }
    let Node::GraphAggregate { group, aggs, fields, input: source } = aggregate.as_ref() else { return None; };
    // Limit this rewrite to a transparent projection. Count aggregate uses
    // before mutating it, so another projection cannot observe the new carrier.
    let mut uses = BTreeMap::<String, usize>::new();
    for item in inner {
        match &item.expr {
            IrExpr::Binding(binding) => *uses.entry(binding.clone()).or_default() += 1,
            IrExpr::Lit(_) => {},
            IrExpr::Call { name, args } if name == "head" => {
                let [IrExpr::Binding(binding)] = args.as_slice() else { return None; };
                *uses.entry(binding.clone()).or_default() += 1;
            }
            _ => return None,
        }
    }
    let mut inner = inner.clone();
    let mut aggs = aggs.clone();
    let mut outer = items.to_vec();
    let mut changed = false;
    for item in &mut inner {
        let IrExpr::Call { name, args } = &item.expr else { continue; };
        if name != "head" { continue; }
        let [IrExpr::Binding(aggregate_name)] = args.as_slice() else { continue; };
        let aggregate_name = aggregate_name.clone();
        if uses.get(&aggregate_name) != Some(&1) { continue; }
        let Some(agg) = aggs.iter_mut().find(|agg| agg.alias == aggregate_name
            && agg.kind == AggKind::CollectNonNull && !agg.distinct) else { continue; };
        let Some(IrExpr::Call { name, args: entries }) = &agg.arg else { continue; };
        if !matches!(name.as_str(), "map" | "make_map")
            || !constructor_bindings(agg.arg.as_ref()?, &mut BTreeSet::new()) { continue; }
        let mut selected_key: Option<String> = None;
        let mut selected = Vec::new();
        // A conservative demand check: arbitrary expressions or escaping the
        // map stay on the native path. The same selected field may be reused.
        let mut safe = true;
        for (index, output) in outer.iter().enumerate() {
            let key = match &output.expr {
                IrExpr::Property { binding, name, .. } if binding == &item.alias => Some(name.clone()),
                IrExpr::Call { name, args } if matches!(name.as_str(), "property" | "cypher_subscript") => {
                    match args.as_slice() {
                        [IrExpr::Binding(binding), IrExpr::Lit(Lit::String(key))] if binding == &item.alias => Some(key.clone()),
                        _ => { safe = false; None }
                    }
                }
                IrExpr::Lit(_) => None,
                IrExpr::Binding(binding) if binding != &item.alias => None,
                _ => { safe = false; None }
            };
            if let Some(key) = key {
                if selected_key.as_ref().is_some_and(|previous| previous != &key) { safe = false; }
                selected_key = Some(key);
                selected.push(index);
            }
        }
        let Some(key) = selected_key else { continue; };
        if !safe { continue; }
        let value = entries.chunks_exact(2).rev()
            .find(|pair| matches!(&pair[0], IrExpr::Lit(Lit::String(k)) if k == &key))
            .map(|pair| pair[1].clone()).unwrap_or(IrExpr::Lit(Lit::Null));
        agg.arg = Some(IrExpr::List(vec![value]));
        let subscript = |value| IrExpr::Call { name: "cypher_subscript".into(), args: vec![value, IrExpr::Lit(Lit::Int(0))] };
        item.expr = subscript(subscript(IrExpr::Binding(aggregate_name)));
        for index in selected { outer[index].expr = IrExpr::Binding(item.alias.clone()); }
        changed = true;
    }
    if !changed { return None; }
    Some((outer, Node::GraphProject {
        mode: *inner_mode, items: inner, error_policy: *error_policy,
        input: Box::new(Node::GraphAggregate {
            group: group.clone(), aggs, fields: fields.clone(), input: source.clone(),
        }),
    }))
}

/// An untyped NULL map has NULL keys; evaluating its operand still matters.
pub(super) fn lower_null_keys(
    ctx: &LoweringContext<'_>,
    plan: &LogicalPlan,
    expr: &IrExpr,
) -> RelResult<Option<Expr>> {
    let IrExpr::Call { name, args } = expr else {
        return Ok(None);
    };
    if name != "keys"
        || args.len() != 1
        || !matches!(&args[0], IrExpr::Binding(_) | IrExpr::Lit(Lit::Null))
    {
        return Ok(None);
    }
    let value = ctx.lower_expr(plan, &args[0])?;
    if value.get_type(plan.schema())? == DataType::Null {
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitution_never_captures_a_constructor_binding() {
        let values = BTreeMap::from([("xs".into(), IrExpr::List(vec![IrExpr::Binding("a".into())]))]);
        let expr = IrExpr::ListTransform {
            list: Box::new(IrExpr::List(vec![IrExpr::Lit(Lit::Int(1))])),
            item: "a".into(),
            map: Box::new(IrExpr::Binding("xs".into())),
        };
        assert!(substitute(&expr, &values).is_none());
    }

    #[test]
    fn selection_does_not_drop_observable_constructor_evaluation() {
        let expr = IrExpr::Call { name: "cypher_subscript".into(), args: vec![
            IrExpr::List(vec![IrExpr::Lit(Lit::Int(1)), IrExpr::Call { name: "rand".into(), args: vec![] }]),
            IrExpr::Lit(Lit::Int(0)),
        ] };
        assert_eq!(select_constructor(expr.clone()), expr);
    }

    #[test]
    fn literal_collection_eligibility_excludes_evaluation_effects() {
        for expression in [
            IrExpr::Call { name: "rand".into(), args: vec![] },
            IrExpr::Binary { op: BinaryOp::Div,
                lhs: Box::new(IrExpr::Lit(Lit::Int(1))),
                rhs: Box::new(IrExpr::Lit(Lit::Int(0))) },
            IrExpr::Binding("outer".into()),
        ] {
            assert!(!literal_tree(&IrExpr::List(vec![IrExpr::Lit(Lit::Int(1)), expression])));
        }
    }
}
