//! Trino SQL adapter. Connector catalogs and credentials remain caller-owned.
use super::{DialectAdapter, SqlDialect, SqlError, SqlResult};
use arrow::{
    array::Array,
    datatypes::{DataType, TimeUnit},
};
use datafusion::{
    common::ScalarValue,
    sql::{
        sqlparser::{
            ast,
            dialect::{Dialect as ParserDialect, GenericDialect},
        },
        unparser::dialect::{DateFieldExtractStyle, Dialect as UnparserDialect},
    },
};

pub mod connectors;

#[derive(Debug)]
pub struct TrinoAdapter;
pub static TRINO: TrinoAdapter = TrinoAdapter;

impl DialectAdapter for TrinoAdapter {
    fn name(&self) -> &'static str {
        "trino"
    }
    fn parser_dialect(&self) -> Box<dyn ParserDialect> {
        Box::new(TrinoParser)
    }
    fn unparser_dialect(&self) -> Box<dyn UnparserDialect> {
        Box::new(TrinoUnparser)
    }
    fn finalize_sql(&self, sql: String) -> SqlResult<String> {
        // sqlparser models VERSION AS OF without Trino's FOR keyword. Change
        // only syntax tokens, never literals, quoted identifiers or comments.
        use datafusion::sql::sqlparser::tokenizer::{Token, Tokenizer};
        // The shared structural codec can introduce casts after expression
        // adaptation; normalize their type ASTs at the final boundary too.
        let mut statements =
            datafusion::sql::sqlparser::parser::Parser::parse_sql(&TrinoParser, &sql)
                .map_err(|e| SqlError::Unsupported(e.to_string()))?;
        let _ = ast::visit_expressions_mut(&mut statements, |expr| {
            if let ast::Expr::Cast { data_type, .. } = expr {
                normalize_type(data_type);
            }
            std::ops::ControlFlow::<()>::Continue(())
        });
        let sql = statements
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("; ");
        let mut tokens = Tokenizer::new(&TrinoParser, &sql)
            .tokenize()
            .map_err(|e| SqlError::Unsupported(e.to_string()))?;
        let significant = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| !matches!(t, Token::Whitespace(_)))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        for window in significant.windows(3) {
            if matches!(&tokens[window[0]],Token::Word(w) if w.quote_style.is_none() && w.value.eq_ignore_ascii_case("VERSION"))
                && matches!(&tokens[window[1]],Token::Word(w) if w.keyword==datafusion::sql::sqlparser::keywords::Keyword::AS)
                && matches!(&tokens[window[2]],Token::Word(w) if w.keyword==datafusion::sql::sqlparser::keywords::Keyword::OF)
            {
                if let Token::Word(w) = &mut tokens[window[0]] {
                    w.value = "FOR VERSION".into();
                }
            }
        }
        Ok(tokens.iter().map(ToString::to_string).collect())
    }
    fn source_provider(
        &self,
        metadata: &crate::ir::rel::source_metadata::SourceMetadata,
        schema: arrow::datatypes::SchemaRef,
    ) -> SqlResult<Option<std::sync::Arc<dyn datafusion::catalog::TableProvider>>> {
        connectors::provider(metadata, schema)
    }
    fn requires_scope_repair(&self) -> bool {
        true
    }
    fn create_table_keyword(&self) -> &'static str {
        "CREATE TABLE"
    }
    fn double_type(&self) -> &'static str {
        "DOUBLE"
    }
    fn sql_type(&self, ty: &DataType) -> SqlResult<String> {
        if crate::ir::functions::domain::is_json(ty) {
            return Ok("JSON".into());
        }
        Ok(match ty {
            DataType::Null | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => {
                "VARCHAR".into()
            }
            DataType::Boolean => "BOOLEAN".into(),
            DataType::Int8 => "TINYINT".into(),
            DataType::Int16 | DataType::UInt8 => "SMALLINT".into(),
            DataType::Int32 | DataType::UInt16 => "INTEGER".into(),
            DataType::Int64 | DataType::UInt32 => "BIGINT".into(),
            DataType::UInt64 => "DECIMAL(20,0)".into(),
            DataType::Float32 => "REAL".into(),
            DataType::Float64 => "DOUBLE".into(),
            DataType::Date32 => "DATE".into(),
            DataType::Timestamp(unit, tz) => format!(
                "TIMESTAMP({}){}",
                precision(unit),
                if tz.is_some() { " WITH TIME ZONE" } else { "" }
            ),
            DataType::Time32(unit) | DataType::Time64(unit) => format!("TIME({})", precision(unit)),
            DataType::Binary | DataType::LargeBinary | DataType::BinaryView => "VARBINARY".into(),
            DataType::Decimal128(p, s) if *s >= 0 && (*s as u8) <= *p => {
                format!("DECIMAL({p},{s})")
            }
            DataType::List(f) | DataType::LargeList(f) | DataType::FixedSizeList(f, _) => {
                format!("ARRAY({})", self.sql_type(f.data_type())?)
            }
            DataType::Struct(fields) => format!(
                "ROW({})",
                fields
                    .iter()
                    .map(|f| Ok(format!(
                        "{} {}",
                        self.quote_ident(f.name()),
                        self.sql_type(f.data_type())?
                    )))
                    .collect::<SqlResult<Vec<_>>>()?
                    .join(", ")
            ),
            _ => return Err(SqlError::Unsupported(format!("trino type {ty}"))),
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
                | ScalarValue::BinaryView(Some(b)) => format!(
                    "X'{}'",
                    b.iter().map(|x| format!("{x:02x}")).collect::<String>()
                ),
                ScalarValue::List(a) => self.array_literal(a.value(0))?,
                ScalarValue::LargeList(a) => self.array_literal(a.value(0))?,
                ScalarValue::FixedSizeList(a) => self.array_literal(a.value(0))?,
                ScalarValue::Float32(Some(v)) => string(&v.to_string()),
                ScalarValue::Float64(Some(v)) => string(&v.to_string()),
                ScalarValue::Boolean(Some(v)) => v.to_string(),
                _ if ty.is_integer() => value.to_string(),
                ScalarValue::Decimal128(..)
                | ScalarValue::Date32(_)
                | ScalarValue::TimestampSecond(..)
                | ScalarValue::TimestampMillisecond(..)
                | ScalarValue::TimestampMicrosecond(..)
                | ScalarValue::TimestampNanosecond(..) => string(
                    &arrow::util::display::array_value_to_string(value.to_array()?.as_ref(), 0)?,
                ),
                _ => return Err(SqlError::Unsupported(format!("trino literal {ty}"))),
            }
        };
        Ok(format!("CAST({literal} AS {target})"))
    }
    fn check_subquery(&self, r: &super::SubqueryRequirements) -> Result<(), &'static str> {
        if r.correlated_limit || (r.correlated && (r.window || r.unnest)) {
            return Err("Trino correlated window/UNNEST or dynamic LIMIT requires decorrelation");
        }
        Ok(())
    }
    fn lower_relation(
        &self,
        plan: &datafusion::logical_expr::LogicalPlan,
        ctx: &super::lowering::LoweringContext,
    ) -> SqlResult<Option<super::lowering::RelationLowering>> {
        if let Some(sql) = connectors::lower(plan)? {
            return Ok(Some(super::lowering::RelationLowering::Sql(sql)));
        }
        super::lowering::builtin_lower_relation(plan, ctx)
    }
    fn function_mapping(
        &self,
        name: &str,
    ) -> Option<crate::ir::functions::logical::SqlFunctionMapping> {
        let value = match name {
            "vector.dot" => vector("dot_product(__arg0, __arg1)"),
            "vector.l2_distance" => vector("euclidean_distance(__arg0, __arg1)"),
            "vector.cosine_similarity" => vector(
                "greatest(-1.0, least(1.0, dot_product(__arg0, __arg1) / nullif(sqrt(dot_product(__arg0, __arg0) * dot_product(__arg1, __arg1)), 0.0)))",
            ),
            // MaxSim uses inner products, matching the shared scorer.
            "vector.maxsim" => maxsim(),
            "fn.contains" => "strpos(__arg0, __arg1) > 0".into(),
            "fn.array_length" | "fn.cardinality" => "cardinality(__arg0)".into(),
            "fn.array_has" => "contains(__arg0, __arg1)".into(),
            "fn.array_concat" => "concat(__args)".into(),
            "fn.array_element" => "element_at(__arg0, nullif(__arg1, 0))".into(),
            "fn.array_to_string" => "array_join(__args)".into(),
            "fn.isfinite" => "is_finite(__arg0)".into(),
            "fn.isnan" => "is_nan(__arg0)".into(),
            _ => {
                let n = name.strip_prefix("fn.")?;
                if !matches!(
                    n,
                    "abs"
                        | "acos"
                        | "asin"
                        | "atan"
                        | "atan2"
                        | "ceil"
                        | "floor"
                        | "sqrt"
                        | "exp"
                        | "ln"
                        | "log10"
                        | "log2"
                        | "sin"
                        | "cos"
                        | "tan"
                        | "signum"
                        | "power"
                        | "round"
                        | "lower"
                        | "upper"
                        | "trim"
                        | "ltrim"
                        | "rtrim"
                        | "replace"
                        | "reverse"
                        | "starts_with"
                        | "ends_with"
                        | "coalesce"
                        | "nullif"
                        | "array_distinct"
                        | "array_sort"
                ) {
                    return None;
                }
                format!("{}(__args)", if n == "signum" { "sign" } else { n })
            }
        };
        Some(crate::ir::functions::logical::SqlFunctionMapping {
            value,
            ordering: None,
        })
    }
    fn lower_scalar_function(
        &self,
        name: &str,
        args: &[ast::Expr],
    ) -> SqlResult<Option<ast::Expr>> {
        let template = match (name, args.len()) {
            ("contains", 2) => "strpos(__arg0, __arg1) > 0",
            ("array_position", 2) => "nullif(array_position(__arg0, __arg1), 0)",
            ("array_position", 3) if args[2].to_string() == "1" => {
                "nullif(array_position(__arg0, __arg1), 0)"
            }
            ("array_position", 3) => {
                return Err(SqlError::Unsupported(
                    "Trino array_position instance is not a DataFusion start offset".into(),
                ));
            }
            ("left", 2) => "substr(__arg0, 1, __arg1)",
            ("right", 2) => "substr(__arg0, -__arg1)",
            ("encode", 2) if args[1].to_string() == "'hex'" => "lower(to_hex(__arg0))",
            _ => return Ok(None),
        };
        Ok(Some(super::functions::portable_template(
            template,
            args,
            SqlDialect::Custom(&TRINO),
        )?))
    }
    fn rewrite_query(&self, query: &mut ast::Query) -> SqlResult<()> {
        if let Some(ast::LimitClause::LimitOffset {
            limit,
            offset: Some(_),
            limit_by,
        }) = &mut query.limit_clause
        {
            if limit.is_some() && limit_by.is_empty() && query.fetch.is_none() {
                query.fetch = Some(ast::Fetch {
                    with_ties: false,
                    percent: false,
                    quantity: limit.take(),
                });
            }
        }
        Ok(())
    }
    fn rewrite_expression(&self, expression: &mut ast::Expr) -> SqlResult<()> {
        match expression {
            ast::Expr::CompoundFieldAccess { root, access_chain } => {
                if let [ast::AccessExpr::Subscript(ast::Subscript::Index { index })] =
                    access_chain.as_slice()
                {
                    *expression = super::functions::portable_template(
                        "element_at(__arg0, nullif(__arg1,0))",
                        &[*root.clone(), index.clone()],
                        SqlDialect::Custom(&TRINO),
                    )?;
                }
            }
            ast::Expr::Dictionary(fields) => {
                let args = fields.iter().map(|f| *f.value.clone()).collect::<Vec<_>>();
                *expression = super::functions::portable_template(
                    "ROW(__args)",
                    &args,
                    SqlDialect::Custom(&TRINO),
                )?;
            }
            ast::Expr::Array(a) => a.named = true,
            ast::Expr::Cast { data_type, .. } => normalize_type(data_type),
            ast::Expr::Function(f) => {
                let parts = f
                    .name
                    .0
                    .iter()
                    .filter_map(|p| p.as_ident())
                    .collect::<Vec<_>>();
                if parts.len() == 3
                    && parts[1].value == "system"
                    && matches!(
                        parts[2].value.as_str(),
                        "query_range" | "table_changes" | "raw_query" | "query"
                    )
                {
                    return Ok(());
                }

                let name = f.name.to_string().trim_matches('"').to_ascii_lowercase();
                let renamed = match name.as_str() {
                    "character_length" | "char_length" => "length",
                    "btrim" => "trim",
                    "array_length" => "cardinality",
                    "array_has" => "contains",
                    "array_concat" => "concat",
                    "array_to_string" => "array_join",
                    "array_reverse" => "reverse",
                    "stddev" => "stddev_samp",
                    "var" => "var_samp",
                    "trunc" => "truncate",
                    "isnan" => "is_nan",
                    "isfinite" => "is_finite",
                    "first_value" if f.over.is_none() => "arbitrary",
                    "row"
                    | "count"
                    | "sum"
                    | "avg"
                    | "min"
                    | "max"
                    | "array_agg"
                    | "arbitrary"
                    | "coalesce"
                    | "nullif"
                    | "if"
                    | "abs"
                    | "ceil"
                    | "floor"
                    | "round"
                    | "truncate"
                    | "sqrt"
                    | "pow"
                    | "power"
                    | "exp"
                    | "ln"
                    | "log2"
                    | "log10"
                    | "sin"
                    | "cos"
                    | "tan"
                    | "asin"
                    | "acos"
                    | "atan"
                    | "atan2"
                    | "sign"
                    | "pi"
                    | "lower"
                    | "upper"
                    | "length"
                    | "concat"
                    | "concat_ws"
                    | "replace"
                    | "substring"
                    | "substr"
                    | "trim"
                    | "ltrim"
                    | "rtrim"
                    | "reverse"
                    | "split"
                    | "starts_with"
                    | "ends_with"
                    | "strpos"
                    | "contains"
                    | "cardinality"
                    | "array_position"
                    | "array_distinct"
                    | "array_sort"
                    | "array_join"
                    | "array_remove"
                    | "element_at"
                    | "slice"
                    | "sequence"
                    | "row_number"
                    | "rank"
                    | "dense_rank"
                    | "lag"
                    | "lead"
                    | "first_value"
                    | "last_value"
                    | "stddev_samp"
                    | "stddev_pop"
                    | "var_samp"
                    | "var_pop"
                    | "to_hex"
                    | "from_hex"
                    | "from_utf8"
                    | "to_utf8"
                    | "regexp_like"
                    | "regexp_replace"
                    | "is_nan"
                    | "is_finite"
                    | "dot_product"
                    | "euclidean_distance"
                    | "reduce"
                    | "transform"
                    | "filter"
                    | "array_max"
                    | "all_match"
                    | "any_match"
                    | "fail"
                    | "json_parse"
                    | "json_format"
                    | "json_extract"
                    | "json_extract_scalar"
                    | "map"
                    | "map_keys"
                    | "map_values" => name.as_str(),
                    _ => {
                        return Err(SqlError::Unsupported(format!(
                            "trino has no mapping for function {name}; register an explicit logical function SQL mapping"
                        )));
                    }
                };
                f.name = ast::ObjectName::from(vec![ast::Ident::new(renamed)]);
            }
            _ => {}
        }
        Ok(())
    }
}
fn vector(score: &str) -> String {
    format!(
        "CASE WHEN __arg0 IS NULL OR __arg1 IS NULL THEN NULL WHEN cardinality(__arg0) = 0 OR cardinality(__arg0) <> cardinality(__arg1) OR any_match(__arg0, __local0 -> __local0 IS NULL OR NOT is_finite(__local0)) OR any_match(__arg1, __local0 -> __local0 IS NULL OR NOT is_finite(__local0)) THEN CAST(fail('invalid search vectors') AS DOUBLE) ELSE CASE WHEN NOT is_finite({score}) THEN CAST(fail('vector score overflow') AS DOUBLE) ELSE {score} END END"
    )
}
fn maxsim() -> String {
    let score = "reduce(transform(__arg0, __local0 -> array_max(transform(__arg1, __local1 -> dot_product(__local0,__local1)))), CAST(0 AS DOUBLE), (__local2,__local3) -> __local2+__local3, __local4 -> __local4)";
    format!(
        "CASE WHEN __arg0 IS NULL OR __arg1 IS NULL THEN NULL WHEN cardinality(__arg0)=0 OR cardinality(__arg1)=0 OR any_match(concat(__arg0,__arg1), __local5 -> __local5 IS NULL OR cardinality(__local5)=0 OR cardinality(__local5)<>cardinality(element_at(__arg0,1)) OR any_match(__local5, __local6 -> __local6 IS NULL OR NOT is_finite(__local6))) THEN CAST(fail('invalid token matrices') AS DOUBLE) WHEN NOT is_finite({score}) THEN CAST(fail('vector score overflow') AS DOUBLE) ELSE {score} END"
    )
}
fn precision(unit: &TimeUnit) -> u64 {
    match unit {
        TimeUnit::Second => 0,
        TimeUnit::Millisecond => 3,
        TimeUnit::Microsecond => 6,
        TimeUnit::Nanosecond => 9,
    }
}
fn string(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}
impl TrinoAdapter {
    fn array_literal(&self, a: arrow::array::ArrayRef) -> SqlResult<String> {
        Ok(format!(
            "ARRAY[{}]",
            (0..a.len())
                .map(|i| self.exchange_literal(&ScalarValue::try_from_array(&a, i)?, a.data_type()))
                .collect::<SqlResult<Vec<_>>>()?
                .join(", ")
        ))
    }
}
fn normalize_type(ty: &mut ast::DataType) {
    match ty {
        ast::DataType::Custom(name, fields) if name.to_string().eq_ignore_ascii_case("ROW") => {
            // sqlparser treats ROW as a custom type and separates field names
            // from their scalar types. Restore the named-row field syntax.
            let mut named: Vec<String> = Vec::new();
            for field in std::mem::take(fields) {
                if field.starts_with('"') || named.is_empty() {
                    named.push(field);
                } else {
                    let previous = named.last_mut().unwrap();
                    previous.push(' ');
                    previous.push_str(&field);
                }
            }
            *fields = named;
        }
        ast::DataType::Bool => *ty = ast::DataType::Boolean,
        ast::DataType::BigIntUnsigned(_)
        | ast::DataType::Int8Unsigned(_)
        | ast::DataType::UInt64
        | ast::DataType::UBigInt => {
            *ty = ast::DataType::Decimal(ast::ExactNumberInfo::PrecisionAndScale(20, 0))
        }
        ast::DataType::IntUnsigned(_)
        | ast::DataType::Int4Unsigned(_)
        | ast::DataType::IntegerUnsigned(_)
        | ast::DataType::UInt32 => *ty = ast::DataType::BigInt(None),
        ast::DataType::SmallIntUnsigned(_)
        | ast::DataType::Int2Unsigned(_)
        | ast::DataType::UInt16 => *ty = ast::DataType::Integer(None),
        ast::DataType::TinyIntUnsigned(_) | ast::DataType::UInt8 => {
            *ty = ast::DataType::SmallInt(None)
        }
        ast::DataType::Text => *ty = ast::DataType::Varchar(None),
        ast::DataType::Array(element) => match element {
            ast::ArrayElemTypeDef::SquareBracket(t, _) | ast::ArrayElemTypeDef::AngleBracket(t) => {
                normalize_type(t);
                *element = ast::ArrayElemTypeDef::Parenthesis(t.clone());
            }
            _ => {}
        },
        _ => {}
    }
}
struct TrinoUnparser;
impl UnparserDialect for TrinoUnparser {
    fn identifier_quote_style(&self, _: &str) -> Option<char> {
        Some('"')
    }
    fn supports_qualify(&self) -> bool {
        false
    }
    fn requires_derived_table_alias(&self) -> bool {
        true
    }
    fn unnest_as_table_factor(&self) -> bool {
        true
    }
    fn use_timestamp_for_date64(&self) -> bool {
        true
    }
    fn large_utf8_cast_dtype(&self) -> ast::DataType {
        ast::DataType::Varchar(None)
    }
    fn date_field_extract_style(&self) -> DateFieldExtractStyle {
        DateFieldExtractStyle::Extract
    }
    fn timestamp_cast_dtype(
        &self,
        unit: &TimeUnit,
        tz: &Option<std::sync::Arc<str>>,
    ) -> ast::DataType {
        ast::DataType::Timestamp(
            Some(precision(unit)),
            if tz.is_some() {
                ast::TimezoneInfo::WithTimeZone
            } else {
                ast::TimezoneInfo::None
            },
        )
    }
    fn window_func_support_window_frame(
        &self,
        name: &str,
        _: &ast::WindowFrameBound,
        _: &ast::WindowFrameBound,
    ) -> bool {
        !matches!(name, "row_number" | "rank" | "dense_rank" | "lead" | "lag")
    }
}

#[derive(Debug)]
struct TrinoParser;
impl ParserDialect for TrinoParser {
    fn is_identifier_start(&self, c: char) -> bool {
        GenericDialect {}.is_identifier_start(c)
    }
    fn is_identifier_part(&self, c: char) -> bool {
        GenericDialect {}.is_identifier_part(c)
    }
    fn supports_table_versioning(&self) -> bool {
        true
    }
    fn supports_lambda_functions(&self) -> bool {
        true
    }
    fn supports_named_fn_args_with_assignment_operator(&self) -> bool {
        true
    }
    fn supports_values_as_table_factor(&self) -> bool {
        true
    }
}
