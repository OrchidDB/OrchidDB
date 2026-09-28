use super::*;
use datafusion::{
    common::ScalarValue,
    datasource::source_as_provider,
    logical_expr::{Distinct, Expr, JoinType, LogicalPlan, Operator},
};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, Serialize)]
pub struct PlanEstimate {
    pub occurrence: String,
    pub operator: String,
    pub estimated_rows: Option<f64>,
    pub estimated_row_width: Option<f64>,
    pub estimated_scan_bytes: Option<f64>,
    pub estimated_input_rows: Option<f64>,
    pub estimated_expanded_rows: Option<f64>,
    pub estimated_work: Option<f64>,
    pub statistics_revisions: Vec<String>,
    pub method: String,
}
#[derive(Clone, Default)]
struct State {
    rows: Option<f64>,
    bytes: Option<f64>,
    input: Option<f64>,
    expanded: Option<f64>,
    work: Option<f64>,
    columns: BTreeMap<String, ColumnStatistics>,
    groups: Vec<GroupStatistics>,
    revisions: BTreeSet<String>,
    filters: BTreeSet<String>,
}
fn add(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    a.zip(b).map(|(a, b)| (a + b).min(1e300))
}
fn scalar(v: &ScalarValue) -> Option<String> {
    if v.is_null() {
        return Some("null".into());
    }
    match v {
        ScalarValue::Utf8(Some(s))
        | ScalarValue::LargeUtf8(Some(s))
        | ScalarValue::Utf8View(Some(s)) => Some(serde_json::to_string(s).ok()?),
        ScalarValue::Boolean(Some(b)) => Some(b.to_string()),
        _ => Some(v.to_string()),
    }
}
fn column<'a>(
    expr: &Expr,
    columns: &'a BTreeMap<String, ColumnStatistics>,
) -> Option<&'a ColumnStatistics> {
    match expr {
        Expr::Column(c) => columns.get(&c.flat_name()).or_else(|| columns.get(&c.name)),
        Expr::Alias(a) => column(&a.expr, columns),
        Expr::Cast(c) => column(&c.expr, columns),
        Expr::TryCast(c) => column(&c.expr, columns),
        Expr::ScalarFunction(f) if f.name() == "get_field" => {
            let parent = column(f.args.first()?, columns)?;
            let name = if let Expr::Literal(ScalarValue::Utf8(Some(s)), _) = f.args.get(1)? {
                s
            } else {
                return None;
            };
            parent.list.as_ref()?.elements.get(name)
        }
        _ => None,
    }
}
fn literal(e: &Expr) -> Option<String> {
    if let Expr::Literal(v, _) = e {
        scalar(v)
    } else {
        None
    }
}
fn equality(c: &ColumnStatistics, value: &str) -> Option<f64> {
    if c.observations == 0 {
        return None;
    }
    if let Some((_, n)) = c.frequent.iter().find(|(v, _)| v == value) {
        return Some(*n as f64 / c.observations as f64);
    }
    let count = c
        .observations
        .saturating_sub(c.nulls)
        .saturating_sub(c.frequent.iter().map(|(_, n)| n).sum());
    let ndv = c
        .estimated_distinct
        .unwrap_or(c.sample_distinct.max(c.frequent.len() as u64 + 1) as f64);
    // Unseen values in a sample are not impossible. A small floor avoids converting
    // absence of observations into a misleading zero-cost plan.
    Some(
        (count as f64 / c.observations as f64 / (ndv - c.frequent.len() as f64).max(1.0))
            .max(1.0 / (c.observations as f64 + 1.0)),
    )
}
fn selectivity(e: &Expr, columns: &BTreeMap<String, ColumnStatistics>) -> Option<f64> {
    match e {
        Expr::BinaryExpr(b) => {
            if b.op == Operator::And {
                return selectivity(&b.left, columns)
                    .zip(selectivity(&b.right, columns))
                    .map(|(a, b)| a * b);
            }
            if b.op == Operator::Or {
                return selectivity(&b.left, columns)
                    .zip(selectivity(&b.right, columns))
                    .map(|(a, b)| a + b - a * b);
            }
            if b.op == Operator::IsNotDistinctFrom
                && matches!(
                    b.right.as_ref(),
                    Expr::Literal(ScalarValue::Boolean(Some(true)), _)
                )
            {
                return selectivity(&b.left, columns);
            }
            // Gremlin has()/property predicates lower to a nonempty singleton list.
            if b.op == Operator::Gt && literal(&b.right).as_deref() == Some("0") {
                if let Expr::ScalarFunction(f) = b.left.as_ref() {
                    if f.name() == "array_length" {
                        if let Some(Expr::Case(c)) = f.args.first() {
                            if c.expr.is_none() && c.when_then_expr.len() == 1 {
                                let (when, then) = &c.when_then_expr[0];
                                let empty=c.else_expr.as_deref().is_some_and(|e|matches!(e,Expr::Literal(ScalarValue::List(a),_) if a.value_length(0)==0));
                                if empty
                                    && matches!(then.as_ref(),Expr::ScalarFunction(f) if f.name()=="make_array"&&!f.args.is_empty())
                                {
                                    return selectivity(when, columns);
                                }
                            }
                        }
                    }
                }
            }
            let (c, v, op) =
                if let (Some(c), Some(v)) = (column(&b.left, columns), literal(&b.right)) {
                    (c, v, b.op)
                } else if let (Some(c), Some(v)) = (column(&b.right, columns), literal(&b.left)) {
                    (
                        c,
                        v,
                        match b.op {
                            Operator::Lt => Operator::Gt,
                            Operator::LtEq => Operator::GtEq,
                            Operator::Gt => Operator::Lt,
                            Operator::GtEq => Operator::LtEq,
                            x => x,
                        },
                    )
                } else {
                    return None;
                };
            match op {
                Operator::Eq => equality(c, &v),
                Operator::NotEq => equality(c, &v)
                    .map(|p| (1.0 - c.nulls as f64 / c.observations.max(1) as f64 - p).max(0.0)),
                Operator::Lt | Operator::LtEq | Operator::Gt | Operator::GtEq
                    if !c.histogram.is_empty() =>
                {
                    let n = c
                        .histogram
                        .iter()
                        .filter(|x| {
                            let o = super::collect::numeric_cmp(x, &v);
                            match op {
                                Operator::Lt => o.is_lt(),
                                Operator::LtEq => !o.is_gt(),
                                Operator::Gt => o.is_gt(),
                                _ => !o.is_lt(),
                            }
                        })
                        .count();
                    Some((n as f64 / c.histogram.len() as f64).clamp(0.001, 0.999))
                }
                _ => None,
            }
        }
        Expr::IsNull(e) => nonnull(e, columns).map(|p| 1.0 - p),
        Expr::IsNotNull(e) => nonnull(e, columns),
        Expr::InList(list) => {
            let c = column(&list.expr, columns)?;
            let mut values = BTreeSet::new();
            let mut p = 0.0;
            for v in &list.list {
                let v = literal(v)?;
                if values.insert(v.clone()) {
                    p += equality(c, &v)?;
                }
            }
            Some(if list.negated {
                1.0 - p.min(1.0)
            } else {
                p.min(1.0)
            })
        }
        Expr::Not(e) => selectivity(e, columns).map(|p| 1.0 - p),
        Expr::IsTrue(e) => selectivity(e, columns),
        Expr::Literal(ScalarValue::Boolean(Some(v)), _) => Some(if *v { 1.0 } else { 0.0 }),
        _ => None,
    }
}
// Frontends construct RDF terms and typed graph identities with pure expressions.
// Preserve their NDV/null estimates without treating generated values as evidence.
fn nonnull(e: &Expr, columns: &BTreeMap<String, ColumnStatistics>) -> Option<f64> {
    if let Some(c) = column(e, columns) {
        return Some(1.0 - c.nulls as f64 / c.observations.max(1) as f64);
    }
    match e {
        Expr::Literal(v, _) => Some(if v.is_null() { 0.0 } else { 1.0 }),
        Expr::Alias(a) => nonnull(&a.expr, columns),
        Expr::Cast(c) => nonnull(&c.expr, columns),
        Expr::Case(c) if c.expr.is_none() => {
            let mut remaining = 1.0;
            let mut probability = 0.0;
            for (when, then) in &c.when_then_expr {
                let p = selectivity(when, columns)?;
                probability += remaining * p * nonnull(then, columns)?;
                remaining *= 1.0 - p;
            }
            Some(
                probability
                    + remaining
                        * c.else_expr
                            .as_deref()
                            .map(|e| nonnull(e, columns))
                            .unwrap_or(Some(0.0))?,
            )
        }
        Expr::ScalarFunction(f) if ["concat", "named_struct", "make_array"].contains(&f.name()) => {
            Some(1.0)
        }
        Expr::ScalarFunction(f) if f.name() == "encode" => nonnull(f.args.first()?, columns),
        _ => None,
    }
}
fn projected_column(e: &Expr, s: &State) -> Option<ColumnStatistics> {
    if let Some(c) = column(e, &s.columns) {
        return Some(c.clone());
    }
    if let Expr::Alias(a) = e {
        return projected_column(&a.expr, s);
    }
    if let Expr::Cast(c) = e {
        return projected_column(&c.expr, s);
    }
    if let Expr::Literal(v, _) = e {
        return Some(ColumnStatistics {
            observations: 1,
            nulls: u64::from(v.is_null()),
            sample_distinct: u64::from(!v.is_null()),
            estimated_distinct: Some(if v.is_null() { 0.0 } else { 1.0 }),
            frequent: scalar(v).map(|v| vec![(v, 1)]).unwrap_or_default(),
            ..Default::default()
        });
    }
    match e {
        Expr::Case(_) => (),
        Expr::ScalarFunction(f) if ["concat", "encode", "named_struct"].contains(&f.name()) => (),
        _ => return None,
    }
    let mut ndv = 1.0;
    let mut width = 0.0;
    for c in e.column_refs() {
        let c = column(&Expr::Column(c.clone()), &s.columns)?;
        ndv = (ndv
            * c.estimated_distinct
                .unwrap_or(c.sample_distinct as f64)
                .max(1.0))
        .min(s.rows?);
        width += c.average_width;
    }
    Some(ColumnStatistics {
        observations: 10000,
        nulls: ((1.0 - nonnull(e, &s.columns)?) * 10000.0) as u64,
        sample_distinct: ndv as u64,
        estimated_distinct: Some(ndv),
        average_width: width,
        ..Default::default()
    })
}
fn joint_selectivity(e: &Expr, s: &State) -> Option<f64> {
    fn terms(e: &Expr, values: &mut BTreeMap<String, String>) -> Option<()> {
        if let Expr::BinaryExpr(b) = e {
            if b.op == Operator::And {
                terms(&b.left, values)?;
                terms(&b.right, values)?;
                return Some(());
            }
            if b.op == Operator::Eq {
                if let Expr::Column(c) = b.left.as_ref() {
                    values.insert(c.name.clone(), literal(&b.right)?);
                    return Some(());
                }
                if let Expr::Column(c) = b.right.as_ref() {
                    values.insert(c.name.clone(), literal(&b.left)?);
                    return Some(());
                }
            }
        }
        None
    }
    let mut values = BTreeMap::new();
    terms(e, &mut values)?;
    if values.len() < 2 {
        return None;
    }
    let g = s.groups.iter().find(|g| {
        g.observations > 0
            && g.columns.len() == values.len()
            && g.columns.iter().all(|c| values.contains_key(c))
    })?;
    let tuple = g
        .columns
        .iter()
        .map(|c| values[c].clone())
        .collect::<Vec<_>>();
    Some(
        g.frequent
            .iter()
            .find(|(v, _)| v == &tuple)
            .map(|(_, n)| *n as f64 / g.observations as f64)
            .unwrap_or(1.0 / (g.observations + 1) as f64),
    )
}
fn filter(s: &mut State, e: &Expr) {
    if let Some(p) = joint_selectivity(e, s) {
        let key = e.to_string();
        if s.filters.insert(key) {
            s.rows = s.rows.map(|r| r * p);
        }
        return;
    }
    if let Expr::BinaryExpr(b) = e {
        if b.op == Operator::And {
            filter(s, &b.left);
            filter(s, &b.right);
            return;
        }
    }
    // Canonicalize qualifier spelling only for duplicate residual-filter accounting.
    use datafusion::common::tree_node::{Transformed, TreeNode};
    let key = e
        .clone()
        .transform_up(|e| {
            if let Expr::Column(c) = e {
                Ok::<_, datafusion::common::DataFusionError>(Transformed::yes(Expr::Column(
                    datafusion::common::Column::from_name(c.name),
                )))
            } else {
                Ok(Transformed::no(e))
            }
        })
        .map(|e| e.data.to_string())
        .unwrap_or_else(|_| e.to_string());
    if s.filters.insert(key) {
        if let Some(p) = selectivity(e, &s.columns) {
            s.rows = s.rows.map(|r| r * p);
        } else {
            s.rows = None;
        }
    }
}
// Preserve tuple identity and endpoint skew when matching distributions are available.
// This is a cost estimate, never evidence that a join or edge is redundant.
fn join_probability(left: &State, right: &State, on: &[(Expr, Expr)]) -> Option<f64> {
    fn distribution(
        s: &State,
        expressions: Vec<&Expr>,
    ) -> Option<(f64, f64, Vec<(Vec<String>, f64)>)> {
        let names = expressions
            .iter()
            .map(|e| {
                if let Expr::Column(c) = e {
                    Some(c.name.clone())
                } else {
                    None
                }
            })
            .collect::<Option<Vec<_>>>()?;
        if names.len() == 1 {
            let c = column(expressions[0], &s.columns)?;
            return Some((
                c.observations as f64,
                c.estimated_distinct.unwrap_or(c.sample_distinct as f64),
                c.frequent
                    .iter()
                    .map(|(v, n)| (vec![v.clone()], *n as f64))
                    .collect(),
            ));
        }
        let g = s.groups.iter().find(|g| {
            g.columns.len() == names.len() && names.iter().all(|n| g.columns.contains(n))
        })?;
        let indices = names
            .iter()
            .map(|n| g.columns.iter().position(|c| c == n).unwrap())
            .collect::<Vec<_>>();
        Some((
            g.observations as f64,
            g.sample_distinct as f64,
            g.frequent
                .iter()
                .map(|(v, n)| (indices.iter().map(|i| v[*i].clone()).collect(), *n as f64))
                .collect(),
        ))
    }
    if on.is_empty() {
        return Some(1.0);
    }
    let (an, ad, a) = distribution(left, on.iter().map(|(a, _)| a).collect())?;
    let (bn, bd, b) = distribution(right, on.iter().map(|(_, b)| b).collect())?;
    if an == 0.0 || bn == 0.0 {
        return None;
    }
    let mut p = 0.0;
    let mut matched_a = 0.0;
    let mut matched_b = 0.0;
    let mut matches = 0.0;
    for (key, n) in &a {
        if key.iter().any(|v| v == "null") {
            continue;
        }
        if let Some((_, m)) = b.iter().find(|(v, _)| v == key) {
            p += n / an * m / bn;
            matched_a += n / an;
            matched_b += m / bn;
            matches += 1.0;
        }
    }
    // Remaining domains may overlap; absence from a bounded sample is not exclusion.
    p += (1.0 - matched_a).max(0.0) * (1.0 - matched_b).max(0.0) / (ad.max(bd) - matches).max(1.0);
    Some(p.clamp(0.0, 1.0))
}
fn run(plan: &LogicalPlan, path: String, out: &mut Vec<PlanEstimate>) -> State {
    let mut children = plan
        .inputs()
        .iter()
        .enumerate()
        .map(|(i, p)| run(p, format!("{path}.{i}"), out))
        .collect::<Vec<_>>();
    let mut s = children.first().cloned().unwrap_or_default();
    match plan {
        LogicalPlan::TableScan(scan) => {
            if let Ok(p) = source_as_provider(&scan.source) {
                if let Some(p) = statistics_provider(&p) {
                    s.groups = p.source.groups.clone();
                    s.rows = p.source.estimated_rows;
                    s.input = s.rows;
                    s.bytes = p.source.estimated_bytes;
                    s.work = s.bytes.zip(s.input).map(|(b, r)| b + 8.0 * r);
                    s.revisions.insert(p.revision.clone());
                    for (name, c) in &p.source.columns {
                        s.columns.insert(name.clone(), c.clone());
                        s.columns
                            .insert(format!("{}.{}", scan.table_name, name), c.clone());
                    }
                    for e in &scan.filters {
                        filter(&mut s, e);
                    }
                }
            }
        }
        LogicalPlan::Filter(f) => filter(&mut s, &f.predicate),
        LogicalPlan::Projection(p) => {
            let mut aliases = BTreeMap::new();
            for (expr, (_, f)) in p.expr.iter().zip(p.schema.iter()) {
                let expr = if let Expr::Alias(a) = expr {
                    a.expr.as_ref()
                } else {
                    expr
                };
                if let Expr::Column(c) = expr {
                    aliases.insert(c.name.clone(), f.name().clone());
                }
            }
            s.groups = s
                .groups
                .iter()
                .filter_map(|g| {
                    let cols = g
                        .columns
                        .iter()
                        .map(|c| aliases.get(c).cloned())
                        .collect::<Option<Vec<_>>>()?;
                    let mut g = g.clone();
                    g.columns = cols;
                    Some(g)
                })
                .collect();
            let mut columns = BTreeMap::new();
            for (expr, (q, f)) in p.expr.iter().zip(p.schema.iter()) {
                if let Some(c) = projected_column(expr, &s) {
                    columns.insert(f.name().clone(), c.clone());
                    if let Some(q) = q {
                        columns.insert(format!("{q}.{}", f.name()), c.clone());
                    }
                }
            }
            s.columns = columns;
        }
        LogicalPlan::SubqueryAlias(a) => {
            let old = s.columns.clone();
            for (_, f) in a.schema.iter() {
                if let Some(c) = old.get(f.name()) {
                    s.columns
                        .insert(format!("{}.{}", a.alias, f.name()), c.clone());
                }
            }
        }
        LogicalPlan::Unnest(u) => {
            let mut mean = None;
            for c in &u.exec_columns {
                if let Some(c) = s
                    .columns
                    .get(&c.flat_name())
                    .or_else(|| s.columns.get(&c.name))
                {
                    if let Some(list) = &c.list {
                        let m = list.total_lengths as f64 / list.parents.max(1) as f64;
                        mean = Some(mean.unwrap_or(0.0f64).max(m));
                    }
                }
            }
            s.rows = s.rows.zip(mean).map(|(r, m)| r * m);
            s.expanded = add(s.expanded.or(Some(0.0)), s.rows);
            s.work = add(s.work, s.rows.map(|r| 8.0 * r));
        }
        LogicalPlan::Aggregate(a) => {
            if a.group_expr.is_empty() {
                s.rows = Some(1.0);
            } else {
                let ndv = a.group_expr.iter().try_fold(1.0, |n, e| {
                    column(e, &s.columns).map(|c| {
                        n * c
                            .estimated_distinct
                            .unwrap_or(c.sample_distinct as f64)
                            .max(1.0)
                    })
                });
                s.rows = s.rows.zip(ndv).map(|(r, n)| r.min(n));
            }
            s.work = add(
                s.work,
                children[0].rows.or(children[0].input).map(|r| 8.0 * r),
            );
        }
        LogicalPlan::Distinct(d) => {
            let expressions = match d {
                Distinct::All(input) => input
                    .schema()
                    .columns()
                    .into_iter()
                    .map(Expr::Column)
                    .collect::<Vec<_>>(),
                Distinct::On(on) => on.on_expr.clone(),
            };
            let names = expressions
                .iter()
                .map(|e| match e {
                    Expr::Column(c) => Some(c.name.clone()),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>();
            let joint = names.and_then(|names| {
                s.groups
                    .iter()
                    .find(|g| {
                        g.columns.len() == names.len()
                            && g.columns.iter().all(|c| names.contains(c))
                    })
                    .map(|g| g.sample_distinct as f64)
            });
            let ndv = joint.or_else(|| {
                expressions.iter().try_fold(1.0, |n, e| {
                    column(e, &s.columns).map(|c| {
                        n * (c.estimated_distinct.unwrap_or(c.sample_distinct as f64)
                            + if c.nulls > 0 { 1.0 } else { 0.0 })
                    })
                })
            });
            s.rows = s.rows.zip(ndv).map(|(r, n)| r.min(n));
            s.work = add(
                s.work,
                children[0].rows.or(children[0].input).map(|r| 8.0 * r),
            );
        }
        LogicalPlan::Join(j) => {
            let r = children.get(1).cloned().unwrap_or_default();
            let lrows = s.rows;
            let rrows = r.rows;
            let mut keys = j.on.clone();
            let mut residuals = Vec::new();
            fn split_join(
                e: &Expr,
                j: &datafusion::logical_expr::Join,
                keys: &mut Vec<(Expr, Expr)>,
                residuals: &mut Vec<Expr>,
            ) {
                if let Expr::BinaryExpr(b) = e {
                    if b.op == Operator::And {
                        split_join(&b.left, j, keys, residuals);
                        split_join(&b.right, j, keys, residuals);
                        return;
                    }
                    if b.op == Operator::Eq {
                        let a = b.left.column_refs();
                        let c = b.right.column_refs();
                        if !a.is_empty() && !c.is_empty() {
                            if a.iter().all(|c| j.left.schema().index_of_column(c).is_ok())
                                && c.iter()
                                    .all(|c| j.right.schema().index_of_column(c).is_ok())
                            {
                                keys.push((*b.left.clone(), *b.right.clone()));
                                return;
                            }
                            if a.iter()
                                .all(|c| j.right.schema().index_of_column(c).is_ok())
                                && c.iter().all(|c| j.left.schema().index_of_column(c).is_ok())
                            {
                                keys.push((*b.right.clone(), *b.left.clone()));
                                return;
                            }
                        }
                    }
                }
                residuals.push(e.clone());
            }
            if let Some(e) = &j.filter {
                split_join(e, j, &mut keys, &mut residuals);
            }
            let mut scale = Some(1.0);
            for (a, b) in &keys {
                scale = scale
                    .zip(projected_column(a, &s).zip(projected_column(b, &r)))
                    .map(|(x, (a, b))| {
                        x / a
                            .estimated_distinct
                            .unwrap_or(a.sample_distinct as f64)
                            .max(b.estimated_distinct.unwrap_or(b.sample_distinct as f64))
                            .max(1.0)
                    });
            }
            let scale = join_probability(&s, &r, &keys).or(scale);
            let inner = lrows
                .zip(rrows)
                .zip(scale)
                .map(|((a, b), p)| (a * b * p).min(1e300));
            s.rows = match j.join_type {
                JoinType::Inner => inner,
                JoinType::Left => inner.zip(lrows).map(|(i, l)| i.max(l)),
                JoinType::Right => inner.zip(rrows).map(|(i, r)| i.max(r)),
                JoinType::Full => inner
                    .zip(lrows.zip(rrows))
                    .map(|(i, (l, r))| i.max(l).max(r)),
                JoinType::LeftSemi => inner.zip(lrows).map(|(i, l)| i.min(l)),
                JoinType::LeftAnti => inner.zip(lrows).map(|(i, l)| (l - i.min(l)).max(0.0)),
                _ => None,
            };
            s.bytes = add(s.bytes, r.bytes);
            s.input = add(s.input, r.input);
            s.expanded = add(s.expanded.or(Some(0.0)), r.expanded.or(Some(0.0)));
            s.work = add(
                add(s.work, r.work),
                s.rows
                    .zip(lrows.zip(rrows))
                    .map(|(n, (l, r))| 8.0 * n + l + 4.0 * r),
            );
            s.revisions.extend(r.revisions);
            s.columns.extend(r.columns);
            s.groups.extend(r.groups);
            s.filters.extend(r.filters);
            for e in &residuals {
                filter(&mut s, e);
            }
        }
        LogicalPlan::Union(_) => {
            s.rows = Some(0.0);
            s.bytes = Some(0.0);
            s.input = Some(0.0);
            s.work = Some(0.0);
            for c in children.drain(..) {
                s.rows = add(s.rows, c.rows);
                s.bytes = add(s.bytes, c.bytes);
                s.input = add(s.input, c.input);
                s.work = add(s.work, c.work);
                s.revisions.extend(c.revisions);
            }
        }
        LogicalPlan::Limit(l) => {
            if let Some(n) = l
                .fetch
                .as_deref()
                .and_then(literal)
                .and_then(|x| x.parse::<f64>().ok())
            {
                s.rows = s.rows.map(|r| r.min(n));
            }
        }
        LogicalPlan::EmptyRelation(e) => {
            s.rows = Some(if e.produce_one_row { 1.0 } else { 0.0 });
            s.bytes = Some(0.0);
            s.input = Some(0.0);
            s.work = Some(0.0);
        }
        _ => {}
    }
    if !s.revisions.is_empty() {
        out.push(PlanEstimate {
            occurrence: path,
            operator: plan.display().to_string(),
            estimated_rows: s.rows,
            estimated_row_width: plan.schema().fields().iter().try_fold(0.0, |n, f| {
                s.columns.get(f.name()).map(|c| n + c.average_width)
            }),
            estimated_scan_bytes: s.bytes,
            estimated_input_rows: s.input,
            estimated_expanded_rows: s.expanded,
            estimated_work: s.work,
            statistics_revisions: s.revisions.iter().cloned().collect(),
            method: "collected distributions; independent-predicate/NDV fallback; unknown filter work uses source input; estimates only"
                .into(),
        });
    }
    s
}
pub fn explain(plan: &LogicalPlan) -> Vec<PlanEstimate> {
    stacker::maybe_grow(8 * 1024 * 1024, 32 * 1024 * 1024, || {
        use datafusion::common::tree_node::TreeNodeRecursion;
        let mut found = false;
        let _ = plan.apply_with_subqueries(|p| {
            if let LogicalPlan::TableScan(s) = p {
                if let Ok(p) = source_as_provider(&s.source) {
                    found |= statistics_provider(&p).is_some();
                }
            }
            Ok::<_, datafusion::common::DataFusionError>(TreeNodeRecursion::Continue)
        });
        if !found {
            return vec![];
        }
        let mut out = Vec::new();
        run(plan, "0".into(), &mut out);
        out
    })
}
pub fn estimate(plan: &LogicalPlan) -> PlanEstimate {
    explain(plan).pop().unwrap_or_default()
}
pub(super) fn filter_probability(plan: &LogicalPlan, e: &Expr) -> Option<f64> {
    let s = run(plan, "0".into(), &mut Vec::new());
    selectivity(e, &s.columns)
}
pub(super) fn input_rows(plan: &LogicalPlan) -> Option<f64> {
    run(plan, "0".into(), &mut Vec::new()).rows
}

/// Source cost and frontier-key NDV for the native access selector. Hints only.
pub(crate) fn source_access_cost(
    plan: &LogicalPlan,
    keys: &[String],
) -> Option<(f64, f64, f64, f64)> {
    let s = run(plan, "0".into(), &mut Vec::new());
    let ndv = if keys.len() == 1 {
        let c = column(
            &Expr::Column(datafusion::common::Column::from_name(&keys[0])),
            &s.columns,
        )?;
        c.estimated_distinct.unwrap_or(c.sample_distinct as f64)
    } else {
        s.groups.iter().find(|g| g.columns == keys)?.sample_distinct as f64
    };
    Some((s.rows?, s.bytes?, ndv, s.work?))
}
