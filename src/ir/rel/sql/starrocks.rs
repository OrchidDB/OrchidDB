use super::{DialectAdapter, SqlDialect, SqlError, SqlResult};
use arrow::{array::Array, datatypes::DataType};
use datafusion::common::ScalarValue;
use datafusion::sql::{
    sqlparser::{ast, dialect::Dialect as ParserDialect},
    unparser::dialect::{
        CustomDialectBuilder, DateFieldExtractStyle, Dialect as UnparserDialect, IntervalStyle,
    },
};

#[derive(Debug)]
pub struct StarRocksAdapter;
pub static STARROCKS: StarRocksAdapter = StarRocksAdapter;

impl DialectAdapter for StarRocksAdapter {
    fn name(&self) -> &'static str {
        "starrocks"
    }
    fn parser_dialect(&self) -> Box<dyn ParserDialect> {
        Box::new(datafusion::sql::sqlparser::dialect::GenericDialect {})
    }
    fn unparser_dialect(&self) -> Box<dyn UnparserDialect> {
        Box::new(StarRocksUnparser)
    }
    fn requires_scope_repair(&self) -> bool {
        true
    }
    fn double_type(&self) -> &'static str {
        "DOUBLE"
    }
    fn sql_type(&self, ty: &DataType) -> SqlResult<String> {
        Ok(match ty {
            DataType::Null => "BOOLEAN".into(),
            DataType::Boolean => "BOOLEAN".into(),
            DataType::Int8 => "TINYINT".into(),
            DataType::Int16 | DataType::UInt8 => "SMALLINT".into(),
            DataType::Int32 | DataType::UInt16 => "INT".into(),
            DataType::Int64 | DataType::UInt32 => "BIGINT".into(),
            DataType::UInt64 => "LARGEINT".into(),
            DataType::Float32 => "FLOAT".into(),
            DataType::Float64 => "DOUBLE".into(),
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "VARCHAR(1048576)".into(),
            DataType::Binary | DataType::LargeBinary | DataType::BinaryView => "VARBINARY".into(),
            DataType::Date32 => "DATE".into(),
            DataType::Timestamp(unit, None)
                if !matches!(unit, arrow::datatypes::TimeUnit::Nanosecond) =>
            {
                "DATETIME".into()
            }
            DataType::Decimal128(p, s) if *p <= 38 && *s >= 0 && (*s as u8) <= *p => {
                format!("DECIMAL({p},{s})")
            }
            DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
                format!("ARRAY<{}>", self.sql_type(f.data_type())?)
            }
            _ => return Err(SqlError::Unsupported(format!("starrocks type {ty}"))),
        })
    }
    fn exchange_literal(&self, value: &ScalarValue, ty: &DataType) -> SqlResult<String> {
        let target = self.sql_type(ty)?;
        let literal = if value.is_null() {
            "NULL".into()
        } else {
            match value {
                ScalarValue::Utf8(Some(s))
                | ScalarValue::LargeUtf8(Some(s))
                | ScalarValue::Utf8View(Some(s)) => string(s),
                ScalarValue::Binary(Some(b))
                | ScalarValue::LargeBinary(Some(b))
                | ScalarValue::BinaryView(Some(b)) => format!("unhex('{}')", hex(b)),
                ScalarValue::List(a) => self.array_literal(a.value(0))?,
                ScalarValue::LargeList(a) => self.array_literal(a.value(0))?,
                ScalarValue::FixedSizeList(a) => self.array_literal(a.value(0))?,
                ScalarValue::Float32(Some(v)) if !v.is_finite() => {
                    return Err(SqlError::Unsupported(
                        "starrocks non-finite float literal".into(),
                    ));
                }
                ScalarValue::Float64(Some(v)) if !v.is_finite() => {
                    return Err(SqlError::Unsupported(
                        "starrocks non-finite float literal".into(),
                    ));
                }
                ScalarValue::Float32(Some(v)) => string(&v.to_string()),
                ScalarValue::Float64(Some(v)) => string(&v.to_string()),
                ScalarValue::Decimal128(..)
                | ScalarValue::Date32(_)
                | ScalarValue::TimestampSecond(_, None)
                | ScalarValue::TimestampMillisecond(_, None)
                | ScalarValue::TimestampMicrosecond(_, None) => {
                    let array = value.to_array()?;
                    string(&arrow::util::display::array_value_to_string(
                        array.as_ref(),
                        0,
                    )?)
                }
                _ => super::literals::sql_literal(SqlDialect::DuckDb, value)?,
            }
        };
        Ok(format!("CAST({literal} AS {target})"))
    }
    fn lower_relation(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
        _: &super::lowering::LoweringContext,
    ) -> SqlResult<Option<super::lowering::RelationLowering>> {
        use datafusion::{
            common::tree_node::{Transformed, TreeNode},
            logical_expr::{Expr, ExprSchemable, Volatility},
        };
        if let datafusion::logical_expr::LogicalPlan::Window(window) = plan {
            let ordering = window.input.schema().columns().into_iter().zip(window.input.schema().fields()).filter_map(|(column, field)| {
                let ty = field.data_type();
                (ty.is_numeric() || matches!(ty, DataType::Boolean | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View | DataType::Date32 | DataType::Date64 | DataType::Timestamp(_, _))).then(|| datafusion::logical_expr::expr::Sort::new(Expr::Column(column), true, true))
            }).collect::<Vec<_>>();
            if !ordering.is_empty() {
                let mut window = window.clone();
                let mut changed = false;
                for expression in &mut window.window_expr {
                    if let Expr::Alias(alias) = expression {
                        if alias.name.starts_with("__apply_corr_key_row") {
                            if let Expr::WindowFunction(function) = alias.expr.as_mut() {
                                if function.fun.name() == "row_number" && function.params.order_by.is_empty() {
                                    function.params.order_by = ordering.clone();
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                if changed {
                    return Ok(Some(super::lowering::RelationLowering::Rewrite(datafusion::logical_expr::LogicalPlan::Window(window))));
                }
            }
        }
        let mut schema = datafusion::common::DFSchema::empty();
        for input in plan.inputs() {
            schema.merge(input.schema());
        }
        schema.merge(plan.schema());
        let names = datafusion::logical_expr::expr_rewriter::NamePreserver::new(plan);
        let mut changed = false;
        let result = plan.clone().map_expressions(|expr| {
            let saved = names.save(&expr);
            expr.transform_up(|expr| {
                if let Expr::TryCast(cast) = &expr {
                    if let DataType::Decimal128(precision, scale) = cast.data_type {
                        let source = cast.expr.get_type(&schema)?;
                        if matches!(source, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) {
                            let udf = datafusion::logical_expr::create_udf("__orchiddb_starrocks_decimal_from_text", vec![source], cast.data_type.clone(), Volatility::Immutable,
                                std::sync::Arc::new(move |args| {
                                    let arrays = datafusion::logical_expr::ColumnarValue::values_to_arrays(args)?;
                                    let values = (0..arrays[0].len()).map(|index| {
                                        let value = ScalarValue::try_from_array(&arrays[0], index)?;
                                        let text = match &value {
                                            ScalarValue::Utf8(value) | ScalarValue::LargeUtf8(value) | ScalarValue::Utf8View(value) => value.as_deref(),
                                            _ => None,
                                        };
                                        Ok(text.and_then(|value| {
                                            use num_traits::{ToPrimitive, Zero};
                                            let decimal = value.parse::<bigdecimal::BigDecimal>().ok()?;
                                            if decimal.is_zero() { return Some(0); }
                                            let digits = i128::from(decimal.digits()) + i128::from(scale) - i128::from(decimal.fractional_digit_count());
                                            if digits <= 0 { return Some(0); }
                                            if digits > i128::from(precision) { return None; }
                                            decimal.with_scale(i64::from(scale)).as_bigint_and_exponent().0.to_i128()
                                        }))
                                    }).collect::<datafusion::common::Result<Vec<_>>>()?;
                                    let result = arrow::array::Decimal128Array::from(values).with_precision_and_scale(precision, scale)?;
                                    if matches!(&args[0], datafusion::logical_expr::ColumnarValue::Scalar(_)) {
                                        Ok(datafusion::logical_expr::ColumnarValue::Scalar(ScalarValue::try_from_array(&result, 0)?))
                                    } else {
                                        Ok(datafusion::logical_expr::ColumnarValue::Array(std::sync::Arc::new(result)))
                                    }
                                }));
                            changed = true;
                            return Ok(Transformed::yes(udf.call(vec![*cast.expr.clone()])));
                        }
                    }
                }
                if let Expr::ScalarFunction(call) = &expr {
                    if call.func.name() == "trunc" && call.args.len() == 1 {
                        if let DataType::Decimal128(precision, scale) = call.args[0].get_type(&schema)? {
                            let ty = DataType::Decimal128(precision, scale);
                            let factor = 10_i128.pow(scale.max(0) as u32);
                            let udf = datafusion::logical_expr::create_udf("__orchiddb_starrocks_decimal_trunc", vec![ty.clone()], ty, Volatility::Immutable,
                                std::sync::Arc::new(move |args| {
                                    let arrays = datafusion::logical_expr::ColumnarValue::values_to_arrays(args)?;
                                    let values = arrays[0].as_any().downcast_ref::<arrow::array::Decimal128Array>().ok_or_else(|| datafusion::common::DataFusionError::Execution("decimal truncation requires Decimal128".into()))?;
                                    let result: arrow::array::Decimal128Array = values.unary(|value| value / factor * factor).with_precision_and_scale(precision, scale)?;
                                    if matches!(&args[0], datafusion::logical_expr::ColumnarValue::Scalar(_)) {
                                        Ok(datafusion::logical_expr::ColumnarValue::Scalar(ScalarValue::try_from_array(&result, 0)?))
                                    } else {
                                        Ok(datafusion::logical_expr::ColumnarValue::Array(std::sync::Arc::new(result)))
                                    }
                                }));
                            changed = true;
                            return Ok(Transformed::yes(udf.call(call.args.clone())));
                        }
                    }
                }
                if let Expr::BinaryExpr(binary) = &expr {
                    if binary.op == datafusion::logical_expr::Operator::Divide
                        && binary.left.get_type(&schema)?.is_integer()
                        && binary.right.get_type(&schema)?.is_integer() {
                        let output = expr.get_type(&schema)?;
                        let input = vec![binary.left.get_type(&schema)?, binary.right.get_type(&schema)?];
                        let result_type = output.clone();
                        let udf = datafusion::logical_expr::create_udf("__orchiddb_starrocks_integer_divide", input, output, Volatility::Immutable,
                            std::sync::Arc::new(move |args| {
                                let arrays = datafusion::logical_expr::ColumnarValue::values_to_arrays(args)?;
                                let left = arrow::compute::cast(&arrays[0], &result_type)?;
                                let right = arrow::compute::cast(&arrays[1], &result_type)?;
                                let result = arrow::compute::kernels::numeric::div(&left, &right)?;
                                if args.iter().all(|arg| matches!(arg, datafusion::logical_expr::ColumnarValue::Scalar(_))) {
                                    Ok(datafusion::logical_expr::ColumnarValue::Scalar(ScalarValue::try_from_array(&result, 0)?))
                                } else {
                                    Ok(datafusion::logical_expr::ColumnarValue::Array(result))
                                }
                            }));
                        changed = true;
                        return Ok(Transformed::yes(udf.call(vec![*binary.left.clone(), *binary.right.clone()])));
                    }
                }
                if let Expr::Cast(cast) = &expr {
                    if matches!(cast.data_type, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) {
                        let source = cast.expr.get_type(&schema)?;
                        if source == DataType::Boolean {
                            let result_type = cast.data_type.clone();
                            let udf = datafusion::logical_expr::create_udf("__orchiddb_starrocks_boolean_text", vec![source], result_type.clone(), Volatility::Immutable,
                                std::sync::Arc::new(move |args| match &args[0] {
                                    datafusion::logical_expr::ColumnarValue::Scalar(value) => Ok(datafusion::logical_expr::ColumnarValue::Scalar(value.cast_to(&result_type)?)),
                                    datafusion::logical_expr::ColumnarValue::Array(value) => Ok(datafusion::logical_expr::ColumnarValue::Array(arrow::compute::cast(value, &result_type)?)),
                                }));
                            changed = true;
                            return Ok(Transformed::yes(udf.call(vec![*cast.expr.clone()])));
                        }
                        if matches!(&source, DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) if matches!(f.data_type(), DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)) {
                            let result_type = cast.data_type.clone();
                            let udf = datafusion::logical_expr::create_udf("__orchiddb_starrocks_list_text", vec![source], cast.data_type.clone(), Volatility::Immutable,
                                std::sync::Arc::new(move |args| match &args[0] {
                                    datafusion::logical_expr::ColumnarValue::Scalar(value) => Ok(datafusion::logical_expr::ColumnarValue::Scalar(value.cast_to(&result_type)?)),
                                    datafusion::logical_expr::ColumnarValue::Array(value) => Ok(datafusion::logical_expr::ColumnarValue::Array(arrow::compute::cast(value, &result_type)?)),
                                }));
                            changed = true;
                            return Ok(Transformed::yes(udf.call(vec![*cast.expr.clone()])));
                        }
                    }
                }
                Ok(Transformed::no(expr))
            })?.map_data(|expr| Ok(saved.restore(expr)))
        })?.data;
        Ok(changed.then_some(super::lowering::RelationLowering::Rewrite(result)))
    }
    fn lower_function(&self, function: &ast::Function) -> SqlResult<Option<ast::Expr>> {
        let name = function.name.0.last().and_then(|p| p.as_ident()).map(|id| id.value.as_str());
        let replacement = match name {
            Some("length") => "char_length",
            Some("quantile_cont") => "percentile_cont",
            Some("quantile_disc") => "percentile_disc",
            _ => return Ok(None),
        };
        let mut function = function.clone();
        function.name = ast::ObjectName::from(vec![ast::Ident::new(replacement)]);
        Ok(Some(ast::Expr::Function(function)))
    }
    fn supports_scalar_function(&self, name: &str) -> bool {
        matches!(name, "__orchiddb_starrocks_decimal_trunc" | "__orchiddb_starrocks_boolean_text" | "__orchiddb_starrocks_list_text" | "__orchiddb_starrocks_integer_divide")
    }
    fn lower_scalar_function(
        &self,
        name: &str,
        args: &[ast::Expr],
    ) -> SqlResult<Option<ast::Expr>> {
        let template = match (name, args.len()) {
            ("__orchiddb_starrocks_boolean_text", 1) => "CASE WHEN __arg0 THEN 'true' WHEN NOT __arg0 THEN 'false' ELSE NULL END",
            ("trunc" | "__orchiddb_starrocks_decimal_trunc", 1) => "truncate(__arg0, 0)",
            ("__orchiddb_starrocks_integer_divide", 2) => "(__arg0 DIV __arg1)",
            ("__orchiddb_starrocks_list_text", 1) => {
                "concat('[', array_join(__arg0, ', ', ''), ']')"
            }
            ("array_position", 3) if args[2].to_string() == "1" => {
                "nullif(array_position(__arg0, __arg1), 0)"
            }
            ("array_position", 2) => "nullif(array_position(__arg0, __arg1), 0)",
            ("encode", 2) if args[1].to_string() == "'hex'" => "lower(hex(__arg0))",
            _ => return Ok(None),
        };
        Ok(Some(super::functions::portable_template(
            template,
            args,
            SqlDialect::Custom(&STARROCKS),
        )?))
    }
    fn finalize_sql(&self, sql: String) -> SqlResult<String> {
        if !sql.starts_with("WITH ") {
            return Ok(sql);
        }
        use std::{
            collections::{BTreeMap, BTreeSet},
            ops::ControlFlow,
        };
        let mut statements = datafusion::sql::sqlparser::parser::Parser::parse_sql(
            self.parser_dialect().as_ref(),
            &sql,
        )
        .map_err(|e| SqlError::Unsupported(format!("starrocks recursive SQL: {e}")))?;
        let Some(ast::Statement::Query(query)) = statements.first_mut() else {
            return Ok(sql);
        };
        let Some(with) = query.with.as_mut() else {
            return Ok(sql);
        };
        let mut recursive = BTreeSet::new();
        let mut inline = BTreeMap::new();
        let mut kept = Vec::new();
        for mut cte in std::mem::take(&mut with.cte_tables) {
            let name = cte.alias.name.value.clone();
            let mut dependencies = BTreeSet::new();
            let _: ControlFlow<()> = ast::visit_relations(&cte.query, |table| {
                if table.0.len() == 1 {
                    if let Some(id) = table.0[0].as_ident() {
                        dependencies.insert(id.value.clone());
                    }
                }
                ControlFlow::Continue(())
            });
            inline_ctes(&mut cte.query, &inline);
            if dependencies.contains(&name) {
                if !matches!(cte.query.body.as_ref(), ast::SetExpr::SetOperation { set_quantifier: ast::SetQuantifier::All, .. }) {
                    return Err(SqlError::Unsupported("starrocks recursive relations require UNION ALL".into()));
                }
                recursive.insert(name);
                kept.push(cte);
            } else if dependencies
                .iter()
                .any(|d| recursive.contains(d) || inline.contains_key(d))
            {
                inline.insert(name, cte.query);
            } else {
                kept.push(cte);
            }
        }
        if recursive.len() > 1 {
            return Err(SqlError::Unsupported("starrocks multiple recursive relations in one region".into()));
        }
        let reuse = kept.iter().any(|cte| cte.alias.name.value.starts_with("__w_sql_cte_"));
        with.cte_tables = kept;
        inline_ctes(query, &inline);
        if reuse {
            if let ast::SetExpr::Select(select) = query.body.as_mut() {
                select.optimizer_hint = Some(ast::OptimizerHint {
                    text: " SET_VAR(cbo_cte_force_reuse_node_count=1, cbo_cte_reuse_rate=0, cbo_cte_max_limit=1024, cbo_prune_subfield=false) ".into(),
                    style: ast::OptimizerHintStyle::MultiLine,
                });
            }
        }
        Ok(query.to_string())
    }
    fn rewrite_input_binding(&self, binding: &mut ast::Cte) -> SqlResult<()> {
        if !matches!(binding.query.body.as_ref(), ast::SetExpr::Values(_)) {
            return Ok(());
        }
        let mut statements = datafusion::sql::sqlparser::parser::Parser::parse_sql(
            self.parser_dialect().as_ref(), "SELECT * FROM (VALUES (NULL)) AS __orchiddb_input",
        ).map_err(|e| SqlError::Unsupported(e.to_string()))?;
        let ast::Statement::Query(mut template) = statements.remove(0) else { unreachable!() };
        let ast::SetExpr::Select(select) = template.body.as_mut() else { unreachable!() };
        let ast::TableFactor::Derived { subquery, .. } = &mut select.from[0].relation else { unreachable!() };
        std::mem::swap(&mut subquery.body, &mut binding.query.body);
        binding.query.body = template.body;
        Ok(())
    }
    fn rewrite_query(&self, query: &mut ast::Query) -> SqlResult<()> {
        if let Some(ast::LimitClause::LimitOffset { limit, offset: Some(offset), .. }) = &mut query.limit_clause {
            if limit.is_none() {
                let amount = offset.value.to_string().parse::<i64>().map_err(|_| SqlError::Unsupported("starrocks nonconstant offset without limit".into()))?;
                let count = i64::MAX.checked_sub(amount).filter(|_| amount >= 0).ok_or_else(|| SqlError::Unsupported("starrocks offset out of range".into()))?;
                *limit = Some(ast::Expr::Value(ast::Value::Number(count.to_string(), false).into()));
            }
        }
        let original = query.to_string();
        let ast::SetExpr::Select(select) = query.body.as_mut() else {
            return Ok(());
        };
        select.optimizer_hint = Some(ast::OptimizerHint {
            text: " SET_VAR(cbo_prune_subfield=false) ".into(),
            style: ast::OptimizerHintStyle::MultiLine,
        });
        for table in &mut select.from {
            for factor in std::iter::once(&mut table.relation).chain(table.joins.iter_mut().map(|join| &mut join.relation)) {
                if let ast::TableFactor::Table { name, args: Some(args), alias, .. } = factor {
                    if name.0.last().and_then(|p| p.as_ident()).is_some_and(|id| id.value == "generate_series") {
                        let sql = format!("SELECT * FROM TABLE(generate_series({}))", args.args.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "));
                        let mut statements = datafusion::sql::sqlparser::parser::Parser::parse_sql(self.parser_dialect().as_ref(), &sql)
                            .map_err(|e| SqlError::Unsupported(e.to_string()))?;
                        let ast::Statement::Query(query) = statements.remove(0) else { unreachable!() };
                        let ast::SetExpr::Select(mut select) = *query.body else { unreachable!() };
                        let mut replacement = select.from.remove(0).relation;
                        let ast::TableFactor::TableFunction { alias: target, .. } = &mut replacement else { unreachable!() };
                        *target = alias.clone();
                        *factor = replacement;
                    }
                }
            }
        }
        let mut unnests = Vec::new();
        for item in &mut select.projection {
            let expr = match item {
                ast::SelectItem::ExprWithAlias { expr, .. }
                | ast::SelectItem::UnnamedExpr(expr) => expr,
                _ => continue,
            };
            let ast::Expr::Function(f) = expr else {
                continue;
            };
            if !f.name.to_string().eq_ignore_ascii_case("unnest") {
                continue;
            }
            let ast::FunctionArguments::List(args) = &f.args else {
                return Err(SqlError::Unsupported("starrocks UNNEST arguments".into()));
            };
            let [ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(array))] =
                args.args.as_slice()
            else {
                return Err(SqlError::Unsupported(
                    "starrocks UNNEST requires one array".into(),
                ));
            };
            let mut n = unnests.len();
            while original.contains(&format!("__orchiddb_unnest_{n}")) {
                n += 1;
            }
            let name = ast::Ident::with_quote('`', format!("__orchiddb_unnest_{n}"));
            let value = ast::Ident::with_quote('`', "value");
            unnests.push(ast::TableFactor::UNNEST {
                alias: Some(ast::TableAlias {
                    name: name.clone(),
                    columns: vec![ast::TableAliasColumnDef {
                        name: value.clone(),
                        data_type: None,
                    }],
                    explicit: true,
                }),
                array_exprs: vec![array.clone()],
                with_offset: false,
                with_offset_alias: None,
                with_ordinality: false,
            });
            *expr = ast::Expr::CompoundIdentifier(vec![name, value]);
        }
        if unnests.len() > 1 {
            return Err(SqlError::Unsupported(
                "starrocks zipped projection UNNEST".into(),
            ));
        }
        for relation in unnests {
            if let Some(from) = select.from.last_mut() {
                from.joins.push(ast::Join {
                    relation,
                    global: false,
                    join_operator: ast::JoinOperator::CrossJoin(ast::JoinConstraint::None),
                });
            } else {
                select.from.push(ast::TableWithJoins {
                    relation,
                    joins: vec![],
                });
            }
        }
        if query.order_by.is_none()
            && select.distinct.is_none()
            && select.having.is_none()
            && matches!(&select.group_by, ast::GroupByExpr::Expressions(exprs, _) if exprs.is_empty())
            && select.projection.iter().all(|item| {
                matches!(
                    item,
                    ast::SelectItem::Wildcard(_)
                        | ast::SelectItem::UnnamedExpr(
                            ast::Expr::Identifier(_) | ast::Expr::CompoundIdentifier(_)
                        )
                        | ast::SelectItem::ExprWithAlias {
                            expr: ast::Expr::Identifier(_) | ast::Expr::CompoundIdentifier(_),
                            ..
                        }
                )
            })
        {
            if let [
                ast::TableWithJoins {
                    relation:
                        ast::TableFactor::Derived {
                            subquery,
                            alias: Some(alias),
                            ..
                        },
                    joins,
                },
            ] = select.from.as_mut_slice()
            {
                if joins.is_empty() {
                    if let Some(mut order) = subquery.order_by.clone() {
                        if let ast::OrderByKind::Expressions(items) = &mut order.kind {
                            for (position, item) in items.iter_mut().enumerate() {
                                let column = match &item.expr {
                                    ast::Expr::Identifier(id) => Some(id.clone()),
                                    ast::Expr::CompoundIdentifier(ids) => ids.last().cloned(),
                                    _ => None,
                                };
                                let ast::SetExpr::Select(inner) = subquery.body.as_mut() else {
                                    continue;
                                };
                                let exposed = column.as_ref().is_some_and(|name| {
                                    inner.projection.iter().any(|p| match p {
                                        ast::SelectItem::Wildcard(_) => true,
                                        ast::SelectItem::ExprWithAlias { alias, .. } => {
                                            alias.value == name.value
                                        }
                                        ast::SelectItem::UnnamedExpr(ast::Expr::Identifier(id)) => {
                                            id.value == name.value
                                        }
                                        ast::SelectItem::UnnamedExpr(
                                            ast::Expr::CompoundIdentifier(ids),
                                        ) => ids.last().is_some_and(|id| id.value == name.value),
                                        _ => false,
                                    })
                                });
                                let column = if exposed {
                                    column.unwrap()
                                } else {
                                    if inner.distinct.is_some()
                                        || select
                                            .projection
                                            .iter()
                                            .any(|p| matches!(p, ast::SelectItem::Wildcard(_)))
                                    {
                                        return Err(SqlError::Unsupported("starrocks hidden ordering key across DISTINCT or wildcard projection".into()));
                                    }
                                    let mut n = position;
                                    while original.contains(&format!("__orchiddb_order_{n}")) {
                                        n += 1;
                                    }
                                    let alias = ast::Ident::with_quote(
                                        '`',
                                        format!("__orchiddb_order_{n}"),
                                    );
                                    inner.projection.push(ast::SelectItem::ExprWithAlias {
                                        expr: item.expr.clone(),
                                        alias: alias.clone(),
                                    });
                                    alias
                                };
                                item.expr =
                                    ast::Expr::CompoundIdentifier(vec![alias.name.clone(), column]);
                            }
                        }
                        query.order_by = Some(order);
                    }
                }
            }
        }
        Ok(())
    }
    fn rewrite_expression(&self, expression: &mut ast::Expr) -> SqlResult<()> {
        match expression {
            ast::Expr::Exists { .. } => {
                return Err(SqlError::Unsupported("starrocks correlated EXISTS requires native execution".into()));
            }
            ast::Expr::IsTrue(arg)
            | ast::Expr::IsFalse(arg)
            | ast::Expr::IsNotTrue(arg)
            | ast::Expr::IsNotFalse(arg) => {
                let arg = *arg.clone();
                let template = match expression {
                    ast::Expr::IsTrue(_) => "coalesce(__arg0, false)",
                    ast::Expr::IsFalse(_) => "coalesce(NOT __arg0, false)",
                    ast::Expr::IsNotTrue(_) => "coalesce(NOT __arg0, true)",
                    _ => "coalesce(__arg0, true)",
                };
                *expression = super::functions::template(template, &[arg])?;
            }
            ast::Expr::Cast { kind, data_type, .. } => {
                if matches!(data_type, ast::DataType::Decimal(ast::ExactNumberInfo::PrecisionAndScale(_, scale)) if *scale > 0) {
                    return Err(SqlError::Unsupported("starrocks decimal casts do not preserve scientific notation".into()));
                }
                if *kind == ast::CastKind::TryCast { *kind = ast::CastKind::Cast; }
                normalize_cast_type(data_type);
            }
            ast::Expr::Array(array) if array.elem.iter().all(|e| matches!(e, ast::Expr::Value(v) if v.value == ast::Value::Null)) => {
                *expression = super::functions::template("CAST(__arg0 AS ARRAY<BOOLEAN>)", &[expression.clone()])?;
            }
            ast::Expr::Array(array) if !array.elem.is_empty() && array.elem.iter().all(|e| matches!(e, ast::Expr::Value(v) if matches!(v.value, ast::Value::SingleQuotedString(_)))) => {
                *expression = super::functions::portable_template("CAST(__arg0 AS ARRAY<VARCHAR(1048576)>)", &[expression.clone()], SqlDialect::Custom(&STARROCKS))?;
            }
            ast::Expr::Value(value) => {
                if let ast::Value::Number(number, _) = &value.value {
                    if number.contains(['.', 'e', 'E']) && number.chars().filter(char::is_ascii_digit).count() > 38 {
                        *expression = super::functions::template(&format!("CAST({} AS DOUBLE)", string(number)), &[])?;
                    }
                } else if let ast::Value::SingleQuotedString(s) = &value.value {
                    if s.contains(['\\', '\0']) {
                        *expression = super::functions::template(&string(s), &[])?;
                    }
                }
            }
            ast::Expr::IsNotDistinctFrom(a, b) | ast::Expr::IsDistinctFrom(a, b) => {
                let args = [*a.clone(), *b.clone()];
                let negate = matches!(expression, ast::Expr::IsDistinctFrom(..));
                *expression = super::functions::template(
                    if negate {
                        "NOT (__arg0 <=> __arg1)"
                    } else {
                        "(__arg0 <=> __arg1)"
                    },
                    &args,
                )?;
            }
            ast::Expr::BinaryOp {
                op: op @ ast::BinaryOperator::DuckIntegerDivide,
                ..
            } => {
                *op = ast::BinaryOperator::MyIntegerDivide;
            }
            ast::Expr::CompoundFieldAccess { root, access_chain } => {
                if let [ast::AccessExpr::Subscript(ast::Subscript::Index { index })] =
                    access_chain.as_slice()
                {
                    *expression = super::functions::template(
                        "array_slice(__arg0, __arg1, 1)[1]",
                        &[*root.clone(), index.clone()],
                    )?;
                }
            }
            ast::Expr::BinaryOp {
                left,
                op: ast::BinaryOperator::StringConcat,
                right,
            } => {
                *expression = super::functions::template(
                    "concat(__arg0, __arg1)",
                    &[*left.clone(), *right.clone()],
                )?;
            }
            ast::Expr::Function(f) => {
                let name = f
                    .name
                    .to_string()
                    .trim_matches(['`', '"'])
                    .to_ascii_lowercase();
                let renamed = match name.as_str() {
                    "length" | "character_length" => "char_length",
                    "chr" => "char",
                    "btrim" => "trim",
                    "array_length" | "cardinality" => "array_length",
                    "array_has" => "array_contains",
                    "make_array" => "array",
                    "strpos" => "instr",
                    "first_value" if f.over.is_none() => "any_value",
                    "stddev" => "stddev_samp",
                    "var" => "var_samp",
                    "unnest" | "count" | "sum" | "avg" | "min" | "max" | "array_agg"
                    | "coalesce" | "nullif" | "abs" | "ceil" | "floor" | "round" | "truncate"
                    | "sqrt" | "pow" | "power" | "exp" | "ln" | "log2" | "log10" | "sin"
                    | "cos" | "tan" | "asin" | "acos" | "atan" | "atan2" | "sign" | "pi"
                    | "lower" | "upper" | "char_length" | "concat" | "concat_ws"
                    | "replace" | "substring" | "substr" | "left" | "right" | "trim" | "ltrim"
                    | "rtrim" | "reverse" | "split" | "starts_with" | "ends_with" | "instr"
                    | "array" | "array_contains" | "array_position" | "array_distinct"
                    | "array_sort" | "array_reverse" | "array_concat" | "array_remove"
                    | "array_append" | "array_to_string" | "row_number" | "rank" | "dense_rank"
                    | "lag" | "lead" | "first_value" | "last_value" | "stddev_samp"
                    | "stddev_pop" | "var_samp" | "var_pop" | "md5" | "sha2" | "unhex" | "hex"
                    | "from_binary" | "if" | "any_value" | "uuid" => name.as_str(),
                    _ => {
                        return Err(SqlError::Unsupported(format!(
                            "starrocks has no mapping for function {name}"
                        )));
                    }
                };
                if f.filter.is_some() || !f.within_group.is_empty() {
                    return Err(SqlError::Unsupported(
                        "starrocks aggregate FILTER/WITHIN GROUP".into(),
                    ));
                }
                f.name = ast::ObjectName::from(vec![ast::Ident::new(renamed)]);
                if renamed == "array_append" {
                    if let ast::FunctionArguments::List(arguments) = &f.args {
                        if let Some(ast::FunctionArg::Unnamed(ast::FunctionArgExpr::Expr(value))) = arguments.args.last() {
                            if is_text_expression(value) {
                                *expression = super::functions::template("CAST(__arg0 AS ARRAY<VARCHAR(1048576)>)", &[expression.clone()])?;
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
impl StarRocksAdapter {
    fn array_literal(&self, a: arrow::array::ArrayRef) -> SqlResult<String> {
        let values = (0..a.len())
            .map(|i| {
                self.exchange_literal(&ScalarValue::try_from_array(a.as_ref(), i)?, a.data_type())
            })
            .collect::<SqlResult<Vec<_>>>()?;
        Ok(format!("[{}]", values.join(", ")))
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn string(value: &str) -> String {
    if !value.contains(['\'', '\\', '\0']) {
        format!("'{value}'")
    } else {
        format!("from_binary(unhex('{}'), 'utf8')", hex(value.as_bytes()))
    }
}

fn inline_ctes(query: &mut ast::Query, ctes: &std::collections::BTreeMap<String, Box<ast::Query>>) {
    use std::ops::ControlFlow;
    struct Inline<'a>(&'a std::collections::BTreeMap<String, Box<ast::Query>>);
    impl ast::VisitorMut for Inline<'_> {
        type Break = ();
        fn pre_visit_table_factor(&mut self, table: &mut ast::TableFactor) -> ControlFlow<()> {
            if let ast::TableFactor::Table {
                name,
                alias,
                args: None,
                ..
            } = table
            {
                if name.0.len() == 1 {
                    if let Some(id) = name.0[0].as_ident() {
                        if let Some(query) = self.0.get(&id.value) {
                            let alias = alias.clone().unwrap_or_else(|| ast::TableAlias {
                                name: id.clone(),
                                columns: vec![],
                                explicit: true,
                            });
                            *table = ast::TableFactor::Derived {
                                lateral: false,
                                subquery: query.clone(),
                                alias: Some(alias),
                                sample: None,
                            };
                        }
                    }
                }
            }
            ControlFlow::Continue(())
        }
    }
    use ast::VisitMut;
    let _ = query.visit(&mut Inline(ctes));
}

struct StarRocksUnparser;
impl UnparserDialect for StarRocksUnparser {
    fn identifier_quote_style(&self, _: &str) -> Option<char> { Some('`') }
    fn supports_qualify(&self) -> bool { false }
    fn requires_derived_table_alias(&self) -> bool { true }
    fn unnest_as_table_factor(&self) -> bool { true }
    fn supports_column_alias_in_table_alias(&self) -> bool { false }
    fn utf8_cast_dtype(&self) -> ast::DataType { varchar_type() }
    fn large_utf8_cast_dtype(&self) -> ast::DataType { varchar_type() }
    fn interval_style(&self) -> IntervalStyle { IntervalStyle::MySQL }
    fn date_field_extract_style(&self) -> DateFieldExtractStyle { DateFieldExtractStyle::Extract }
    fn timestamp_cast_dtype(&self, _: &arrow::datatypes::TimeUnit, _: &Option<std::sync::Arc<str>>) -> ast::DataType { ast::DataType::Datetime(None) }
    fn window_func_support_window_frame(&self, _: &str, _: &ast::WindowFrameBound, _: &ast::WindowFrameBound) -> bool { false }
    fn scalar_function_to_sql_overrides(&self, unparser: &datafusion::sql::unparser::Unparser, name: &str, args: &[datafusion::logical_expr::Expr]) -> datafusion::common::Result<Option<ast::Expr>> {
        CustomDialectBuilder::new().with_date_field_extract_style(DateFieldExtractStyle::Extract).build().scalar_function_to_sql_overrides(unparser, name, args)
    }
}
fn varchar_type() -> ast::DataType {
    ast::DataType::Varchar(Some(ast::CharacterLength::IntegerLength { length: 1048576, unit: None }))
}

fn normalize_cast_type(ty: &mut ast::DataType) {
    match ty {
        ast::DataType::Bool => *ty = ast::DataType::Boolean,
        ast::DataType::BigIntUnsigned(_) | ast::DataType::Int8Unsigned(_) | ast::DataType::UInt64 | ast::DataType::UBigInt => *ty = ast::DataType::Custom(ast::ObjectName::from(vec![ast::Ident::new("LARGEINT")]), vec![]),
        ast::DataType::IntUnsigned(_) | ast::DataType::Int4Unsigned(_) | ast::DataType::IntegerUnsigned(_) | ast::DataType::UInt32 => *ty = ast::DataType::BigInt(None),
        ast::DataType::SmallIntUnsigned(_) | ast::DataType::Int2Unsigned(_) | ast::DataType::UInt16 => *ty = ast::DataType::Int(None),
        ast::DataType::TinyIntUnsigned(_) | ast::DataType::UInt8 => *ty = ast::DataType::SmallInt(None),
        ast::DataType::Varchar(None) | ast::DataType::Text => *ty = varchar_type(),
        ast::DataType::Array(ast::ArrayElemTypeDef::AngleBracket(inner) | ast::ArrayElemTypeDef::Parenthesis(inner) | ast::ArrayElemTypeDef::SquareBracket(inner, _)) => normalize_cast_type(inner),
        _ => {}
    }
}

fn is_text_expression(expr: &ast::Expr) -> bool {
    match expr {
        ast::Expr::Nested(inner) => is_text_expression(inner),
        ast::Expr::Cast { data_type: ast::DataType::Varchar(_) | ast::DataType::Text, .. } => true,
        ast::Expr::Value(value) => matches!(value.value, ast::Value::SingleQuotedString(_)),
        ast::Expr::Function(function) => function.name.0.last().and_then(|part| part.as_ident()).is_some_and(|name| matches!(name.value.as_str(), "concat" | "from_binary")),
        _ => false,
    }
}
