//! Typed comparisons for heterogeneous primitive property columns.
use super::*;
use crate::ir::functions::scalar_union as transport;

pub(super) fn comparison(
    plan: &LogicalPlan,
    lhs_source: &IrExpr,
    rhs_source: &IrExpr,
    lhs: Expr,
    rhs: Expr,
    op: BinaryOp,
) -> RelResult<Option<Expr>> {
    let lt = lhs.get_type(plan.schema())?;
    let rt = rhs.get_type(plan.schema())?;
    let lu = transport::is_type(&lt);
    let ru = transport::is_type(&rt);
    if !lu && !ru {
        return Ok(None);
    }
    let null = lit(ScalarValue::Boolean(None));
    let mismatch = match op {
        BinaryOp::Eq => lit(false),
        BinaryOp::Neq => lit(true),
        _ => null.clone(),
    };
    if collections::is_encoded_property(plan, lhs_source)
        || collections::is_encoded_property(plan, rhs_source)
    {
        return Err(RelError::Unsupported("Comparison of heterogeneous primitive and encoded compound properties requires runtime values".into()));
    }
    let compound = |expr: &IrExpr| {
        matches!(expr, IrExpr::List(_))
            || matches!(expr, IrExpr::Call { name, .. } if matches!(name.as_str(), "map" | "make_map"))
    };
    if (!lu && compound(lhs_source)) || (!ru && compound(rhs_source)) {
        return Ok(Some(
            datafusion::logical_expr::when(lhs.is_null().or(rhs.is_null()), null)
                .otherwise(mismatch)?,
        ));
    }
    let tag = |e: Expr| df_core::get_field(e, transport::TAG);
    let mut cases = Vec::new();
    cases.push((
        Box::new(lhs.clone().is_null().or(rhs.clone().is_null())),
        Box::new(null),
    ));
    for (id, name, kind) in [
        (1_i8, transport::INTEGER, DataType::Int64),
        (2, transport::STRING, DataType::Utf8),
        (3, transport::BOOLEAN, DataType::Boolean),
    ] {
        let compatible = |ty: &DataType| {
            *ty == kind
                || kind == DataType::Int64
                    && matches!(ty, DataType::Int8 | DataType::Int16 | DataType::Int32)
        };
        if (!lu && !compatible(&lt)) || (!ru && !compatible(&rt)) {
            continue;
        }
        let condition = match (lu, ru) {
            (true, true) => tag(lhs.clone())
                .eq(lit(id))
                .and(tag(rhs.clone()).eq(lit(id))),
            (true, false) => tag(lhs.clone()).eq(lit(id)),
            _ => tag(rhs.clone()).eq(lit(id)),
        };
        let left = if lu {
            df_core::get_field(lhs.clone(), name)
        } else {
            lhs.clone()
        };
        let right = if ru {
            df_core::get_field(rhs.clone(), name)
        } else {
            rhs.clone()
        };
        cases.push((Box::new(condition), Box::new(binary(left, op, right))));
    }
    // Floating point against integer payloads requires exact mixed-number
    // semantics, rather than SQL's potentially lossy double promotion.
    if (!lu
        && lt.is_numeric()
        && !matches!(
            lt,
            DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64
        ))
        || (!ru
            && rt.is_numeric()
            && !matches!(
                rt,
                DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64
            ))
    {
        return Err(RelError::Unsupported(
            "Heterogeneous numeric comparison requires exact numeric semantics".into(),
        ));
    }
    Ok(Some(Expr::Case(Case::new(
        None,
        cases,
        Some(Box::new(mismatch)),
    ))))
}

/// Cypher orders strings, then booleans, then numbers. The wire tag order is
/// deliberately unrelated to this language order and must never sort directly.
pub(super) fn order_keys(
    context: &LoweringContext<'_>,
    plan: &LogicalPlan,
    expr: &IrExpr,
) -> RelResult<Option<Vec<Expr>>> {
    let IrExpr::Call { name, args } = expr else {
        return Ok(None);
    };
    if name != "cypher_order_key" || args.len() != 1 {
        return Ok(None);
    }
    let Ok(value) = context.lower_expr(plan, &args[0]) else {
        return Ok(None);
    };
    if !transport::is_type(&value.get_type(plan.schema())?) {
        return Ok(None);
    }
    let tag = df_core::get_field(value.clone(), transport::TAG);
    let rank =
        datafusion::logical_expr::when(value.clone().is_null(), lit(ScalarValue::Int32(None)))
            .when(tag.clone().eq(lit(2_i8)), lit(0_i32))
            .when(tag.clone().eq(lit(3_i8)), lit(1_i32))
            .otherwise(lit(2_i32))?;
    Ok(Some(vec![
        rank,
        df_core::get_field(value.clone(), transport::STRING),
        df_core::get_field(value.clone(), transport::BOOLEAN),
        df_core::get_field(value, transport::INTEGER),
    ]))
}

/// Promote a primitive source column without coercing its payload to text.
pub(super) fn promote(plan: &LogicalPlan, value: Expr) -> RelResult<Expr> {
    let ty = value.get_type(plan.schema())?;
    if transport::is_type(&ty) {
        return Ok(value);
    }
    let (tag, field) = match ty {
        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 => {
            (1_i8, transport::INTEGER)
        }
        DataType::Utf8 => (2_i8, transport::STRING),
        DataType::Boolean => (3_i8, transport::BOOLEAN),
        DataType::Null => return Ok(lit(ScalarValue::try_from(&transport::data_type())?)),
        _ => {
            return Err(RelError::Unsupported(
                "unsupported heterogeneous property promotion".into(),
            ));
        }
    };
    let mut args = vec![lit(transport::TAG), lit(tag)];
    for (name, kind) in [
        (transport::INTEGER, DataType::Int64),
        (transport::STRING, DataType::Utf8),
        (transport::BOOLEAN, DataType::Boolean),
    ] {
        let payload = if name == field {
            Expr::Cast(Cast::new(Box::new(value.clone()), kind))
        } else {
            lit(ScalarValue::try_from(&kind)?)
        };
        args.extend([lit(name), payload]);
    }
    let packed = Expr::Cast(Cast::new(
        Box::new(df_core::named_struct(args)),
        transport::data_type(),
    ));
    Ok(datafusion::logical_expr::when(
        value.is_null(),
        lit(ScalarValue::try_from(&transport::data_type())?),
    )
    .otherwise(packed)?)
}

/// Identify direct primitive-union operands before generic SQL casts can expose
/// their transport struct as a string, map, or truth value.
pub(super) fn direct_operand(plan: &LogicalPlan, expr: &IrExpr) -> bool {
    let column = match expr {
        IrExpr::Binding(binding) => resolve_column_name(plan, binding),
        IrExpr::Property { binding, name, .. } => Some(prop_col(binding, name)),
        IrExpr::Lit(Lit::Scalar(value)) => return transport::is_type(&value.data_type()),
        _ => None,
    };
    column
        .and_then(|column| col_exact(column).get_type(plan.schema()).ok())
        .is_some_and(|ty| transport::is_type(&ty))
}
