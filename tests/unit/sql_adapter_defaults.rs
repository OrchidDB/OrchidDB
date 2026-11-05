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

#[derive(Debug)]
struct RelationProtocol(u8);
static RELATION_PROTOCOLS: [RelationProtocol;9] = [RelationProtocol(0),RelationProtocol(1),RelationProtocol(2),RelationProtocol(3),RelationProtocol(4),RelationProtocol(5),RelationProtocol(6),RelationProtocol(7),RelationProtocol(8)];
impl DialectAdapter for RelationProtocol {
    fn name(&self)->&'static str {"coverage_relation_protocol"}
    fn parser_dialect(&self)->Box<dyn ParserDialect>{Box::new(GenericDialect{})}
    fn unparser_dialect(&self)->Box<dyn UnparserDialect>{Box::new(DefaultDialect{})}
    fn sql_type(&self,_:&DataType)->SqlResult<String>{Ok("BIGINT".into())}
    fn lower_relation(&self,plan:&LogicalPlan,context:&super::super::lowering::LoweringContext)->SqlResult<Option<super::super::lowering::RelationLowering>> {
        use super::super::lowering::{RelationLowering,SqlTemplate};
        Ok(Some(match self.0 {
            0 => RelationLowering::Sql(Parser::parse_sql(&GenericDialect{},"DELETE FROM forbidden").unwrap().remove(0)),
            1 => RelationLowering::Rewrite(LogicalPlanBuilder::empty(false).build().unwrap()),
            2..=4 | 6 => RelationLowering::Dependent {
                source:Arc::new(plan.clone()),
                template:SqlTemplate{sql:"SELECT $1 AS id".into(),parameters:plan.schema().fields().len()+usize::from(self.0==3),dialect:if self.0==4 {"wrong_engine"} else {context.dialect.name()}.into()},
                schema:if self.0==2 {Arc::new(datafusion::common::DFSchema::empty())} else {plan.schema().clone()},
            },
            5 => RelationLowering::Rewrite(plan.clone()),
            7 => RelationLowering::Sql(Parser::parse_sql(&GenericDialect{},"SELECT 7 AS id").unwrap().remove(0)),
            _ => return Ok(None),
        }))
    }
}
fn protocol_plan()->LogicalPlan {
    LogicalPlanBuilder::empty(true).project(vec![datafusion::prelude::lit(7i64).alias("id")]).unwrap().build().unwrap()
}

#[test]
fn relational_adapter_validation_rejects_statement_schema_parameter_and_engine_changes() {
    use super::super::lowering::{lower_relation,LoweringContext,LoweringMode,RelationLowering};
    let plan=protocol_plan();
    for (index,message) in [(0,"query statement"),(1,"rewrite changed the output schema"),(2,"dependent lowering changed the output schema"),(3,"parameter count must match source columns"),(4,"changed owning engine dialect")] {
        for mode in [LoweringMode::InIsland,LoweringMode::Bound] {
            let context=LoweringContext{dialect:SqlDialect::Custom(&RELATION_PROTOCOLS[index]),mode};
            let error=lower_relation(&plan,&context).unwrap_err().to_string();
            assert!(error.contains(message),"{index}: {error}");
        }
    }
    for index in 5..9 {
        let context=LoweringContext{dialect:SqlDialect::Custom(&RELATION_PROTOCOLS[index]),mode:LoweringMode::InIsland};
        match (index,lower_relation(&plan,&context).unwrap()) {
            (5,Some(RelationLowering::Rewrite(replacement)))=>assert_eq!(replacement,plan),
            (6,Some(RelationLowering::Dependent{source,template,schema}))=>{
                assert_eq!(*source,plan);assert_eq!(schema,plan.schema().clone());assert_eq!(template.parameters,1);
            },
            (7,Some(RelationLowering::Sql(ast::Statement::Query(_)))) | (8,None)=>{},
            other=>panic!("unexpected lowering {other:?}"),
        }
    }
}

#[test]
fn physical_sql_relations_remain_opaque_and_reject_acquiring_inputs_or_expressions() {
    use super::super::lowering::{transform_relations,rewrite_relations};
    let plan=protocol_plan();
    let dialect=SqlDialect::Custom(&RELATION_PROTOCOLS[7]);
    let transformed=transform_relations(plan.clone(),dialect).unwrap();
    assert_eq!(transformed.schema(),plan.schema());
    let LogicalPlan::Extension(extension)=&transformed else {panic!("expected adapter SQL boundary")};
    assert_eq!(extension.node.name(),"SqlRelation");
    assert!(extension.node.inputs().is_empty());assert!(extension.node.expressions().is_empty());
    assert!(transformed.display_indent().to_string().contains("SELECT 7 AS id"));
    let rebuilt=extension.node.with_exprs_and_inputs(vec![],vec![]).unwrap();
    assert_eq!(rebuilt.schema(),extension.node.schema());
    for (expressions,inputs) in [(vec![datafusion::prelude::lit(1i64)],vec![]),(vec![],vec![plan.clone()])] {
        assert!(extension.node.with_exprs_and_inputs(expressions,inputs).unwrap_err().to_string().contains("cannot acquire inputs"));
    }
    assert_eq!(transform_relations(transformed.clone(),dialect).unwrap(),transformed);
    assert_eq!(rewrite_relations(plan.clone(),SqlDialect::Custom(&RELATION_PROTOCOLS[5])).unwrap(),plan);
    assert_eq!(rewrite_relations(plan.clone(),SqlDialect::Postgres).unwrap(),plan);
    assert!(transform_relations(plan,SqlDialect::Custom(&RELATION_PROTOCOLS[6])).unwrap_err().to_string().contains("requires dependent placement"));
}

#[test]
fn prepared_sql_templates_reject_invalid_statements_and_placeholders_without_reusing_values() {
    use super::super::lowering::SqlTemplate;
    for dialect in [SqlDialect::Postgres,SqlDialect::DuckDb,SqlDialect::Custom(&super::super::trino::TRINO)] {
        for (sql,message) in [("SELECT 1; SELECT 2","one statement"),("DELETE FROM x","query statement"),("","one statement")] {
            let template=SqlTemplate{sql:sql.into(),parameters:0,dialect:dialect.name().into()};
            assert!(template.bind(&[]).unwrap_err().to_string().contains(message));
        }
        for placeholder in ["$0","$2","$184467440737095516160000"] {
            let template=SqlTemplate{sql:format!("SELECT {placeholder}"),parameters:1,dialect:dialect.name().into()};
            assert!(template.bind(&[ScalarValue::Int64(Some(7))]).unwrap_err().to_string().contains("invalid SQL parameter"));
        }
        let template=SqlTemplate{sql:"SELECT $1 AS first, $1 AS second, '$1' AS literal".into(),parameters:1,dialect:dialect.name().into()};
        let prepared=template.prepare().unwrap();
        assert!(prepared.bind(&[]).unwrap_err().to_string().contains("parameter count mismatch"));
        let a=prepared.bind(&[ScalarValue::Int64(Some(7))]).unwrap();
        let b=prepared.bind(&[ScalarValue::Int64(Some(9))]).unwrap();
        for (sql,number) in [(&a,"7"),(&b,"9")] {
            let parser=dialect.parser_dialect();
            let ast::Statement::Query(query)=Parser::parse_sql(parser.as_ref(),sql).unwrap().remove(0) else {panic!("query")};
            let ast::SetExpr::Select(select)=*query.body else {panic!("select")};
            assert_eq!(select.projection.len(),3);
            for (index,item) in select.projection.iter().enumerate() {
                let ast::SelectItem::ExprWithAlias{expr,alias}=item else {panic!("named projection")};
                assert_eq!(alias.value,["first","second","literal"][index]);
                let mut value=expr;
                while let ast::Expr::Cast{expr,..}=value {value=expr;}
                let ast::Expr::Value(value)=value else {panic!("bound literal {value:?}")};
                let expected=if index==2 {ast::Value::SingleQuotedString("$1".into())} else {ast::Value::Number(number.into(),false)};
                assert_eq!(value.value,expected,"{sql}");
            }
        }
        assert_eq!(prepared.bind(&[ScalarValue::Int64(Some(7))]).unwrap(),a);
        assert!(template.bind(&[]).unwrap_err().to_string().contains("parameter count mismatch"));
    }
}
