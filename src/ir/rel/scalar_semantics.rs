//! Proven primitive Cypher semantics. Unsupported domains retain runtime validation.
use super::*;

pub(super) fn stable_scalar_input(expr: &IrExpr) -> bool {
    crate::ir::rules::effects::effects_for_bound_scalar(expr).safe_to_duplicate()
}

// The shared adapter preserves this membership-specific proof domain; it is
// intentionally wider than scalar stability and remains conditional on binding.
pub(super) fn repeatable_membership_input(expr: &IrExpr) -> bool {
    crate::ir::rules::effects::effects_for_bound_membership(expr, &|expr|
        collection_evaluation::lowercase_label_transform(expr).is_some()
    ).safe_to_duplicate()
}

// A finite set of possible outputs does not prove total evaluation. Follow
// only literal construction and column transport, excluding CASE conditions,
// casts, arithmetic, and functions which could throw before producing a value.
fn total_domain_column(plan: &LogicalPlan, column: &str) -> bool {
    fn expression(plan: &LogicalPlan, expr: &Expr) -> bool {
        match expr {
            Expr::Alias(alias) => expression(plan, &alias.expr),
            Expr::Column(column) => total_domain_column(plan, &column.name),
            Expr::Literal(..) => true,
            Expr::ScalarFunction(call) if call.func.name() == "make_array" =>
                call.args.iter().all(|arg| expression(plan, arg)),
            _ => false,
        }
    }
    match plan {
        LogicalPlan::Projection(p) => p.schema.index_of_column_by_name(None, column)
            .and_then(|i| p.expr.get(i)).is_some_and(|e| expression(&p.input, e)),
        LogicalPlan::Aggregate(a) => a.schema.index_of_column_by_name(None, column)
            .and_then(|i| a.group_expr.get(i)).is_some_and(|e| expression(&a.input, e)),
        LogicalPlan::SubqueryAlias(a) => total_domain_column(&a.input, column),
        LogicalPlan::Filter(f) => total_domain_column(&f.input, column),
        LogicalPlan::Sort(s) => total_domain_column(&s.input, column),
        LogicalPlan::Limit(l) => total_domain_column(&l.input, column),
        LogicalPlan::Unnest(u) => u.schema.index_of_column_by_name(None, column)
            .and_then(|i| u.dependency_indices.get(i))
            .is_some_and(|i| total_domain_column(&u.input, u.input.schema().field(*i).name())),
        // finite_values independently requires the compiler-owned Snapshot.
        LogicalPlan::TableScan(_) => true,
        LogicalPlan::Union(u) => u.schema.index_of_column_by_name(None, column)
            .is_some_and(|i| u.inputs.iter().all(|input|
                total_domain_column(input, input.schema().field(i).name()))),
        LogicalPlan::Join(j) => {
            let left = j.left.schema().index_of_column_by_name(None, column).is_some();
            let right = j.right.schema().index_of_column_by_name(None, column).is_some();
            match (left, right) {
                (true, false) => total_domain_column(&j.left, column),
                (false, true) => total_domain_column(&j.right, column),
                _ => false,
            }
        }
        LogicalPlan::Window(w) => total_domain_column(&w.input, column),
        _ => false,
    }
}

// Rust str::trim's Unicode White_Space set, rather than SQL's default ASCII space.
fn cypher_trim(value: Expr) -> Expr {
    df_string::trim(vec![
        cast_utf8(value),
        lit(
            "\u{0009}\u{000a}\u{000b}\u{000c}\u{000d}\u{0020}\u{0085}\u{00a0}\u{1680}\u{2000}\u{2001}\u{2002}\u{2003}\u{2004}\u{2005}\u{2006}\u{2007}\u{2008}\u{2009}\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}",
        ),
    ])
}
fn finite_float(value: Expr) -> Expr {
    value
        .clone()
        .gt(lit(f64::NEG_INFINITY))
        .and(value.lt(lit(f64::INFINITY)))
}
fn typed_null(ty: &DataType) -> RelResult<Expr> {
    Ok(lit(ScalarValue::try_from(ty)?))
}
fn list_item_type(ty: &DataType) -> Option<&DataType> {
    match ty {
        DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
            Some(f.data_type())
        }
        _ => None,
    }
}

/// Orchid v1 string conversion SQL implementation; selected only after typed
/// policy/capability resolution. Repeated token evaluation requires stable input.
pub(super) fn conformant_string_to_float(value: Expr) -> RelResult<Expr> {
    let token = df_string::replace(cypher_trim(value), lit("_"), lit(""));
    let valid = df_regex::regexp_like(token.clone(),
        lit(r"^[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?$"), None);
    let parsed = Expr::TryCast(TryCast::new(Box::new(token), DataType::Float64));
    Ok(datafusion::logical_expr::when(valid.and(finite_float(parsed.clone())), parsed)
        .otherwise(lit(ScalarValue::Float64(None)))?)
}

impl LoweringContext<'_> {
    pub(super) fn lower_cypher_conversion(
        &self,
        plan: &LogicalPlan,
        name: &str,
        arg: &IrExpr,
    ) -> RelResult<Expr> {
        // Recover only literals propagated through transparent projections. This
        // is exact folding, not a claim that arbitrary string casts are SQL-safe.
        if name == "cypher_convert.tointeger" {
            if let IrExpr::Binding(binding) = arg {
                if let Some(Value::String(text)) = temporal_constant_bindings(plan).get(binding) {
                    let constant = IrExpr::Call {
                        name: name.to_owned(),
                        args: vec![IrExpr::Lit(Lit::String(text.clone()))],
                    };
                    if let Some(folded) = self.try_constant_fold(&constant)? {
                        return Ok(folded);
                    }
                }
            }
        }
        let value = self.lower_expr(plan, arg)?;
        let ty = value.get_type(plan.schema())?;
        let int = matches!(
            ty,
            DataType::Int8
                | DataType::Int16
                | DataType::Int32
                | DataType::Int64
                | DataType::UInt8
                | DataType::UInt16
                | DataType::UInt32
                | DataType::UInt64
        );
        let float = matches!(ty, DataType::Float32 | DataType::Float64);
        let text = matches!(
            ty,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
        );
        let null = ty == DataType::Null;
        let cast = |v: Expr, t: DataType| Expr::Cast(Cast::new(Box::new(v), t));
        let try_cast = |v: Expr, t: DataType| Expr::TryCast(TryCast::new(Box::new(v), t));
        match name {
            "cypher_convert.toboolean" if null || ty == DataType::Boolean => {
                Ok(cast(value, DataType::Boolean))
            }
            "cypher_convert.toboolean" if int => Ok(value.not_eq(lit(0_i64))),
            "cypher_convert.toboolean" if text && stable_scalar_input(arg) => {
                let token = df_string::lower(cypher_trim(value));
                Ok(
                    datafusion::logical_expr::when(token.clone().eq(lit("true")), lit(true))
                        .when(token.eq(lit("false")), lit(false))
                        .otherwise(lit(ScalarValue::Boolean(None)))?,
                )
            }
            "cypher_convert.tofloat" if null || int || float => Ok(cast(value, DataType::Float64)),
            "cypher_convert.tofloat" if text => self.select_string_to_float(value, arg, &ty),
            "cypher_convert.tointeger" if text => {
                // Compiler-owned immutable snapshots and literal UNWIND inputs
                // can prove the complete string domain. Evaluate each distinct
                // token with the shared Cypher evaluator: SQL casts differ on
                // hex signs, underscores, decimal truncation, and saturation.
                // A column read is total and repeatable; arbitrary expressions
                // remain fenced even if their possible results are finite.
                let Expr::Column(column) = &value else {
                    return Err(RelError::Unsupported(
                        "Cypher string toInteger requires a proven immutable domain".into(),
                    ));
                };
                if !total_domain_column(plan, &column.name) {
                    return Err(RelError::Unsupported(
                        "Cypher string toInteger source requires total evaluation".into(),
                    ));
                }
                let values = temporal_semantics::finite_values(plan, &column.name)
                    .ok_or_else(|| RelError::Unsupported(
                        "Cypher string toInteger requires a proven immutable domain".into(),
                    ))?;
                let mut arms = Vec::new();
                for token in values {
                    if token.is_null() { continue; }
                    let text = match &token {
                        ScalarValue::Utf8(Some(text))
                        | ScalarValue::LargeUtf8(Some(text))
                        | ScalarValue::Utf8View(Some(text)) => text,
                        _ => return Err(RelError::Unsupported(
                            "Cypher string toInteger domain contains non-string values".into(),
                        )),
                    };
                    let constant = IrExpr::Call {
                        name: name.to_owned(),
                        args: vec![IrExpr::Lit(Lit::String(text.clone()))],
                    };
                    let folded = self.try_constant_fold(&constant)?.ok_or_else(||
                        RelError::Unsupported("Cypher integer conversion cannot be folded".into()))?;
                    arms.push((Box::new(value.clone().eq(lit(token))),
                        Box::new(folded.cast_to(&DataType::Int64, plan.schema())?)));
                }
                if arms.is_empty() { return Ok(lit(ScalarValue::Int64(None))); }
                Ok(Expr::Case(Case::new(None, arms,
                    Some(Box::new(lit(ScalarValue::Int64(None)))))))
            }
            "cypher_convert.tointeger" if null || int || ty == DataType::Boolean => {
                Ok(try_cast(value, DataType::Int64))
            }
            "cypher_convert.tointeger" if float && stable_scalar_input(arg) => {
                let value = cast(value, DataType::Float64);
                // Match the existing runtime's finite-float truncation and saturation.
                // 2^63 is the first f64 outside i64; MAX as f64 rounds to that value.
                let converted = datafusion::logical_expr::when(
                    value.clone().gt_eq(lit(9223372036854775808.0_f64)),
                    lit(i64::MAX),
                )
                .when(
                    value.clone().lt_eq(lit(-9223372036854775808.0_f64)),
                    lit(i64::MIN),
                )
                .otherwise(try_cast(
                    df_math::trunc(vec![value.clone()]),
                    DataType::Int64,
                ))?;
                Ok(
                    datafusion::logical_expr::when(finite_float(value), converted)
                        .otherwise(lit(ScalarValue::Int64(None)))?,
                )
            }
            _ => Err(RelError::Unsupported(format!(
                "Cypher conversion {name} for {ty} requires runtime semantics"
            ))),
        }
    }

    /// Strict Cypher alias: never turn an invalid number/list into a string.
    pub(super) fn lower_cypher_contains(
        &self,
        plan: &LogicalPlan,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let [left, right] = args else {
            return Err(RelError::Unsupported("contains_fn arity".into()));
        };
        let left = self.lower_expr(plan, left)?;
        let right = self.lower_expr(plan, right)?;
        for value in [&left, &right] {
            let ty = value.get_type(plan.schema())?;
            if !matches!(
                ty,
                DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Null
            ) {
                return Err(RelError::Unsupported(format!(
                    "Cypher CONTAINS requires string operands, found {ty}"
                )));
            }
        }
        Ok(df_string::contains(cast_utf8(left), cast_utf8(right)))
    }

    /// Same-element list concat/append/prepend, with Cypher's strict NULL rule.
    pub(super) fn lower_cypher_list_add(
        &self,
        plan: &LogicalPlan,
        left: &IrExpr,
        right: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        let lhs = self.lower_list_operand(plan, left)?;
        let rhs = self.lower_list_operand(plan, right)?;
        let lt = lhs.get_type(plan.schema())?;
        let rt = rhs.get_type(plan.schema())?;
        let li = list_item_type(&lt);
        let ri = list_item_type(&rt);
        if li.is_none() && ri.is_none() {
            return Ok(None);
        }
        if !repeatable_list_add_input(self, plan, left) || !repeatable_list_add_input(self, plan, right) {
            return Err(RelError::Unsupported(
                "Cypher list addition requires single-evaluation operands".into(),
            ));
        }
        let (lhs, rhs, result_type) = match (li, ri) {
            (Some(a), Some(b)) if a == b || a == &DataType::Null || b == &DataType::Null => {
                let target = if a == &DataType::Null {
                    rt.clone()
                } else {
                    lt.clone()
                };
                (
                    lhs.cast_to(&target, plan.schema())?,
                    rhs.cast_to(&target, plan.schema())?,
                    target,
                )
            }
            (Some(a), None) if a == &rt || rt == DataType::Null => (lhs, rhs, lt.clone()),
            (None, Some(b)) if b == &lt || lt == DataType::Null => (lhs, rhs, rt.clone()),
            _ => {
                return Err(RelError::Unsupported(
                    "Heterogeneous Cypher list addition requires native runtime types".into(),
                ));
            }
        };
        let null = lhs.clone().is_null().or(rhs.clone().is_null());
        let result = match (li.is_some(), ri.is_some()) {
            (true, true) => datafusion::functions_nested::expr_fn::array_concat(vec![lhs, rhs]),
            (true, false) => datafusion::functions_nested::expr_fn::array_append(lhs, rhs),
            (false, true) => datafusion::functions_nested::expr_fn::array_prepend(lhs, rhs),
            _ => unreachable!(),
        };
        Ok(Some(
            datafusion::logical_expr::when(null, typed_null(&result_type)?).otherwise(result)?,
        ))
    }
}

// Homogeneous integer/boolean arrays have exactly Cypher's lexicographic
// ordering, including recursively nested NULLs sorting after present values.
// Strings retain a fence because legacy runtime strings can carry temporal or
// blob ordering; floats retain a fence until all adapter NaN behavior is proven.
pub(super) fn cypher_list_order_type(ty: &DataType) -> bool {
    fn item(ty: &DataType) -> bool {
        match ty {
            DataType::List(field) | DataType::LargeList(field) | DataType::FixedSizeList(field, _) => item(field.data_type()),
            DataType::Null | DataType::Boolean | DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 => true,
            _ => false,
        }
    }
    matches!(ty, DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)) && item(ty)
}

fn repeatable_list_add_input(ctx: &LoweringContext<'_>, plan: &LogicalPlan, expr: &IrExpr) -> bool {
    let addition = match expr {
        IrExpr::Binary { op: BinaryOp::Add, lhs, rhs } => Some((lhs.as_ref(), rhs.as_ref())),
        IrExpr::Call { name, args } if name == "cypher_add" && args.len() == 2 => Some((&args[0], &args[1])),
        _ => None,
    };
    if let Some((left, right)) = addition {
        // Scalar addition can overflow. Only repeat this node when its result
        // is list-valued, and both of its operands independently pass the proof.
        let list = |operand: &IrExpr| ctx.lower_expr(plan, operand).ok()
            .and_then(|value| value.get_type(plan.schema()).ok())
            .is_some_and(|ty| list_item_type(&ty).is_some());
        return (list(left) || list(right)) && repeatable_list_add_input(ctx, plan, left)
            && repeatable_list_add_input(ctx, plan, right);
    }
    match expr {
        IrExpr::List(items) => items.iter().all(|item| repeatable_list_add_input(ctx, plan, item)),
        _ => repeatable_membership_input(expr),
    }
}
