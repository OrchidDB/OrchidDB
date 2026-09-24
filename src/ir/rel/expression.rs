//! Scalar expression lowering and function/type classification.

use super::*;

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_expr(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        if matches!(expr, IrExpr::Call { name, .. } if name.starts_with("gremlin_string_") || name == "gremlin_cast_date") {
            return Err(RelError::Unsupported("Gremlin scalar semantics require native values".into()));
        }
        if matches!(
            expr,
            IrExpr::Binary { .. }
                | IrExpr::Not(_)
                | IrExpr::IsNull(_)
                | IrExpr::IsNotNull(_)
                | IrExpr::StringPredicate { .. }
                | IrExpr::Case { .. }
                | IrExpr::Call { .. }
                | IrExpr::ListTransform { .. }
                | IrExpr::ListFilter { .. }
                | IrExpr::ListReduce { .. }
        ) && expr_is_constant(expr, &[])
        {
            if let Some(folded) = self.try_constant_fold(expr)? {
                return Ok(folded);
            }
        }
        match expr {
            IrExpr::Lit(Lit::Int(value))
                if self.language == Language::Gremlin && i32::try_from(*value).is_ok() =>
            {
                Ok(lit(ScalarValue::Int32(Some(*value as i32))))
            }
            IrExpr::Lit(lit_value) => Ok(lit_to_expr(lit_value)),
            IrExpr::List(_) if self.language == Language::Gremlin => Err(RelError::Unsupported(
                "Gremlin list literal requires native values".into(),
            )),
            IrExpr::List(items) if self.language == Language::Cypher => {
                let values = items
                    .iter()
                    .map(|item| self.lower_expr(plan, item))
                    .collect::<RelResult<Vec<_>>>()?;
                // SQL arrays must not silently coerce mixed Cypher values to
                // strings. Unsupported heterogeneous shapes use the runtime.
                let types = values
                    .iter()
                    .map(|value| value.get_type(plan.schema()))
                    .collect::<datafusion::common::Result<Vec<_>>>()?;
                let mut concrete = types.iter().filter(|kind| **kind != DataType::Null);
                if let Some(first) = concrete.next() {
                    if concrete.any(|kind| kind != first) {
                        return Err(RelError::Unsupported(
                            "heterogeneous Cypher collection requires runtime values".into(),
                        ));
                    }
                }
                Ok(datafusion::functions_nested::expr_fn::make_array(values))
            }
            IrExpr::List(items) => self.lower_collection_expr(plan, items),
            IrExpr::Binding(binding) => {
                if let Some(column) = resolve_column_name(plan, binding) {
                    Ok(col_exact(column))
                } else if let Some(shape) = has_binding_shape(plan, binding) {
                    if self.language == Language::Gremlin {
                        gremlin_element_display_expr(plan, binding)
                    } else if self.language == Language::Cypher && self.options.mapping.is_none() {
                        // A scalar SQL string cannot preserve graph identity
                        // inside lists, maps or grouping keys. Keep the real
                        // element in the runtime value channel.
                        Err(RelError::Unsupported(
                            "Cypher graph value requires runtime identity".into(),
                        ))
                    } else {
                        self.cypher_element_display_expr(plan, binding, shape)
                    }
                } else {
                    Err(RelError::Unsupported(format!(
                        "unavailable binding `{binding}`"
                    )))
                }
            }
            IrExpr::Property { binding, name, .. } => {
                let col = prop_col(binding, name);
                if has_exact_col(plan, &col) {
                    Ok(col_exact(col))
                } else if has_binding_shape(plan, binding).is_some() {
                    Ok(lit(ScalarValue::Utf8(None)))
                } else {
                    Err(RelError::Unsupported(format!(
                        "property `{binding}.{name}` without element binding"
                    )))
                }
            }
            IrExpr::Id(binding) => {
                let col = id_col(binding);
                if !has_exact_col(plan, &col) {
                    return Err(RelError::Unsupported(format!("id({binding})")));
                }
                // Gremlin source and hasId filters pair a label predicate
                // with the per-label row id. Cypher's ID() result is the
                // provider-qualified `table:offset` value below.
                if self.language == Language::Gremlin {
                    return Ok(col_exact(col));
                }
                // Kuzu's `ID()` yields an internal id that prints as
                // `table:offset`, not a bare offset. Mirror
                // `interpreter::element_id` so the same element gets the same
                // id whichever path evaluated it.
                let table_index = match has_binding_shape(plan, binding) {
                    Some(BindingShape::Node) => label_index_case(
                        col_exact(label_col(binding)),
                        self.graph.node_label_order(),
                        0,
                        1,
                    ),
                    Some(BindingShape::Edge) => {
                        rel_index_case(col_exact(label_col(binding)), self.graph)
                    }
                    // Not an element binding — nothing to qualify it with.
                    None => return Ok(col_exact(col)),
                };
                Ok(concat_exprs(vec![
                    table_index,
                    lit(":"),
                    cast_utf8(col_exact(col)),
                ]))
            }
            IrExpr::Label(binding) => {
                let col = label_col(binding);
                if has_exact_col(plan, &col) {
                    Ok(col_exact(col))
                } else {
                    Err(RelError::Unsupported(format!("label({binding})")))
                }
            }
            IrExpr::HasLabel { binding, label } => {
                let col = label_col(binding);
                if has_exact_col(plan, &col) {
                    Ok(binary(
                        col_exact(col),
                        BinaryOp::Eq,
                        lit(ScalarValue::Utf8(Some(label.clone()))),
                    ))
                } else {
                    Err(RelError::Unsupported(format!(
                        "has_label({binding}, {label})"
                    )))
                }
            }
            IrExpr::Binary { op, lhs, rhs } => self.lower_comparison_or_binary(plan, lhs, *op, rhs),
            IrExpr::Not(inner) => Ok(Expr::Not(Box::new(self.lower_expr(plan, inner)?))),
            IrExpr::StringPredicate {
                op,
                target,
                pattern,
            } => {
                let target = self.lower_expr(plan, target)?;
                let pattern = self.lower_expr(plan, pattern)?;
                Ok(match op {
                    StringOp::StartsWith => df_string::starts_with(target, pattern),
                    StringOp::EndsWith => df_string::ends_with(target, pattern),
                    StringOp::Contains => df_string::contains(target, pattern),
                })
            }
            IrExpr::IsNull(inner) => Ok(self.lower_expr(plan, inner)?.is_null()),
            IrExpr::IsNotNull(inner) => Ok(self.lower_expr(plan, inner)?.is_not_null()),
            IrExpr::IsBound(binding) => {
                if has_exact_col(plan, binding) {
                    Ok(col_exact(binding).is_not_null())
                } else if has_binding_shape(plan, binding).is_some() {
                    Ok(col_exact(id_col(binding)).is_not_null())
                } else {
                    Ok(lit(false))
                }
            }
            IrExpr::Case { arms, otherwise } => {
                let when_then_expr = arms
                    .iter()
                    .filter(|(when, _)| !matches!(when, IrExpr::IsBound(binding)
                        if !has_exact_col(plan, binding) && has_binding_shape(plan, binding).is_none()))
                    .map(|(when, then)| {
                        Ok((
                            Box::new(self.lower_expr(plan, when)?),
                            Box::new(self.lower_expr(plan, then)?),
                        ))
                    })
                    .collect::<RelResult<Vec<_>>>()?;
                let else_expr = otherwise
                    .as_ref()
                    .map(|expr| self.lower_expr(plan, expr).map(Box::new))
                    .transpose()?;
                if when_then_expr.is_empty() {
                    return Ok(else_expr
                        .map(|expr| *expr)
                        .unwrap_or_else(|| lit(ScalarValue::Null)));
                }
                Ok(Expr::Case(Case::new(None, when_then_expr, else_expr)))
            }
            IrExpr::Call { name, args } if name == "path_or_self" => {
                if self.language == Language::Gremlin {
                    return Err(RelError::Unsupported("Gremlin path requires native runtime values".into()));
                }
                let Some(fallback) = args.get(1) else {
                    return Err(RelError::Unsupported("path_or_self arity".into()));
                };
                self.lower_expr(plan, fallback)
            }
            IrExpr::Call { name, args } if name.eq_ignore_ascii_case("range") => {
                let values = constant_range_values(args)?;
                Ok(lit(rel_display_value(
                    &Value::List(values),
                    self.language,
                    literal_collection_context(self.language),
                )))
            }
            IrExpr::Call { name, args } if name == "cypher_slice" && args.len() == 3 => {
                let array = if matches!(&args[0], IrExpr::List(_)) {
                    self.lower_native_list(plan, &args[0])?
                } else {
                    self.lower_list_operand(plan, &args[0])?
                };
                if !matches!(
                    array.get_type(plan.schema())?,
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                ) {
                    return Err(RelError::Unsupported(
                        "Cypher slice requires a native collection".into(),
                    ));
                }
                let length = datafusion::functions_nested::expr_fn::array_length(array.clone());
                let bound = |value: Expr| {
                    let relative = Expr::Case(Case::new(
                        None,
                        vec![(
                            Box::new(binary(value.clone(), BinaryOp::Lt, lit(0_i64))),
                            Box::new(binary(length.clone(), BinaryOp::Add, value.clone())),
                        )],
                        Some(Box::new(value)),
                    ));
                    Expr::Case(Case::new(
                        None,
                        vec![
                            (
                                Box::new(binary(relative.clone(), BinaryOp::Lt, lit(0_i64))),
                                Box::new(lit(0_i64)),
                            ),
                            (
                                Box::new(binary(relative.clone(), BinaryOp::Gt, length.clone())),
                                Box::new(length.clone()),
                            ),
                        ],
                        Some(Box::new(relative)),
                    ))
                };
                let start = self.lower_expr(plan, &args[1])?;
                let end = self.lower_expr(plan, &args[2])?;
                let null_bound = Expr::or(start.clone().is_null(), end.clone().is_null());
                let sliced = datafusion::functions_nested::expr_fn::array_slice(
                    array,
                    binary(bound(start), BinaryOp::Add, lit(1_i64)),
                    bound(end),
                    None,
                );
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(Box::new(null_bound), Box::new(lit(ScalarValue::Null)))],
                    Some(Box::new(sliced)),
                )))
            }
            IrExpr::Call { name, args } if name == "list_slice" && args.len() == 3 => {
                let array = self.lower_list_operand(plan, &args[0])?;
                let start = self.lower_expr(plan, &args[1])?;
                let end = self.lower_expr(plan, &args[2])?;
                if matches!(
                    array.get_type(plan.schema())?,
                    DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
                ) && !collections::is_encoded_property(plan, &args[0])
                {
                    return Ok(collections::string_slice_expr(array, start, end));
                }
                Ok(datafusion::functions_nested::expr_fn::array_slice(
                    array, start, end, None,
                ))
            }
            IrExpr::Call { name, args } if is_constant_collection_function(name) => {
                self.lower_constant_collection_function(plan, name, args)
            }
            IrExpr::Call { name, args } if name == "integer_literal" && args.len() == 1 => {
                self.lower_integer_literal(&args[0])
            }
            IrExpr::Call { name, args } if is_label_function(name) && args.len() == 1 => {
                match &args[0] {
                    IrExpr::Binding(binding) => {
                        self.lower_expr(plan, &IrExpr::Label(binding.clone()))
                    }
                    arg => self.lower_expr(plan, arg),
                }
            }
            IrExpr::Call { name, args } if is_id_function(name) && args.len() == 1 => {
                match &args[0] {
                    IrExpr::Binding(binding) => self.lower_expr(plan, &IrExpr::Id(binding.clone())),
                    arg => self.lower_expr(plan, arg),
                }
            }
            IrExpr::Call { name, args } if name.eq_ignore_ascii_case("uuid") && args.len() == 1 => {
                Ok(df_string::lower(self.lower_expr(plan, &args[0])?))
            }
            IrExpr::Call { name, args }
                if name.eq_ignore_ascii_case("gen_random_uuid") && args.is_empty() =>
            {
                Ok(df_string::uuid())
            }
            IrExpr::Call { name, args }
                if name.eq_ignore_ascii_case("gremlin_cast_int") && args.len() == 1 =>
            {
                // TinkerPop narrows fractional values toward zero. DuckDB's
                // direct floating-to-integer cast rounds, so make the
                // language choice explicit in the relational expression.
                let value = self.lower_expr(plan, &args[0])?;
                Ok(Expr::Cast(Cast::new(
                    Box::new(df_math::trunc(vec![value])),
                    DataType::Int32,
                )))
            }
            IrExpr::Call { name, args } if name == "cast_number" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                Ok(match data_type {
                    DataType::Int8
                    | DataType::Int16
                    | DataType::Int32
                    | DataType::Int64
                    | DataType::UInt8
                    | DataType::UInt16
                    | DataType::UInt32
                    | DataType::UInt64
                    | DataType::Float32
                    | DataType::Float64
                    | DataType::Decimal128(_, _) => value,
                    DataType::Boolean => Expr::Cast(Cast::new(Box::new(value), DataType::Int64)),
                    _ => Expr::TryCast(TryCast::new(Box::new(value), DataType::Float64)),
                })
            }
            IrExpr::Call { name, args } if name == "gremlin_cast_date" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                Ok(match data_type {
                    DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => value,
                    DataType::Date32 | DataType::Timestamp(_, _) => cast_utf8(value),
                    _ => cast_utf8(Expr::TryCast(TryCast::new(
                        Box::new(value),
                        DataType::Timestamp(arrow::datatypes::TimeUnit::Millisecond, None),
                    ))),
                })
            }
            IrExpr::Call { name, args } if is_cast_function(name, args) => {
                if let Some(target) = cast_target_text(name, args)
                    && target.trim().trim_matches('"').trim().ends_with("[]")
                {
                    // Catalog list properties are canonical display strings
                    // at the relational boundary, so a list cast rewrites
                    // that text; `casts.rs` proves the rewrite per value.
                    return self.lower_list_cast(plan, name, args, target);
                }
                let (mut value, data_type, lenient) = self.cast_parts(plan, name, args)?;
                if name.eq_ignore_ascii_case("tointeger")
                    && matches!(
                        value.get_type(plan.schema())?,
                        DataType::Float16
                            | DataType::Float32
                            | DataType::Float64
                            | DataType::Decimal128(_, _)
                            | DataType::Decimal256(_, _)
                    )
                {
                    value = df_math::trunc(vec![value]);
                }
                let cast = if lenient {
                    Expr::TryCast(TryCast::new(Box::new(value), data_type))
                } else {
                    Expr::Cast(Cast::new(Box::new(value), data_type))
                };
                Ok(cast)
            }
            IrExpr::Call { name, args } if is_mod_function(name) && args.len() == 2 => {
                Ok(Expr::BinaryExpr(BinaryExpr::new(
                    Box::new(self.lower_expr(plan, &args[0])?),
                    Operator::Modulo,
                    Box::new(self.lower_expr(plan, &args[1])?),
                )))
            }
            IrExpr::Call { name, args } if is_abs_function(name) && args.len() == 1 => {
                Ok(df_math::abs(self.lower_expr(plan, &args[0])?))
            }
            IrExpr::Call { name, args } if is_pow_function(name) && args.len() == 2 => {
                Ok(df_math::power(
                    self.lower_expr(plan, &args[0])?,
                    self.lower_expr(plan, &args[1])?,
                ))
            }
            IrExpr::Call { name, args } if is_unary_math_function(name) && args.len() == 1 => {
                self.lower_unary_math_function(plan, name, &args[0])
            }
            IrExpr::Call { name, args } if is_binary_math_function(name) && args.len() == 2 => {
                self.lower_binary_math_function(plan, name, &args[0], &args[1])
            }
            IrExpr::Call { name, args } if is_date_function(name) && args.len() == 1 => {
                Ok(cast_utf8(self.lower_expr(plan, &args[0])?))
            }
            IrExpr::Call { name, args }
                if matches!(
                    normalize_function_name(name).as_str(),
                    "date_part" | "date_trunc"
                ) =>
            {
                self.lower_temporal_function(plan, name, args)
            }
            IrExpr::Call { name, args } if is_string_function(name) => {
                self.lower_string_function(plan, name, args)
            }
            IrExpr::Call { name, args } if is_core_variadic_function(name) => {
                self.lower_core_variadic_function(plan, name, args)
            }
            IrExpr::Call { name, args } if name == "gremlin_math_bin" && args.len() == 3 => {
                let IrExpr::Lit(Lit::String(op)) = &args[0] else {
                    return Err(RelError::Unsupported(
                        "dynamic Gremlin math operator".into(),
                    ));
                };
                let number =
                    |expr: Expr| Expr::TryCast(TryCast::new(Box::new(expr), DataType::Float64));
                let lhs = number(self.lower_expr(plan, &args[1])?);
                let rhs = number(self.lower_expr(plan, &args[2])?);
                let op = match op.as_str() {
                    "add" => BinaryOp::Add,
                    "sub" => BinaryOp::Sub,
                    "mul" => BinaryOp::Mul,
                    "div" => BinaryOp::Div,
                    _ => {
                        return Err(RelError::Unsupported(format!(
                            "Gremlin math operator `{op}`"
                        )));
                    }
                };
                Ok(binary(lhs, op, rhs))
            }
            IrExpr::Call { name, args } if name == "format_concat" && !args.is_empty() => {
                let pieces = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(concat_exprs(pieces))
            }
            IrExpr::Call { name, args } if name == "conjoin" && args.len() == 2 => {
                let value = self.lower_expr(plan, &args[0])?;
                let delimiter = cast_utf8(self.lower_expr(plan, &args[1])?);
                let data_type = value.get_type(plan.schema())?;
                Ok(
                    if matches!(
                        data_type,
                        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                    ) {
                        datafusion::functions_nested::expr_fn::array_to_string(value, delimiter)
                    } else if matches!(
                        data_type,
                        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
                    ) {
                        concat_exprs(vec![cast_utf8(value), delimiter])
                    } else {
                        cast_utf8(value)
                    },
                )
            }
            IrExpr::Call { name, args } if name == "null_to_sentinel" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(value.clone().is_null()),
                        Box::new(lit("\0gremlin.null")),
                    )],
                    Some(Box::new(cast_utf8(value))),
                )))
            }
            IrExpr::Call { name, args } if name == "gremlin_dedup_key" && args.len() == 1 => {
                // Relational maps are already canonical display values. For
                // scalar and element keys, Gremlin dedup uses the value as-is.
                self.lower_expr(plan, &args[0])
            }
            IrExpr::Call { name, args }
                if name == "list_restore_null_sentinels" && args.len() == 1 =>
            {
                let value = self.lower_native_list(plan, &args[0])?;
                Ok(datafusion::functions_nested::expr_fn::array_replace_all(
                    value,
                    lit("\0gremlin.null"),
                    lit(ScalarValue::Utf8(None)),
                ))
            }
            IrExpr::Call { name, args } if name.eq_ignore_ascii_case("xor") && args.len() == 2 => {
                let lhs = self.lower_expr(plan, &args[0])?;
                let rhs = self.lower_expr(plan, &args[1])?;
                Ok(Expr::or(
                    Expr::and(lhs.clone(), Expr::Not(Box::new(rhs.clone()))),
                    Expr::and(Expr::Not(Box::new(lhs)), rhs),
                ))
            }
            IrExpr::Call { name, args } if is_exists_function(name) && args.len() == 1 => {
                Ok(self.lower_expr(plan, &args[0])?.is_not_null())
            }
            IrExpr::Call { name, args } if is_in_function(name) && args.len() == 2 => {
                let expr = self.lower_expr(plan, &args[0])?;
                let IrExpr::List(values) = &args[1] else {
                    return Err(RelError::Unsupported("dynamic IN list".into()));
                };
                let list = values
                    .iter()
                    .map(|value| self.lower_expr(plan, value))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(Expr::InList(InList::new(Box::new(expr), list, false)))
            }
            IrExpr::Call { name, args } if name == "typeof_matches" && args.len() == 2 => {
                let IrExpr::Lit(Lit::String(type_name)) = &args[1] else {
                    return Err(RelError::Unsupported("dynamic typeOf target".into()));
                };
                self.lower_typeof_matches(plan, &args[0], type_name)
            }
            IrExpr::Call { name, args } if name == "map_has_key" && args.len() == 2 => {
                Ok(lit(false))
            }
            IrExpr::Call { name, args } if name == "union_value" => {
                let (_, value) = union_constructor_field(args)?;
                self.lower_expr(plan, value)
            }
            IrExpr::Call { name, args } if name == "union_tag" && args.len() == 1 => {
                if let IrExpr::Call {
                    name: constructor,
                    args,
                } = &args[0]
                    && constructor == "union_value"
                {
                    let (tag, _) = union_constructor_field(args)?;
                    Ok(lit(tag.to_string()))
                } else if let Some(tag) = self.lower_value_union_tag(plan, &args[0])? {
                    Ok(tag)
                } else if let IrExpr::Property { binding, name, .. } = &args[0] {
                    let column = union_tag_col(binding, name);
                    if has_exact_col(plan, &column) {
                        Ok(col_exact(column))
                    } else {
                        Err(RelError::Unsupported(format!(
                            "union_tag metadata is unavailable for `{binding}.{name}`"
                        )))
                    }
                } else {
                    Err(RelError::Unsupported(
                        "union_tag over a stored/dynamic union".into(),
                    ))
                }
            }
            IrExpr::Call { name, args } if name == "union_extract" && args.len() == 2 => {
                let IrExpr::Call {
                    name: constructor,
                    args: constructor_args,
                } = &args[0]
                else {
                    return Err(RelError::Unsupported(
                        "union_extract over a stored/dynamic union".into(),
                    ));
                };
                if constructor != "union_value" {
                    return Err(RelError::Unsupported(
                        "union_extract over a stored/dynamic union".into(),
                    ));
                }
                let (tag, value) = union_constructor_field(constructor_args)?;
                let IrExpr::Lit(Lit::String(requested)) = &args[1] else {
                    return Err(RelError::Unsupported(
                        "union_extract with a dynamic tag".into(),
                    ));
                };
                if tag.eq_ignore_ascii_case(requested) {
                    self.lower_expr(plan, value)
                } else {
                    Ok(lit(ScalarValue::Utf8(None)))
                }
            }
            IrExpr::Call { name, args }
                if name == "select_key_or_binding_pop" && args.len() == 5 =>
            {
                if let IrExpr::Binding(label) = &args[1] {
                    self.check_select_pop(label, &args[4])?;
                }
                self.lower_select_key_or_binding(plan, &args[1])
            }
            IrExpr::Call { name, args }
                if (name == "value_map" || name == "value_map_tokens") && !args.is_empty() =>
            {
                self.lower_value_map(plan, name, args)
            }
            IrExpr::Call { name, args } if name == "gremlin_unfold_items" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                if matches!(
                    data_type,
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                ) {
                    Ok(value)
                } else {
                    // Gremlin unfolds a scalar, including null, as one item.
                    Ok(datafusion::functions_nested::expr_fn::make_array(vec![
                        value,
                    ]))
                }
            }
            IrExpr::Call { name, args } if name == "local_count" && args.len() == 1 => {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                if matches!(
                    data_type,
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                ) {
                    Ok(Expr::Cast(Cast::new(
                        Box::new(datafusion::functions_nested::expr_fn::array_length(value)),
                        DataType::Int64,
                    )))
                } else {
                    Ok(lit(1_i64))
                }
            }
            IrExpr::Call { name, args }
                if matches!(name.as_str(), "local_min" | "local_max") && args.len() == 1 =>
            {
                let value = self.lower_expr(plan, &args[0])?;
                let data_type = value.get_type(plan.schema())?;
                if matches!(
                    data_type,
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
                ) {
                    Ok(if name == "local_min" {
                        datafusion::functions_nested::expr_fn::array_min(value)
                    } else {
                        datafusion::functions_nested::expr_fn::array_max(value)
                    })
                } else {
                    Ok(value)
                }
            }
            IrExpr::Call { name, args }
                if matches!(
                    name.as_str(),
                    "list_combine" | "list_merge" | "list_intersect"
                ) && args.len() == 2 =>
            {
                if self.language == Language::Gremlin && name != "list_combine" {
                    return Err(RelError::Unsupported(
                        "Gremlin set result requires typed runtime shaping".into(),
                    ));
                }
                let lhs = self.lower_native_list(plan, &args[0])?;
                let rhs = self.lower_native_list(plan, &args[1])?;
                Ok(match name.as_str() {
                    "list_combine" => {
                        datafusion::functions_nested::expr_fn::array_concat(vec![lhs, rhs])
                    }
                    "list_merge" => datafusion::functions_nested::expr_fn::array_distinct(
                        datafusion::functions_nested::expr_fn::array_concat(vec![lhs, rhs]),
                    ),
                    "list_intersect" => {
                        datafusion::functions_nested::expr_fn::array_intersect(lhs, rhs)
                    }
                    _ => unreachable!(),
                })
            }
            IrExpr::Call { name, args }
                if matches!(name.as_str(), "sack_apply" | "fold_reduce") && args.len() == 3 =>
            {
                self.lower_gremlin_state_call(plan, name, args)
            }
            IrExpr::Call { name, args } if name == "map" => self.lower_cypher_map(plan, args),
            IrExpr::Call { name, args } if name == "make_map" => self.lower_make_map(plan, args),
            IrExpr::Call { name, args } if name == "cypher_subscript" && args.len() == 2 => {
                self.lower_cypher_subscript(plan, &args[0], &args[1])
            }
            IrExpr::Call { name, args } if name.starts_with("cypher_") && args.len() == 2 => {
                let op = match name.as_str() {
                    "cypher_eq" => BinaryOp::Eq,
                    "cypher_neq" => BinaryOp::Neq,
                    "cypher_lt" => BinaryOp::Lt,
                    "cypher_lte" => BinaryOp::Lte,
                    "cypher_gt" => BinaryOp::Gt,
                    "cypher_gte" => BinaryOp::Gte,
                    _ => {
                        return Err(RelError::Unsupported(format!(
                            "function `{name}` is not relationally lowered yet"
                        )));
                    }
                };
                self.lower_comparison_or_binary(plan, &args[0], op, &args[1])
            }
            IrExpr::Call { name, args } if name == "gremlin_compare" && args.len() == 3 => {
                let IrExpr::Lit(Lit::String(op)) = &args[0] else {
                    return Err(RelError::Unsupported("dynamic Gremlin comparison".into()));
                };
                let lhs = self.lower_expr(plan, &args[1])?;
                let rhs = self.lower_expr(plan, &args[2])?;
                let left_type = lhs.get_type(plan.schema())?;
                let right_type = rhs.get_type(plan.schema())?;
                let integer = |ty: &DataType| matches!(ty, DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64);
                let equality = op == "eq" || op == "neq";
                let safe = integer(&left_type) && integer(&right_type)
                    || left_type == right_type && (left_type == DataType::Boolean || equality && matches!(left_type, DataType::Utf8 | DataType::LargeUtf8));
                if !safe {
                    // Floating/decimal promotion, signed zero and compound values
                    // require NumberHelper/native Gremlin comparability.
                    return Err(RelError::Unsupported("Gremlin comparison requires native promotion".into()));
                }
                let operator = match op.as_str() {
                    "eq" => BinaryOp::Eq, "neq" => BinaryOp::Neq,
                    "lt" => BinaryOp::Lt, "lte" => BinaryOp::Lte,
                    "gt" => BinaryOp::Gt, "gte" => BinaryOp::Gte,
                    _ => return Err(RelError::Unsupported("unknown Gremlin comparison".into())),
                };
                if equality {
                    let both_null = Expr::and(lhs.clone().is_null(), rhs.clone().is_null());
                    let equal = Expr::or(both_null, datafusion::functions::core::expr_fn::coalesce(vec![binary(lhs, BinaryOp::Eq, rhs), lit(false)]));
                    Ok(if op == "neq" { Expr::Not(Box::new(equal)) } else { equal })
                } else { Ok(binary(lhs, operator, rhs)) }
            }
            IrExpr::ListFilter { list, item, predicate } if self.options.mapping.is_some() => {
                let IrExpr::Call { name, args } = list.as_ref() else {
                    return Err(RelError::Unsupported("Mapped list filter requires property values".into()));
                };
                let [IrExpr::Binding(binding), IrExpr::List(keys)] = args.as_slice() else {
                    return Err(RelError::Unsupported("Mapped list filter requires named properties".into()));
                };
                if name != "requested_property_values" { return Err(RelError::Unsupported("Mapped list filter source".into())); }
                use datafusion::common::tree_node::{TreeNode, Transformed};
                use datafusion::functions_nested::expr_fn::{make_array, array_concat};
                let mut result: Option<Expr> = None;
                for key in keys {
                    let IrExpr::Lit(Lit::String(key)) = key else { return Err(RelError::Unsupported("Dynamic mapped property key".into())); };
                    let column = prop_col(binding, key);
                    if !has_exact_col(plan, &column) { continue; }
                    let value = col_exact(&column);
                    // Introduce the iterator only for type/name resolution, then
                    // substitute its scalar column into the SQL predicate.
                    let mut fields = plan.schema().fields().iter().map(|f|col_exact(f.name())).collect::<Vec<_>>();
                    fields.push(value.clone().alias(item));
                    let scope = LogicalPlanBuilder::from(plan.clone()).project(fields)?.build()?;
                    let condition = self.lower_expr(&scope, predicate)?.transform_up(|expr| {
                        if matches!(&expr, Expr::Column(c) if &c.name == item) {
                            Ok(Transformed::yes(value.clone()))
                        } else { Ok(Transformed::no(expr)) }
                    })?.data;
                    let selected = datafusion::logical_expr::when(value.clone().is_not_null().and(condition), make_array(vec![value])).otherwise(make_array(vec![]))?;
                    result = Some(match result { Some(previous) => array_concat(vec![previous, selected]), None => selected });
                }
                Ok(result.unwrap_or_else(||make_array(vec![])))
            }
            IrExpr::Call { name, args } if name == "requested_property_values" && self.options.mapping.is_some() => {
                let [IrExpr::Binding(binding), IrExpr::List(keys)] = args.as_slice() else {
                    return Err(RelError::Unsupported("Mapped property projection requires literal keys".into()));
                };
                let mut columns = Vec::new();
                if keys.is_empty() {
                    let prefix = format!("{binding}__prop__");
                    columns.extend(plan.schema().fields().iter().filter(|f| f.name().starts_with(&prefix)).map(|f|col_exact(f.name())));
                } else {
                    for key in keys {
                        let IrExpr::Lit(Lit::String(key)) = key else { return Err(RelError::Unsupported("Dynamic mapped property key".into())); };
                        let column = prop_col(binding, key);
                        if has_exact_col(plan, &column) { columns.push(col_exact(column)); }
                    }
                }
                let types = columns.iter().map(|c|c.get_type(plan.schema())).collect::<datafusion::common::Result<Vec<_>>>()?;
                if types.windows(2).any(|pair|pair[0]!=pair[1]) { return Err(RelError::Unsupported("Mapped property values have incompatible SQL types".into())); }
                use datafusion::functions_nested::expr_fn::{make_array, array_concat};
                let mut result: Option<Expr> = None;
                for column in columns {
                    let item = datafusion::logical_expr::when(column.clone().is_not_null(),make_array(vec![column])).otherwise(make_array(vec![]))?;
                    result = Some(match result { Some(previous) => array_concat(vec![previous,item]), None => item });
                }
                Ok(result.unwrap_or_else(||make_array(vec![])))
            }
            IrExpr::Call { name, .. } if name == "requested_property_values" => {
                Err(RelError::Unsupported(
                    "Heterogeneous Gremlin property values require native runtime values".into(),
                ))
            }
            IrExpr::Call { name, args } => {
                let args = args
                    .iter()
                    .map(|arg| {
                        if matches!(arg, IrExpr::List(_)) {
                            self.lower_native_list(plan, arg)
                        } else if let Some(native) = collections::native_list_property(plan, arg) {
                            Ok(native)
                        } else {
                            self.lower_expr(plan, arg)
                        }
                    })
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(crate::ir::functions::native_scalar(
                    name,
                    args,
                    plan.schema(),
                )?)
            }
            other => Err(RelError::Unsupported(format!(
                "expression `{other:?}` is not relationally lowered yet"
            ))),
        }
    }

    /// Evaluate a constant expression through the interpreter so folding
    /// matches engine semantics exactly (including Kuzu-style error text).
    /// Returns `Ok(None)` when the interpreter cannot evaluate it for an
    /// internal reason, letting the relational lowering take over.
    pub(super) fn try_constant_fold(&self, expr: &IrExpr) -> RelResult<Option<Expr>> {
        let row = InterpreterRow::new();
        match interpreter_eval(expr, &row, self.graph) {
            Ok(Value::Map(_)) if self.language == Language::Cypher => {
                // Rendering a map here destroys its key/value types before
                // later WITH expressions can read them. Keep it in runtime
                // form until a faithful native representation is available.
                Err(RelError::Unsupported(
                    "Cypher map requires runtime values".into(),
                ))
            }
            Ok(Value::List(items)) if self.language == Language::Cypher => {
                fn native(value: &Value) -> RelResult<Expr> {
                    match value {
                        Value::List(items) => {
                            let values = items.iter().map(native).collect::<RelResult<Vec<_>>>()?;
                            // Constant values carry exact types before SQL
                            // planning; heterogeneous lists remain runtime data.
                            let schema = datafusion::common::DFSchema::empty();
                            let types = values
                                .iter()
                                .map(|value| value.get_type(&schema))
                                .collect::<datafusion::common::Result<Vec<_>>>()?;
                            let mut concrete = types.iter().filter(|kind| **kind != DataType::Null);
                            if let Some(first) = concrete.next() {
                                if concrete.any(|kind| kind != first) {
                                    return Err(RelError::Unsupported(
                                        "heterogeneous Cypher constant requires runtime values"
                                            .into(),
                                    ));
                                }
                            }
                            Ok(datafusion::functions_nested::expr_fn::make_array(values))
                        }
                        _ => value_literal_expr(value),
                    }
                }
                native(&Value::List(items)).map(Some)
            }
            Ok(
                Value::List(_)
                | Value::Map(_)
                | Value::TypedMap(_) | Value::Token(_) | Value::Direction(_)
                | Value::Path(_)
                | Value::BigInt(_)
                | Value::BigDecimal(_),
            ) if self.language == Language::Gremlin => {
                // The relational display renderer serializes these values to
                // text (or bounded decimals). Preserve native Gremlin types
                // by evaluating this expression at the runtime boundary.
                Err(RelError::Unsupported(
                    "Gremlin constant requires native values".into(),
                ))
            }
            Ok(value) => Ok(Some(constant_fold_result_expr(&value, self.language))),
            Err(err) => {
                let message = err.to_string();
                if looks_like_engine_error(&message) {
                    Err(RelError::Unsupported(message))
                } else {
                    Ok(None)
                }
            }
        }
    }

    pub(super) fn cast_parts(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<(Expr, DataType, bool)> {
        let normalized = name.to_ascii_lowercase();
        let (value_arg, target_name, lenient) = if normalized == "cast" {
            let [value, target] = args else {
                return Err(RelError::Unsupported("cast arity".into()));
            };
            let IrExpr::Lit(Lit::String(target_name)) = target else {
                return Err(RelError::Unsupported("dynamic cast target".into()));
            };
            (value, target_name.as_str(), false)
        } else {
            let [value] = args else {
                return Err(RelError::Unsupported(format!("{name} arity")));
            };
            let lenient = matches!(
                normalized.as_str(),
                "tointeger" | "tofloat" | "toboolean" | "tostring"
            );
            (value, cast_target_from_function_name(&normalized)?, lenient)
        };
        let value = self.lower_expr(plan, value_arg)?;
        let data_type = data_type_for_cast_target(target_name)?;
        Ok((value, data_type, lenient))
    }
}

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_temporal_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let [unit, value] = args else {
            return Err(RelError::Unsupported(format!("{name} arity")));
        };
        let unit = match constant_value_expr(unit)? {
            Some(Value::String(unit)) => lit(normalize_temporal_unit(&unit)),
            _ => cast_utf8(self.lower_expr(plan, unit)?),
        };
        let value = self.lower_expr(plan, value)?;
        let original_type = value.get_type(plan.schema())?;
        let temporal = if matches!(
            original_type,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
        ) {
            Expr::TryCast(TryCast::new(
                Box::new(value.clone()),
                DataType::Timestamp(arrow::datatypes::TimeUnit::Nanosecond, None),
            ))
        } else {
            value.clone()
        };
        match normalize_function_name(name).as_str() {
            "date_part" => Ok(df_datetime::date_part(unit, temporal)),
            "date_trunc" => {
                let rendered = cast_utf8(df_datetime::date_trunc(unit, temporal));
                if matches!(
                    original_type,
                    DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
                ) {
                    // Date values and timestamps share the catalog's textual
                    // boundary. Preserve date-only output when the source has
                    // no time component; timestamps retain midnight fields.
                    let is_date = binary(
                        df_unicode::length(cast_utf8(value)),
                        BinaryOp::Eq,
                        lit(10_i64),
                    );
                    Ok(Expr::Case(Case::new(
                        None,
                        vec![(
                            Box::new(is_date),
                            Box::new(df_unicode::substring(
                                rendered.clone(),
                                lit(1_i64),
                                lit(10_i64),
                            )),
                        )],
                        Some(Box::new(rendered)),
                    )))
                } else {
                    Ok(rendered)
                }
            }
            _ => unreachable!(),
        }
    }

    pub(super) fn lower_cypher_subscript(
        &self,
        plan: &LogicalPlan,
        target: &IrExpr,
        index: &IrExpr,
    ) -> RelResult<Expr> {
        let target_expr = if matches!(target, IrExpr::List(_)) {
            self.lower_native_list(plan, target)?
        } else {
            self.lower_list_operand(plan, target)?
        };
        let data_type = target_expr.get_type(plan.schema())?;
        let index = Expr::Cast(Cast::new(
            Box::new(self.lower_expr(plan, index)?),
            DataType::Int64,
        ));
        match data_type {
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
                let position = Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(binary(index.clone(), BinaryOp::Gte, lit(0_i64))),
                        Box::new(binary(index.clone(), BinaryOp::Add, lit(1_i64))),
                    )],
                    Some(Box::new(index)),
                ));
                Ok(datafusion::functions_nested::expr_fn::array_element(
                    target_expr,
                    position,
                ))
            }
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
                let length = df_unicode::length(cast_utf8(target_expr.clone()));
                let valid = Expr::and(
                    binary(
                        index.clone(),
                        BinaryOp::Gte,
                        binary(lit(0_i64), BinaryOp::Sub, length.clone()),
                    ),
                    binary(index.clone(), BinaryOp::Lt, length.clone()),
                );
                let position = Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(binary(index.clone(), BinaryOp::Lt, lit(0_i64))),
                        Box::new(binary(
                            binary(length, BinaryOp::Add, index.clone()),
                            BinaryOp::Add,
                            lit(1_i64),
                        )),
                    )],
                    Some(Box::new(binary(index.clone(), BinaryOp::Add, lit(1_i64)))),
                ));
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(valid),
                        Box::new(df_unicode::substring(
                            cast_utf8(target_expr),
                            position,
                            lit(1_i64),
                        )),
                    )],
                    Some(Box::new(lit(ScalarValue::Utf8(None)))),
                )))
            }
            DataType::Null => Ok(lit(ScalarValue::Utf8(None))),
            other => Err(RelError::Unsupported(format!(
                "cypher subscript over type {other}"
            ))),
        }
    }

    pub(super) fn lower_integer_literal(&self, arg: &IrExpr) -> RelResult<Expr> {
        let Some(text) = integer_literal_text(arg) else {
            return Err(RelError::Unsupported(
                "integer_literal argument must be a literal string".into(),
            ));
        };
        if let Ok(value) = text.parse::<i64>() {
            return Ok(lit(value));
        }
        Ok(lit(rel_display_value(
            &Value::BigInt(
                BigInt::from_str(&text.replace('_', "")).map_err(|_| {
                    RelError::Unsupported(format!("invalid integer literal `{text}`"))
                })?,
            ),
            self.language,
            DisplayContext::Scalar,
        )))
    }

    pub(super) fn lower_unary_math_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        arg: &IrExpr,
    ) -> RelResult<Expr> {
        let arg = self.lower_expr(plan, arg)?;
        let normalized = normalize_function_name(name);
        match normalized.as_str() {
            "acos" => Ok(df_math::acos(arg)),
            "acosh" => Ok(df_math::acosh(arg)),
            "asin" => Ok(df_math::asin(arg)),
            "asinh" => Ok(df_math::asinh(arg)),
            "atan" => Ok(df_math::atan(arg)),
            "atanh" => Ok(df_math::atanh(arg)),
            "cbrt" => Ok(df_math::cbrt(arg)),
            "ceil" | "ceiling" => Ok(df_math::ceil(arg)),
            "cos" => Ok(df_math::cos(arg)),
            "cosh" => Ok(df_math::cosh(arg)),
            "cot" => Ok(df_math::cot(arg)),
            "degrees" => Ok(df_math::degrees(arg)),
            "exp" => Ok(df_math::exp(arg)),
            "factorial" => Ok(df_math::factorial(arg)),
            "floor" => Ok(df_math::floor(arg)),
            "ln" | "log" => Ok(df_math::ln(arg)),
            "log2" => Ok(df_math::log2(arg)),
            "log10" => Ok(df_math::log10(arg)),
            "radians" => Ok(df_math::radians(arg)),
            "round" => Ok(df_math::round(vec![arg])),
            "sign" | "signum" => Ok(df_math::signum(arg)),
            "sin" => Ok(df_math::sin(arg)),
            "sinh" => Ok(df_math::sinh(arg)),
            "sqrt" => Ok(df_math::sqrt(arg)),
            "tan" => Ok(df_math::tan(arg)),
            "tanh" => Ok(df_math::tanh(arg)),
            "trunc" | "truncate" => Ok(df_math::trunc(vec![arg])),
            _ => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
        }
    }

    pub(super) fn lower_binary_math_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        lhs: &IrExpr,
        rhs: &IrExpr,
    ) -> RelResult<Expr> {
        let lhs = self.lower_expr(plan, lhs)?;
        let rhs = self.lower_expr(plan, rhs)?;
        let normalized = normalize_function_name(name);
        match normalized.as_str() {
            "atan2" => Ok(df_math::atan2(lhs, rhs)),
            "gcd" => Ok(df_math::gcd(lhs, rhs)),
            "lcm" => Ok(df_math::lcm(lhs, rhs)),
            "log" => Ok(df_math::log(lhs, rhs)),
            "nanvl" => Ok(df_math::nanvl(lhs, rhs)),
            "round" => Ok(df_math::round(vec![lhs, rhs])),
            "trunc" | "truncate" => Ok(df_math::trunc(vec![lhs, rhs])),
            _ => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
        }
    }

    pub(super) fn lower_string_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let normalized = normalize_function_name(name);
        match normalized.as_str() {
            "concat" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::concat(args))
            }
            "concat_ws" => {
                let [delimiter, rest @ ..] = args else {
                    return Err(RelError::Unsupported("concat_ws arity".into()));
                };
                let delimiter = self.lower_expr(plan, delimiter)?;
                let rest = rest
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::concat_ws(delimiter, rest))
            }
            "contains" | "strcontains" => {
                let [value, needle] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::contains(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, needle)?),
                ))
            }
            "prefix" | "starts_with" | "startswith" => {
                let [value, prefix] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::starts_with(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, prefix)?),
                ))
            }
            "suffix" | "ends_with" | "endswith" => {
                let [value, suffix] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::ends_with(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, suffix)?),
                ))
            }
            "lcase" | "lower" | "tolower" | "gremlin_lcase" | "local_lcase" => {
                let [value] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::lower(cast_utf8(self.lower_expr(plan, value)?)))
            }
            "ucase" | "upper" | "toupper" | "gremlin_ucase" | "local_ucase" => {
                let [value] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                Ok(df_string::upper(cast_utf8(self.lower_expr(plan, value)?)))
            }
            "trim" | "local_trim" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::trim(args))
            }
            "ltrim" | "local_ltrim" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::ltrim(args))
            }
            "rtrim" | "local_rtrim" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg).map(cast_utf8))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_string::rtrim(args))
            }
            "replace" => {
                let [value, from, to] = args else {
                    return Err(RelError::Unsupported("replace arity".into()));
                };
                Ok(df_string::replace(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, from)?),
                    cast_utf8(self.lower_expr(plan, to)?),
                ))
            }
            "reverse" | "local_reverse_strings" => {
                let [value] = args else {
                    return Err(RelError::Unsupported("reverse arity".into()));
                };
                Ok(df_unicode::reverse(cast_utf8(
                    self.lower_expr(plan, value)?,
                )))
            }
            "left" => {
                let [value, count] = args else {
                    return Err(RelError::Unsupported("left arity".into()));
                };
                Ok(df_unicode::left(
                    cast_utf8(self.lower_expr(plan, value)?),
                    self.lower_expr(plan, count)?,
                ))
            }
            "right" => {
                let [value, count] = args else {
                    return Err(RelError::Unsupported("right arity".into()));
                };
                Ok(df_unicode::right(
                    cast_utf8(self.lower_expr(plan, value)?),
                    self.lower_expr(plan, count)?,
                ))
            }
            "substring" | "substr" => {
                let [value, start, rest @ ..] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                let start = binary(self.lower_expr(plan, start)?, BinaryOp::Add, lit(1_i64));
                let value = cast_utf8(self.lower_expr(plan, value)?);
                match rest {
                    [] => Ok(df_unicode::substr(value, start)),
                    [len] => Ok(df_unicode::substring(
                        value,
                        start,
                        self.lower_expr(plan, len)?,
                    )),
                    _ => Err(RelError::Unsupported(format!("{name} arity"))),
                }
            }
            "gremlin_substring" => {
                let [value, start, rest @ ..] = args else {
                    return Err(RelError::Unsupported("gremlin_substring arity".into()));
                };
                let value = cast_utf8(self.lower_expr(plan, value)?);
                let start_expr = self.lower_expr(plan, start)?;
                let pos = binary(start_expr.clone(), BinaryOp::Add, lit(1_i64));
                match rest {
                    [] => Ok(df_unicode::substr(value, pos)),
                    [end] => {
                        let end = self.lower_expr(plan, end)?;
                        let len = binary(end, BinaryOp::Sub, start_expr);
                        Ok(df_unicode::substring(value, pos, len))
                    }
                    _ => Err(RelError::Unsupported("gremlin_substring arity".into())),
                }
            }
            "length" | "local_length" | "char_length" | "character_length" => {
                let [value] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                // `length(p)` over a path is its hop count. The path renders
                // as text, so without this it would measure that text.
                if let IrExpr::Binding(binding) = value {
                    let hops = path_len_col(binding);
                    if has_exact_col(plan, &hops) {
                        return Ok(col_exact(hops));
                    }
                }
                Ok(df_unicode::length(cast_utf8(self.lower_expr(plan, value)?)))
            }
            "size" => {
                let [value] = args else {
                    return Err(RelError::Unsupported("size arity".into()));
                };
                if let Some(Value::List(items)) = constant_value_expr(value)? {
                    return Ok(lit(items.len() as i64));
                }
                let lowered = self.lower_list_operand(plan, value)?;
                let data_type = lowered.get_type(plan.schema())?;
                match data_type {
                    DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
                        Ok(Expr::Cast(Cast::new(
                            Box::new(datafusion::functions_nested::expr_fn::array_length(lowered)),
                            DataType::Int64,
                        )))
                    }
                    DataType::Map(_, _) => Ok(Expr::Cast(Cast::new(
                        Box::new(datafusion::functions_nested::expr_fn::cardinality(lowered)),
                        DataType::Int64,
                    ))),
                    DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
                        Ok(df_unicode::length(cast_utf8(lowered)))
                    }
                    other => Err(RelError::Unsupported(format!(
                        "size over non-collection type {other}"
                    ))),
                }
            }
            "lpad" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_unicode::lpad(args))
            }
            "rpad" => {
                let args = args
                    .iter()
                    .map(|arg| self.lower_expr(plan, arg))
                    .collect::<RelResult<Vec<_>>>()?;
                Ok(df_unicode::rpad(args))
            }
            "regexp_replace" => {
                let [value, pattern, replacement, rest @ ..] = args else {
                    return Err(RelError::Unsupported("regexp_replace arity".into()));
                };
                let flags = match rest {
                    [] => None,
                    [flags] => Some(cast_utf8(self.lower_expr(plan, flags)?)),
                    _ => return Err(RelError::Unsupported("regexp_replace arity".into())),
                };
                Ok(df_regex::regexp_replace(
                    cast_utf8(self.lower_expr(plan, value)?),
                    cast_utf8(self.lower_expr(plan, pattern)?),
                    cast_utf8(self.lower_expr(plan, replacement)?),
                    flags,
                ))
            }
            "regexp_full_match" | "regexp_matches" | "regexp_like" => {
                let [value, pattern, rest @ ..] = args else {
                    return Err(RelError::Unsupported(format!("{name} arity")));
                };
                let pattern = if normalized == "regexp_full_match" {
                    match constant_value_expr(pattern)? {
                        Some(Value::String(pattern)) => lit(format!("^({pattern})$")),
                        _ => cast_utf8(self.lower_expr(plan, pattern)?),
                    }
                } else {
                    cast_utf8(self.lower_expr(plan, pattern)?)
                };
                let flags = match rest {
                    [] => None,
                    [flags] => Some(cast_utf8(self.lower_expr(plan, flags)?)),
                    _ => return Err(RelError::Unsupported(format!("{name} arity"))),
                };
                Ok(df_regex::regexp_like(
                    cast_utf8(self.lower_expr(plan, value)?),
                    pattern,
                    flags,
                ))
            }
            _ => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
        }
    }

    pub(super) fn lower_core_variadic_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let lowered = args
            .iter()
            .map(|arg| self.lower_expr(plan, arg))
            .collect::<RelResult<Vec<_>>>()?;
        match normalize_function_name(name).as_str() {
            "coalesce" | "ifnull" => Ok(df_core::coalesce(lowered)),
            "greatest" => Ok(df_core::greatest(lowered)),
            "least" => Ok(df_core::least(lowered)),
            "nullif" if lowered.len() == 2 => {
                let value = lowered[0].clone();
                let sentinel = lowered[1].clone();
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(binary(value.clone(), BinaryOp::Eq, sentinel)),
                        Box::new(lit(ScalarValue::Null)),
                    )],
                    Some(Box::new(value)),
                )))
            }
            "constant_or_null" if lowered.len() == 2 => {
                let value = lowered[0].clone();
                let nullable = lowered[1].clone();
                Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(nullable.is_null()),
                        Box::new(lit(ScalarValue::Null)),
                    )],
                    Some(Box::new(value)),
                )))
            }
            _ => Err(RelError::Unsupported(format!(
                "function `{name}` is not relationally lowered yet"
            ))),
        }
    }

    pub(super) fn lower_typeof_matches(
        &self,
        plan: &LogicalPlan,
        target: &IrExpr,
        type_name: &str,
    ) -> RelResult<Expr> {
        let normalized = normalize_type_name(type_name);
        if let IrExpr::Binding(binding) = target
            && let Some(shape) = has_binding_shape(plan, binding)
        {
            let matched = match shape {
                BindingShape::Node => matches!(normalized.as_str(), "vertex" | "node"),
                BindingShape::Edge => matches!(normalized.as_str(), "edge" | "relationship"),
            };
            return Ok(lit(matched));
        }
        let expr = self.lower_expr(plan, target)?;
        let data_type = expr.get_type(plan.schema())?;
        Ok(lit(data_type_matches_gremlin_type(&data_type, &normalized)))
    }
}

pub(super) fn is_label_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("label") || name.eq_ignore_ascii_case("cypher_label")
}

pub(super) fn is_id_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("id") || name.eq_ignore_ascii_case("ID")
}

pub(super) fn is_mod_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("mod")
}

pub(super) fn is_abs_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("abs")
}

pub(super) fn is_pow_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("pow") || name.eq_ignore_ascii_case("power")
}

pub(super) fn normalize_function_name(name: &str) -> String {
    name.to_ascii_lowercase().replace('-', "_")
}

pub(super) fn normalize_temporal_unit(unit: &str) -> String {
    let normalized = unit.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "years" => "year",
        "months" => "month",
        "weeks" => "week",
        "days" => "day",
        "hours" => "hour",
        "minutes" => "minute",
        "seconds" => "second",
        "milliseconds" => "millisecond",
        "microseconds" => "microsecond",
        "nanoseconds" => "nanosecond",
        "quarters" => "quarter",
        "decades" => "decade",
        "centuries" => "century",
        "millennia" | "millenniums" => "millennium",
        _ => normalized.as_str(),
    }
    .to_string()
}

pub(super) fn expression_has_wide_numeric_cast(expr: &IrExpr) -> bool {
    let IrExpr::Call { name, args } = expr else {
        return false;
    };
    cast_target_text(name, args).is_some_and(|target| {
        matches!(
            target
                .trim()
                .trim_matches('"')
                .to_ascii_uppercase()
                .as_str(),
            "INT128" | "UINT128"
        )
    })
}

pub(super) fn is_unary_math_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "acos"
            | "acosh"
            | "asin"
            | "asinh"
            | "atan"
            | "atanh"
            | "cbrt"
            | "ceil"
            | "ceiling"
            | "cos"
            | "cosh"
            | "cot"
            | "degrees"
            | "exp"
            | "factorial"
            | "floor"
            | "ln"
            | "log"
            | "log2"
            | "log10"
            | "radians"
            | "round"
            | "sign"
            | "signum"
            | "sin"
            | "sinh"
            | "sqrt"
            | "tan"
            | "tanh"
            | "trunc"
            | "truncate"
    )
}

pub(super) fn is_binary_math_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "atan2" | "gcd" | "lcm" | "log" | "nanvl" | "round" | "trunc" | "truncate"
    )
}

pub(super) fn is_date_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("date") || name.eq_ignore_ascii_case("to_date")
}

pub(super) fn is_date_constructor(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "date" | "to_date" | "timestamp" | "interval" | "duration"
    )
}

pub(super) fn is_constant_collection_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "array_slice"
            | "array_append"
            | "array_cat"
            | "array_concat"
            | "array_contains"
            | "array_has"
            | "array_indexof"
            | "array_position"
            | "array_prepend"
            | "array_push_back"
            | "array_push_front"
            | "element_at"
            | "list_append"
            | "list_any_value"
            | "list_cat"
            | "list_concat"
            | "list_contains"
            | "list_distinct"
            | "list_element"
            | "list_extract"
            | "list_has_all"
            | "list_has"
            | "list_indexof"
            | "list_join"
            | "list_prepend"
            | "list_position"
            | "list_product"
            | "list_reverse"
            | "list_reverse_sort"
            | "list_slice"
            | "list_sort"
            | "list_sum"
            | "list_to_string"
            | "list_unique"
            | "map_keys"
    )
}

pub(super) fn is_string_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "char_length"
            | "character_length"
            | "concat"
            | "concat_ws"
            | "contains"
            | "ends_with"
            | "endswith"
            | "gremlin_lcase"
            | "gremlin_substring"
            | "gremlin_ucase"
            | "lcase"
            | "left"
            | "length"
            | "local_length"
            | "local_lcase"
            | "local_ltrim"
            | "local_reverse_strings"
            | "local_rtrim"
            | "local_trim"
            | "local_ucase"
            | "lower"
            | "lpad"
            | "ltrim"
            | "prefix"
            | "regexp_full_match"
            | "regexp_like"
            | "regexp_matches"
            | "regexp_replace"
            | "replace"
            | "reverse"
            | "right"
            | "rpad"
            | "rtrim"
            | "size"
            | "starts_with"
            | "startswith"
            | "strcontains"
            | "substr"
            | "substring"
            | "suffix"
            | "tolower"
            | "toupper"
            | "trim"
            | "ucase"
            | "upper"
    )
}

pub(super) fn is_core_variadic_function(name: &str) -> bool {
    matches!(
        normalize_function_name(name).as_str(),
        "coalesce" | "constant_or_null" | "greatest" | "ifnull" | "least" | "nullif"
    )
}

pub(super) fn is_exists_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("exists")
}

pub(super) fn is_in_function(name: &str) -> bool {
    name.eq_ignore_ascii_case("in")
}

pub(super) fn is_cast_function(name: &str, args: &[IrExpr]) -> bool {
    let normalized = name.to_ascii_lowercase();
    (normalized == "cast" && args.len() == 2) || cast_target_from_function_name(&normalized).is_ok()
}

pub(super) fn cast_target_text<'a>(name: &'a str, args: &'a [IrExpr]) -> Option<&'a str> {
    let normalized = name.to_ascii_lowercase();
    if normalized == "cast" {
        let IrExpr::Lit(Lit::String(target)) = args.get(1)? else {
            return None;
        };
        Some(target)
    } else {
        cast_target_from_function_name(&normalized).ok()
    }
}

pub(super) fn cast_target_from_function_name(name: &str) -> RelResult<&'static str> {
    match name {
        "tointeger" => Ok("INT64"),
        "tofloat" => Ok("DOUBLE"),
        "toboolean" => Ok("BOOL"),
        "tostring" => Ok("STRING"),
        "to_bool" | "to_boolean" => Ok("BOOL"),
        "to_string" | "string" | "cast_string" => Ok("STRING"),
        "cast_byte" => Ok("INT8"),
        "cast_short" => Ok("INT16"),
        "to_int8" => Ok("INT8"),
        "to_int16" => Ok("INT16"),
        "to_int32" | "cast_int" | "gremlin_cast_int" => Ok("INT32"),
        "to_int64" | "to_serial" | "cast_long" => Ok("INT64"),
        "to_int128" => Ok("INT128"),
        "to_uint8" => Ok("UINT8"),
        "to_uint16" => Ok("UINT16"),
        "to_uint32" => Ok("UINT32"),
        "to_uint64" => Ok("UINT64"),
        "to_uint128" => Ok("UINT128"),
        "to_float" | "cast_float" => Ok("FLOAT"),
        "to_double" | "cast_double" => Ok("DOUBLE"),
        "cast_bool" | "cast_boolean" => Ok("BOOL"),
        "cast_bigint" => Ok("DECIMAL(38,0)"),
        "cast_bigdecimal" => Ok("DECIMAL(38,6)"),
        _ => Err(RelError::Unsupported(format!(
            "function `{name}` is not relationally lowered yet"
        ))),
    }
}

pub(super) fn data_type_for_cast_target(type_name: &str) -> RelResult<DataType> {
    let normalized = type_name
        .trim()
        .trim_matches('"')
        .to_ascii_uppercase()
        .replace(' ', "");
    if let Some(decimal) = normalized
        .strip_prefix("DECIMAL(")
        .and_then(|value| value.strip_suffix(')'))
    {
        let mut parts = decimal.split(',');
        let precision = parts
            .next()
            .and_then(|value| value.parse::<u8>().ok())
            .ok_or_else(|| {
                RelError::Unsupported(format!("invalid decimal target `{type_name}`"))
            })?;
        let scale = parts
            .next()
            .and_then(|value| value.parse::<i8>().ok())
            .ok_or_else(|| {
                RelError::Unsupported(format!("invalid decimal target `{type_name}`"))
            })?;
        if parts.next().is_some() {
            return Err(RelError::Unsupported(format!(
                "invalid decimal target `{type_name}`"
            )));
        }
        return Ok(DataType::Decimal128(precision, scale));
    }
    match normalized.as_str() {
        "BOOL" | "BOOLEAN" => Ok(DataType::Boolean),
        "INT8" => Ok(DataType::Int8),
        "INT16" => Ok(DataType::Int16),
        "INT32" => Ok(DataType::Int32),
        "INT64" | "SERIAL" => Ok(DataType::Int64),
        "UINT8" => Ok(DataType::UInt8),
        "UINT16" => Ok(DataType::UInt16),
        "UINT32" => Ok(DataType::UInt32),
        "UINT64" => Ok(DataType::UInt64),
        "FLOAT" => Ok(DataType::Float32),
        "DOUBLE" | "FLOAT64" => Ok(DataType::Float64),
        "DECIMAL" => Ok(DataType::Decimal128(18, 3)),
        "STRING" | "VARCHAR" | "UUID" => Ok(DataType::Utf8),
        "DATE" => Ok(DataType::Date32),
        "TIMESTAMP" | "TIMESTAMP_US" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Microsecond,
            None,
        )),
        "TIMESTAMP_NS" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Nanosecond,
            None,
        )),
        "TIMESTAMP_MS" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Millisecond,
            None,
        )),
        "TIMESTAMP_SEC" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Second,
            None,
        )),
        "TIMESTAMP_TZ" => Ok(DataType::Timestamp(
            arrow::datatypes::TimeUnit::Microsecond,
            Some("UTC".into()),
        )),
        // 128-bit integers have no native Arrow representation; a
        // zero-scale decimal covers the numeric range these cases use and
        // prints identically. Values beyond 38 digits fail the cast, which
        // surfaces as an execution error rather than a wrong result.
        "INT128" | "UINT128" => Ok(DataType::Decimal128(38, 0)),
        other => Err(RelError::Unsupported(format!(
            "cast target `{other}` is not relationally lowered yet"
        ))),
    }
}

pub(super) fn normalize_type_name(type_name: &str) -> String {
    type_name
        .trim()
        .trim_start_matches("GType.")
        .trim_start_matches("java.lang.")
        .trim_start_matches("java.math.")
        .to_ascii_lowercase()
}

pub(super) fn data_type_matches_gremlin_type(data_type: &DataType, type_name: &str) -> bool {
    match data_type {
        DataType::Null => type_name == "null",
        DataType::Boolean => matches!(type_name, "boolean" | "bool"),
        DataType::Int8 => type_name == "byte",
        DataType::UInt8 => matches!(type_name, "uint8" | "byte"),
        DataType::Int16 => type_name == "short",
        DataType::UInt16 => type_name == "uint16",
        DataType::Int32 => matches!(type_name, "int" | "integer"),
        DataType::UInt32 => type_name == "uint32",
        DataType::Int64 => matches!(type_name, "long" | "int" | "integer"),
        DataType::UInt64 => type_name == "uint64",
        DataType::Float32 => type_name == "float",
        DataType::Float64 => type_name == "double",
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
            matches!(type_name, "string" | "char" | "character")
        }
        DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
            matches!(type_name, "list" | "set" | "graph")
        }
        _ => false,
    }
}

impl LoweringContext<'_> {
    /// Keep exact integer literals out of floating-point coercion. An integer
    /// not representable as f64 cannot equal any floating-point column value.
    pub(super) fn lower_comparison_or_binary(
        &self,
        plan: &LogicalPlan,
        left: &IrExpr,
        op: BinaryOp,
        right: &IrExpr,
    ) -> RelResult<Expr> {
        let lhs = self.lower_expr(plan, left)?;
        let rhs = self.lower_expr(plan, right)?;
        if matches!(
            op,
            BinaryOp::Eq
                | BinaryOp::Neq
                | BinaryOp::Lt
                | BinaryOp::Lte
                | BinaryOp::Gt
                | BinaryOp::Gte
        ) {
            for (original, other) in [(left, &rhs), (right, &lhs)] {
                let IrExpr::Call { name, args } = original else {
                    continue;
                };
                if name != "integer_literal" || args.len() != 1 {
                    continue;
                }
                let Some(text) = integer_literal_text(&args[0]) else {
                    continue;
                };
                let Ok(integer) = BigInt::from_str(&text.replace('_', "")) else {
                    continue;
                };
                if integer.to_i64().is_some()
                    || !matches!(
                        other.get_type(plan.schema())?,
                        DataType::Float32 | DataType::Float64
                    )
                {
                    continue;
                }
                if !matches!(op, BinaryOp::Eq | BinaryOp::Neq) {
                    return Err(RelError::Unsupported("exact ordering between a wide integer literal and a float requires graph runtime execution".into()));
                }
                let rounded = integer.to_f64();
                let exact =
                    rounded.filter(|value| BigInt::from_f64(*value).as_ref() == Some(&integer));
                if let Some(exact) = exact {
                    return Ok(binary(other.clone(), op, lit(exact)));
                }
                return Ok(Expr::Case(Case::new(
                    None,
                    vec![(
                        Box::new(other.clone().is_null()),
                        Box::new(lit(ScalarValue::Boolean(None))),
                    )],
                    Some(Box::new(lit(op == BinaryOp::Neq))),
                )));
            }
        }
        Ok(binary(lhs, op, rhs))
    }
}

pub(super) fn integer_literal_text(expr: &IrExpr) -> Option<String> {
    match expr {
        IrExpr::Lit(Lit::String(value)) => Some(value.clone()),
        IrExpr::Lit(Lit::Int(value)) => Some(value.to_string()),
        _ => None,
    }
}

pub(super) fn literal_i64(expr: &IrExpr) -> Option<i64> {
    match expr {
        IrExpr::Lit(Lit::Int(value)) => Some(*value),
        IrExpr::Call { name, args } if name == "integer_literal" && args.len() == 1 => {
            integer_literal_text(&args[0]).and_then(|value| value.replace('_', "").parse().ok())
        }
        IrExpr::Call { name, args } if is_cast_function(name, args) => {
            args.first().and_then(literal_i64)
        }
        _ => None,
    }
}

pub(super) fn binary(lhs: Expr, op: BinaryOp, rhs: Expr) -> Expr {
    let op = match op {
        BinaryOp::Eq => Operator::Eq,
        BinaryOp::Neq => Operator::NotEq,
        BinaryOp::Lt => Operator::Lt,
        BinaryOp::Lte => Operator::LtEq,
        BinaryOp::Gt => Operator::Gt,
        BinaryOp::Gte => Operator::GtEq,
        BinaryOp::Add => Operator::Plus,
        BinaryOp::Sub => Operator::Minus,
        BinaryOp::Mul => Operator::Multiply,
        BinaryOp::Div => Operator::Divide,
        BinaryOp::And => Operator::And,
        BinaryOp::Or => Operator::Or,
    };
    Expr::BinaryExpr(BinaryExpr::new(Box::new(lhs), op, Box::new(rhs)))
}

pub(super) fn string_concat(lhs: Expr, rhs: Expr) -> Expr {
    Expr::BinaryExpr(BinaryExpr::new(
        Box::new(lhs),
        Operator::StringConcat,
        Box::new(rhs),
    ))
}

pub(super) fn concat_exprs(mut exprs: Vec<Expr>) -> Expr {
    assert!(!exprs.is_empty(), "concat_exprs requires an expression");
    // Keep concatenations balanced. Element/path rendering can contain dozens
    // of segments, and a left-deep expression makes DataFusion's recursive
    // unparser consume enough stack to abort an otherwise ordinary query.
    while exprs.len() > 1 {
        let mut next = Vec::with_capacity(exprs.len().div_ceil(2));
        let mut pairs = exprs.into_iter();
        while let Some(left) = pairs.next() {
            next.push(match pairs.next() {
                Some(right) => string_concat(left, right),
                None => left,
            });
        }
        exprs = next;
    }
    exprs.pop().expect("non-empty concatenation")
}

pub(super) fn cast_utf8(expr: Expr) -> Expr {
    Expr::Cast(Cast::new(Box::new(expr), DataType::Utf8))
}
