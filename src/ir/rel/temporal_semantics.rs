//! Operand-sensitive fences for the lossless temporal transport envelope.
//!
//! Transport uses a nominal string domain, never a temporal comparison key.
//! An unrelated temporal column must not prohibit ordinary integer/string SQL.
use super::*;
use arrow::array::Array;
mod runtime_date;
pub(super) mod runtime_typed;
pub(super) use runtime_date::lower_runtime_date;

fn binding_type<'a>(plan: &'a LogicalPlan, binding: &str) -> Option<&'a DataType> {
    let column = resolve_column_name(plan, binding)?;
    plan.schema()
        .field_with_unqualified_name(&column)
        .ok()
        .map(|field| field.data_type())
}

fn list_type(kind: &DataType) -> bool {
    matches!(
        kind,
        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
    )
}

/// Cardinality does not inspect temporal payloads. Limit this exemption to a
/// direct typed list operand; nested expressions still lower recursively and
/// retain their own semantic/error/evaluation guards.
fn direct_list(plan: &LogicalPlan, expr: &IrExpr) -> bool {
    match expr {
        IrExpr::Binding(binding) => binding_type(plan, binding).is_some_and(list_type),
        IrExpr::Lit(Lit::Scalar(value)) => list_type(&value.data_type()),
        _ => false,
    }
}

/// True if this expression may interpret transported temporal values. False
/// permits ordinary lowering, which remains responsible for argument typing.
/// Unknown binding types conservatively inherit the original schema fence.
pub(super) fn needs_temporal_semantics(plan: &LogicalPlan, expr: &IrExpr) -> bool {
    let unknown = || {
        plan.schema()
            .fields()
            .iter()
            .any(|field| crate::ir::temporal::transport::contains_type(field.data_type()))
    };
    match expr {
        IrExpr::Binding(binding) => binding_type(plan, binding)
            .map(crate::ir::temporal::transport::contains_type)
            .unwrap_or_else(unknown),
        IrExpr::Lit(Lit::Scalar(value)) => {
            crate::ir::temporal::transport::contains_type(&value.data_type())
        }
        IrExpr::Lit(_) => false,
        // NULL checks operate on the envelope's null bitmap, not its payload.
        // Their operands are still lowered and validated independently.
        IrExpr::IsNull(_) | IrExpr::IsNotNull(_) | IrExpr::IsBound(_) => false,
        IrExpr::Call { name, args } => {
            if name == "size" && args.len() == 1 && direct_list(plan, &args[0]) {
                return false;
            }
            name.starts_with("cypher_temporal.")
                || args.iter().any(|arg| needs_temporal_semantics(plan, arg))
        }
        IrExpr::Binary { lhs, rhs, .. } => {
            needs_temporal_semantics(plan, lhs) || needs_temporal_semantics(plan, rhs)
        }
        IrExpr::Not(inner) => needs_temporal_semantics(plan, inner),
        IrExpr::StringPredicate {
            target, pattern, ..
        } => needs_temporal_semantics(plan, target) || needs_temporal_semantics(plan, pattern),
        IrExpr::List(items) => items
            .iter()
            .any(|item| needs_temporal_semantics(plan, item)),
        IrExpr::Case { arms, otherwise } => {
            arms.iter().any(|(a, b)| {
                needs_temporal_semantics(plan, a) || needs_temporal_semantics(plan, b)
            }) || otherwise
                .as_deref()
                .is_some_and(|other| needs_temporal_semantics(plan, other))
        }
        IrExpr::Property { binding, name, .. } => {
            let property = prop_col(binding, name);
            if let Ok(field) = plan.schema().field_with_unqualified_name(&property) {
                crate::ir::temporal::transport::contains_type(field.data_type())
            } else {
                binding_type(plan, binding)
                    .map(crate::ir::temporal::transport::contains_type)
                    .unwrap_or_else(unknown)
            }
        }
        // Lambda bindings need their element types, not the outer schema.
        // Keep the conservative fence until collection lowering supplies them.
        IrExpr::ListTransform { .. } | IrExpr::ListFilter { .. } | IrExpr::ListReduce { .. } | IrExpr::Quantifier { .. } => {
            unknown() || super::constants::has_temporal_constructor(expr)
        }
        IrExpr::Id(_) | IrExpr::Label(_) | IrExpr::HasLabel { .. } | IrExpr::SimplePath(_) => false,
    }
}

// Bound generated CASE size independently of input row count. This is a
// specialization for compiler-owned finite values, never a sample of data.
const MAX_ORDER_DOMAIN: usize = 64;

/// Read-only provider for compiler-owned graph snapshots. The inner MemTable
/// is private, so later inserts cannot invalidate the finite-domain proof.
#[derive(Debug)]
pub(super) struct Snapshot {
    inner: MemTable,
    domains: BTreeMap<String, Vec<ScalarValue>>,
    batches: Vec<RecordBatch>,
}
impl Snapshot {
    pub(super) fn batches(&self) -> &[RecordBatch] {
        &self.batches
    }
    pub(super) fn new(inner: MemTable, batches: &[RecordBatch]) -> Self {
        let mut domains = BTreeMap::new();
        // Exact domain discovery is optional and bounded independently of
        // graph size. Keep all table data when the proof budget is exceeded.
        if batches.iter().map(RecordBatch::num_rows).sum::<usize>() <= 4096
            && let Some(batch) = batches.first()
        {
            for (index, field) in batch.schema().fields().iter().enumerate() {
                if !crate::ir::temporal::transport::is_type(field.data_type())
                    && !matches!(field.data_type(), DataType::Int32 | DataType::Int64 | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)
                    && !(field.name().ends_with("__cypher_labels")
                        && matches!(field.data_type(), DataType::List(item) if item.data_type() == &DataType::Utf8))
                {
                    continue;
                }
                let mut values = Vec::new();
                let mut valid = true;
                'batches: for batch in batches {
                    for row in 0..batch.num_rows() {
                        let Ok(value) = ScalarValue::try_from_array(batch.column(index), row)
                        else {
                            valid = false;
                            break 'batches;
                        };
                        if !values.contains(&value) {
                            values.push(value);
                        }
                        if values.len() > MAX_ORDER_DOMAIN {
                            valid = false;
                            break 'batches;
                        }
                    }
                }
                if valid {
                    domains.insert(field.name().clone(), values);
                }
            }
        }
        Self {
            inner,
            domains,
            batches: batches.to_vec(),
        }
    }
}
#[async_trait::async_trait]
impl datafusion::catalog::TableProvider for Snapshot {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
    fn schema(&self) -> arrow::datatypes::SchemaRef {
        self.inner.schema()
    }
    fn constraints(&self) -> Option<&datafusion::common::Constraints> {
        self.inner.constraints()
    }
    fn table_type(&self) -> datafusion::logical_expr::TableType {
        self.inner.table_type()
    }
    async fn scan(
        &self,
        state: &dyn datafusion::catalog::Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> datafusion::common::Result<Arc<dyn datafusion::physical_plan::ExecutionPlan>> {
        self.inner.scan(state, projection, filters, limit).await
    }
}

pub(super) fn finite_values(plan: &LogicalPlan, column: &str) -> Option<Vec<ScalarValue>> {
    fn expression_values(plan: &LogicalPlan, expr: &Expr) -> Option<Vec<ScalarValue>> {
        // A finite result set alone does not prove single evaluation when a
        // later dictionary lookup repeats an expression through projections.
        if expr.is_volatile() { return None; }
        match expr {
            Expr::Alias(alias) => expression_values(plan, &alias.expr),
            Expr::Column(column) => finite_values(plan, &column.name),
            Expr::Literal(value, _) => Some(vec![value.clone()]),
            Expr::Case(case) => {
                let mut values = Vec::new();
                for branch in case
                    .when_then_expr
                    .iter()
                    .map(|(_, then)| then.as_ref())
                    .chain(case.else_expr.as_deref())
                {
                    for value in expression_values(plan, branch)? {
                        if !values.contains(&value) {
                            values.push(value);
                        }
                        if values.len() > MAX_ORDER_DOMAIN {
                            return None;
                        }
                    }
                }
                // SQL CASE without ELSE has an implicit NULL result. Its
                // domain must include that branch even though no expression
                // node represents it (e.g. NULL comparison OR TRUE is TRUE).
                if case.else_expr.is_none() {
                    let null = ScalarValue::try_from(&expr.get_type(plan.schema()).ok()?).ok()?;
                    if !values.contains(&null) {
                        values.push(null);
                    }
                    if values.len() > MAX_ORDER_DOMAIN {
                        return None;
                    }
                }
                Some(values)
            }
            Expr::ScalarFunction(call) if call.func.name() == "make_array" => {
                if call.args.len() > MAX_ORDER_DOMAIN {
                    return None;
                }
                let elements = call
                    .args
                    .iter()
                    .map(|arg| {
                        let values = expression_values(plan, arg)?;
                        if values.len() == 1 {
                            values.into_iter().next()
                        } else {
                            None
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;
                let kind = elements.iter().find(|v| !v.is_null())?.data_type();
                let elements = elements
                    .into_iter()
                    .map(|value| {
                        if value.is_null() {
                            ScalarValue::try_from(&kind).ok()
                        } else if value.data_type() == kind {
                            Some(value)
                        } else {
                            None
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;
                Some(vec![ScalarValue::List(ScalarValue::new_list_nullable(
                    &elements, &kind,
                ))])
            }
            _ => None,
        }
    }
    match plan {
        LogicalPlan::Projection(projection) => {
            let index = projection.schema.index_of_column_by_name(None, column)?;
            expression_values(&projection.input, projection.expr.get(index)?)
        }
        LogicalPlan::Aggregate(aggregate) => {
            let index = aggregate.schema.index_of_column_by_name(None, column)?;
            // Group keys retain their input domains. Aggregate result values
            // are not inferred from input extrema or population samples.
            expression_values(&aggregate.input, aggregate.group_expr.get(index)?)
        }
        LogicalPlan::SubqueryAlias(alias) => finite_values(&alias.input, column),
        LogicalPlan::Filter(filter) => finite_values(&filter.input, column),
        LogicalPlan::Sort(sort) => finite_values(&sort.input, column),
        LogicalPlan::Limit(limit) => finite_values(&limit.input, column),
        LogicalPlan::Unnest(unnest) => {
            let output_index = unnest.schema.index_of_column_by_name(None, column)?;
            let input_index = *unnest.dependency_indices.get(output_index)?;
            let input_field = unnest.input.schema().field(input_index);
            let values = finite_values(&unnest.input, input_field.name())?;
            if !unnest.list_type_columns.iter().any(|(index, _)| *index == input_index)
                && !unnest.struct_type_columns.contains(&input_index)
            {
                return Some(values);
            }
            // A single list level may expose temporal or primitive scalars.
            if !crate::ir::temporal::transport::is_type(
                unnest.schema.field(output_index).data_type(),
            ) && !matches!(unnest.schema.field(output_index).data_type(), DataType::Int64 | DataType::Int32 | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) {
                return None;
            }
            let mut result = Vec::new();
            for value in values {
                let ScalarValue::List(array) = value else {
                    return None;
                };
                let elements = array.value(0);
                if array.is_null(0) || elements.is_empty() {
                    if unnest.options.preserve_nulls {
                        result.push(ScalarValue::try_from(unnest.schema.field(output_index).data_type()).ok()?);
                        if result.len() > MAX_ORDER_DOMAIN { return None; }
                    }
                    continue;
                }
                if result.len() + elements.len() > MAX_ORDER_DOMAIN {
                    return None;
                }
                for index in 0..elements.len() {
                    result.push(ScalarValue::try_from_array(&elements, index).ok()?);
                }
            }
            Some(result)
        }
        LogicalPlan::TableScan(scan) => {
            let provider = datafusion::datasource::source_as_provider(&scan.source).ok()?;
            provider
                .as_any()
                .downcast_ref::<Snapshot>()?
                .domains
                .get(column)
                .cloned()
        }
        LogicalPlan::Union(union) => {
            let index = union.schema.index_of_column_by_name(None, column)?;
            let mut values = Vec::new();
            for input in &union.inputs {
                for value in finite_values(input, input.schema().field(index).name())? {
                    if !values.contains(&value) {
                        values.push(value);
                    }
                    if values.len() > MAX_ORDER_DOMAIN {
                        return None;
                    }
                }
            }
            Some(values)
        }
        LogicalPlan::Join(join) => {
            // A unique column's domain survives joins; outer joins can only
            // add NULL, which dictionary CASE expressions preserve.
            let left = join
                .left
                .schema()
                .index_of_column_by_name(None, column)
                .is_some();
            let right = join
                .right
                .schema()
                .index_of_column_by_name(None, column)
                .is_some();
            let input = match (left, right) {
                (true, false) => &join.left,
                (false, true) => &join.right,
                _ => return None,
            };
            let mut values = finite_values(input, column)?;
            let null_extended = match join.join_type {
                JoinType::Full => true,
                JoinType::Left => right,
                JoinType::Right => left,
                _ => false,
            };
            if null_extended { values.push(
                ScalarValue::try_from(
                    input
                        .schema()
                        .field_with_unqualified_name(column)
                        .ok()?
                        .data_type(),
                )
                .ok()?,
            ); }
            Some(values)
        }
        LogicalPlan::Window(window)
            if window
                .input
                .schema()
                .index_of_column_by_name(None, column)
                .is_some() =>
        {
            finite_values(&window.input, column)
        }
        _ => None,
    }
}

/// An exact semantic order key for a small, proven constant temporal domain.
/// Serialization is used only for exact lookup of known values; ranks derive
/// exclusively from the shared temporal comparator. Ties retain equal ranks.
pub(super) fn finite_order_key(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    expr: &IrExpr,
) -> Option<Expr> {
    // Cypher's frontend explicitly marks total-order operands. The finite
    // dictionary supplies that operation itself, rather than lowering the
    // marker as an ordinary scalar call on its serialized argument.
    let expr = match expr {
        IrExpr::Call { name, args } if name == "cypher_order_key" && args.len() == 1 => &args[0],
        expr => expr,
    };
    let (operand, values) = match expr {
        IrExpr::Binding(binding) => {
            if !binding_type(plan, binding).is_some_and(crate::ir::temporal::transport::is_type) {
                return None;
            }
            let column = resolve_column_name(plan, binding)?;
            (col_exact(&column), finite_values(plan, &column)?)
        }
        IrExpr::Property { binding, name, .. } if has_exact_col(plan, &prop_col(binding, name)) => {
            let column = prop_col(binding, name);
            (col_exact(&column), finite_values(plan, &column)?)
        }
        _ => finite_expression(context, plan, expr)?,
    };
    if values.len() > MAX_ORDER_DOMAIN {
        return None;
    }
    let mut decoded = Vec::new();
    for value in values {
        if value.is_null() {
            continue;
        }
        let temporal = crate::ir::temporal::transport::decode(&value).ok()?;
        if !decoded.iter().any(|(existing, _)| existing == &value) {
            decoded.push((value, temporal));
        }
    }
    if decoded.is_empty() {
        return Some(lit(ScalarValue::Int64(None)));
    }
    // When partial evaluation already produced a CASE, replace its temporal
    // results with ranks directly. This avoids repeating the full dictionary
    // once per output value (quadratic SQL growth).
    if let Expr::Case(case) = &operand {
        let rank_value = |expr: &Expr| -> Option<Box<Expr>> {
            let Expr::Literal(value, _) = expr else {
                return None;
            };
            if value.is_null() {
                return Some(Box::new(lit(ScalarValue::Int64(None))));
            }
            let (_, temporal) = decoded.iter().find(|(known, _)| known == value)?;
            let mut rank = 0_i64;
            for (_, other) in &decoded {
                if other.compare(temporal)? == std::cmp::Ordering::Less {
                    rank += 1;
                }
            }
            Some(Box::new(lit(rank)))
        };
        let arms = case
            .when_then_expr
            .iter()
            .map(|(when, then)| Some((when.clone(), rank_value(then)?)))
            .collect::<Option<Vec<_>>>()?;
        let otherwise = match case.else_expr.as_deref() {
            Some(expr) => Some(rank_value(expr)?),
            None => None,
        };
        return Some(Expr::Case(Case::new(case.expr.clone(), arms, otherwise)));
    }
    let mut arms = Vec::with_capacity(decoded.len());
    for (value, temporal) in &decoded {
        let mut rank = 0_i64;
        for (_, other) in &decoded {
            match other.compare(temporal)? {
                std::cmp::Ordering::Less => rank += 1,
                _ => {}
            }
        }
        arms.push((
            Box::new(operand.clone().eq(lit(value.clone()))),
            Box::new(lit(rank)),
        ));
    }
    Some(Expr::Case(Case::new(
        None,
        arms,
        Some(Box::new(lit(ScalarValue::Int64(None)))),
    )))
}

/// Enumerate only proven, small domains and evaluate with the shared language
/// implementation. This is partial evaluation, not serialized-value semantics.
fn finite_expression(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    expr: &IrExpr,
) -> Option<(Expr, Vec<ScalarValue>)> {
    fn rewrite(
        plan: &LogicalPlan,
        expr: &IrExpr,
        leaves: &mut BTreeMap<String, Vec<ScalarValue>>,
    ) -> Option<IrExpr> {
        if expr_is_constant(expr, &[]) {
            return Some(expr.clone());
        }
        match expr {
            IrExpr::Binding(name) => {
                let column = resolve_column_name(plan, name)?;
                leaves
                    .entry(column.clone())
                    .or_insert(finite_values(plan, &column)?);
                Some(IrExpr::Binding(column))
            }
            IrExpr::Property { binding, name, .. }
                if has_exact_col(plan, &prop_col(binding, name)) =>
            {
                let column = prop_col(binding, name);
                leaves
                    .entry(column.clone())
                    .or_insert(finite_values(plan, &column)?);
                Some(IrExpr::Binding(column))
            }
            IrExpr::Property {
                binding,
                name,
                policy,
            } => {
                let column = resolve_column_name(plan, binding)?;
                leaves
                    .entry(column.clone())
                    .or_insert(finite_values(plan, &column)?);
                Some(IrExpr::Property {
                    binding: column,
                    name: name.clone(),
                    policy: *policy,
                })
            }
            IrExpr::Binary { op, lhs, rhs } => Some(IrExpr::Binary {
                op: *op,
                lhs: Box::new(rewrite(plan, lhs, leaves)?),
                rhs: Box::new(rewrite(plan, rhs, leaves)?),
            }),
            IrExpr::Call { name, args }
                if matches!(
                    name.as_str(),
                    "cypher_eq"
                        | "cypher_neq"
                        | "cypher_lt"
                        | "cypher_lte"
                        | "cypher_gt"
                        | "cypher_gte"
                        | "cypher_subscript"
                ) =>
            {
                Some(IrExpr::Call {
                    name: name.clone(),
                    args: args
                        .iter()
                        .map(|arg| rewrite(plan, arg, leaves))
                        .collect::<Option<Vec<_>>>()?,
                })
            }
            _ => None,
        }
    }
    let mut leaves = BTreeMap::new();
    let rewritten = rewrite(plan, expr, &mut leaves)?;
    if leaves.is_empty()
        || !leaves
            .values()
            .flatten()
            .any(|value| crate::ir::temporal::transport::is_type(&value.data_type()))
    {
        return None;
    }
    let count = leaves
        .values()
        .try_fold(1usize, |n, values| n.checked_mul(values.len()))?;
    if count == 0 || count > MAX_ORDER_DOMAIN {
        return None;
    }
    let mut rows = vec![(KernelRow::new(), lit(true))];
    for (column, values) in leaves {
        let mut next = Vec::new();
        for (row, condition) in rows {
            for value in &values {
                let mut row = row.clone();
                let runtime = if value.is_null() {
                    Value::Null
                } else if crate::ir::temporal::transport::is_type(&value.data_type()) {
                    Value::Temporal(crate::ir::temporal::transport::decode(value).ok()?)
                } else {
                    return None;
                };
                row.bindings.insert(column.clone(), runtime);
                let predicate = if value.is_null() {
                    col_exact(&column).is_null()
                } else {
                    col_exact(&column).eq(lit(value.clone()))
                };
                next.push((row, condition.clone().and(predicate)));
            }
        }
        rows = next;
    }
    let mut evaluated = Vec::new();
    let mut kind = None;
    for (row, condition) in rows {
        // An error for any possible combination retains runtime evaluation.
        let value = eval_scalar(&rewritten, &row, context.graph).ok()?;
        let Expr::Literal(value, _) = super::constants::value_literal_expr(&value).ok()? else {
            return None;
        };
        if !value.is_null() {
            if kind.as_ref().is_some_and(|kind| kind != &value.data_type()) {
                return None;
            }
            kind = Some(value.data_type());
        }
        evaluated.push((condition, value));
    }
    let kind = kind.unwrap_or(DataType::Null);
    let mut values = Vec::new();
    let arms = evaluated
        .into_iter()
        .map(|(condition, value)| {
            let value = if value.is_null() {
                ScalarValue::try_from(&kind).ok()?
            } else {
                value
            };
            if !values.contains(&value) {
                values.push(value.clone());
            }
            Some((Box::new(condition), Box::new(lit(value))))
        })
        .collect::<Option<Vec<_>>>()?;
    Some((
        Expr::Case(Case::new(
            None,
            arms,
            Some(Box::new(lit(ScalarValue::try_from(&kind).ok()?))),
        )),
        values,
    ))
}

pub(super) fn lower_finite(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    expr: &IrExpr,
) -> Option<Expr> {
    if !crate::ir::diagnostics::constant_folding_enabled() { return None; }
    if matches!(expr, IrExpr::Binding(_) | IrExpr::Lit(_))
        || matches!(expr, IrExpr::Property { binding, name, .. } if has_exact_col(plan, &prop_col(binding, name)))
        || !needs_temporal_semantics(plan, expr)
    {
        return None;
    }
    finite_expression(context, plan, expr).map(|(expr, _)| expr)
}

#[cfg(test)]
mod finite_domain_tests {
    use super::*;

    #[test]
    fn outer_unnest_empty_or_null_lists_include_null_in_domain() {
        let empty = ScalarValue::List(ScalarValue::new_list(&[], &DataType::Int64, true));
        let null = ScalarValue::try_from(&empty.data_type()).unwrap();
        for source in [empty, null] {
            let input = LogicalPlanBuilder::empty(true)
                .project(vec![lit(source).alias("step")]).unwrap().build().unwrap();
            let mut options = datafusion::common::UnnestOptions::default();
            options.preserve_nulls = true;
            let plan = LogicalPlanBuilder::from(input)
                .unnest_column_with_options(Column::new_unqualified("step"), options)
                .unwrap().build().unwrap();
            assert_eq!(finite_values(&plan, "step"), Some(vec![ScalarValue::Int64(None)]));
        }
    }
}
