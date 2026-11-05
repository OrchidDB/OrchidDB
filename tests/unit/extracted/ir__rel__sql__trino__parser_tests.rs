
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

#[cfg(feature = "trino")]
#[tokio::test]
#[ignore = "requires Trino"]
async fn trino_typed_graph_transport_preserves_nested_property_values() {
    use std::sync::Arc;
    use arrow::datatypes::Field;
    use datafusion::logical_expr::{LogicalPlanBuilder, col, lit};
    use datafusion::functions::core::expr_fn::named_struct;
    use crate::ir::rel::native_values as native;
    use crate::execution::trino::{TrinoOptions, TrinoSession};
    use futures::StreamExt;
    let endpoint = std::env::var("ORCHIDDB_TEST_OPA_URL").expect("Trino URL");
    let trino = TrinoSession::source(TrinoOptions::new(endpoint, "orchid_test")).unwrap();
    let list = ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Int64(Some(7)), ScalarValue::Int64(None)], &DataType::Int64));
    let mut values = vec![
        ScalarValue::Binary(Some(vec![0,39,92,255])),
        ScalarValue::Date32(Some(19782)),
        ScalarValue::TimestampNanosecond(Some(1700000000123456789), None),
        ScalarValue::TimestampMicrosecond(Some(1700000000123456), Some("UTC".into())),
        ScalarValue::Time32Second(Some(45296)),
        ScalarValue::Time32Millisecond(Some(45296123)),
        ScalarValue::Time64Microsecond(Some(45296123456)),
        ScalarValue::Time64Nanosecond(Some(45296123456789)),
        list.clone(),
        list.cast_to(&DataType::LargeList(Arc::new(Field::new("item", DataType::Int64, true)))).unwrap(),
        list.cast_to(&DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Int64, true)), 2)).unwrap(),
        ScalarValue::Utf8(Some("雪🙂'".into())),
    ];
    values.push(ScalarValue::try_from(values[0].data_type()).unwrap());
    for value in values {
        let descriptor = named_struct(vec![
            lit("kind"), lit("node"), lit("id"), lit(7_i64), lit("label"), lit("Person"),
            lit("source_keys"), lit(true), lit("properties"), named_struct(vec![lit("payload"), lit(value.clone())]),
        ]);
        let input = LogicalPlanBuilder::empty(true).project(vec![descriptor.clone().alias("descriptor")]).unwrap().build().unwrap();
        let plan = LogicalPlanBuilder::from(input).project(vec![native::typed::call("element", vec![col("descriptor")]).alias("result")]).unwrap().build().unwrap();
        let specialized = specialize(plan).unwrap();
        let sql = super::super::unparse::unparse_plan(specialized, SqlDialect::Custom(&TRINO)).unwrap();
        let mut stream = trino.execute(&sql, vec![]).await.unwrap();
        let mut batches = vec![];
        while let Some(batch) = stream.next().await { batches.push(batch.unwrap_or_else(|error| panic!("{value:?}: {error}: {sql}"))); }
        assert_eq!(batches.iter().map(|batch|batch.num_rows()).sum::<usize>(), 1);
        let actual = crate::execution::graph_values::decode_array(batches[0].column(0).clone()).unwrap();
        let schema = datafusion::common::DFSchema::empty();
        let physical = datafusion_physical_expr::create_physical_expr(&descriptor, &schema, &datafusion::logical_expr::execution_props::ExecutionProps::new()).unwrap();
        let empty = arrow::array::RecordBatch::try_new_with_options(Arc::new(arrow::datatypes::Schema::empty()), vec![], &arrow::array::RecordBatchOptions::new().with_row_count(Some(1))).unwrap();
        let expected_descriptor = physical.evaluate(&empty).unwrap().into_array(1).unwrap();
        let expected = native::typed::evaluate("element", &[expected_descriptor], 1).unwrap();
        assert_eq!(ScalarValue::try_from_array(&actual,0).unwrap(), ScalarValue::try_from_array(&expected,0).unwrap(), "{value:?}");
    }
}

#[cfg(feature = "trino")]
#[tokio::test]
#[ignore = "requires Trino"]
async fn trino_typed_string_keys_match_native_byte_framing() {
    use datafusion::logical_expr::{LogicalPlanBuilder, col, lit};
    use crate::ir::rel::native_values as native;
    use crate::execution::trino::{TrinoOptions, TrinoSession};
    use futures::StreamExt;
    let endpoint = std::env::var("ORCHIDDB_TEST_OPA_URL").expect("Trino URL");
    let trino = TrinoSession::source(TrinoOptions::new(endpoint, "orchid_test")).unwrap();
    for text in [None, Some(""), Some("雪🙂'"), Some("a\0b")] {
        for value in [ScalarValue::Utf8(text.map(str::to_owned)), ScalarValue::LargeUtf8(text.map(str::to_owned)), ScalarValue::Utf8View(text.map(str::to_owned))] {
            let input = LogicalPlanBuilder::empty(true).project(vec![lit(value.clone()).alias("value")]).unwrap().build().unwrap();
            let plan = LogicalPlanBuilder::from(input).project(vec![native::typed::call("key", vec![col("value")]).alias("key")]).unwrap().build().unwrap();
            let sql = super::super::unparse::unparse_plan(specialize(plan).unwrap(), SqlDialect::Custom(&TRINO)).unwrap();
            let mut stream = trino.execute(&sql, vec![]).await.unwrap();
            let mut rows = vec![];
            while let Some(batch) = stream.next().await {
                let batch = batch.unwrap_or_else(|error| panic!("{value:?}: {error}: {sql}"));
                for row in 0..batch.num_rows() { rows.push(ScalarValue::try_from_array(batch.column(0),row).unwrap()); }
            }
            let expected = native::typed::evaluate("key", &[value.to_array().unwrap()], 1).unwrap();
            assert_eq!(rows, vec![ScalarValue::try_from_array(&expected,0).unwrap()], "{value:?}");
        }
    }
}

#[cfg(feature = "trino")]
#[tokio::test]
#[ignore = "requires Trino"]
async fn trino_typed_path_seeds_and_appends_preserve_order_and_native_encoding() {
    use std::sync::Arc;
    use datafusion::logical_expr::{LogicalPlanBuilder, lit, Expr};
    use datafusion::functions::core::expr_fn::named_struct;
    use crate::ir::{rel::native_values as native, value::Value};
    use crate::execution::trino::{TrinoOptions, TrinoSession};
    use futures::StreamExt;
    let evaluate = |expr: &Expr| {
        let schema = datafusion::common::DFSchema::empty();
        let physical = datafusion_physical_expr::create_physical_expr(expr, &schema, &datafusion::logical_expr::execution_props::ExecutionProps::new()).unwrap();
        let batch = arrow::array::RecordBatch::try_new_with_options(Arc::new(arrow::datatypes::Schema::empty()), vec![], &arrow::array::RecordBatchOptions::new().with_row_count(Some(1))).unwrap();
        physical.evaluate(&batch).unwrap().into_array(1).unwrap()
    };
    let endpoint = std::env::var("ORCHIDDB_TEST_OPA_URL").expect("Trino URL");
    let trino = TrinoSession::source(TrinoOptions::new(endpoint, "orchid_test")).unwrap();
    for literal_seed in [false, true] {
        for ids in [vec![], vec![7_i64], vec![7,8,7]] {
            let mut expression = native::literal(&Value::Path(vec![])).unwrap();
            if literal_seed { expression = lit(ScalarValue::try_from_array(&evaluate(&expression),0).unwrap()); }
            for id in &ids {
                let descriptor = named_struct(vec![lit("kind"),lit("node"),lit("id"),lit(*id),lit("label"),lit("Person"),lit("source_keys"),lit(true),lit("properties"),named_struct(vec![lit("score"),lit(id*10)])]);
                expression = native::typed::call("path_append",vec![expression,native::typed::call("element",vec![descriptor])]);
            }
            let expected = evaluate(&expression);
            let plan = LogicalPlanBuilder::empty(true).project(vec![expression.alias("path")]).unwrap().build().unwrap();
            let sql = super::super::unparse::unparse_plan(specialize(plan).unwrap(), SqlDialect::Custom(&TRINO)).unwrap();
            let mut stream = trino.execute(&sql,vec![]).await.unwrap();
            let mut actual = vec![];
            while let Some(batch) = stream.next().await {
                let batch = batch.unwrap_or_else(|error|panic!("{ids:?}/{literal_seed}: {error}: {sql}"));
                let decoded = crate::execution::graph_values::decode_array(batch.column(0).clone()).unwrap();
                for row in 0..decoded.len() { actual.push(ScalarValue::try_from_array(&decoded,row).unwrap()); }
            }
            assert_eq!(actual,vec![ScalarValue::try_from_array(&expected,0).unwrap()],"{ids:?}/{literal_seed}");
        }
    }
}

#[test]
fn trino_typed_graph_transport_declines_maps_and_unions_without_stringifying_them() {
    use std::sync::Arc;
    use arrow::datatypes::{Field, Schema, UnionFields, UnionMode};
    use datafusion::logical_expr::{LogicalPlan, LogicalPlanBuilder, col};
    let key_value = DataType::Struct(vec![Field::new("key",DataType::Utf8,false),Field::new("value",DataType::Int64,true)].into());
    for kind in [DataType::Map(Arc::new(Field::new("entries",key_value,false)),false), DataType::Union(UnionFields::try_new([0], [Field::new("integer",DataType::Int64,true)]).unwrap(),UnionMode::Dense)] {
        let descriptor = DataType::Struct(vec![
            Field::new("kind",DataType::Utf8,false), Field::new("id",DataType::Int64,false),
            Field::new("label",DataType::Utf8,false), Field::new("source_keys",DataType::Boolean,false),
            Field::new("properties",DataType::Struct(vec![Field::new("payload",kind.clone(),true)].into()),false),
        ].into());
        let input = LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {produce_one_row:false,schema:Arc::new(datafusion::common::DFSchema::try_from(Schema::new(vec![Field::new("descriptor",descriptor,true)])).unwrap())});
        let plan = LogicalPlanBuilder::from(input).project(vec![crate::ir::rel::native_values::typed::call("element",vec![col("descriptor")]).alias("value")]).unwrap().build().unwrap();
        let error = specialize(plan).unwrap_err().to_string();
        assert!(error.contains("Trino graph transport"),"{kind}: {error}");
    }
}

#[test]
fn trino_typed_specialization_preserves_non_path_native_constants_and_aliases() {
    use std::sync::Arc;
    use crate::ir::{rel::native_values as native, value::Value};
    use datafusion::logical_expr::{LogicalPlanBuilder, lit, Expr};
    use datafusion::logical_expr::simplify::SimplifyContext;
    use datafusion::optimizer::simplify_expressions::ExprSimplifier;
    let schema = Arc::new(datafusion::common::DFSchema::empty());
    let simplifier = ExprSimplifier::new(SimplifyContext::default().with_schema(schema.clone()));
    for value in [Value::Null, Value::Long(7), Value::String("雪".into()), Value::List(vec![Value::Long(1),Value::Null])] {
        let expression = native::literal(&value).unwrap();
        let literal = simplifier.simplify(simplifier.coerce(expression,&schema).unwrap()).unwrap();
        let Expr::Literal(scalar,_) = &literal else { panic!("native literal expected: {literal:?}") };
        let plan = LogicalPlanBuilder::empty(true).project(vec![literal.clone().alias("user.alias"),lit(9_i64).alias("ordinary")]).unwrap().build().unwrap();
        let transformed = specialize(plan.clone()).unwrap();
        assert_eq!(transformed,plan,"non-path native constants need no transport rewrite: {value:?}");
        assert_eq!(transformed.schema().field(0).name(),"user.alias");
        assert_eq!(transformed.schema().field(0).data_type(),&scalar.data_type());
    }
}
