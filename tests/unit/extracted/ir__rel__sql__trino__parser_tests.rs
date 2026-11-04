
    use super::*;

    #[test]
    fn float_literals_keep_their_type_without_decimal_precision_limits() {
        use datafusion::logical_expr::lit;
        use datafusion::sql::unparser::Unparser;
        let dialect = SqlDialect::Custom(&TRINO);
        let unparser_dialect = dialect.unparser_dialect();
        let unparser = Unparser::new(unparser_dialect.as_ref());
        for value in [
            ScalarValue::Float64(Some(f64::MAX)),
            ScalarValue::Float64(Some(f64::MIN_POSITIVE)),
            ScalarValue::Float64(Some(f64::from_bits(1))),
            ScalarValue::Float64(Some(-0.0)),
            ScalarValue::Float64(Some(f64::INFINITY)),
            ScalarValue::Float64(None),
            ScalarValue::Float32(Some(f32::MAX)),
            ScalarValue::Float32(Some(f32::from_bits(1))),
            ScalarValue::Float32(None),
        ] {
            let expected = TRINO.exchange_literal(&value, &value.data_type()).unwrap();
            let encoded = super::super::unparse::encode_expression_literals(lit(value), dialect).unwrap();
            assert!(encoded.transformed);
            let sql = super::super::logical_functions::with_expression(
                &encoded.data, &datafusion::common::DFSchema::empty(), dialect, || {
                    let mut expression = unparser.expr_to_sql(&encoded.data)?;
                    super::super::functions::prepare_ast(&mut expression, dialect)?;
                    Ok(expression.to_string())
                },
            ).unwrap();
            assert_eq!(sql, format!("({expected})"));
        }
    }

    #[test]
    fn nested_trino_cast_types_survive_template_and_final_sql_parsing() {
        for ty in [
            "ARRAY(VARCHAR)",
            "MAP(VARCHAR, ARRAY(BIGINT))",
            "ROW(a VARCHAR, b ARRAY(BIGINT))",
            "ROW(\"a\" VARCHAR, \"b\" ARRAY(ROW(\"value\" DECIMAL(20,2))))",
        ] {
            let expr = super::super::functions::portable_template(
                &format!("CAST(__arg0 AS {ty})"),
                &[ast::Expr::Value(ast::Value::Null.into())],
                SqlDialect::Custom(&TRINO),
            ).unwrap();
            let sql = TRINO.finalize_sql(format!("SELECT {expr}")).unwrap();
            let statements = datafusion::sql::sqlparser::parser::Parser::parse_sql(&TrinoParser, &sql).unwrap();
            assert_eq!(statements.len(), 1);
            assert!(!sql.contains("ARRAY<"), "{sql}");
            if ty.contains("\"value\"") { assert!(sql.contains("\"value\" DECIMAL("), "{sql}"); }
        }
    }
    #[test]
    fn occurrence_ordinals_and_outer_row_relations_require_materialization() {
        use datafusion::logical_expr::{Expr, ExprFunctionExt, LogicalPlanBuilder, col, lit};
        use super::super::lowering::{LoweringContext,LoweringMode};
        let context=LoweringContext {dialect:SqlDialect::Custom(&TRINO),mode:LoweringMode::InIsland};
        let input=LogicalPlanBuilder::empty(true).project(vec![lit(1_i64).alias("x")]).unwrap().build().unwrap();
        let unordered=datafusion::functions_window::expr_fn::row_number().alias("ordinal");
        let plan=LogicalPlanBuilder::from(input.clone()).window(vec![unordered]).unwrap().build().unwrap();
        assert!(TRINO.lower_relation(&plan,&context).unwrap_err().to_string().contains("materialized completion"));
        for function in [
            datafusion::functions_window::expr_fn::row_number().order_by(vec![col("x").sort(true,false)]).build().unwrap(),
            datafusion::functions_window::expr_fn::row_number().partition_by(vec![col("x")]).build().unwrap(),
        ] {
            let plan=LogicalPlanBuilder::from(input.clone()).window(vec![function.alias("rank")]).unwrap().build().unwrap();
            assert!(TRINO.lower_relation(&plan,&context).is_ok());
        }
        let reference=Expr::OuterReferenceColumn(std::sync::Arc::new(arrow::datatypes::Field::new("x",DataType::Int64,true)),datafusion::common::Column::from_name("x"));
        let correlated=LogicalPlanBuilder::empty(true).project(vec![reference.alias("seed")]).unwrap().build().unwrap();
        assert!(TRINO.lower_relation(&correlated,&context).unwrap_err().to_string().contains("outer-row relation"));
        assert!(TRINO.lower_relation(&input,&context).is_ok());
    }

    #[test]
    fn correlated_seed_after_scope_repair_is_not_pushed_to_trino() {
        use datafusion::sql::sqlparser::parser::Parser;
        for sql in [
            "SELECT count(*) FROM (SELECT outer_id AS id) seed JOIN edges ON seed.id=edges.src",
            "SELECT count(*) FROM (SELECT outer_row.id AS id) seed JOIN edges ON seed.id=edges.src",
        ] {
            let mut statements=Parser::parse_sql(&TrinoParser,sql).unwrap();
            let ast::Statement::Query(query)=&mut statements[0] else {panic!()};
            assert!(TRINO.rewrite_query(query).unwrap_err().to_string().contains("outer-row relation"));
        }
        for sql in ["SELECT count(*) FROM (SELECT 1 AS id) seed", "SELECT count(*) FROM (SELECT id FROM nodes) seed"] {
            let mut statements=Parser::parse_sql(&TrinoParser,sql).unwrap();
            let ast::Statement::Query(query)=&mut statements[0] else {panic!()};
            TRINO.rewrite_query(query).unwrap();
        }
    }


#[test]
fn temporal_type_precision_and_timezone_survive_trino_type_lowering() {
    let unparser=TRINO.unparser_dialect();
    assert_eq!(TRINO.double_type(),"DOUBLE");
    assert!(TRINO.supports_scalar_function("random"));
    assert!(!TRINO.supports_scalar_function("not_registered"));
    assert!(unparser.use_timestamp_for_date64());
    assert_eq!(unparser.large_utf8_cast_dtype(),ast::DataType::Varchar(None));
    for (unit,digits) in [(TimeUnit::Second,0),(TimeUnit::Millisecond,3),(TimeUnit::Microsecond,6),(TimeUnit::Nanosecond,9)] {
        for zone in [None,Some(std::sync::Arc::<str>::from("UTC"))] {
            let suffix=if zone.is_some(){" WITH TIME ZONE"}else{""};
            assert_eq!(TRINO.sql_type(&DataType::Timestamp(unit.clone(),zone.clone())).unwrap(),format!("TIMESTAMP({digits}){suffix}"));
            assert_eq!(unparser.timestamp_cast_dtype(&unit,&zone),ast::DataType::Timestamp(Some(digits),if zone.is_some(){ast::TimezoneInfo::WithTimeZone}else{ast::TimezoneInfo::None}));
        }
        let ty=if digits<6 {DataType::Time32(unit)} else {DataType::Time64(unit)};
        assert_eq!(TRINO.sql_type(&ty).unwrap(),format!("TIME({digits})"));
    }
}

#[test]
fn trino_map_and_decimal_types_reject_invalid_or_unsupported_domains() {
    use arrow::datatypes::Field;
    use std::sync::Arc;
    let valid=DataType::Map(Arc::new(Field::new("entries",DataType::Struct(vec![
        Field::new("key",DataType::Utf8,false),Field::new("value",DataType::Int64,true)].into()),false)),false);
    assert_eq!(TRINO.sql_type(&valid).unwrap(),"MAP(VARCHAR, BIGINT)");
    for (inner,message) in [(DataType::Int64,"entries must be a struct"),
        (DataType::Struct(vec![Field::new("key",DataType::Utf8,false)].into()),"requires key and value fields")] {
        assert!(TRINO.sql_type(&DataType::Map(Arc::new(Field::new("entries",inner,false)),false)).unwrap_err().to_string().contains(message));
    }
    assert_eq!(TRINO.sql_type(&DataType::Decimal128(12,3)).unwrap(),"DECIMAL(12,3)");
    for ty in [DataType::Decimal128(12,-1),DataType::Decimal128(12,13),DataType::Duration(TimeUnit::Second)] {
        assert!(TRINO.sql_type(&ty).unwrap_err().to_string().contains("trino type"));
    }
}
