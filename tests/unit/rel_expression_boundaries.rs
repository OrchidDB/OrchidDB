use super::*;
use crate::ir::policy::{PropertyMissing, OptionalMissing};
use crate::ir::plan::ProjectErrorPolicy;

#[test]
fn collection_type_unification_widens_only_compatible_integer_domains() {
    for left in [DataType::Int8, DataType::Int16, DataType::Int32, DataType::Int64] {
        for right in [DataType::Int8, DataType::Int16, DataType::Int32, DataType::Int64] {
            let expected = if left == right { left.clone() } else { DataType::Int64 };
            assert_eq!(nullable_collection_type(&left, &right), Some(expected));
        }
    }
    let metadata = std::collections::HashMap::from([("source".into(), "left".into())]);
    let left_field = Field::new("item", DataType::Int8, false).with_metadata(metadata.clone());
    let left = DataType::List(Arc::new(left_field));
    let right = DataType::List(Arc::new(Field::new("other", DataType::Int32, true)));
    assert_eq!(nullable_collection_type(&left, &right), Some(DataType::List(Arc::new(
        Field::new("item", DataType::Int64, true).with_metadata(metadata)))));
    for kind in [DataType::Boolean, DataType::Date32, left.clone(), DataType::Struct(vec![Field::new("x", DataType::Utf8, true)].into())] {
        assert_eq!(nullable_collection_type(&DataType::Null, &kind), Some(kind.clone()));
        assert_eq!(nullable_collection_type(&kind, &DataType::Null), Some(kind));
    }
    for (left, right) in [(DataType::Int64, DataType::Float64), (DataType::Int64, DataType::UInt64),
        (DataType::Utf8, DataType::Boolean),
        (left, DataType::List(Arc::new(Field::new("item", DataType::Boolean, true))))] {
        assert_eq!(nullable_collection_type(&left, &right), None);
    }
}

fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call { name: name.into(), args }
}

#[test]
fn literal_conditionals_prune_only_unreachable_arguments() {
    let poison = call("effectful", vec![]);
    let unknown = IrExpr::Binding("unknown".into());
    let null = IrExpr::Lit(Lit::Null);
    let value = IrExpr::Lit(Lit::Int(7));
    let typed_null = IrExpr::Lit(Lit::Scalar(ScalarValue::Int64(None)));
    for (name, input, expected) in [
        ("fn.nvl2", vec![typed_null.clone(), poison.clone(), value.clone()], vec![typed_null.clone(), null.clone(), value.clone()]),
        ("FN.NVL2", vec![value.clone(), value.clone(), poison.clone()], vec![value.clone(), value.clone(), null.clone()]),
        ("fn.coalesce", vec![typed_null, unknown.clone(), value.clone(), poison.clone()], vec![IrExpr::Lit(Lit::Scalar(ScalarValue::Int64(None))), unknown.clone(), value.clone(), null.clone()]),
        ("fn.nvl", vec![value.clone(), poison.clone()], vec![value.clone(), null.clone()]),
        ("fn.ifnull", vec![value.clone(), poison.clone()], vec![value.clone(), null.clone()]),
    ] {
        assert_eq!(prune_literal_conditional_branches(&call(name, input)), Some(call(name, expected)), "{name}");
    }
    for expr in [
        unknown.clone(),
        call("fn.coalesce", vec![]),
        call("fn.coalesce", vec![null.clone(), unknown.clone()]),
        call("fn.coalesce", vec![value.clone(), null.clone()]),
        call("fn.nvl2", vec![unknown.clone(), value.clone(), poison.clone()]),
        call("fn.nvl2", vec![null.clone(), poison.clone()]),
        call("fn.nvl", vec![unknown, poison.clone()]),
        call("fn.ifnull", vec![null.clone(), poison.clone()]),
        call("unrelated", vec![value, poison]),
    ] {
        assert!(prune_literal_conditional_branches(&expr).is_none(), "{expr:?}");
    }
}

fn cypher_context(graph: &PropertyGraph) -> LoweringContext<'_> {
    LoweringContext {
        island_memo: None, graph, options: RelBackendOptions::default(),
        policy: GraphPlanPolicy::cypher(), language: Language::Cypher, scan_counter: 0,
        correlate_plan: None, rdf_typed_terms_used: false, scan_logical_labels: false,
        gremlin_label_binds: Default::default(), unordered_collects: Default::default(),
        unordered_distincts: Default::default(),
    }
}
fn scalar_scope(fields: Vec<Field>) -> LogicalPlan {
    LogicalPlan::EmptyRelation(datafusion::logical_expr::EmptyRelation {
        produce_one_row: false,
        schema: Arc::new(datafusion::common::DFSchema::try_from(Schema::new(fields)).unwrap()),
    })
}
fn binding(name: &str) -> IrExpr { IrExpr::binding(name) }

#[test]
fn cypher_scalar_columns_preserve_physical_types_without_redundant_casts() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Boolean, DataType::Int8, DataType::Int16, DataType::Int32,
        DataType::Int64, DataType::UInt64, DataType::Float32, DataType::Float64,
        DataType::Utf8, DataType::Decimal128(38, 8)] {
        let plan = scalar_scope(vec![Field::new("x", kind.clone(), true)]);
        for expr in [binding("x"), call("cypher_property_value", vec![binding("x")]),
            call("cypher_order_key", vec![binding("x")])] {
            let result = ctx.lower_sql_expr(&plan, &expr).unwrap();
            assert_eq!(result, col_exact("x"), "{kind:?}: {expr:?}");
            assert_eq!(result.get_type(plan.schema()).unwrap(), kind);
        }
    }
}

#[test]
fn cypher_case_eliminates_missing_bindings_before_lowering_dead_values() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("x", DataType::Int64, true)]);
    for otherwise in [None, Some(Box::new(binding("x")))] {
        let expr = IrExpr::Case {
            arms: vec![(IrExpr::IsBound("missing".into()), binding("also_missing"))],
            otherwise: otherwise.clone(),
        };
        let lowered = ctx.lower_sql_expr(&plan, &expr).unwrap();
        assert_eq!(lowered, if otherwise.is_some() { col_exact("x") } else { lit(ScalarValue::Null) });
    }
    assert_eq!(ctx.lower_sql_expr(&plan, &IrExpr::IsBound("missing".into())).unwrap(), lit(false));
    assert_eq!(ctx.lower_sql_expr(&plan, &IrExpr::IsBound("x".into())).unwrap(), col_exact("x").is_not_null());
}

#[test]
fn cypher_case_aligns_null_branches_and_rejects_lossy_common_types() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Int64, DataType::Float64, DataType::Utf8, DataType::Boolean] {
        let plan = scalar_scope(vec![Field::new("x",kind.clone(),true),Field::new("p",DataType::Boolean,true)]);
        for null in [ScalarValue::Null, ScalarValue::Utf8(None), ScalarValue::Int64(None)] {
            let expr = IrExpr::Case { arms: vec![(binding("p"), IrExpr::Lit(Lit::Scalar(null)))], otherwise: Some(Box::new(binding("x"))) };
            let result = ctx.lower_sql_expr(&plan, &expr).unwrap();
            let Expr::Case(case) = result else { panic!("expected CASE") };
            assert_eq!(case.when_then_expr[0].1.get_type(plan.schema()).unwrap(), kind);
            assert_eq!(*case.else_expr.unwrap(), col_exact("x"));
        }
    }
    let plan = scalar_scope(vec![Field::new("n",DataType::Int64,true),Field::new("s",DataType::Utf8,true),Field::new("p",DataType::Boolean,true)]);
    let expr = IrExpr::Case { arms: vec![(binding("p"),binding("n"))], otherwise: Some(Box::new(binding("s"))) };
    assert!(ctx.lower_sql_expr(&plan,&expr).unwrap_err().to_string().contains("Heterogeneous Cypher CASE"));
}

#[test]
fn cypher_predicate_lowering_retains_boolean_and_types_null_conditions() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("p",DataType::Boolean,true)]);
    assert_eq!(ctx.lower_case_condition(&plan,&binding("p")).unwrap(),col_exact("p"));
    for null in [ScalarValue::Null, ScalarValue::Int64(None), ScalarValue::Utf8(None)] {
        assert_eq!(ctx.lower_case_condition(&plan,&IrExpr::Lit(Lit::Scalar(null))).unwrap(),lit(ScalarValue::Boolean(None)));
    }
    for expr in [IrExpr::Not(Box::new(binding("p"))),IrExpr::IsNull(Box::new(binding("p"))),IrExpr::IsNotNull(Box::new(binding("p")))] {
        assert_eq!(ctx.lower_sql_expr(&plan,&expr).unwrap().get_type(plan.schema()).unwrap(),DataType::Boolean);
    }
}

#[test]
fn cypher_nested_property_guards_require_runtime_validation() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let scalar = Arc::new(Field::new("item",DataType::Int64,true));
    for kind in [DataType::List(scalar.clone()),DataType::LargeList(scalar.clone()),DataType::FixedSizeList(scalar,2)] {
        let plan = scalar_scope(vec![Field::new("x",kind,true)]);
        assert_eq!(ctx.lower_sql_expr(&plan,&call("cypher_property_value",vec![binding("x")])).unwrap(),col_exact("x"));
    }
    let nested = DataType::Struct(vec![Field::new("k",DataType::Int64,true)].into());
    for kind in [nested.clone(),DataType::List(Arc::new(Field::new("item",nested,true)))] {
        let plan = scalar_scope(vec![Field::new("x",kind,true)]);
        assert!(ctx.lower_sql_expr(&plan,&call("cypher_property_value",vec![binding("x")])).unwrap_err().to_string().contains("property type requires runtime validation"));
    }
}

#[test]
fn cypher_unavailable_identity_and_label_inputs_have_specific_errors() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![]);
    for (expr,message) in [
        (binding("missing"),"unavailable binding"),
        (IrExpr::Id("missing".into()),"id(missing)"),
        (IrExpr::Label("missing".into()),"label(missing)"),
        (IrExpr::HasLabel {binding:"missing".into(),label:"Person".into()},"has_label"),
        (call("cypher_id",vec![binding("missing")]),"Mapped identity requires"),
        (call("cypher_labels",vec![binding("missing")]),"logical node label metadata"),
        (call("cypher_has_label",vec![binding("missing"),binding("dynamic")]),"bound element and label"),
    ] {
        assert!(ctx.lower_sql_expr(&plan,&expr).unwrap_err().to_string().contains(message),"{expr:?}");
    }
}

#[test]
fn cypher_lists_widen_integer_columns_but_never_stringify_heterogeneous_values() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Int8,DataType::Int16,DataType::Int32,DataType::Int64] {
        let plan = scalar_scope(vec![Field::new("x",kind,true),Field::new("y",DataType::Int64,true)]);
        let expr = IrExpr::List(vec![binding("x"),IrExpr::Lit(Lit::Null),binding("y")]);
        let result = ctx.lower_sql_expr(&plan,&expr).unwrap();
        let DataType::List(field) = result.get_type(plan.schema()).unwrap() else {panic!("expected typed list")};
        assert_eq!(field.data_type(),&DataType::Int64);
    }
    for kind in [DataType::Utf8,DataType::Float64,DataType::Boolean] {
        let plan = scalar_scope(vec![Field::new("x",kind,true),Field::new("y",DataType::Int64,true)]);
        let expr = IrExpr::List(vec![binding("x"),binding("y")]);
        assert!(ctx.lower_sql_expr(&plan,&expr).unwrap_err().to_string().contains("heterogeneous Cypher collection"));
    }
}

#[test]
fn cypher_dynamic_collection_dispatch_checks_arity_before_lowering_operands() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![]);
    for (name,arity) in [("list_append",2),("array_append",2),("array_push_back",2),
        ("list_prepend",2),("array_prepend",2),("array_push_front",2),
        ("list_extract",2),("list_element",2),("element_at",2),("list_unique",1),
        ("list_contains",2),("list_has",2),("array_contains",2),("array_has",2),("list_has_all",2)] {
        for count in 0..4 {
            if count == arity {continue;}
            let args = vec![binding("missing");count];
            assert!(ctx.lower_dynamic_collection_function(&plan,name,&args).unwrap().is_none(),"{name}/{count}");
        }
    }
    assert!(ctx.lower_dynamic_collection_function(&plan,"unknown",&[]).unwrap().is_none());
}

#[test]
fn cypher_constant_list_index_fast_path_preserves_selected_column() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let plan = scalar_scope(vec![Field::new("x",DataType::Int64,true),Field::new("y",DataType::Int64,true)]);
    let items = IrExpr::List(vec![binding("x"),binding("y")]);
    for name in ["list_extract","list_element","element_at"] {
        for (index,expected) in [(1,"x"),(2,"y"),(-1,"y"),(-2,"x")] {
            let result = ctx.lower_dynamic_collection_function(&plan,name,&[items.clone(),IrExpr::Lit(Lit::Int(index))]).unwrap().unwrap();
            assert_eq!(result,col_exact(expected),"{name}/{index}");
        }
    }
    for name in ["list_element","element_at"] {
        for index in [0,3,-3,i64::MIN,i64::MAX] {
            let result = ctx.lower_dynamic_collection_function(&plan,name,&[items.clone(),IrExpr::Lit(Lit::Int(index))]).unwrap().unwrap();
            assert_eq!(result,lit(ScalarValue::Utf8(None)));
        }
        assert!(ctx.lower_dynamic_collection_function(&plan,name,&[items.clone(),binding("x")]).unwrap().is_none());
        assert!(ctx.lower_dynamic_collection_function(&plan,name,&[binding("x"),IrExpr::Lit(Lit::Int(1))]).unwrap().is_none());
    }
}

#[test]
fn cypher_list_dispatch_preserves_native_list_types_and_predicates() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let kind = DataType::List(Arc::new(Field::new("item",DataType::Int64,true)));
    let plan = scalar_scope(vec![Field::new("xs",kind.clone(),true),Field::new("x",DataType::Int64,true)]);
    for name in ["list_append","array_append","array_push_back","list_prepend","array_prepend","array_push_front"] {
        let result = ctx.lower_dynamic_collection_function(&plan,name,&[binding("xs"),binding("x")]).unwrap().unwrap();
        assert_eq!(result.get_type(plan.schema()).unwrap(),kind,"{name}");
    }
    for name in ["list_contains","list_has","array_contains","array_has"] {
        let result = ctx.lower_dynamic_collection_function(&plan,name,&[binding("xs"),binding("x")]).unwrap().unwrap();
        assert_eq!(result.get_type(plan.schema()).unwrap(),DataType::Boolean,"{name}");
    }
    let result = ctx.lower_dynamic_collection_function(&plan,"list_has_all",&[binding("xs"),binding("xs")]).unwrap().unwrap();
    assert_eq!(result.get_type(plan.schema()).unwrap(),DataType::Boolean);
    let result = ctx.lower_dynamic_collection_function(&plan,"list_unique",&[binding("xs")]).unwrap().unwrap();
    assert_eq!(result.get_type(plan.schema()).unwrap(),DataType::Int64);
    assert_eq!(ctx.lower_native_list(&plan,&binding("xs")).unwrap(),col_exact("xs"));
    assert!(ctx.lower_native_list(&plan,&binding("x")).is_err());
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn cypher_case_lowered_sql_preserves_nulls_and_exact_large_integers() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for condition in [Some(true),Some(false),None] {
        let plan = LogicalPlanBuilder::empty(true).project(vec![
            lit(ScalarValue::Boolean(condition)).alias("p"),
            lit(9_007_199_254_740_993_i64).alias("x"),
        ]).unwrap().build().unwrap();
        for null_else in [false,true] {
            let expr = IrExpr::Case {arms:vec![(binding("p"),binding("x"))],otherwise:Some(Box::new(
                if null_else {IrExpr::Lit(Lit::Null)} else {IrExpr::Lit(Lit::Int(-7))}
            ))};
            let lowered = ctx.lower_sql_expr(&plan,&expr).unwrap();
            let output = LogicalPlanBuilder::from(plan.clone()).project(vec![lowered.alias("result")]).unwrap().build().unwrap();
            let batches = crate::ir::rel::execute_test_plan(output).await.unwrap();
            assert_eq!(batches.iter().map(|b| b.num_rows()).sum::<usize>(),1);
            let actual = ScalarValue::try_from_array(batches[0].column(0),0).unwrap();
            let expected = if condition == Some(true) {Some(9_007_199_254_740_993)} else if null_else {None} else {Some(-7)};
            assert_eq!(actual,ScalarValue::Int64(expected));
        }
    }
}

#[cfg(feature = "duckdb")]
#[tokio::test]
async fn cypher_collection_aliases_execute_with_order_duplicates_and_null_elements() {
    use datafusion::logical_expr::LogicalPlanBuilder;
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    let items = ScalarValue::List(ScalarValue::new_list(&[
        ScalarValue::Int64(Some(7)),ScalarValue::Int64(None),ScalarValue::Int64(Some(7))
    ],&DataType::Int64,true));
    let plan = LogicalPlanBuilder::empty(true).project(vec![lit(items).alias("xs"),lit(9_i64).alias("x")]).unwrap().build().unwrap();
    for name in ["list_append","array_append","array_push_back","list_prepend","array_prepend","array_push_front"] {
        let lowered = ctx.lower_dynamic_collection_function(&plan,name,&[binding("xs"),binding("x")]).unwrap().unwrap();
        let output = LogicalPlanBuilder::from(plan.clone()).project(vec![lowered.alias("result")]).unwrap().build().unwrap();
        let batches = crate::ir::rel::execute_test_plan(output).await.unwrap();
        let ScalarValue::List(array) = ScalarValue::try_from_array(batches[0].column(0),0).unwrap() else {panic!("list expected")};
        let actual = array.value(0);
        let expected = if name.contains("prepend") || name.ends_with("front") {vec![Some(9),Some(7),None,Some(7)]} else {vec![Some(7),None,Some(7),Some(9)]};
        assert_eq!(actual.len(),expected.len());
        for (index,value) in expected.into_iter().enumerate() {
            assert_eq!(ScalarValue::try_from_array(&actual,index).unwrap(),ScalarValue::Int64(value),"{name}/{index}");
        }
    }
}

#[test]
fn cypher_graph_identity_and_distinct_count_preserve_source_key_types() {
    let graph = PropertyGraph::new(); let ctx = cypher_context(&graph);
    for kind in [DataType::Int32,DataType::Int64,DataType::UInt64,DataType::Utf8] {
        let plan = scalar_scope(vec![Field::new(id_col("n"),kind.clone(),true),Field::new(label_col("n"),DataType::Utf8,true)]);
        assert_eq!(ctx.lower_sql_expr(&plan,&IrExpr::Id("n".into())).unwrap(),col_exact(id_col("n")));
        assert_eq!(ctx.lower_count_operand(&plan,&binding("n")).unwrap(),col_exact(id_col("n")));
        assert_eq!(ctx.lower_sql_expr(&plan,&IrExpr::IsBound("n".into())).unwrap(),col_exact(id_col("n")).is_not_null());
        assert_eq!(ctx.lower_sql_expr(&plan,&IrExpr::Label("n".into())).unwrap(),col_exact(label_col("n")));
        let distinct = ctx.lower_distinct_count_operand(&plan,&binding("n")).unwrap();
        let Expr::Case(case) = distinct else {panic!("count distinct must exclude absent optional nodes")};
        assert_eq!(*case.when_then_expr[0].0,col_exact(id_col("n")).is_not_null());
        let DataType::Struct(fields) = case.when_then_expr[0].1.get_type(plan.schema()).unwrap() else {panic!("identity must include label and source key")};
        assert_eq!(fields[1].data_type(),&kind);
    }
    let plan = scalar_scope(vec![Field::new(id_col("n"),DataType::Struct(vec![Field::new("part",DataType::Int64,true)].into()),true),Field::new(label_col("n"),DataType::Utf8,true)]);
    assert!(ctx.lower_sql_expr(&plan,&IrExpr::Id("n".into())).unwrap_err().to_string().contains("composite or heterogeneous source IDs"));
}

#[test]
fn cypher_same_type_casts_preserve_columns_without_sql_cast_nodes() {
    let graph=PropertyGraph::new();let ctx=cypher_context(&graph);
    for (kind,target) in [(DataType::Boolean,"BOOLEAN"),(DataType::Int8,"INT8"),(DataType::Int16,"INT16"),
        (DataType::Int32,"INT32"),(DataType::Int64,"INT64"),(DataType::UInt64,"UINT64"),
        (DataType::Float32,"FLOAT32"),(DataType::Float64,"FLOAT64"),(DataType::Utf8,"VARCHAR"),
        (DataType::Decimal128(18,3),"DECIMAL(18,3)")] {
        let plan=scalar_scope(vec![Field::new("x",kind,true)]);
        let expr=call("cast",vec![binding("x"),IrExpr::Lit(Lit::String(target.into()))]);
        assert_eq!(ctx.lower_sql_expr(&plan,&expr).unwrap(),col_exact("x"),"{target}");
    }
    for (kind,function) in [(DataType::Int64,"tointeger"),(DataType::Float64,"tofloat"),(DataType::Boolean,"toboolean"),(DataType::Utf8,"tostring")] {
        let plan=scalar_scope(vec![Field::new("x",kind,true)]);
        assert_eq!(ctx.lower_sql_expr(&plan,&call(function,vec![binding("x")])).unwrap(),col_exact("x"),"{function}");
    }
}

#[test]
fn property_star_without_catalog_properties_retains_one_typed_null_cell() {
    let graph=PropertyGraph::new(); let ctx=cypher_context(&graph);
    let plan=scalar_scope(vec![Field::new(id_col("n"),DataType::Int64,false),Field::new(label_col("n"),DataType::Utf8,false)]);
    let result=ctx.project_item_exprs(&plan,"out",&call("cypher_property_star",vec![binding("n")])).unwrap();
    assert_eq!(result,vec![lit(ScalarValue::Utf8(None)).alias(format!("out{STAR_SEP}"))]);
    assert_eq!(result[0].get_type(plan.schema()).unwrap(),DataType::Utf8);
}

#[test]
fn property_star_uses_struct_shadow_field_order_and_types_without_neighbor_fields() {
    let graph=PropertyGraph::new(); let ctx=cypher_context(&graph);
    let fields=[("z",DataType::Int64),("a",DataType::Boolean),("text",DataType::Utf8)];
    let mut schema=fields.iter().map(|(name,kind)|Field::new(struct_field_col("n","payload",name),kind.clone(),true)).collect::<Vec<_>>();
    schema.push(Field::new(struct_field_col("n","other","hidden"),DataType::Int64,true));
    let plan=scalar_scope(schema);
    let property=IrExpr::property("n","payload",PropertyMissing::NullOnMissing);
    let result=ctx.project_item_exprs(&plan,"out",&call("cypher_property_star",vec![property])).unwrap();
    assert_eq!(result.len(),3);
    for (expr,(name,kind)) in result.iter().zip(fields) {
        assert_eq!(expr,&col_exact(struct_field_col("n","payload",name)).alias(format!("out{STAR_SEP}{name}")));
        assert_eq!(expr.get_type(plan.schema()).unwrap(),kind);
    }
}

#[test]
fn property_equality_crosses_identity_projection_and_one_row_inner_apply_only() {
    let graph=PropertyGraph::new(); let mut ctx=cypher_context(&graph);
    ctx.options.mapping=Some(Arc::new(mapping::GraphMapping::new()));
    let source=Node::GraphNodeScan { graph:"g".into(),binding:"a".into(),labels:LabelExpr::Any };
    let predicate=IrExpr::eq(IrExpr::property("a","age",PropertyMissing::NullOnMissing),IrExpr::Lit(Lit::Int(7)));
    let projection=|expr|Node::GraphProject { mode:ProjectMode::ReplaceScope,items:vec![ProjectionItem { alias:"a".into(),expr }],error_policy:ProjectErrorPolicy::PropagateError,input:Box::new(source.clone()) };
    let identity=projection(binding("a"));
    let result=ctx.push_expand_equality(&predicate,&identity).unwrap().unwrap();
    let Node::GraphProject { input,.. }=result else { panic!("identity projection must remain") };
    assert_eq!(*input,Node::GraphFilter { condition:predicate.clone(),input:Box::new(source.clone()) });
    assert!(ctx.push_expand_equality(&predicate,&projection(IrExpr::Lit(Lit::Int(3)))).unwrap().is_none());
    let apply=|kind,left|Node::GraphApply { kind,correlation:vec![],outputs:vec!["a".into()],optional_missing:OptionalMissing::Null,left:Box::new(left),right:Box::new(identity.clone()) };
    let result=ctx.push_expand_equality(&predicate,&apply(ApplyKind::Inner,Node::GraphOneRow)).unwrap().unwrap();
    let Node::GraphApply { right,.. }=result else { panic!("apply must remain") };
    let Node::GraphProject { input,.. }=*right else { panic!("identity projection must remain") };
    assert_eq!(*input,Node::GraphFilter { condition:predicate.clone(),input:Box::new(source.clone()) });
    for plan in [apply(ApplyKind::Optional,Node::GraphOneRow),apply(ApplyKind::Inner,source)] {
        assert!(ctx.push_expand_equality(&predicate,&plan).unwrap().is_none());
    }
}
