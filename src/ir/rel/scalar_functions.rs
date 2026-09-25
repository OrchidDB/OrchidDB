//! Relational lowering for math, string, and variadic scalar functions.
use super::*;

impl LoweringContext<'_> {
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

}
