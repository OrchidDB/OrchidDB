//! Elementwise three-valued equality for homogeneous primitive lists.
//!
//! SQL audit [L17](../../../tools/diagnostics/sql-lowering-audit.md#l17-list-membership-and-equality): Backend array equality is not assumed to implement three-valued
//! language equality. Keep element alignment, cardinality, null, and effect proofs.
use super::*;
use arrow::array::Array;
use datafusion::common::ExprSchema;
use datafusion::common::tree_node::{Transformed, TreeNode};
use datafusion::logical_expr::Subquery;

impl LoweringContext<'_> {
    pub(super) fn lower_primitive_list_equality(
        &self, plan: &LogicalPlan, lhs: Expr, rhs: Expr, op: BinaryOp,
    ) -> RelResult<Option<Expr>> {
        if !matches!(op, BinaryOp::Eq | BinaryOp::Neq | BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte) { return Ok(None); }
        let ordering = !matches!(op, BinaryOp::Eq | BinaryOp::Neq);
        let lt = lhs.get_type(plan.schema())?;
        let rt = rhs.get_type(plan.schema())?;
        let (DataType::List(l), DataType::List(r)) = (&lt, &rt) else { return Ok(None); };
        // Keep arithmetic/error-sensitive list producers on their established
        // runtime path until their evaluation contract is proven compatible.
        if !repeatable(plan, &lhs) || !repeatable(plan, &rhs) { return Ok(None); }
        fn semantic_list_item(ty: &DataType) -> bool {
            match ty {
                DataType::List(field) => semantic_list_item(field.data_type()),
                DataType::Null | DataType::Boolean | DataType::Int8 | DataType::Int16 |
                DataType::Int32 | DataType::Int64 | DataType::UInt8 | DataType::UInt16 |
                DataType::UInt32 | DataType::UInt64 | DataType::Float32 | DataType::Float64 |
                DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => true,
                _ => false,
            }
        }
        // A shared semantic helper avoids expanding list comparisons into an
        // UNNEST/aggregate subquery. Each operand is evaluated once; the legacy
        // relational implementation remains available without language helpers.
        if self.options.language_functions && !self.options.native_values
            && semantic_list_item(l.data_type()) && semantic_list_item(r.data_type()) {
            let operation = match op {
                BinaryOp::Eq => "eq", BinaryOp::Neq => "neq",
                BinaryOp::Lt => "lt", BinaryOp::Lte => "lte",
                BinaryOp::Gt => "gt", BinaryOp::Gte => "gte", _ => unreachable!(),
            };
            return Ok(Some(language_functions::list_compare(lhs, rhs, operation)));
        }
        if l.data_type() != r.data_type() || !matches!(l.data_type(), DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 | DataType::Boolean) { return Ok(None); }
        // The relational fallback repeats operands in guards and subqueries.
        if !repeatable(plan, &lhs) || !repeatable(plan, &rhs) { return Ok(None); }
        let mut outer = Vec::new();
        let mut correlate = |expr: Expr| -> RelResult<Expr> {
            Ok(expr.transform_up(|e| match e {
                Expr::Column(column) => {
                    let field = plan.schema().field_from_column(&column)?.clone();
                    let reference = Expr::OuterReferenceColumn(field, column);
                    outer.push(reference.clone());
                    Ok(Transformed::yes(reference))
                }
                _ => Ok(Transformed::no(e)),
            })?.data)
        };
        let left = correlate(lhs.clone())?;
        let right = correlate(rhs.clone())?;
        let length = datafusion::functions_nested::expr_fn::array_length;
        let mut reserved = BTreeSet::new();
        let mut fresh = |base: &str| {
            let name = unique_internal_alias(plan, &reserved, base.to_owned());
            reserved.insert(name.clone());
            name
        };
        let index = fresh("__w_list_equality_index");
        let unequal = fresh("__w_list_unequal");
        let unknown = fresh("__w_list_unknown");
        let output = fresh("__w_list_equal");
        let first = fresh("__w_list_first_nonmatch");
        let extent = if ordering {
            datafusion::logical_expr::when(length(left.clone()).lt(length(right.clone())), length(left.clone()))
                .otherwise(length(right.clone()))?
        } else { length(left.clone()) };
        let indices = datafusion::functions_nested::expr_fn::gen_series(lit(1_i64), Expr::Cast(Cast::new(Box::new(extent), DataType::Int64)), lit(1_i64));
        // These scopes must remain inline: lifting a correlated input to a
        // top-level CTE would detach its outer references. The projection
        // barriers also keep aggregate consumers outside UNNEST's SELECT.
        drop(fresh);
        fn fresh_scope(plan: &LogicalPlan, reserved: &mut BTreeSet<String>, base: &str) -> String {
            loop {
                let name = unique_internal_alias(plan, reserved, base);
                reserved.insert(name.clone());
                let guard = format!("{name}_guard");
                if !has_exact_col(plan, &guard) && reserved.insert(guard) { return name; }
            }
        }
        let input_scope = fresh_scope(plan, &mut reserved, "__w_list_equality_input");
        let item_scope = fresh_scope(plan, &mut reserved, "__w_list_equality_items");
        let listed = LogicalPlanBuilder::empty(true).project(vec![indices.alias(&index)])?.build()?;
        let listed = collections::unnest_scope(listed, input_scope)?;
        let listed = collections::unnest_input(listed)?;
        let options = datafusion::common::UnnestOptions { preserve_nulls: false, ..Default::default() };
        let expanded = LogicalPlanBuilder::from(listed).unnest_column_with_options(Column::new_unqualified(&index), options)?.build()?;
        let expanded = collections::unnest_scope(expanded, item_scope)?;
        let item = |value| datafusion::functions_nested::expr_fn::array_element(value, col_exact(&index));
        let equal = item(left.clone()).eq(item(right.clone()));
        if ordering {
            let first_index = datafusion::logical_expr::when(Expr::IsNotTrue(Box::new(equal)), col_exact(&index))
                .otherwise(lit(ScalarValue::Int64(None)))?;
            let aggregate = LogicalPlanBuilder::from(expanded).aggregate(Vec::<Expr>::new(), vec![df_min(first_index).alias(&first)])?.build()?;
            let a = datafusion::functions_nested::expr_fn::array_element(left.clone(), col_exact(&first));
            let b = datafusion::functions_nested::expr_fn::array_element(right.clone(), col_exact(&first));
            // Match the shared runtime: its first NULL/NULL pair compares as
            // equal and terminates ordering, while a one-sided NULL is unknown.
            let first_result = datafusion::logical_expr::when(a.clone().is_null().and(b.clone().is_null()), lit(matches!(op, BinaryOp::Lte | BinaryOp::Gte)))
                .otherwise(binary(a, op, b))?;
            let truth = datafusion::logical_expr::when(col_exact(&first).is_null(), binary(length(left), op, length(right)))
                .otherwise(first_result)?;
            let subplan = LogicalPlanBuilder::from(aggregate).project(vec![truth.alias(&output)])?.build()?;
            let elementwise = Expr::ScalarSubquery(Subquery { subquery: Arc::new(subplan), outer_ref_columns: outer, spans: Default::default() });
            return Ok(Some(datafusion::logical_expr::when(lhs.is_null().or(rhs.is_null()), lit(ScalarValue::Boolean(None)))
                .otherwise(elementwise)?));
        }
        let indicator = |condition| datafusion::logical_expr::when(condition, lit(1_i64)).otherwise(lit(ScalarValue::Int64(None)));
        let aggregate = LogicalPlanBuilder::from(expanded).aggregate(Vec::<Expr>::new(), vec![
            df_count(indicator(Expr::IsFalse(Box::new(equal.clone())))?).alias(&unequal),
            df_count(indicator(equal.is_null())?).alias(&unknown),
        ])?.build()?;
        let truth = datafusion::logical_expr::when(col_exact(&unequal).gt(lit(0_i64)), lit(false))
            .when(col_exact(&unknown).gt(lit(0_i64)), lit(ScalarValue::Boolean(None))).otherwise(lit(true))?;
        let subplan = LogicalPlanBuilder::from(aggregate).project(vec![truth.alias(&output)])?.build()?;
        let elementwise = Expr::ScalarSubquery(Subquery { subquery: Arc::new(subplan), outer_ref_columns: outer, spans: Default::default() });
        let equal = datafusion::logical_expr::when(lhs.clone().is_null().or(rhs.clone().is_null()), lit(ScalarValue::Boolean(None)))
            .when(length(lhs).not_eq(length(rhs)), lit(false)).otherwise(elementwise)?;
        Ok(Some(if op == BinaryOp::Neq { Expr::Not(Box::new(equal)) } else { equal }))
    }
}

// Bounds describe non-null values only. NULL propagation is handled by SQL.
// Resolve group keys and projections so size(list) - 1 can be proved safe,
// while arbitrary integer subtraction (which may overflow) remains fenced.
pub(super) fn integer_bounds(plan: &LogicalPlan, expr: &Expr) -> Option<(i128, i128)> {
    match expr {
        Expr::Alias(alias) => integer_bounds(plan, &alias.expr),
        Expr::Literal(value, _) => {
            let value = match value {
                ScalarValue::Int64(Some(v)) => i128::from(*v),
                ScalarValue::Int32(Some(v)) => i128::from(*v),
                ScalarValue::UInt64(Some(v)) => i128::from(*v),
                ScalarValue::UInt32(Some(v)) => i128::from(*v),
                _ => return None,
            };
            Some((value, value))
        }
        Expr::Column(column) => {
            match plan {
                LogicalPlan::Projection(p) => {
                    let index = p.schema.index_of_column(column).ok()?;
                    integer_bounds(&p.input, p.expr.get(index)?)
                }
                LogicalPlan::Aggregate(a) => {
                    let index = a.schema.index_of_column(column).ok()?;
                    if let Some(group) = a.group_expr.get(index) {
                        integer_bounds(&a.input, group)
                    } else {
                        let mut aggregate = a.aggr_expr.get(index.checked_sub(a.group_expr.len())?)?;
                        if let Expr::Alias(alias) = aggregate { aggregate = &alias.expr; }
                        let Expr::AggregateFunction(call) = aggregate else { return None; };
                        // Language-aware grouping carries the original group
                        // value with FIRST_VALUE beside its equality key. A
                        // selected input cannot widen its non-null domain.
                        if call.func.name() != "first_value" || call.params.args.len() != 1 { return None; }
                        integer_bounds(&a.input, &call.params.args[0])
                    }
                }
                LogicalPlan::SubqueryAlias(a) => integer_bounds(&a.input, &col_exact(&column.name)),
                LogicalPlan::Filter(a) => integer_bounds(&a.input, expr),
                LogicalPlan::Sort(a) => integer_bounds(&a.input, expr),
                LogicalPlan::Limit(a) => integer_bounds(&a.input, expr),
                LogicalPlan::Window(a) if a.input.schema().has_column(column) => integer_bounds(&a.input, expr),
                LogicalPlan::Unnest(a) => {
                    let output = a.schema.index_of_column(column).ok()?;
                    let input = *a.dependency_indices.get(output)?;
                    if a.list_type_columns.iter().any(|(i, _)| *i == input) {
                        return integer_list_item_bounds(&a.input, &col_exact(a.input.schema().field(input).name()));
                    }
                    if a.struct_type_columns.contains(&input) { return None; }
                    integer_bounds(&a.input, &col_exact(a.input.schema().field(input).name()))
                }
                LogicalPlan::Join(a) => {
                    let left = a.left.schema().has_column(column);
                    let right = a.right.schema().has_column(column);
                    match (left, right) {
                        (true, false) => integer_bounds(&a.left, expr),
                        (false, true) => integer_bounds(&a.right, expr),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        Expr::Cast(cast) => {
            let (lo, hi) = integer_bounds(plan, &cast.expr)?;
            let (min, max) = integer_type_bounds(&cast.data_type)?;
            (lo >= min && hi <= max).then_some((lo, hi))
        }
        Expr::ScalarFunction(call) if call.func.inner().as_any().is::<datafusion::functions::math::abs::AbsFunc>() && call.args.len() == 1 => {
            let (lo, hi) = integer_bounds(plan, &call.args[0])?;
            let (min, max) = integer_type_bounds(&expr.get_type(plan.schema()).ok()?)?;
            if lo == min && min < 0 { return None; }
            let upper = lo.checked_abs()?.max(hi.checked_abs()?);
            let lower = if lo <= 0 && hi >= 0 { 0 } else { lo.checked_abs()?.min(hi.checked_abs()?) };
            (upper <= max).then_some((lower, upper))
        }
        Expr::ScalarFunction(call) if call.func.name() == "array_length" && call.args.len() == 1 => {
            // Arrow List offsets are signed 32-bit; this is an exact physical
            // upper bound, not a sample or a guessed input cardinality.
            match call.args[0].get_type(plan.schema()).ok()? {
                DataType::List(_) => Some((0, i128::from(i32::MAX))),
                DataType::FixedSizeList(_, n) => Some((i128::from(n), i128::from(n))),
                _ => None,
            }
        }
        Expr::BinaryExpr(binary) => {
            // Modulo by a proven positive divisor is safe for every value
            // of its integer lhs, even when the producer's range is unknown.
            // Do not generalize this type-domain fallback to other arithmetic.
            let (lmin, lmax) = integer_bounds(plan, &binary.left).or_else(|| {
                if binary.op == Operator::Modulo {
                    integer_type_bounds(&binary.left.get_type(plan.schema()).ok()?)
                } else { None }
            })?;
            let (rmin, rmax) = integer_bounds(plan, &binary.right)?;
            let (lo, hi) = match binary.op {
                Operator::Plus => (lmin.checked_add(rmin)?, lmax.checked_add(rmax)?),
                Operator::Minus => (lmin.checked_sub(rmax)?, lmax.checked_sub(rmin)?),
                Operator::Multiply => {
                    let corners = [lmin.checked_mul(rmin)?, lmin.checked_mul(rmax)?, lmax.checked_mul(rmin)?, lmax.checked_mul(rmax)?];
                    (*corners.iter().min()?, *corners.iter().max()?)
                }
                // Positive divisors exclude both zero and MIN % -1. Keep
                // other cases fenced rather than widening Cypher error rules.
                Operator::Modulo if rmin > 0 => {
                    let magnitude = rmax.checked_sub(1)?;
                    (if lmin < 0 { lmin.max(-magnitude) } else { 0 },
                     if lmax > 0 { lmax.min(magnitude) } else { 0 })
                }
                _ => return None,
            };
            let (min, max) = integer_type_bounds(&expr.get_type(plan.schema()).ok()?)?;
            (lo >= min && hi <= max).then_some((lo, hi))
        }
        _ => None,
    }
}

/// Domain of nonnull integer elements, derived only from actual constructors
/// and transparent aliases. Empty/all-null lists provide no arithmetic proof.
fn integer_list_item_bounds(plan: &LogicalPlan, expr: &Expr) -> Option<(i128, i128)> {
    match expr {
        Expr::Alias(alias) => integer_list_item_bounds(plan, &alias.expr),
        Expr::Column(column) => match plan {
            LogicalPlan::Projection(p) => integer_list_item_bounds(&p.input, p.expr.get(p.schema.index_of_column(column).ok()?)?),
            LogicalPlan::SubqueryAlias(a) => integer_list_item_bounds(&a.input, &col_exact(&column.name)),
            LogicalPlan::Filter(a) => integer_list_item_bounds(&a.input, expr),
            LogicalPlan::Sort(a) => integer_list_item_bounds(&a.input, expr),
            LogicalPlan::Limit(a) => integer_list_item_bounds(&a.input, expr),
            LogicalPlan::Window(a) if a.input.schema().has_column(column) => integer_list_item_bounds(&a.input, expr),
            LogicalPlan::Unnest(a) => {
                let output = a.schema.index_of_column(column).ok()?;
                let input = *a.dependency_indices.get(output)?;
                if a.list_type_columns.iter().any(|(i, _)| *i == input) || a.struct_type_columns.contains(&input) { return None; }
                integer_list_item_bounds(&a.input, &col_exact(a.input.schema().field(input).name()))
            }
            _ => None,
        },
        Expr::ScalarFunction(call) if call.func.inner().as_any().is::<datafusion::functions_nested::make_array::MakeArray>() => {
            let mut bounds: Option<(i128, i128)> = None;
            for item in &call.args {
                if matches!(item, Expr::Literal(value, _) if value.is_null()) { continue; }
                let (lo, hi) = integer_bounds(plan, item)?;
                bounds = Some(bounds.map_or((lo, hi), |(a,b)| (a.min(lo),b.max(hi))));
            }
            bounds
        }
        Expr::Literal(ScalarValue::List(array), _) if !array.is_null(0) => {
            let values = array.value(0);
            let mut bounds: Option<(i128, i128)> = None;
            for index in 0..values.len() {
                if values.is_null(index) { continue; }
                let value = ScalarValue::try_from_array(&values, index).ok()?;
                let (lo,hi) = integer_bounds(plan, &lit(value))?;
                bounds = Some(bounds.map_or((lo,hi), |(a,b)| (a.min(lo),b.max(hi))));
            }
            bounds
        }
        _ => None,
    }
}

/// Validate every potentially throwing arithmetic operation, including its
/// intermediates. Nested scalar queries validate their own lexical domain.
pub(super) fn arithmetic_is_total(plan: &LogicalPlan, expr: &Expr) -> bool {
    use datafusion::common::tree_node::TreeNodeRecursion;
    let mut safe = true;
    let visited = expr.apply(|node| {
        let arithmetic = matches!(node, Expr::BinaryExpr(binary) if matches!(binary.op,
            Operator::Plus | Operator::Minus | Operator::Multiply | Operator::Divide | Operator::Modulo))
            || matches!(node, Expr::ScalarFunction(call) if call.func.inner().as_any().is::<datafusion::functions::math::abs::AbsFunc>());
        if arithmetic && integer_bounds(plan, node).is_none() { safe = false; }
        Ok(TreeNodeRecursion::Continue)
    });
    safe && visited.is_ok()
}

fn integer_type_bounds(ty: &DataType) -> Option<(i128, i128)> {
    match ty {
        DataType::Int64 => Some((i128::from(i64::MIN), i128::from(i64::MAX))),
        DataType::Int32 => Some((i128::from(i32::MIN), i128::from(i32::MAX))),
        DataType::UInt64 => Some((0, i128::from(u64::MAX))),
        DataType::UInt32 => Some((0, i128::from(u32::MAX))),
        _ => None,
    }
}

pub(super) fn repeatable(plan: &LogicalPlan, expr: &Expr) -> bool {
    match expr {
        Expr::Column(_) | Expr::Literal(_, _) => true,
        Expr::BinaryExpr(e) => {
            (matches!(e.op, Operator::Eq | Operator::NotEq | Operator::Lt | Operator::LtEq | Operator::Gt | Operator::GtEq | Operator::And | Operator::Or)
                || integer_bounds(plan, expr).is_some())
                && repeatable(plan, &e.left) && repeatable(plan, &e.right)
        }
        Expr::Cast(cast) => integer_bounds(plan, expr).is_some() && repeatable(plan, &cast.expr),
        Expr::ScalarFunction(call) if call.func.name() == "generate_series" => {
            let [start, stop, step] = call.args.as_slice() else { return false; };
            let Some((step_min, step_max)) = integer_bounds(plan, step) else { return false; };
            // Exclude zero and overflow at the terminating increment. This
            // also ensures repeated guards cannot suppress a range error.
            let Some((start_min, start_max)) = integer_bounds(plan, start) else { return false; };
            let Some((stop_min, stop_max)) = integer_bounds(plan, stop) else { return false; };
            (step_min > 0 || step_max < 0)
                && start_min.min(stop_min) + step_min >= i128::from(i64::MIN)
                && start_max.max(stop_max) + step_max <= i128::from(i64::MAX)
                && call.args.iter().all(|arg| repeatable(plan, arg))
        }
        Expr::ScalarFunction(call) => matches!(call.func.name(), "array_length" | "array_element" | "make_array") && call.args.iter().all(|arg| repeatable(plan, arg)),
        _ => false,
    }
}

/// Compact correlated consumers may cause SQL projection substitution. Fence
/// non-immutable producers even when the immediate list expression is a column.
/// This check deliberately does not change the older equality lowering rules.
pub(super) fn immutable_source_plan(plan: &LogicalPlan) -> bool {
    use datafusion::common::tree_node::TreeNodeRecursion;
    use datafusion::logical_expr::Volatility;
    let mut immutable = true;
    let visited = plan.apply_with_subqueries(|node| {
        node.apply_expressions(|expr| expr.apply(|expr| {
            if let Expr::ScalarFunction(call) = expr {
                immutable &= call.func.signature().volatility == Volatility::Immutable;
            }
            Ok(TreeNodeRecursion::Continue)
        }))?;
        Ok(TreeNodeRecursion::Continue)
    });
    immutable && visited.is_ok()
}

/// A list constructor is nonnull even when some elements are NULL. Match the
/// concrete built-in implementation, never a user function sharing its name.
pub(super) fn list_nullable(plan: &LogicalPlan, expr: &Expr) -> RelResult<bool> {
    if let Expr::ScalarFunction(call) = expr {
        if call.func.inner().as_any().is::<datafusion::functions_nested::make_array::MakeArray>() {
            return Ok(false);
        }
    }
    Ok(expr.nullable(plan.schema())?)
}

#[cfg(test)]
mod bounded_quantifier_arithmetic_tests {
    use super::*;

    fn scope() -> LogicalPlan {
        let values = datafusion::functions_nested::expr_fn::make_array(vec![lit(1_i64),lit(9_i64),lit(ScalarValue::Int64(None))]);
        LogicalPlanBuilder::empty(true).project(vec![values.alias("x")]).unwrap()
            .unnest_column(Column::new_unqualified("x")).unwrap().build().unwrap()
    }

    #[test]
    fn constructor_item_bounds_prove_runtime_arithmetic() {
        let plan=scope();
        let x=col_exact("x");
        assert_eq!(integer_bounds(&plan,&x),Some((1,9)));
        assert_eq!(integer_bounds(&plan,&(x.clone()*x.clone())),Some((1,81)));
        assert_eq!(integer_bounds(&plan,&(x.clone()+x.clone())),Some((2,18)));
        let remainder=Expr::BinaryExpr(datafusion::logical_expr::BinaryExpr::new(Box::new(x.clone()),Operator::Modulo,Box::new(x.clone())));
        assert!(arithmetic_is_total(&plan,&remainder));
        assert!(arithmetic_is_total(&plan,&datafusion::functions::math::expr_fn::abs(x-lit(5_i64))));
    }

    #[test]
    fn overflow_and_zero_divisor_are_not_total() {
        let plan=scope();
        for expr in [lit(i64::MAX)+lit(1_i64),lit(i64::MIN)-lit(1_i64),lit(i64::MAX)*lit(2_i64),datafusion::functions::math::expr_fn::abs(lit(i64::MIN))] {
            assert!(!arithmetic_is_total(&plan,&expr),"{expr}");
        }
        for divisor in [lit(0_i64),lit(-1_i64),col_exact("x")-lit(5_i64)] {
            let expr=Expr::BinaryExpr(datafusion::logical_expr::BinaryExpr::new(Box::new(lit(i64::MIN)),Operator::Modulo,Box::new(divisor)));
            assert!(!arithmetic_is_total(&plan,&expr),"{expr}");
        }
    }

    #[test]
    fn positive_modulo_is_total_for_unknown_integer_input() {
        let schema=Arc::new(Schema::new(vec![Field::new("x",DataType::Int64,true)]));
        let table=MemTable::try_new(schema,vec![vec![]]).unwrap();
        let plan=LogicalPlanBuilder::scan("unknown",provider_as_source(Arc::new(table)),None).unwrap().build().unwrap();
        let x=col_exact("x");
        assert_eq!(integer_bounds(&plan,&x),None);
        let expr=Expr::BinaryExpr(datafusion::logical_expr::BinaryExpr::new(Box::new(x.clone()),Operator::Modulo,Box::new(lit(2_i64))));
        assert_eq!(integer_bounds(&plan,&expr),Some((-1,1)));
        assert!(arithmetic_is_total(&plan,&expr));
        assert!(!arithmetic_is_total(&plan,&(x+lit(1_i64))));
    }

    #[test]
    fn shadowed_dynamic_source_does_not_retain_constructor_domain() {
        let plan=LogicalPlanBuilder::from(scope()).project(vec![col_exact("x").alias("list")]).unwrap().build().unwrap();
        assert_eq!(integer_list_item_bounds(&plan,&col_exact("list")),None);
        assert_eq!(integer_list_item_bounds(&plan,&datafusion::functions_nested::expr_fn::make_array(vec![])),None);
    }
}
