use super::*;

fn context(graph: &PropertyGraph) -> LoweringContext<'_> {
    LoweringContext {
        island_memo:None,graph,options:RelBackendOptions {native_values:true,..Default::default()},
        policy:GraphPlanPolicy::cypher(),language:Language::Cypher,scan_counter:0,correlate_plan:None,
        rdf_typed_terms_used:false,scan_logical_labels:false,gremlin_label_binds:Default::default(),
        unordered_collects:Default::default(),unordered_distincts:Default::default(),
    }
}
fn scope(element: DataType) -> LogicalPlan {
    LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
        produce_one_row:false,
        schema:Arc::new(datafusion::common::DFSchema::try_from(Schema::new(vec![
            Field::new("xs",DataType::List(Arc::new(Field::new("item",element,true))),true),
            Field::new("offset",DataType::Int64,true),
        ])).unwrap()),
    })
}
fn transform(body: IrExpr) -> IrExpr {
    IrExpr::ListTransform {list:Box::new(IrExpr::binding("xs")),item:"item".into(),map:Box::new(body)}
}
fn call(name: &str,args: Vec<IrExpr>) -> IrExpr {IrExpr::Call {name:name.into(),args}}

#[test]
fn lambda_capture_is_deduplicated_and_parameter_is_not_captured() {
    let graph=PropertyGraph::new(); let context=context(&graph); let plan=scope(DataType::Int64);
    let sum=|lhs,rhs| IrExpr::Binary {op:BinaryOp::Add,lhs:Box::new(lhs),rhs:Box::new(rhs)};
    let body=sum(IrExpr::binding("item"),sum(IrExpr::binding("offset"),IrExpr::binding("offset")));
    let lowered=context.lower_sql_lambda(&plan,&transform(body)).unwrap();
    let Expr::ScalarFunction(call)=lowered else {panic!("expected native lambda")};
    let lambda=definition(&call.func).unwrap();
    assert_eq!(lambda.kind,"transform"); assert_eq!(call.args.len(),2);
    assert_eq!(lambda.captures.len(),1);
    assert_ne!(lambda.parameter,lambda.captures[0]);
    assert_eq!(call.args[1],col_exact("offset"));
    assert_eq!(lambda.return_type(&[]).unwrap(),DataType::List(Arc::new(Field::new("item",DataType::Int64,true))));
    assert_eq!(lambda.signature().volatility,Volatility::Volatile);
    // Compiler-only lambdas cannot be evaluated as ordinary scalar UDFs.
    let error=lambda.invoke_with_args(ScalarFunctionArgs {
        args:vec![],arg_fields:vec![],number_rows:0,
        return_field:Arc::new(Field::new("result",lambda.result.clone(),true)),
        config_options:Default::default(),
    }).unwrap_err();
    assert!(error.to_string().contains("List lambda is emitted as native SQL"));
    assert!(lambda.body.to_string().contains(&lambda.parameter));
    assert!(lambda.body.to_string().contains(&lambda.captures[0]));
}

#[test]
fn list_consumer_guards_preserve_unsupported_and_semantic_errors() {
    let graph=PropertyGraph::new(); let context=context(&graph); let plan=scope(DataType::Int64);
    for expr in [IrExpr::binding("xs"),call("size",vec![]),call("size",vec![IrExpr::binding("xs"),IrExpr::Lit(Lit::Int(1))]),call("unrelated",vec![transform(IrExpr::binding("item"))])] {
        assert!(context.lower_list_consumer(&plan,&expr).unwrap().is_none());
    }
    assert!(context.lower_sql_lambda(&plan,&IrExpr::binding("xs")).unwrap_err().to_string().contains("Expected list lambda"));
    let missing=transform(IrExpr::binding("missing"));
    let ordinary=context.lower_expr(&plan,&missing).unwrap_err().to_string();
    let consumer=context.lower_typed_list_input(&plan,&missing).unwrap_err().to_string();
    assert_eq!(ordinary,consumer);
    let invalid_sum=call("list_sum",vec![IrExpr::binding("xs")]);
    for element in [DataType::Boolean,DataType::Utf8,DataType::UInt64] {
        let error=context.lower_sql_lambda(&scope(element),&invalid_sum).unwrap_err().to_string();
        assert!(error.contains("list_sum requires a signed integer or floating-point list"),"{error}");
    }
}
