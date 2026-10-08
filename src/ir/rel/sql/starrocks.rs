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
        Box::new(
            CustomDialectBuilder::new()
                .with_identifier_quote_style('`')
                .with_requires_derived_table_alias(true)
                .with_unnest_as_table_factor(true)
                .with_window_func_support_window_frame(false)
                .with_supports_column_alias_in_table_alias(false)
                .with_large_utf8_cast_dtype(ast::DataType::Varchar(None))
                .with_timestamp_cast_dtype(
                    ast::DataType::Datetime(None),
                    ast::DataType::Datetime(None),
                )
                .with_date_field_extract_style(DateFieldExtractStyle::Extract)
                .with_interval_style(IntervalStyle::MySQL)
                .build(),
        )
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
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "VARCHAR".into(),
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
                if let Expr::BinaryExpr(binary) = &expr {
                    if binary.op == datafusion::logical_expr::Operator::Divide
                        && binary.left.get_type(&schema)?.is_integer()
                        && binary.right.get_type(&schema)?.is_integer() {
                        let mut binary = binary.clone();
                        binary.op = datafusion::logical_expr::Operator::IntegerDivide;
                        changed = true;
                        return Ok(Transformed::yes(Expr::BinaryExpr(binary)));
                    }
                }
                if let Expr::Cast(cast) = &expr {
                    if matches!(cast.data_type, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) {
                        let source = cast.expr.get_type(&schema)?;
                        if matches!(&source, DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) if matches!(f.data_type(), DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View)) {
                            let udf = datafusion::logical_expr::create_udf("__orchiddb_starrocks_list_text", vec![source], cast.data_type.clone(), Volatility::Immutable,
                                std::sync::Arc::new(|_| Err(datafusion::common::DataFusionError::NotImplemented("SQL-only list display".into()))));
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
    fn lower_scalar_function(
        &self,
        name: &str,
        args: &[ast::Expr],
    ) -> SqlResult<Option<ast::Expr>> {
        let template = match (name, args.len()) {
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
        if !sql.starts_with("WITH RECURSIVE ") {
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
        with.cte_tables = kept;
        inline_ctes(query, &inline);
        Ok(query.to_string())
    }
    fn rewrite_query(&self, query: &mut ast::Query) -> SqlResult<()> {
        let original = query.to_string();
        let ast::SetExpr::Select(select) = query.body.as_mut() else {
            return Ok(());
        };
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
            ast::Expr::Cast { kind, .. } if *kind == ast::CastKind::TryCast => {
                *kind = ast::CastKind::Cast;
            }
            ast::Expr::Array(array) if !array.elem.is_empty() && array.elem.iter().all(|e| matches!(e, ast::Expr::Value(v) if matches!(v.value, ast::Value::SingleQuotedString(_)))) => {
                *expression = super::functions::portable_template("CAST(__arg0 AS ARRAY<VARCHAR(1048576)>)", &[expression.clone()], SqlDialect::Custom(&STARROCKS))?;
            }
            ast::Expr::Value(value) => {
                if let ast::Value::SingleQuotedString(s) = &value.value {
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
                    "character_length" => "char_length",
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
                    | "lower" | "upper" | "char_length" | "length" | "concat" | "concat_ws"
                    | "replace" | "substring" | "substr" | "left" | "right" | "trim" | "ltrim"
                    | "rtrim" | "reverse" | "split" | "starts_with" | "ends_with" | "instr"
                    | "array" | "array_contains" | "array_position" | "array_distinct"
                    | "array_sort" | "array_reverse" | "array_concat" | "array_remove"
                    | "array_append" | "array_to_string" | "row_number" | "rank" | "dense_rank"
                    | "lag" | "lead" | "first_value" | "last_value" | "stddev_samp"
                    | "stddev_pop" | "var_samp" | "var_pop" | "md5" | "sha2" | "unhex" | "hex"
                    | "from_binary" | "if" | "any_value" => name.as_str(),
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
    format!("from_binary(unhex('{}'), 'utf8')", hex(value.as_bytes()))
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
