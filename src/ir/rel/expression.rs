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
                if matches!(self.language, Language::Gremlin | Language::Cypher) {
                    return Err(RelError::Unsupported("Graph paths require native runtime values".into()));
                }
                let Some(fallback) = args.get(1) else {
                    return Err(RelError::Unsupported("path_or_self arity".into()));
                };
                self.lower_expr(plan, fallback)
            }
            IrExpr::Call { name, args } if name == "cypher_id" && self.options.mapping.is_some() => {
                if let [IrExpr::Binding(binding)] = args.as_slice() {
                    self.lower_expr(plan, &IrExpr::Id(binding.clone()))
                } else { Err(RelError::Unsupported("Mapped identity requires an element binding".into())) }
            }
            IrExpr::Call { name, args } if name.eq_ignore_ascii_case("range") || name == "cypher_range" => {
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
                for value in [&start, &end] {
                    if !matches!(value.get_type(plan.schema())?,
                        DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64
                        | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 | DataType::Null)
                    {
                        return Err(RelError::Unsupported("Cypher slice bounds require runtime type validation".into()));
                    }
                }
                let null_bound = Expr::or(start.clone().is_null(), end.clone().is_null());
                let sliced = datafusion::functions_nested::expr_fn::array_slice(
                    array,
                    Expr::Cast(Cast::new(Box::new(binary(bound(start), BinaryOp::Add, lit(1_i64))), DataType::Int64)),
                    Expr::Cast(Cast::new(Box::new(bound(end)), DataType::Int64)),
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
                let lhs = self.lower_expr(plan, &args[0])?;
                let rhs = self.lower_expr(plan, &args[1])?;
                let lt = lhs.get_type(plan.schema())?;
                let rt = rhs.get_type(plan.schema())?;
                if lt != rt && !(lt.is_numeric() && rt.is_numeric())
                    && lt != DataType::Null && rt != DataType::Null {
                    return Err(RelError::Unsupported("Cypher comparisons must not coerce unrelated operand types".into()));
                }
                if matches!(lt, DataType::List(_) | DataType::LargeList(_) | DataType::Struct(_))
                    || matches!(rt, DataType::List(_) | DataType::LargeList(_) | DataType::Struct(_)) {
                    return Err(RelError::Unsupported("Cypher compound comparison requires three-valued element semantics".into()));
                }
                let comparison = self.lower_comparison_or_binary(plan, &args[0], op, &args[1])?;
                if lt.is_numeric() && rt.is_numeric() {
                    let mut nan = lit(false);
                    if matches!(lt, DataType::Float32 | DataType::Float64) { nan = nan.or(df_math::isnan(lhs)); }
                    if matches!(rt, DataType::Float32 | DataType::Float64) { nan = nan.or(df_math::isnan(rhs)); }
                    return Ok(Expr::Case(Case::new(None,
                        vec![(Box::new(nan), Box::new(lit(matches!(op,BinaryOp::Neq))))], Some(Box::new(comparison)))));
                }
                Ok(comparison)
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
            Ok(Value::Temporal(_)) => Err(RelError::Unsupported("Typed Cypher temporal value requires native value transport".into())),
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
        let index_expr = self.lower_expr(plan, index)?;
        if !matches!(index_expr.get_type(plan.schema())?,
            DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 |
            DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 | DataType::Null)
        {
            return Err(RelError::Unsupported("Cypher subscript index requires runtime type validation".into()));
        }
        if matches!(data_type, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) {
            return Err(RelError::Unsupported("Cypher strings are not indexable lists".into()));
        }
        let index = Expr::Cast(Cast::new(
            Box::new(index_expr),
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
        if self.language == Language::Cypher && matches!(op, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div) {
            let lt = lhs.get_type(plan.schema())?;
            let rt = rhs.get_type(plan.schema())?;
            if matches!(lt, DataType::Utf8 | DataType::Utf8View | DataType::LargeUtf8 | DataType::List(_) | DataType::LargeList(_))
                || matches!(rt, DataType::Utf8 | DataType::Utf8View | DataType::LargeUtf8 | DataType::List(_) | DataType::LargeList(_)) {
                return Err(RelError::Unsupported("Cypher overloaded arithmetic requires typed value computation".into()));
            }
        }
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
