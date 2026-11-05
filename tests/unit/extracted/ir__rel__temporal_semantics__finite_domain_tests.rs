use super::*;

#[test]
fn temporal_list_cardinality_is_safe_only_for_direct_typed_operands() {
    let plan = LogicalPlanBuilder::empty(true).build().unwrap();
    let date = ScalarValue::Date32(Some(0));
    let list = ScalarValue::List(ScalarValue::new_list(
        &[date.clone()],
        &DataType::Date32,
        true,
    ));
    let call = |name: &str, args| IrExpr::Call {
        name: name.into(),
        args,
    };
    let field = Arc::new(Field::new("item", DataType::Date32, true));
    for kind in [
        DataType::List(field.clone()),
        DataType::LargeList(field.clone()),
        DataType::FixedSizeList(field, 1),
    ] {
        let values = arrow::compute::cast(&list.to_array().unwrap(), &kind).unwrap();
        for scalar in [
            ScalarValue::try_from_array(&values, 0).unwrap(),
            ScalarValue::try_from(&kind).unwrap(),
        ] {
            let literal = IrExpr::Lit(Lit::Scalar(scalar));
            assert!(needs_temporal_semantics(&plan, &literal));
            assert!(!needs_temporal_semantics(
                &plan,
                &call("size", vec![literal.clone()])
            ));
            assert!(needs_temporal_semantics(
                &plan,
                &call("size", vec![call("reverse", vec![literal.clone()])])
            ));
            assert!(needs_temporal_semantics(
                &plan,
                &call("size", vec![literal, IrExpr::Lit(Lit::Int(1))])
            ));
        }
    }
    assert!(needs_temporal_semantics(
        &plan,
        &call("size", vec![IrExpr::Lit(Lit::Scalar(date))])
    ));
}

#[test]
fn finite_domains_include_null_extension_and_survive_limits() {
    let input = |name: &str, value: i64| {
        LogicalPlanBuilder::empty(true)
            .project(vec![lit(value).alias(name)])
            .unwrap()
            .build()
            .unwrap()
    };
    for kind in [
        JoinType::Inner,
        JoinType::Left,
        JoinType::Right,
        JoinType::Full,
    ] {
        let joined = LogicalPlanBuilder::from(input("left_value", 1))
            .join(
                input("right_value", 2),
                kind,
                (Vec::<Column>::new(), Vec::<Column>::new()),
                Some(lit(true)),
            )
            .unwrap()
            .limit(0, Some(1))
            .unwrap()
            .build()
            .unwrap();
        for (column, value, nullable) in [
            (
                "left_value",
                1,
                matches!(kind, JoinType::Right | JoinType::Full),
            ),
            (
                "right_value",
                2,
                matches!(kind, JoinType::Left | JoinType::Full),
            ),
        ] {
            let mut expected = vec![ScalarValue::Int64(Some(value))];
            if nullable {
                expected.push(ScalarValue::Int64(None));
            }
            assert_eq!(finite_values(&joined, column), Some(expected));
        }
        assert_eq!(finite_values(&joined, "missing"), None);
    }
}

#[tokio::test]
async fn finite_temporal_arithmetic_order_keys_preserve_ties_and_nulls() {
    let graph = PropertyGraph::new();
    let context = LoweringContext {
        island_memo: None,
        graph: &graph,
        options: RelBackendOptions::default(),
        policy: crate::ir::policy::GraphPlanPolicy::cypher(),
        language: Language::Cypher,
        scan_counter: 0,
        correlate_plan: None,
        rdf_typed_terms_used: false,
        scan_logical_labels: false,
        gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(),
        unordered_distincts: Default::default(),
    };
    let date = |text: &str| {
        crate::ir::temporal::transport::encoded_scalar(
            &crate::ir::temporal::parse("date", text).unwrap(),
        )
        .unwrap()
    };
    let null = ScalarValue::try_from(&date("2020-01-01").data_type()).unwrap();
    for only_null in [false, true] {
        let values = if only_null {
            vec![null.clone()]
        } else {
            vec![
                date("2020-01-03"),
                date("2020-01-01"),
                date("2020-01-03"),
                null.clone(),
            ]
        };
        let mut inputs = values.into_iter().map(|value| {
            LogicalPlanBuilder::empty(true)
                .project(vec![lit(value).alias("date")])
                .unwrap()
                .build()
                .unwrap()
        });
        let mut input = inputs.next().unwrap();
        for next in inputs {
            input = LogicalPlanBuilder::from(input)
                .union(next)
                .unwrap()
                .build()
                .unwrap();
        }
        let expression = IrExpr::Binary {
            op: crate::ir::expr::BinaryOp::Add,
            lhs: Box::new(IrExpr::binding("date")),
            rhs: Box::new(IrExpr::Call {
                name: "cypher_temporal.duration".into(),
                args: vec![IrExpr::Lit(Lit::String("P1D".into()))],
            }),
        };
        let key = finite_order_key(&context, &input, &expression).expect("finite temporal rank");
        if !only_null {
            let Expr::Case(case) = &key else {
                panic!("partial evaluation should retain one CASE")
            };
            assert!(case.when_then_expr.iter().all(|(_, value)| matches!(
                value.as_ref(),
                Expr::Literal(ScalarValue::Int64(_), _)
            )));
        }
        let plan = LogicalPlanBuilder::from(input)
            .project(vec![key.alias("rank")])
            .unwrap()
            .build()
            .unwrap();
        let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
        let actual = batches
            .iter()
            .flat_map(|batch| {
                (0..batch.num_rows())
                    .map(|row| ScalarValue::try_from_array(batch.column(0), row).unwrap())
            })
            .collect::<Vec<_>>();
        let expected = if only_null {
            vec![ScalarValue::Int64(None)]
        } else {
            vec![Some(1), Some(0), Some(1), None]
                .into_iter()
                .map(ScalarValue::Int64)
                .collect()
        };
        assert_eq!(actual, expected);
    }
}

#[test]
fn outer_unnest_empty_or_null_lists_include_null_in_domain() {
    let empty = ScalarValue::List(ScalarValue::new_list(&[], &DataType::Int64, true));
    let null = ScalarValue::try_from(&empty.data_type()).unwrap();
    for source in [empty, null] {
        let input = LogicalPlanBuilder::empty(true)
            .project(vec![lit(source).alias("step")])
            .unwrap()
            .build()
            .unwrap();
        let mut options = datafusion::common::UnnestOptions::default();
        options.preserve_nulls = true;
        let plan = LogicalPlanBuilder::from(input)
            .unnest_column_with_options(Column::new_unqualified("step"), options)
            .unwrap()
            .build()
            .unwrap();
        assert_eq!(
            finite_values(&plan, "step"),
            Some(vec![ScalarValue::Int64(None)])
        );
    }
}

#[test]
fn case_domains_include_implicit_null_deduplicate_and_respect_proof_budget() {
    let project = |expr| LogicalPlanBuilder::empty(true).project(vec![expr]).unwrap().build().unwrap();
    let case = |count: usize, otherwise: Option<Expr>| Expr::Case(Case::new(None,
        (0..count).map(|n| (Box::new(lit(true)),Box::new(lit(n as i64)))).collect(), otherwise.map(Box::new)));
    let plan = project(case(2,None).alias("value"));
    assert_eq!(finite_values(&plan,"value"),Some(vec![ScalarValue::Int64(Some(0)),ScalarValue::Int64(Some(1)),ScalarValue::Int64(None)]));
    let plan = project(case(2,Some(lit(0_i64))).alias("value"));
    assert_eq!(finite_values(&plan,"value"),Some(vec![ScalarValue::Int64(Some(0)),ScalarValue::Int64(Some(1))]));
    for (count,otherwise,known) in [(63,None,true),(64,None,false),(64,Some(lit(0_i64)),true),(65,Some(lit(0_i64)),false)] {
        assert_eq!(finite_values(&project(case(count,otherwise).alias("value")),"value").is_some(),known);
    }
    let plan = LogicalPlanBuilder::from(plan).filter(lit(true)).unwrap()
        .sort(vec![datafusion::prelude::col("value").sort(true,true)]).unwrap()
        .limit(0,Some(1)).unwrap().alias("renamed").unwrap().build().unwrap();
    assert_eq!(finite_values(&plan,"value"),Some(vec![ScalarValue::Int64(Some(0)),ScalarValue::Int64(Some(1))]));
    let grouped = LogicalPlanBuilder::from(plan).aggregate(vec![datafusion::prelude::col("value")],vec![datafusion::functions_aggregate::expr_fn::count(lit(1_i64)).alias("count")]).unwrap().build().unwrap();
    assert_eq!(finite_values(&grouped,"value"),Some(vec![ScalarValue::Int64(Some(0)),ScalarValue::Int64(Some(1))]));
    assert!(finite_values(&grouped,"count").is_none());
}

#[tokio::test]
async fn finite_list_domains_follow_unnest_without_losing_nulls_duplicates_or_passthrough() {
    use datafusion::functions_nested::expr_fn::make_array;
    let plan = LogicalPlanBuilder::empty(true).project(vec![make_array(vec![lit(7_i64),lit(ScalarValue::Int64(None)),lit(7_i64)]).alias("items"),lit(9_i64).alias("stable")]).unwrap().build().unwrap();
    let values = vec![ScalarValue::Int64(Some(7)),ScalarValue::Int64(None),ScalarValue::Int64(Some(7))];
    assert_eq!(finite_values(&plan,"items"),Some(vec![ScalarValue::List(ScalarValue::new_list_nullable(&values,&DataType::Int64))]));
    let plan = LogicalPlanBuilder::from(plan).unnest_column(Column::new_unqualified("items")).unwrap().build().unwrap();
    assert_eq!(finite_values(&plan,"items"),Some(values.clone()));
    assert_eq!(finite_values(&plan,"stable"),Some(vec![ScalarValue::Int64(Some(9))]));
    let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
    let actual = batches.iter().flat_map(|b| (0..b.num_rows()).map(|i| ScalarValue::try_from_array(b.column(0),i).unwrap())).collect::<Vec<_>>();
    assert_eq!(actual,values);
    for values in [vec![],vec![lit(ScalarValue::Int64(None))],(0..65).map(|n|lit(n as i64)).collect(),vec![Expr::Case(Case::new(None,vec![(Box::new(lit(true)),Box::new(lit(1_i64)))],Some(Box::new(lit(2_i64)))))] ] {
        let plan = LogicalPlanBuilder::empty(true).project(vec![make_array(values).alias("items")]).unwrap().build().unwrap();
        assert!(finite_values(&plan,"items").is_none());
    }
    for (scalar, expected) in [
        (ScalarValue::List(ScalarValue::new_list_nullable(&[],&DataType::Int64)),Some(vec![])),
        (ScalarValue::List(ScalarValue::new_list_nullable(&[ScalarValue::Float64(Some(1.0))],&DataType::Float64)),None),
        (ScalarValue::List(ScalarValue::new_list_nullable(&(0..65).map(|n|ScalarValue::Int64(Some(n))).collect::<Vec<_>>(),&DataType::Int64)),None),
    ] {
        let input = LogicalPlanBuilder::empty(true).project(vec![lit(scalar).alias("items")]).unwrap().build().unwrap();
        let mut options = datafusion::common::UnnestOptions::default(); options.preserve_nulls=false;
        let plan = LogicalPlanBuilder::from(input).unnest_column_with_options(Column::new_unqualified("items"),options).unwrap().build().unwrap();
        assert_eq!(finite_values(&plan,"items"),expected);
    }
}

#[test]
fn snapshot_domain_budget_never_discards_data_or_infers_unsupported_fields() {
    use arrow::array::{Int64Array,BooleanArray};
    for (rows,distinct,known) in [(64,64,true),(65,65,false),(4096,1,true),(4097,1,false)] {
        let schema = Arc::new(arrow::datatypes::Schema::new(vec![Field::new("value",DataType::Int64,false),Field::new("flag",DataType::Boolean,false)]));
        let batch = RecordBatch::try_new(schema.clone(),vec![Arc::new(Int64Array::from_iter_values((0..rows).map(|i|(i%distinct) as i64))),Arc::new(BooleanArray::from(vec![true;rows]))]).unwrap();
        let snapshot = Snapshot::new(MemTable::try_new(schema,vec![vec![batch.clone()]]).unwrap(),&[batch]);
        assert_eq!(snapshot.batches().iter().map(RecordBatch::num_rows).sum::<usize>(),rows);
        let plan = LogicalPlanBuilder::scan("snapshot",datafusion::datasource::provider_as_source(Arc::new(snapshot)),None).unwrap().build().unwrap();
        let domain = finite_values(&plan,"value");
        assert_eq!(domain.is_some(),known,"rows={rows} distinct={distinct}");
        if known { assert_eq!(domain.unwrap(),(0..distinct).map(|i|ScalarValue::Int64(Some(i as i64))).collect::<Vec<_>>()); }
        assert!(finite_values(&plan,"flag").is_none());
    }
}

#[tokio::test]
async fn direct_finite_temporal_order_preserves_duplicates_nulls_and_property_bindings() {
    let graph = PropertyGraph::new();
    let context = LoweringContext {
        island_memo: None, graph: &graph, options: RelBackendOptions::default(),
        policy: crate::ir::policy::GraphPlanPolicy::cypher(), language: Language::Cypher,
        scan_counter: 0, correlate_plan: None, rdf_typed_terms_used: false,
        scan_logical_labels: false, gremlin_label_binds: Default::default(),
        unordered_collects: Default::default(), unordered_distincts: Default::default(),
    };
    let scalar = |kind,text| crate::ir::temporal::transport::encoded_scalar(&crate::ir::temporal::parse(kind,text).unwrap()).unwrap();
    let early = scalar("date","2000-01-01");
    let late = scalar("date","2010-01-01");
    let null = ScalarValue::try_from(&early.data_type()).unwrap();
    for property in [false,true] {
        let column = if property {prop_col("node","born")} else {"value".into()};
        let mut inputs = [late.clone(),early.clone(),late.clone(),null.clone()].into_iter().map(|value| LogicalPlanBuilder::empty(true).project(vec![lit(value).alias(&column)]).unwrap().build().unwrap());
        let mut input = inputs.next().unwrap();
        for next in inputs {input=LogicalPlanBuilder::from(input).union(next).unwrap().build().unwrap();}
        let operand = if property {IrExpr::property("node","born",crate::ir::policy::PropertyMissing::NullOnMissing)} else {IrExpr::binding(&column)};
        for wrapped in [false,true] {
            let expr = if wrapped {IrExpr::Call {name:"cypher_order_key".into(),args:vec![operand.clone()]}} else {operand.clone()};
            let key = finite_order_key(&context,&input,&expr).unwrap();
            let plan = LogicalPlanBuilder::from(input.clone()).project(vec![key.alias("rank")]).unwrap().build().unwrap();
            let batches = crate::ir::rel::execute_test_plan(plan).await.unwrap();
            let actual = batches.iter().flat_map(|batch|(0..batch.num_rows()).map(|i|ScalarValue::try_from_array(batch.column(0),i).unwrap())).collect::<Vec<_>>();
            assert_eq!(actual,vec![ScalarValue::Int64(Some(1)),ScalarValue::Int64(Some(0)),ScalarValue::Int64(Some(1)),ScalarValue::Int64(None)]);
        }
    }
    let first = LogicalPlanBuilder::empty(true).project(vec![lit(early).alias("value")]).unwrap().build().unwrap();
    let second = LogicalPlanBuilder::empty(true).project(vec![lit(scalar("localtime","12:00")).alias("value")]).unwrap().build().unwrap();
    let mixed = LogicalPlanBuilder::from(first).union(second).unwrap().build().unwrap();
    assert!(finite_order_key(&context,&mixed,&IrExpr::binding("value")).is_none());
    assert!(finite_order_key(&context,&mixed,&IrExpr::binding("missing")).is_none());
}
