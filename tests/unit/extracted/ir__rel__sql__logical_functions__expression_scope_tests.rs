
    use super::*;

    fn list_literal(value: i64) -> (Expr, String) {
        let scalar = ScalarValue::List(ScalarValue::new_list_nullable(
            &[ScalarValue::Int64(Some(value))], &DataType::Int64,
        ));
        let expr = super::super::structured::literal_expr(&scalar, SqlDialect::DuckDb).unwrap();
        let Expr::ScalarFunction(call) = &expr else { panic!("logical literal expected") };
        let name = call.func.name().to_owned();
        (expr, name)
    }

    #[test]
    fn nested_lambda_literal_mappings_restore_outer_scope() {
        let schema = datafusion::common::DFSchema::empty();
        let (outer, outer_name) = list_literal(71);
        let (inner, inner_name) = list_literal(93);
        assert!(mapping(&outer_name, 0).is_none());
        with_expression(&outer, &schema, SqlDialect::DuckDb, || {
            let expected = mapping(&outer_name, 0).unwrap();
            with_expression(&inner, &schema, SqlDialect::DuckDb, || {
                assert_eq!(mapping(&outer_name, 0), Some(expected.clone()));
                assert!(mapping(&inner_name, 0).is_some());
                Ok(())
            })?;
            assert_eq!(mapping(&outer_name, 0), Some(expected));
            assert!(mapping(&inner_name, 0).is_none());
            Ok(())
        }).unwrap();
        assert!(mapping(&outer_name, 0).is_none());
    }

    #[test]
    fn failed_lambda_emission_does_not_leak_literal_mappings() {
        let schema = datafusion::common::DFSchema::empty();
        let (expr, name) = list_literal(59);
        let error = with_expression(&expr, &schema, SqlDialect::DuckDb, || {
            assert!(mapping(&name, 0).is_some());
            Err::<(), _>(SqlError::Unsupported("failed body".into()))
        }).unwrap_err();
        assert!(error.to_string().contains("failed body"));
        assert!(mapping(&name, 0).is_none());
    }

fn score(name:&str,sql:&str,reverse:bool,dialect:SqlDialect)->(Expr,String) {
    use crate::ir::functions::logical::{LogicalFunction,SqlOrderingMapping};
    use datafusion::logical_expr::{create_udf,Volatility};
    let native=create_udf("native_score",vec![DataType::Int64],DataType::Int64,Volatility::Immutable,
        Arc::new(|args|Ok(args[0].clone())));
    let udf=LogicalFunction::new(name,Arc::new(native),BTreeMap::from([(dialect.name().into(),SqlFunctionMapping{
        value:sql.into(),ordering:Some(SqlOrderingMapping{expression:"-__arg0".into(),reverse})})])).into_udf();
    let name=udf.name().to_owned();(udf.call(vec![lit(7_i64)]),name)
}
fn statement(sql:&str)->datafusion::sql::sqlparser::ast::Statement {
    datafusion::sql::sqlparser::parser::Parser::parse_sql(&datafusion::sql::sqlparser::dialect::GenericDialect{},sql).unwrap().remove(0)
}

#[test]
fn logical_ordering_preserves_null_policy_and_reverses_only_bare_score_keys() {
    for dialect in [SqlDialect::DuckDb,SqlDialect::Postgres,SqlDialect::Custom(&super::super::trino::TRINO)] {
        for reverse in [false,true] {
            let (expr,name)=score("ordering_contract","__arg0",reverse,dialect);
            with_expression(&expr,&datafusion::common::DFSchema::empty(),dialect,|| {
                for (direction,reversed) in [(""," DESC"),(" ASC"," DESC"),(" DESC"," ASC")] {
                    for nulls in [""," NULLS FIRST"," NULLS LAST"] {
                        let suffix=if reverse {reversed} else {direction};
                        let mut query=statement(&format!("SELECT row_number() OVER (ORDER BY {name}(v){direction}{nulls}) FROM t ORDER BY \"{name}\"(v){direction}{nulls}, {name}(v) + 1 ASC"));
                        adapt_ordering(&mut query,dialect)?;
                        let expected=statement(&format!("SELECT row_number() OVER (ORDER BY (-(v)){suffix}{nulls}) FROM t ORDER BY (-(v)){suffix}{nulls}, {name}(v) + 1 ASC"));
                        assert_eq!(query,expected,"{} reverse={reverse}",dialect.name());
                    }
                }
                for sql in [format!("SELECT 1 ORDER BY missing(v)"),format!("SELECT 1 ORDER BY {name}()"),format!("SELECT 1 ORDER BY {name}(v,w)")] {
                    let mut query=statement(&sql);let original=query.clone();adapt_ordering(&mut query,dialect)?;assert_eq!(query,original);
                }
                for sql in [format!("SELECT 1 ORDER BY {name}(*)"),format!("SELECT row_number() OVER (ORDER BY {name}(*))")] {
                    let mut query=statement(&sql);assert!(adapt_ordering(&mut query,dialect).unwrap_err().to_string().contains("invalid logical ordering arguments"));
                }
                Ok(())
            }).unwrap();
            assert!(mapping(&name,1).is_none());
        }
    }
}

#[test]
fn conflicting_function_mappings_fail_before_replacing_outer_scope() {
    let dialect=SqlDialect::DuckDb;
    let (outer,name)=score("same_identity","__arg0",false,dialect);
    let (conflicting,_)=score("same_identity","__arg0 + 1",false,dialect);
    let schema=datafusion::common::DFSchema::empty();
    with_expression(&outer,&schema,dialect,|| {
        let expected=mapping(&name,1).unwrap();
        let mut called=false;
        let error=with_expression(&conflicting,&schema,dialect,||{called=true;Ok(())}).unwrap_err();
        assert!(error.to_string().contains("conflicting logical function definitions"));assert!(!called);
        assert_eq!(mapping(&name,1),Some(expected.clone()));
        let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).project(vec![outer.clone().alias("a"),conflicting.clone().alias("b")])?.build()?;
        let error=with_plan(&plan,dialect,||{called=true;Ok(())}).unwrap_err();
        assert!(error.to_string().contains("conflicting logical function definitions"));assert!(!called);
        assert_eq!(mapping(&name,1),Some(expected));
        Ok(())
    }).unwrap();
    assert!(mapping(&name,1).is_none());
}

#[test]
fn plan_and_expression_mapping_scopes_restore_after_unwind_and_error() {
    let dialect=SqlDialect::DuckDb;
    let schema=datafusion::common::DFSchema::empty();
    let (outer,outer_name)=score("outer_scope","__arg0",false,dialect);
    let (inner,inner_name)=score("inner_scope","__arg0",false,dialect);
    let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).project(vec![inner.clone().alias("value")]).unwrap().build().unwrap();
    with_expression(&outer,&schema,dialect,|| {
        let expected=mapping(&outer_name,1).unwrap();
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||with_plan(&plan,dialect,|| -> SqlResult<()> {
            assert!(mapping(&outer_name,1).is_none());assert!(mapping(&inner_name,1).is_some());panic!("intentional emission failure")
        })));
        assert!(result.is_err());assert_eq!(mapping(&outer_name,1),Some(expected.clone()));assert!(mapping(&inner_name,1).is_none());
        let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(||with_expression(&inner,&schema,dialect,|| -> SqlResult<()> {
            assert!(mapping(&outer_name,1).is_some());assert!(mapping(&inner_name,1).is_some());panic!("intentional lambda failure")
        })));
        assert!(result.is_err());assert_eq!(mapping(&outer_name,1),Some(expected.clone()));assert!(mapping(&inner_name,1).is_none());
        let error=with_plan(&plan,dialect,||Err::<(),_>(SqlError::Unsupported("failed plan emission".into()))).unwrap_err();
        assert!(error.to_string().contains("failed plan emission"));assert_eq!(mapping(&outer_name,1),Some(expected));assert!(mapping(&inner_name,1).is_none());
        Ok(())
    }).unwrap();
    assert!(mapping(&outer_name,1).is_none());
}

#[derive(Debug)]
struct MappingOverride;
static MAPPING_OVERRIDE:MappingOverride=MappingOverride;
impl super::super::DialectAdapter for MappingOverride {
    fn name(&self)->&'static str {"coverage_scope_override"}
    fn parser_dialect(&self)->Box<dyn datafusion::sql::sqlparser::dialect::Dialect> {
        Box::new(datafusion::sql::sqlparser::dialect::GenericDialect{})
    }
    fn unparser_dialect(&self)->Box<dyn datafusion::sql::unparser::dialect::Dialect> {
        Box::new(datafusion::sql::unparser::dialect::DefaultDialect{})
    }
    fn sql_type(&self,_:&DataType)->SqlResult<String>{Ok("BIGINT".into())}
    fn function_mapping(&self,name:&str)->Option<SqlFunctionMapping> {
        (name=="overridden_score").then(||SqlFunctionMapping{value:"__arg0 * 2".into(),ordering:None})
    }
}

#[test]
fn adapter_overrides_take_precedence_and_identical_plan_definitions_coexist() {
    let dialect=SqlDialect::Custom(&MAPPING_OVERRIDE);
    let schema=datafusion::common::DFSchema::empty();
    let (first,name)=score("overridden_score","__arg0 + 1",true,dialect);
    let (second,_)=score("overridden_score","__arg0 + 3",false,dialect);
    let expected=SqlFunctionMapping{value:"__arg0 * 2".into(),ordering:None};
    let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).project(vec![first.clone().alias("a"),second.clone().alias("b")]).unwrap().build().unwrap();
    with_plan(&plan,dialect,|| {
        assert_eq!(mapping(&name,1),Some(expected.clone()));
        with_expression(&second,&schema,dialect,|| {
            assert_eq!(mapping(&name,1),Some(expected.clone()));
            let mut query=statement(&format!("SELECT 1 ORDER BY {name}(v) DESC NULLS FIRST"));let original=query.clone();
            adapt_ordering(&mut query,dialect)?;assert_eq!(query,original);
            Ok(())
        })?;
        Ok(())
    }).unwrap();
    assert!(mapping(&name,1).is_none());
    let dialect=SqlDialect::DuckDb;
    let (same,name)=score("repeated_score","__arg0",false,dialect);
    let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).project(vec![same.clone().alias("a"),same.alias("b")]).unwrap().build().unwrap();
    with_plan(&plan,dialect,||{assert!(mapping(&name,1).is_some());Ok(())}).unwrap();
    assert!(mapping(&name,1).is_none());
}

#[test]
fn unavailable_implementations_do_not_register_or_leak_across_plan_scopes() {
    let (expr,name)=score("postgres_only_score","__arg0",false,SqlDialect::Postgres);
    let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).project(vec![expr.clone().alias("value")]).unwrap().build().unwrap();
    let schema=datafusion::common::DFSchema::empty();
    for dialect in [SqlDialect::DuckDb,SqlDialect::Custom(&MAPPING_OVERRIDE)] {
        with_plan(&plan,dialect,||{assert!(mapping(&name,1).is_none());Ok(())}).unwrap();
        with_expression(&expr,&schema,dialect,||{assert!(mapping(&name,1).is_none());Ok(())}).unwrap();
    }
    with_plan(&plan,SqlDialect::Postgres,||{
        assert!(mapping(&name,1).is_some());assert!(mapping(&name,0).is_none());assert!(mapping(&name,2).is_none());
        Ok(())
    }).unwrap();
    assert!(mapping(&name,1).is_none());
}
