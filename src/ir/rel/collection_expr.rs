//! List and map expression lowering and constant collection operations.

use super::*;

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_collection_expr(&self, plan: &LogicalPlan, items: &[IrExpr]) -> RelResult<Expr> {
        if let Some(value) = constant_value_expr(&IrExpr::List(items.to_vec()))? {
            return Ok(lit(rel_display_value(
                &value,
                self.language,
                literal_collection_context(self.language),
            )));
        }
        let mut pieces = vec![lit("[")];
        for (index, item) in items.iter().enumerate() {
            if index > 0 {
                pieces.push(lit(","));
            }
            let value = self.lower_expr(plan, item)?;
            let data_type = value.get_type(plan.schema())?;
            if matches!(self.language, Language::Cypher | Language::Gql) {
                // The interpreter prints a null list element as empty text
                // (`[NULL]` is `[]`, `[NULL, NULL]` is `[,]`) and floats with
                // six decimals. `||` propagates NULL, so every element is
                // rendered and then defaulted to the empty string.
                let rendered = match data_type {
                    DataType::Boolean => Expr::Case(Case::new(
                        None,
                        vec![
                            (Box::new(value.clone()), Box::new(lit("True"))),
                            (Box::new(Expr::Not(Box::new(value))), Box::new(lit("False"))),
                        ],
                        None,
                    )),
                    DataType::Null => lit(""),
                    _ => render_property_text_expr(value, &data_type),
                };
                pieces.push(df_core::coalesce(vec![rendered, lit("")]));
                continue;
            }
            pieces.push(match data_type {
                DataType::Boolean => Expr::Case(Case::new(
                    None,
                    vec![(Box::new(value.clone()), Box::new(lit("True")))],
                    Some(Box::new(Expr::Case(Case::new(
                        None,
                        vec![(
                            Box::new(value.is_null()),
                            Box::new(lit(ScalarValue::Utf8(None))),
                        )],
                        Some(Box::new(lit("False"))),
                    )))),
                )),
                _ => cast_utf8(value),
            });
        }
        pieces.push(lit("]"));
        Ok(concat_exprs(pieces))
    }

    pub(super) fn lower_constant_collection_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        if let Some(value) = constant_collection_function_value(name, args)? {
            return Ok(constant_result_expr(
                &value,
                self.language,
                literal_collection_context(self.language),
            ));
        }
        if let Some(expr) = self.lower_dynamic_collection_function(plan, name, args)? {
            return Ok(expr);
        }
        Err(RelError::Unsupported(format!(
            "function `{name}` is not relationally lowered yet"
        )))
    }

    pub(super) fn lower_dynamic_collection_function(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Option<Expr>> {
        let normalized = normalize_function_name(name);
        match normalized.as_str() {
            "list_append" | "array_append" | "array_push_back" => {
                let [items, item] = args else {
                    return Ok(None);
                };
                Ok(Some(self.lower_list_insert(plan, items, item, false)?))
            }
            "list_prepend" | "array_prepend" | "array_push_front" => {
                let [items, item] = args else {
                    return Ok(None);
                };
                Ok(Some(self.lower_list_insert(plan, items, item, true)?))
            }
            "list_element" | "list_extract" | "element_at" => {
                let [IrExpr::List(items), index] = args else {
                    return Ok(None);
                };
                let Some(index) = literal_i64(index) else {
                    return Ok(None);
                };
                let Some(item) = list_element_1_based_expr(items, index) else {
                    return Ok(Some(lit(ScalarValue::Utf8(None))));
                };
                Ok(Some(self.lower_expr(plan, item)?))
            }
            "list_unique" => {
                let [items] = args else {
                    return Ok(None);
                };
                let distinct = datafusion::functions_nested::expr_fn::array_distinct(
                    self.lower_native_list(plan, items)?,
                );
                let count = Expr::Cast(Cast::new(
                    Box::new(datafusion::functions_nested::expr_fn::array_length(
                        distinct,
                    )),
                    DataType::Int64,
                ));
                // DataFusion and DuckDB both omit nulls from array_distinct.
                // Cypher's list_unique follows the same rule, so the resulting
                // array length is already the desired count.
                Ok(Some(count))
            }
            "list_contains" | "list_has" | "array_contains" | "array_has" => {
                let [items, needle] = args else {
                    return Ok(None);
                };
                Ok(Some(datafusion::functions_nested::expr_fn::array_has(
                    self.lower_native_list(plan, items)?,
                    self.lower_expr(plan, needle)?,
                )))
            }
            "list_has_all" => {
                let [items, needles] = args else {
                    return Ok(None);
                };
                Ok(Some(datafusion::functions_nested::expr_fn::array_has_all(
                    self.lower_native_list(plan, items)?,
                    self.lower_native_list(plan, needles)?,
                )))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn lower_native_list(&self, plan: &LogicalPlan, expr: &IrExpr) -> RelResult<Expr> {
        if let IrExpr::List(items) = expr {
            return Ok(datafusion::functions_nested::expr_fn::make_array(
                items
                    .iter()
                    .map(|item| self.lower_expr(plan, item))
                    .collect::<RelResult<Vec<_>>>()?,
            ));
        }
        let lowered = self.lower_list_operand(plan, expr)?;
        let data_type = lowered.get_type(plan.schema())?;
        if matches!(
            data_type,
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _)
        ) {
            Ok(lowered)
        } else {
            Err(RelError::Unsupported(format!(
                "list operation over non-list type {data_type}"
            )))
        }
    }

    pub(super) fn lower_list_insert(
        &self,
        plan: &LogicalPlan,
        items: &IrExpr,
        item: &IrExpr,
        prepend: bool,
    ) -> RelResult<Expr> {
        let (items, item) = if matches!(items, IrExpr::List(_)) {
            (
                self.lower_native_list(plan, items)?,
                self.lower_expr(plan, item)?,
            )
        } else {
            collections::lower_list_insert_operands(self, plan, items, item)?
        };
        let items_type = items.get_type(plan.schema())?;
        let item_type = item.get_type(plan.schema())?;
        match items_type {
            DataType::List(_) | DataType::LargeList(_) | DataType::FixedSizeList(_, _) => {
                if prepend {
                    Ok(datafusion::functions_nested::expr_fn::array_prepend(
                        item, items,
                    ))
                } else {
                    Ok(datafusion::functions_nested::expr_fn::array_append(
                        items, item,
                    ))
                }
            }
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
                let items = cast_utf8(items);
                let rendered_item = render_property_text_expr(item, &item_type);
                let empty = binary(items.clone(), BinaryOp::Eq, lit("[]"));
                if prepend {
                    let suffix = Expr::Case(Case::new(
                        None,
                        vec![(Box::new(empty), Box::new(lit("]")))],
                        Some(Box::new(concat_exprs(vec![
                            lit(","),
                            df_unicode::substring(
                                items.clone(),
                                lit(2_i64),
                                binary(df_unicode::length(items), BinaryOp::Sub, lit(1_i64)),
                            ),
                        ]))),
                    ));
                    Ok(concat_exprs(vec![lit("["), rendered_item, suffix]))
                } else {
                    let prefix = Expr::Case(Case::new(
                        None,
                        vec![(Box::new(empty), Box::new(lit("[")))],
                        Some(Box::new(concat_exprs(vec![
                            df_unicode::substring(
                                items.clone(),
                                lit(1_i64),
                                binary(df_unicode::length(items), BinaryOp::Sub, lit(1_i64)),
                            ),
                            lit(","),
                        ]))),
                    ));
                    Ok(concat_exprs(vec![prefix, rendered_item, lit("]")]))
                }
            }
            other => Err(RelError::Unsupported(format!(
                "list operation over non-list type {other}"
            ))),
        }
    }

}

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_select_key_or_binding(
        &self,
        plan: &LogicalPlan,
        binding_arg: &IrExpr,
    ) -> RelResult<Expr> {
        let IrExpr::Binding(binding) = binding_arg else {
            return Err(RelError::Unsupported(
                "dynamic Gremlin select binding".into(),
            ));
        };
        if has_exact_col(plan, binding) {
            Ok(col_exact(binding))
        } else if has_binding_shape(plan, binding).is_some() {
            gremlin_element_display_expr(plan, binding)
        } else {
            Err(RelError::Unsupported(format!(
                "Gremlin select binding `{binding}` is not available relationally"
            )))
        }
    }

    /// Gremlin `valueMap()` over an element binding: renders the tagged
    /// map text (`m[{"age":"[29]","name":"[marko]"}]`) that the harness
    /// comparator normalizes identically to the interpreter's output.
    pub(super) fn lower_value_map(&self, plan: &LogicalPlan, name: &str, args: &[IrExpr]) -> RelResult<Expr> {
        if self.language == Language::Gremlin {
            return Err(RelError::Unsupported("Gremlin map requires native runtime values".into()));
        }

        let IrExpr::Binding(binding) = &args[0] else {
            return Err(RelError::Unsupported(
                "value_map over a non-binding target".into(),
            ));
        };
        let Some(shape) = has_binding_shape(plan, binding) else {
            return Err(RelError::Unsupported(format!(
                "value_map target `{binding}` is not an element binding"
            )));
        };
        let requested = match args.get(1) {
            Some(IrExpr::List(items)) if !items.is_empty() => Some(
                items
                    .iter()
                    .filter_map(|item| match item {
                        IrExpr::Lit(Lit::String(key)) => Some(key.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>(),
            ),
            _ => None,
        };
        let keys = match requested {
            Some(keys) => keys
                .into_iter()
                .filter(|key| has_exact_col(plan, &prop_col(binding, key)))
                .collect(),
            None => self.element_property_keys(plan, binding, shape),
        };
        let mut body: Vec<Expr> = Vec::new();
        if name == "value_map_tokens" {
            let literal_bool = |arg: Option<&IrExpr>, default: bool| match arg {
                Some(IrExpr::Lit(Lit::Bool(value))) => *value,
                _ => default,
            };
            if shape != BindingShape::Node {
                return Err(RelError::Unsupported(
                    "value_map_tokens over a non-node binding".into(),
                ));
            }
            if literal_bool(args.get(2), true) {
                let display = match has_exact_col(plan, &prop_col(binding, "name")) {
                    true => col_exact(prop_col(binding, "name")),
                    false => concat_exprs(vec![
                        col_exact(label_col(binding)),
                        lit("#"),
                        cast_utf8(col_exact(id_col(binding))),
                    ]),
                };
                body.push(concat_exprs(vec![
                    lit(",\"t[id]\":\"v["),
                    display,
                    lit("].id\""),
                ]));
            }
            if literal_bool(args.get(3), true) {
                body.push(concat_exprs(vec![
                    lit(",\"t[label]\":\""),
                    col_exact(label_col(binding)),
                    lit("\""),
                ]));
            }
        }
        for key in keys {
            let name = prop_col(binding, &key);
            let Some(data_type) = plan_column_type(plan, &name) else {
                continue;
            };
            let column = col_exact(&name);
            let rendered = match data_type {
                DataType::Boolean => Expr::Case(Case::new(
                    None,
                    vec![(Box::new(column.clone()), Box::new(lit("true")))],
                    Some(Box::new(lit("false"))),
                )),
                DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => column.clone(),
                _ => cast_utf8(column.clone()),
            };
            let entry = concat_exprs(vec![lit(format!(",\"{key}\":\"[")), rendered, lit("]\"")]);
            body.push(Expr::Case(Case::new(
                None,
                vec![(Box::new(column.is_null()), Box::new(lit("")))],
                Some(Box::new(entry)),
            )));
        }
        if body.is_empty() {
            return Ok(lit("m[{}]"));
        }
        let entries = cast_utf8(df_unicode::substr(concat_exprs(body), lit(2_i64)));
        Ok(concat_exprs(vec![lit("m[{"), entries, lit("}]")]))
    }

    pub(super) fn lower_cypher_map(&self, plan: &LogicalPlan, args: &[IrExpr]) -> RelResult<Expr> {
        if let Some(value) = constant_cypher_map(args)? {
            return Ok(lit(rel_display_value(
                &value,
                self.language,
                DisplayContext::Tagged,
            )));
        }
        if let [IrExpr::List(keys), IrExpr::List(values)] = args {
            if keys.len() != values.len() {
                return Err(RelError::Unsupported(
                    "map key/value length mismatch".into(),
                ));
            }
            if keys
                .iter()
                .enumerate()
                .any(|(index, key)| keys[..index].contains(key))
            {
                // The duplicate-key error includes the row-dependent key.
                // Let the interpreter produce that exact public error until
                // SQL error expressions are part of the result boundary.
                return Err(RelError::Unsupported("dynamic duplicate map key".into()));
            }
            let mut pieces = vec![lit("{")];
            for (index, (key, value)) in keys.iter().zip(values).enumerate() {
                if index > 0 {
                    pieces.push(lit(", "));
                }
                pieces.push(cast_utf8(self.lower_expr(plan, key)?));
                pieces.push(lit("="));
                pieces.push(cast_utf8(self.lower_expr(plan, value)?));
            }
            pieces.push(lit("}"));
            return Ok(concat_exprs(pieces));
        }
        if args.len() % 2 != 0 {
            return Err(RelError::Unsupported("map arity".into()));
        }
        let mut pieces = Vec::new();
        pieces.push(lit("m[{"));
        for (idx, pair) in args.chunks(2).enumerate() {
            let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
                return Err(RelError::Unsupported("dynamic map key".into()));
            };
            if idx > 0 {
                pieces.push(lit(","));
            }
            pieces.push(lit(format!("\"{}\":\"", escape_debug_string(key))));
            pieces.push(cast_utf8(self.lower_expr(plan, &pair[1])?));
            pieces.push(lit("\""));
        }
        pieces.push(lit("}]"));
        Ok(concat_exprs(pieces))
    }

    pub(super) fn lower_make_map(&self, plan: &LogicalPlan, args: &[IrExpr]) -> RelResult<Expr> {
        if self.language == Language::Gremlin {
            return Err(RelError::Unsupported("Gremlin map requires native runtime values".into()));
        }

        if args.len() % 2 != 0 {
            return Err(RelError::Unsupported("make_map arity".into()));
        }
        let mut pieces = Vec::new();
        pieces.push(lit("Map({"));
        for (idx, pair) in args.chunks(2).enumerate() {
            let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
                return Err(RelError::Unsupported("dynamic make_map key".into()));
            };
            if idx > 0 {
                pieces.push(lit(", "));
            }
            pieces.push(lit(format!("\"{}\": String(\"", escape_debug_string(key))));
            pieces.push(cast_utf8(self.lower_expr(plan, &pair[1])?));
            pieces.push(lit("\")"));
        }
        pieces.push(lit("})"));
        Ok(concat_exprs(pieces))
    }

}

pub(super) fn constant_values(args: &[IrExpr]) -> RelResult<Option<Vec<Value>>> {
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        let Some(value) = constant_value_expr(arg)? else {
            return Ok(None);
        };
        values.push(value);
    }
    Ok(Some(values))
}

pub(super) fn constant_temporal_value(name: &str, args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [arg] = args else {
        return Ok(None);
    };
    let Some(value) = constant_value_expr(arg)? else {
        return Ok(None);
    };
    let normalized = normalize_function_name(name);
    match (normalized.as_str(), value) {
        ("date" | "to_date" | "timestamp", Value::String(value) | Value::DateTime(value)) => {
            Ok(Some(Value::DateTime(value)))
        }
        ("interval" | "duration", Value::String(value)) => Ok(Some(Value::String(value))),
        (_, Value::Null) => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

pub(super) fn constant_collection_function_value(name: &str, args: &[IrExpr]) -> RelResult<Option<Value>> {
    let normalized = normalize_function_name(name);
    match normalized.as_str() {
        "array_slice" | "list_slice" => constant_array_slice(args),
        "array_append" | "array_push_back" => constant_list_append(args, false),
        "array_prepend" | "array_push_front" => constant_list_append(args, true),
        "array_indexof" | "array_position" | "list_indexof" | "list_position" => {
            constant_list_position(args)
        }
        "array_contains" | "array_has" | "list_contains" | "list_has" => {
            constant_list_contains(args)
        }
        "element_at" | "list_element" | "list_extract" => constant_list_extract(args),
        "list_any_value" => constant_list_any_value(args),
        "list_distinct" => constant_list_distinct(args),
        "list_has_all" => constant_list_has_all(args),
        "list_product" => constant_list_product(args),
        "list_reverse" => constant_list_reverse(args),
        "list_sort" => constant_list_sort(args, false),
        "list_reverse_sort" => constant_list_sort(args, true),
        "list_sum" => constant_list_sum(args),
        "list_to_string" | "list_join" => constant_list_to_string(args),
        "list_unique" => constant_list_unique(args),
        "list_append" => constant_list_append(args, false),
        "list_prepend" => constant_list_append(args, true),
        "list_cat" | "list_concat" | "array_cat" | "array_concat" => constant_list_concat(args),
        "map_keys" => constant_map_keys(args),
        _ => Ok(None),
    }
}

pub(super) fn constant_array_slice(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [target, start, end] = args else {
        return Ok(None);
    };
    let Some(target) = constant_value_expr(target)? else {
        return Ok(None);
    };
    let Some(start) = constant_value_expr(start)? else {
        return Ok(None);
    };
    let Some(end) = constant_value_expr(end)? else {
        return Ok(None);
    };
    Ok(Some(match target {
        Value::List(items) | Value::Path(items) => {
            Value::List(list_slice_range(&items, &start, &end))
        }
        Value::String(value) => Value::String(string_slice_range(&value, &start, &end)),
        Value::Null => Value::Null,
        _ => return Ok(None),
    }))
}

pub(super) fn constant_list_extract(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [target, index] = args else {
        return Ok(None);
    };
    let Some(target) = constant_value_expr(target)? else {
        return Ok(None);
    };
    let Some(index) = constant_value_expr(index)? else {
        return Ok(None);
    };
    let Some(index) = index.as_i64() else {
        return Ok(Some(Value::Null));
    };
    Ok(Some(match target {
        Value::List(items) | Value::Path(items) => list_element_1_based(&items, index),
        Value::String(value) => string_index_1_based(&value, index),
        Value::Null => Value::Null,
        _ => return Ok(None),
    }))
}

pub(super) fn constant_list_position(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items, needle] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let Some(needle) = constant_value_expr(needle)? else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    if matches!(needle, Value::Null) {
        return Ok(Some(Value::Null));
    }
    for (idx, item) in items.iter().enumerate() {
        if list_semantic_eq(item, &needle) {
            return Ok(Some(Value::Long((idx + 1) as i64)));
        }
    }
    Ok(Some(Value::Long(0)))
}

pub(super) fn constant_list_contains(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items, needle] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let Some(needle) = constant_value_expr(needle)? else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    if matches!(needle, Value::Null) {
        return Ok(Some(Value::Null));
    }
    Ok(Some(Value::Bool(
        items.iter().any(|item| list_semantic_eq(item, &needle)),
    )))
}

pub(super) fn constant_list_distinct(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    match items {
        Value::List(items) | Value::Path(items) => {
            Ok(Some(Value::List(list_distinct_values(&items, false))))
        }
        Value::Null => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

pub(super) fn constant_list_unique(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    match items {
        Value::List(items) | Value::Path(items) => Ok(Some(Value::Int(
            list_distinct_values(&items, false).len() as i64,
        ))),
        Value::Null => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

pub(super) fn constant_list_any_value(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    match items {
        Value::List(items) | Value::Path(items) => Ok(Some(
            items
                .into_iter()
                .find(|item| !matches!(item, Value::Null))
                .unwrap_or(Value::Null),
        )),
        Value::Null => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

pub(super) fn constant_list_has_all(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [haystack, needles] = args else {
        return Ok(None);
    };
    let Some(haystack) = constant_value_expr(haystack)? else {
        return Ok(None);
    };
    let Some(needles) = constant_value_expr(needles)? else {
        return Ok(None);
    };
    let haystack = match haystack {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    let needles = match needles {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    for needle in &needles {
        if matches!(needle, Value::Null) {
            continue;
        }
        if !haystack.iter().any(|item| list_semantic_eq(item, needle)) {
            return Ok(Some(Value::Bool(false)));
        }
    }
    Ok(Some(Value::Bool(true)))
}

pub(super) fn constant_list_reverse(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    match items {
        Value::List(mut items) | Value::Path(mut items) => {
            items.reverse();
            Ok(Some(Value::List(items)))
        }
        Value::Null => Ok(Some(Value::Null)),
        _ => Ok(None),
    }
}

pub(super) fn constant_list_sum(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    let mut sum = 0.0;
    let mut int_only = true;
    for item in &items {
        if matches!(item, Value::Null) {
            continue;
        }
        let Some(value) = value_to_f64(item) else {
            return Ok(Some(Value::String(format!(
                "Binder exception: Unsupported inner data type for LIST_SUM: {}",
                item.type_name().to_ascii_uppercase()
            ))));
        };
        if matches!(item, Value::Float(_) | Value::Float32(_)) {
            int_only = false;
        }
        sum += value;
    }
    Ok(Some(if int_only {
        Value::Long(sum as i64)
    } else {
        Value::Float(sum)
    }))
}

pub(super) fn constant_list_product(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [items] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    if items
        .iter()
        .filter(|item| !matches!(item, Value::Null))
        .any(|item| value_to_bigint(item).is_none() && value_to_f64(item).is_none())
    {
        return Ok(Some(Value::String(
            "Binder exception: Unsupported inner data type for LIST_PRODUCT: STRING".to_string(),
        )));
    }
    if items.iter().any(|item| matches!(item, Value::Float(_))) {
        return Ok(Some(Value::Float(
            items
                .iter()
                .filter_map(value_to_f64)
                .fold(1.0, |product, value| product * value),
        )));
    }
    if items.iter().any(|item| matches!(item, Value::Float32(_))) {
        let product = items
            .iter()
            .filter_map(value_to_f64)
            .fold(1.0_f32, |product, value| product * value as f32);
        return Ok(Some(Value::Float32(product)));
    }
    let product = items
        .iter()
        .filter_map(value_to_bigint)
        .fold(BigInt::from(1), |product, value| product * value);
    Ok(Some(
        product
            .to_i64()
            .map(Value::Long)
            .unwrap_or(Value::BigInt(product)),
    ))
}

pub(super) fn constant_list_to_string(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [first, second] = args else {
        return Ok(None);
    };
    let Some(first) = constant_value_expr(first)? else {
        return Ok(None);
    };
    let Some(second) = constant_value_expr(second)? else {
        return Ok(None);
    };
    let (items, delimiter) = match (&first, &second) {
        (Value::List(items) | Value::Path(items), Value::String(delimiter)) => {
            (items.clone(), delimiter.clone())
        }
        (Value::String(delimiter), Value::List(items) | Value::Path(items)) => {
            (items.clone(), delimiter.clone())
        }
        (Value::Null, _) | (_, Value::Null) => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    let parts = items
        .iter()
        .filter(|item| !matches!(item, Value::Null))
        .map(display_for_list_to_string)
        .collect::<Vec<_>>();
    Ok(Some(Value::String(parts.join(&delimiter))))
}

pub(super) fn constant_list_sort(args: &[IrExpr], reverse_default: bool) -> RelResult<Option<Value>> {
    let Some(values) = constant_values(args)? else {
        return Ok(None);
    };
    let [items, rest @ ..] = values.as_slice() else {
        return Ok(None);
    };
    let items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    let (descending, nulls_last) = if reverse_default {
        match rest {
            [] => (true, false),
            [Value::String(nulls)] => (true, nulls.eq_ignore_ascii_case("NULLS LAST")),
            _ => return Ok(None),
        }
    } else {
        match rest {
            [] => (false, false),
            [Value::String(dir)] => (dir.eq_ignore_ascii_case("DESC"), false),
            [Value::String(dir), Value::String(nulls)] => (
                dir.eq_ignore_ascii_case("DESC"),
                nulls.eq_ignore_ascii_case("NULLS LAST"),
            ),
            _ => return Ok(None),
        }
    };
    Ok(Some(Value::List(sort_list_values(
        items, descending, nulls_last,
    ))))
}


pub(super) fn constant_list_append(args: &[IrExpr], prepend: bool) -> RelResult<Option<Value>> {
    let [items, item] = args else {
        return Ok(None);
    };
    let Some(items) = constant_value_expr(items)? else {
        return Ok(None);
    };
    let Some(item) = constant_value_expr(item)? else {
        return Ok(None);
    };
    let mut items = match items {
        Value::List(items) | Value::Path(items) => items,
        Value::Null => return Ok(Some(Value::Null)),
        _ => return Ok(None),
    };
    if prepend {
        items.insert(0, item);
    } else {
        items.push(item);
    }
    Ok(Some(Value::List(items)))
}

pub(super) fn constant_list_concat(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [left, right] = args else {
        return Ok(None);
    };
    let Some(left) = constant_value_expr(left)? else {
        return Ok(None);
    };
    let Some(right) = constant_value_expr(right)? else {
        return Ok(None);
    };
    match (left, right) {
        (Value::Null, _) | (_, Value::Null) => Ok(Some(Value::Null)),
        (Value::List(mut left), Value::List(right))
        | (Value::List(mut left), Value::Path(right))
        | (Value::Path(mut left), Value::List(right))
        | (Value::Path(mut left), Value::Path(right)) => {
            left.extend(right);
            Ok(Some(Value::List(left)))
        }
        _ => Ok(None),
    }
}

pub(super) fn constant_map_keys(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let [target] = args else {
        return Ok(None);
    };
    let Some(target) = constant_value_expr(target)? else {
        return Ok(None);
    };
    Ok(Some(match target {
        Value::Map(map) => Value::List(
            visible_map_keys(&map)
                .into_iter()
                .map(Value::String)
                .collect(),
        ),
        Value::Null => Value::Null,
        _ => Value::List(Vec::new()),
    }))
}

pub(super) fn constant_unwind_values(expr: &IrExpr, outer: bool) -> RelResult<Option<Vec<Value>>> {
    let mut values = match expr {
        IrExpr::Lit(Lit::Null) => Vec::new(),
        IrExpr::Lit(value) => vec![lit_to_value(value)],
        IrExpr::List(items) => {
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let Some(value) = constant_value_expr(item)? else {
                    return Ok(None);
                };
                values.push(value);
            }
            values
        }
        IrExpr::Call { name, args } if name.eq_ignore_ascii_case("range") || name == "cypher_range" => {
            constant_range_values(args)?
        }
        _ => return Ok(None),
    };
    if values.is_empty() && outer {
        values.push(Value::Null);
    }
    Ok(Some(values))
}

pub(super) fn constant_range_values(args: &[IrExpr]) -> RelResult<Vec<Value>> {
    let ([start, stop] | [start, stop, _]) = args else {
        return Err(RelError::Unsupported("range arity".into()));
    };
    let start = literal_i64(start)
        .ok_or_else(|| RelError::Unsupported("range start must be a literal integer".into()))?;
    let stop = literal_i64(stop)
        .ok_or_else(|| RelError::Unsupported("range stop must be a literal integer".into()))?;
    let step = if let Some(step) = args.get(2) {
        literal_i64(step)
            .ok_or_else(|| RelError::Unsupported("range step must be a literal integer".into()))?
    } else {
        1
    };
    if step == 0 {
        return Err(RelError::Unsupported("range step cannot be zero".into()));
    }
    let mut values = Vec::new();
    let mut current = start;
    while (step > 0 && current <= stop) || (step < 0 && current >= stop) {
        if values.len() > 100_000 {
            return Err(RelError::Unsupported(
                "range literal is too large for eager relational expansion".into(),
            ));
        }
        values.push(Value::Int(current));
        current = match current.checked_add(step) {
            Some(next) => next,
            None => break,
        };
    }
    Ok(values)
}

pub(super) fn sort_list_values(items: &[Value], descending: bool, nulls_last: bool) -> Vec<Value> {
    let null_count = items
        .iter()
        .filter(|item| matches!(item, Value::Null))
        .count();
    let mut sorted = items
        .iter()
        .filter(|item| !matches!(item, Value::Null))
        .cloned()
        .collect::<Vec<_>>();
    sorted.sort_by(compare_values);
    if descending {
        sorted.reverse();
    }

    let nulls = std::iter::repeat(Value::Null).take(null_count);
    if nulls_last {
        sorted.extend(nulls);
        sorted
    } else {
        nulls.chain(sorted).collect()
    }
}

pub(super) fn slice_bounds(len: usize, start: &Value, end: &Value) -> (usize, usize) {
    let len_i = len as i64;
    let resolve_start = |value: &Value| -> i64 {
        match value {
            Value::Null => 0,
            _ => match value.as_i64() {
                Some(value) if value < 0 => len_i + value,
                Some(value) => value - 1,
                None => 0,
            },
        }
    };
    let resolve_end = |value: &Value| -> i64 {
        match value {
            Value::Null => len_i,
            _ => match value.as_i64() {
                Some(value) if value < 0 => len_i + value + 1,
                Some(value) => value,
                None => len_i,
            },
        }
    };
    let start = resolve_start(start).clamp(0, len_i) as usize;
    let end = resolve_end(end).clamp(0, len_i) as usize;
    (start.min(end), end)
}

pub(super) fn list_slice_range(items: &[Value], start: &Value, end: &Value) -> Vec<Value> {
    let (start, end) = slice_bounds(items.len(), start, end);
    items[start..end].to_vec()
}

pub(super) fn list_element_1_based(items: &[Value], index: i64) -> Value {
    if index == 0 {
        return Value::Null;
    }
    let zero_based = if index < 0 {
        items.len() as i64 + index
    } else {
        index - 1
    };
    if zero_based < 0 || zero_based >= items.len() as i64 {
        Value::Null
    } else {
        items[zero_based as usize].clone()
    }
}

pub(super) fn list_element_1_based_expr(items: &[IrExpr], index: i64) -> Option<&IrExpr> {
    if index == 0 {
        return None;
    }
    let zero_based = if index < 0 {
        items.len() as i64 + index
    } else {
        index - 1
    };
    if zero_based < 0 || zero_based >= items.len() as i64 {
        None
    } else {
        items.get(zero_based as usize)
    }
}

pub(super) fn string_index_1_based(text: &str, index: i64) -> Value {
    if index == 0 {
        return Value::Null;
    }
    let chars = text.chars().collect::<Vec<_>>();
    if chars.is_empty() {
        return Value::Null;
    }
    let zero_based = if index < 0 {
        chars.len() as i64 + index
    } else {
        index - 1
    };
    if zero_based < 0 || zero_based >= chars.len() as i64 {
        Value::Null
    } else {
        Value::String(chars[zero_based as usize].to_string())
    }
}

pub(super) fn string_slice_range(text: &str, start: &Value, end: &Value) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let (start, end) = slice_bounds(chars.len(), start, end);
    chars[start..end].iter().collect()
}

pub(super) fn list_distinct_values(items: &[Value], include_null: bool) -> Vec<Value> {
    let mut seen = Vec::new();
    for item in items {
        if !include_null && matches!(item, Value::Null) {
            continue;
        }
        if !seen.iter().any(|seen| list_semantic_eq(seen, item)) {
            seen.push(item.clone());
        }
    }
    seen
}

pub(super) fn list_semantic_eq(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Null, _) | (_, Value::Null) => false,
        (Value::List(left), Value::List(right)) | (Value::Path(left), Value::Path(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right.iter())
                    .all(|(left, right)| list_semantic_eq(left, right))
        }
        (Value::Map(left), Value::Map(right)) => {
            visible_map_keys(left).len() == visible_map_keys(right).len()
                && visible_map_keys(left).into_iter().all(|key| {
                    let Some(value) = left.get(&key) else {
                        return false;
                    };
                    right
                        .get(&key)
                        .is_some_and(|right_value| list_semantic_eq(value, right_value))
                })
        }
        _ => left.three_valued_eq(right) == Some(true),
    }
}

pub(super) fn constant_cypher_map(args: &[IrExpr]) -> RelResult<Option<Value>> {
    let mut map = BTreeMap::new();
    if args.len() == 2
        && let (IrExpr::List(keys), IrExpr::List(values)) = (&args[0], &args[1])
    {
        if keys.len() != values.len() {
            return Err(RelError::Unsupported(
                "map key/value length mismatch".into(),
            ));
        }
        for (key, value) in keys.iter().zip(values.iter()) {
            let Some(key) = constant_value_expr(key)? else {
                return Ok(None);
            };
            let Some(value) = constant_value_expr(value)? else {
                return Ok(None);
            };
            let key = cypher_plain_value(&key);
            if map.insert(key.clone(), value).is_some() {
                return Err(RelError::Unsupported(format!(
                    "Runtime exception: Found duplicate key: {key} in map."
                )));
            }
        }
        return Ok(Some(Value::Map(map)));
    }
    if args.len() % 2 != 0 {
        return Ok(None);
    }
    for pair in args.chunks(2) {
        let IrExpr::Lit(Lit::String(key)) = &pair[0] else {
            return Ok(None);
        };
        let Some(value) = constant_value_expr(&pair[1])? else {
            return Ok(None);
        };
        map.insert(key.clone(), value);
    }
    Ok(Some(Value::Map(map)))
}
