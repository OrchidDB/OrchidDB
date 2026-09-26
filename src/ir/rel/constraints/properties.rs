use super::{Fact, provider::ConstrainedProvider};
use datafusion::{
    common::{
        DFSchema,
        tree_node::{TreeNode, TreeNodeRecursion},
    },
    datasource::source_as_provider,
    logical_expr::{Expr, ExprSchemable, JoinType, LogicalPlan, Operator},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Origin {
    pub table: String,
    pub column: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ForeignKey {
    pub columns: Vec<usize>,
    pub target: String,
    pub references: Vec<String>,
    pub evidence: String,
}
#[derive(Debug, Clone, Default, Serialize)]
pub struct PlanProperties {
    /// Unique under grouping / NULL-equal semantics.
    pub unique_keys: Vec<Vec<usize>>,
    /// Unique for ordinary non-null equality joins (nullable UNIQUE is sufficient).
    pub equality_keys: Vec<Vec<usize>>,
    pub non_null: BTreeSet<usize>,
    pub dependencies: Vec<(Vec<usize>, Vec<usize>)>,
    pub origins: Vec<Option<Origin>>,
    pub constants: BTreeMap<usize, String>,
    pub foreign_keys: Vec<ForeignKey>,
    /// Every source row is represented, allowing FK existence proofs.
    pub complete_sources: BTreeSet<String>,
    pub evidence: BTreeSet<String>,
    /// Removing this subtree cannot skip expression errors or effects.
    pub removable: bool,
}
impl PlanProperties {
    pub fn unique_on(&self, cols: &[usize], null_equal: bool) -> bool {
        let closure = self.closure(cols);
        if self
            .unique_keys
            .iter()
            .any(|key| key.iter().all(|c| closure.contains(c)))
        {
            return true;
        }
        // An FD into a nullable UNIQUE column does not prove at-most-one:
        // multiple rows may share its NULL. Ordinary equality excludes NULL
        // only for columns actually compared by the join.
        !null_equal
            && self
                .equality_keys
                .iter()
                .any(|key| key.iter().all(|c| cols.contains(c)))
    }
    pub fn closure(&self, cols: &[usize]) -> BTreeSet<usize> {
        let mut result: BTreeSet<_> = cols.iter().copied().collect();
        loop {
            let old = result.len();
            for (a, b) in &self.dependencies {
                if a.iter().all(|x| result.contains(x)) {
                    result.extend(b);
                }
            }
            if old == result.len() {
                return result;
            }
        }
    }
}
/// Only identity expressions carry source lineage. Arbitrary casts may collapse keys.
pub(crate) fn input_column(expr: &Expr, schema: &DFSchema) -> Option<usize> {
    match expr {
        Expr::Column(c) => schema.index_of_column(c).ok(),
        Expr::Alias(a) => input_column(&a.expr, schema),
        Expr::Cast(c)
            if c.expr
                .get_type(schema)
                .ok()
                .is_some_and(|from| injective_cast(&from, &c.data_type)) =>
        {
            input_column(&c.expr, schema)
        }
        _ => None,
    }
}
fn injective_cast(from: &arrow::datatypes::DataType, to: &arrow::datatypes::DataType) -> bool {
    use arrow::datatypes::DataType::*;
    let integer = |t: &arrow::datatypes::DataType| match t {
        Int8 => Some((true, 8)),
        Int16 => Some((true, 16)),
        Int32 => Some((true, 32)),
        Int64 => Some((true, 64)),
        UInt8 => Some((false, 8)),
        UInt16 => Some((false, 16)),
        UInt32 => Some((false, 32)),
        UInt64 => Some((false, 64)),
        _ => None,
    };
    from == to
        || match (integer(from), integer(to)) {
            (Some((a, x)), Some((b, y))) => (a == b && x <= y) || (!a && b && x < y),
            _ => false,
        }
}
pub(crate) fn harmless(expr: &Expr, schema: &DFSchema) -> bool {
    match expr {
        Expr::Column(_) | Expr::Literal(..) => true,
        Expr::Alias(a) => harmless(&a.expr, schema),
        Expr::IsNull(e) | Expr::IsNotNull(e) | Expr::Not(e) => harmless(e, schema),
        Expr::BinaryExpr(b)
            if matches!(
                b.op,
                Operator::Eq
                    | Operator::NotEq
                    | Operator::Lt
                    | Operator::LtEq
                    | Operator::Gt
                    | Operator::GtEq
                    | Operator::And
                    | Operator::Or
            ) =>
        {
            let left = b.left.get_type(schema).ok();
            let right = b.right.get_type(schema).ok();
            // Implicit coercion (e.g. integer = 'bad') can throw even though
            // the parsed expression has no explicit Cast yet.
            left.is_some()
                && left == right
                && harmless(&b.left, schema)
                && harmless(&b.right, schema)
        }
        _ => false,
    }
}
pub(crate) fn referenced(expr: &Expr, schema: &DFSchema) -> BTreeSet<usize> {
    let mut result = BTreeSet::new();
    let _ = expr.apply(|e| {
        if let Expr::Column(c) = e {
            if let Ok(i) = schema.index_of_column(c) {
                result.insert(i);
            }
        }
        Ok(TreeNodeRecursion::Continue)
    });
    result
}
fn remap(p: PlanProperties, map: &[Option<usize>]) -> PlanProperties {
    let index = |i: &usize| map.iter().position(|x| x == &Some(*i));
    let indices = |v: &Vec<usize>| v.iter().map(index).collect::<Option<Vec<_>>>();
    let mut out = PlanProperties {
        unique_keys: p.unique_keys.iter().filter_map(indices).collect(),
        equality_keys: p.equality_keys.iter().filter_map(indices).collect(),
        non_null: map
            .iter()
            .enumerate()
            .filter_map(|(o, i)| i.filter(|i| p.non_null.contains(i)).map(|_| o))
            .collect(),
        dependencies: p
            .dependencies
            .iter()
            .filter_map(|(a, b)| Some((indices(a)?, b.iter().filter_map(index).collect())))
            .collect(),
        origins: map
            .iter()
            .map(|i| i.and_then(|i| p.origins.get(i).cloned().flatten()))
            .collect(),
        foreign_keys: p
            .foreign_keys
            .iter()
            .filter_map(|f| {
                Some(ForeignKey {
                    columns: indices(&f.columns)?,
                    ..f.clone()
                })
            })
            .collect(),
        constants: map
            .iter()
            .enumerate()
            .filter_map(|(o, i)| Some((o, p.constants.get(&(*i)?)?.clone())))
            .collect(),
        complete_sources: p.complete_sources,
        evidence: p.evidence,
        removable: p.removable,
    };
    // Repeated projections of the same column determine each other.
    for (a, i) in map.iter().enumerate() {
        if i.is_some() {
            for (b, j) in map.iter().enumerate() {
                if a != b && i == j {
                    out.dependencies.push((vec![a], vec![b]));
                }
            }
        }
    }
    for key in &out.unique_keys {
        out.dependencies
            .push((key.clone(), (0..map.len()).collect()));
    }
    out
}
pub fn analyze(plan: &LogicalPlan) -> PlanProperties {
    let mut p = analyze_node(plan);
    fn minimal(keys: &mut Vec<Vec<usize>>) {
        for k in keys.iter_mut() {
            k.sort_unstable();
            k.dedup();
        }
        keys.sort_by(|a, b| a.len().cmp(&b.len()).then(a.cmp(b)));
        keys.dedup();
        let mut result: Vec<Vec<usize>> = Vec::new();
        for key in keys.drain(..) {
            if !result.iter().any(|k| k.iter().all(|i| key.contains(i))) {
                result.push(key);
            }
        }
        *keys = result;
    }
    minimal(&mut p.unique_keys);
    minimal(&mut p.equality_keys);
    p.dependencies.sort();
    p.dependencies.dedup();
    p
}
fn analyze_node(plan: &LogicalPlan) -> PlanProperties {
    let width = plan.schema().fields().len();
    let mut p = PlanProperties {
        origins: vec![None; width],
        ..Default::default()
    };
    match plan {
        LogicalPlan::TableScan(s) => {
            let Ok(provider) = source_as_provider(&s.source) else {
                return p;
            };
            let schema = provider.schema();
            if let Some(view) = provider.get_logical_plan() {
                let mut facts = analyze(&view);
                if !s.filters.is_empty() || s.fetch.is_some() {
                    facts.complete_sources.clear();
                }
                facts.removable &= s.filters.iter().all(|e| harmless(e, plan.schema()));
                return remap(
                    facts,
                    &s.projection
                        .clone()
                        .unwrap_or_else(|| (0..schema.fields().len()).collect())
                        .into_iter()
                        .map(Some)
                        .collect::<Vec<_>>(),
                );
            }
            p.origins = (0..schema.fields().len()).map(|_| None).collect();
            p.non_null = schema
                .fields()
                .iter()
                .enumerate()
                .filter_map(|(i, f)| (!f.is_nullable()).then_some(i))
                .collect();
            // The provider contract already guarantees its primary/unique constraints.
            if let Some(cs) = provider.constraints() {
                for c in cs.iter() {
                    match c {
                        datafusion::common::Constraint::PrimaryKey(k) => {
                            p.non_null.extend(k);
                            p.unique_keys.push(k.clone());
                            p.equality_keys.push(k.clone());
                        }
                        datafusion::common::Constraint::Unique(k) => {
                            p.equality_keys.push(k.clone());
                        }
                    }
                }
            }
            if let Some(c) = provider.as_any().downcast_ref::<ConstrainedProvider>() {
                p.origins = schema
                    .fields()
                    .iter()
                    .map(|f| {
                        Some(Origin {
                            table: c.table.clone(),
                            column: f.name().clone(),
                        })
                    })
                    .collect();
                p.removable = c.inner.get_logical_plan().is_none()
                    && c.inner.table_type() == datafusion::logical_expr::TableType::Base;
                if s.filters.is_empty() && s.fetch.is_none() {
                    p.complete_sources.insert(c.table.clone());
                }
                let indices = |names: &Vec<String>| {
                    names
                        .iter()
                        .map(|n| schema.index_of(n).unwrap())
                        .collect::<Vec<_>>()
                };
                for fact in &c.facts {
                    p.evidence
                        .insert(format!("{}.{} ({:?})", c.table, fact.name, fact.evidence));
                    match &fact.fact {
                        Fact::NonNull { columns } => p.non_null.extend(indices(columns)),
                        Fact::Unique {
                            columns,
                            nulls_equal,
                        } => {
                            let k = indices(columns);
                            p.equality_keys.push(k.clone());
                            if *nulls_equal {
                                p.unique_keys.push(k);
                            }
                        }
                        Fact::FunctionalDependency {
                            determinant,
                            dependent,
                        } => p
                            .dependencies
                            .push((indices(determinant), indices(dependent))),
                        Fact::ForeignKey {
                            columns,
                            target,
                            references,
                        } => p.foreign_keys.push(ForeignKey {
                            columns: indices(columns),
                            target: target.clone(),
                            references: references.clone(),
                            evidence: fact.name.clone(),
                        }),
                    }
                }
            }
            for key in &p.equality_keys {
                if key.iter().all(|i| p.non_null.contains(i)) && !p.unique_keys.contains(key) {
                    p.unique_keys.push(key.clone());
                }
            }
            for key in &p.unique_keys {
                p.dependencies
                    .push((key.clone(), (0..schema.fields().len()).collect()));
            }
            p.removable &= s.filters.iter().all(|e| harmless(e, plan.schema()));
            remap(
                p,
                &s.projection
                    .clone()
                    .unwrap_or_else(|| (0..schema.fields().len()).collect())
                    .into_iter()
                    .map(Some)
                    .collect::<Vec<_>>(),
            )
        }
        LogicalPlan::Projection(n) => {
            p = remap(
                analyze(&n.input),
                &n.expr
                    .iter()
                    .map(|e| input_column(e, n.input.schema()))
                    .collect::<Vec<_>>(),
            );
            p.removable &= n.expr.iter().all(|e| harmless(e, n.input.schema()));
            fn literal(e: &Expr) -> Option<String> {
                match e {
                    Expr::Literal(v, _) if !v.is_null() => Some(format!("{v:?}")),
                    Expr::Alias(a) => literal(&a.expr),
                    _ => None,
                }
            }
            for (i, e) in n.expr.iter().enumerate() {
                if let Some(v) = literal(e) {
                    p.non_null.insert(i);
                    p.constants.insert(i, v);
                }
            }
            p
        }
        LogicalPlan::SubqueryAlias(n) => analyze(&n.input),
        LogicalPlan::Filter(n) => {
            p = analyze(&n.input);
            p.complete_sources.clear();
            p.removable &= harmless(&n.predicate, n.input.schema());
            fn nonnull(e: &Expr, s: &DFSchema, out: &mut BTreeSet<usize>) {
                match e {
                    Expr::IsNotNull(c) => {
                        if let Some(i) = input_column(c, s) {
                            out.insert(i);
                        }
                    }
                    Expr::BinaryExpr(b) if b.op == Operator::And => {
                        nonnull(&b.left, s, out);
                        nonnull(&b.right, s, out);
                    }
                    Expr::BinaryExpr(b)
                        if matches!(
                            b.op,
                            Operator::Eq
                                | Operator::NotEq
                                | Operator::Lt
                                | Operator::LtEq
                                | Operator::Gt
                                | Operator::GtEq
                        ) =>
                    {
                        for e in [&b.left, &b.right] {
                            if let Some(i) = input_column(e, s) {
                                out.insert(i);
                            }
                        }
                    }
                    _ => {}
                }
            }
            nonnull(&n.predicate, n.input.schema(), &mut p.non_null);
            for k in &p.equality_keys {
                if k.iter().all(|i| p.non_null.contains(i)) && !p.unique_keys.contains(k) {
                    p.unique_keys.push(k.clone());
                }
            }
            p
        }
        LogicalPlan::Sort(n) => {
            p = analyze(&n.input);
            p.removable = false; // Sorting can invoke type-specific comparison kernels.
            if n.fetch.is_some() {
                p.complete_sources.clear();
            }
            p
        }
        LogicalPlan::Limit(n) => {
            p = analyze(&n.input);
            p.complete_sources.clear();
            p.removable = false;
            p
        }
        LogicalPlan::Distinct(datafusion::logical_expr::Distinct::All(n)) => {
            p = analyze(n);
            p.unique_keys.push((0..width).collect());
            p.equality_keys.push((0..width).collect());
            p
        }
        LogicalPlan::Aggregate(n) => {
            let source = analyze(&n.input);
            let map: Vec<_> = n
                .group_expr
                .iter()
                .map(|e| input_column(e, n.input.schema()))
                .chain(n.aggr_expr.iter().map(|_| None))
                .collect();
            p = remap(source, &map);
            p.complete_sources.clear();
            p.removable = false;
            if !n
                .group_expr
                .iter()
                .any(|e| matches!(e, Expr::GroupingSet(_)))
            {
                let k: Vec<_> = (0..n.group_expr.len()).collect();
                p.unique_keys.push(k.clone());
                p.equality_keys.push(k.clone());
                p.dependencies.push((k, (0..width).collect()));
            }
            p
        }
        LogicalPlan::Join(n) => {
            let l = analyze(&n.left);
            let r = analyze(&n.right);
            let lw = n.left.schema().fields().len();
            let pairs: Option<Vec<_>> =
                n.on.iter()
                    .map(|(a, b)| {
                        Some((
                            input_column(a, n.left.schema())?,
                            input_column(b, n.right.schema())?,
                        ))
                    })
                    .collect();
            let (lk, rk): (Vec<_>, Vec<_>) = pairs.unwrap_or_default().into_iter().unzip();
            let lu = l.unique_on(&lk, true);
            let ru = r.unique_on(&rk, true);
            match n.join_type {
                JoinType::LeftSemi | JoinType::LeftAnti => {
                    p = l;
                    p.complete_sources.clear();
                    p.removable = false;
                    return p;
                }
                JoinType::RightSemi | JoinType::RightAnti => {
                    p = r;
                    p.complete_sources.clear();
                    p.removable = false;
                    return p;
                }
                JoinType::Inner | JoinType::Left | JoinType::Right | JoinType::Full => {}
                _ => return p,
            }
            p.origins = l.origins.iter().chain(&r.origins).cloned().collect();
            p.evidence = l.evidence.union(&r.evidence).cloned().collect();
            if matches!(n.join_type, JoinType::Inner | JoinType::Left) {
                p.non_null.extend(&l.non_null);
                p.foreign_keys.extend(l.foreign_keys.clone());
                if ru {
                    p.unique_keys.extend(l.unique_keys.clone());
                    p.equality_keys.extend(l.equality_keys.clone());
                }
                p.dependencies.extend(l.dependencies.clone());
            }
            if matches!(n.join_type, JoinType::Inner | JoinType::Right) {
                p.non_null.extend(r.non_null.iter().map(|i| i + lw));
                p.foreign_keys
                    .extend(r.foreign_keys.iter().map(|f| ForeignKey {
                        columns: f.columns.iter().map(|i| i + lw).collect(),
                        ..f.clone()
                    }));
                if lu {
                    p.unique_keys.extend(
                        r.unique_keys
                            .iter()
                            .map(|k| k.iter().map(|i| i + lw).collect::<Vec<_>>()),
                    );
                    p.equality_keys.extend(
                        r.equality_keys
                            .iter()
                            .map(|k| k.iter().map(|i| i + lw).collect::<Vec<_>>()),
                    );
                }
                p.dependencies.extend(r.dependencies.iter().map(|(a, b)| {
                    (
                        a.iter().map(|i| i + lw).collect(),
                        b.iter().map(|i| i + lw).collect(),
                    )
                }));
            }
            if n.join_type == JoinType::Inner {
                for a in &l.unique_keys {
                    for b in &r.unique_keys {
                        p.unique_keys
                            .push(a.iter().copied().chain(b.iter().map(|i| i + lw)).collect());
                    }
                }
            }
            p
        }
        // Union/unnest/recursion/native extensions are proof barriers unless a
        // dedicated rule establishes uniqueness, occurrence identity and lineage.
        _ => p,
    }
}
