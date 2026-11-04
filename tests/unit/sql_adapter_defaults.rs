//! Unimplemented adapter capabilities must fail closed without changing SQL.
use super::*;
use std::sync::Arc;
use arrow::datatypes::Schema;
use datafusion::logical_expr::LogicalPlanBuilder;
use datafusion::sql::{sqlparser::{dialect::GenericDialect,parser::Parser},unparser::dialect::DefaultDialect};
#[derive(Debug)]
struct Minimal(u8);
static FIRST:Minimal=Minimal(1);
static SECOND:Minimal=Minimal(2);
impl DialectAdapter for Minimal {
    fn name(&self)->&'static str {"coverage_minimal_adapter"}
    fn parser_dialect(&self)->Box<dyn ParserDialect>{Box::new(GenericDialect{})}
    fn unparser_dialect(&self)->Box<dyn UnparserDialect>{Box::new(DefaultDialect{})}
    fn sql_type(&self,_:&DataType)->SqlResult<String>{Ok("BIGINT".into())}
}
#[test]
fn default_adapter_rejects_undeclared_value_function_and_subquery_capabilities() {
    let dialect=SqlDialect::Custom(&FIRST);
    let error=FIRST.exchange_literal(&ScalarValue::Int64(Some(7)),&DataType::Int64).unwrap_err().to_string();
    assert!(error.contains("no literal codec for Int64"),"{error}");
    assert!(!dialect.supports_scalar_function("unknown"));
    assert!(dialect.lower_scalar_function("unknown",&[]).unwrap().is_none());
    assert!(FIRST.function_mapping("unknown").is_none());
    let mut expression=Parser::new(&GenericDialect{}).try_with_sql("unknown(7)").unwrap().parse_expr().unwrap();
    let ast::Expr::Function(function)=&expression else {panic!()};
    assert!(dialect.lower_function(function).unwrap().is_none());
    assert!(FIRST.rewrite_expression(&mut expression).unwrap_err().to_string().contains("no mapping for function unknown"));
    let mut expression=Parser::new(&GenericDialect{}).try_with_sql("7 + 2").unwrap().parse_expr().unwrap();
    let original=expression.clone();
    FIRST.rewrite_expression(&mut expression).unwrap();
    assert_eq!(expression,original);
    for kind in [super::super::SubqueryKind::Scalar,super::super::SubqueryKind::Exists,
        super::super::SubqueryKind::In,super::super::SubqueryKind::SetComparison] {
        let requirements=super::super::SubqueryRequirements{kind,correlated:false,unnest:false,aggregate:false,window:false,correlated_limit:false,output_types:vec![DataType::Int64]};
        assert_eq!(FIRST.check_subquery(&requirements),Err("adapter has no implementation for this subquery shape"));
    }
}
#[test]
fn default_adapter_preserves_ast_and_leaves_source_and_relation_binding_unclaimed() {
    let dialect=SqlDialect::Custom(&FIRST);
    assert!(!dialect.requires_scope_repair());
    assert_eq!(FIRST.create_table_keyword(),"CREATE TEMPORARY TABLE");
    assert_eq!(FIRST.double_type(),"DOUBLE PRECISION");
    assert_eq!(FIRST.finalize_sql("SELECT 1".into()).unwrap(),"SELECT 1");
    for (name,expected) in [("ordinary","ordinary"),("select","\"select\""),("a\"b","\"a\"\"b\"")] {
        assert_eq!(FIRST.quote_ident(name),expected);
        assert_eq!(dialect.identifier(name).to_string(),expected);
    }
    let metadata=crate::ir::rel::source_metadata::SourceMetadata{table:"people".into(),format:None,options:BTreeMap::new(),indexes:vec![]};
    let schema=Arc::new(Schema::empty());
    assert!(FIRST.source_provider(&metadata,schema.clone()).unwrap().is_none());
    assert!(FIRST.bind_source_arguments(&metadata,&BTreeMap::new(),schema).unwrap().is_none());
    let plan=LogicalPlanBuilder::empty(true).build().unwrap();
    assert!(FIRST.relation_placement(&plan).is_none());
    let context=super::super::lowering::LoweringContext{dialect,mode:super::super::lowering::LoweringMode::InIsland};
    assert!(dialect.lower_relation(&plan,&context).unwrap().is_none());
    let mut statements=Parser::parse_sql(&GenericDialect{},"WITH x AS (SELECT 1) SELECT * FROM x").unwrap();
    let ast::Statement::Query(query)=&mut statements[0] else {panic!()};
    let original=query.clone();
    dialect.rewrite_input_binding(&mut query.with.as_mut().unwrap().cte_tables[0]).unwrap();
    FIRST.rewrite_query(query).unwrap();
    assert_eq!(*query,original);
}
#[test]
fn registering_another_instance_cannot_replace_an_existing_adapter() {
    assert_eq!(FIRST.0,1);assert_eq!(SECOND.0,2);
    SqlDialect::register(&FIRST).unwrap();
    SqlDialect::register(&FIRST).unwrap();
    assert!(SqlDialect::register(&SECOND).unwrap_err().to_string().contains("already registered"));
}
