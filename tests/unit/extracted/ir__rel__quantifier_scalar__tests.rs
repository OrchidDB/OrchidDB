
    use super::*;
use arrow::array::Array;
    #[test]
    fn effectful_and_throwing_predicates_stay_out_of_scalar_path() {
        let item=IrExpr::Binding("x".into());
        for predicate in [
            IrExpr::Call{name:"rand".into(),args:vec![]},
            IrExpr::Call{name:"mod".into(),args:vec![item.clone(),IrExpr::Lit(Lit::Int(0))]},
            IrExpr::Call{name:"mod".into(),args:vec![item.clone(),IrExpr::Lit(Lit::Int(-1))]},
            IrExpr::Binary{op:BinaryOp::Div,lhs:Box::new(item.clone()),rhs:Box::new(IrExpr::Lit(Lit::Int(1)))},
        ] { assert!(!candidate_predicate(&predicate),"{predicate:?}"); }
    }
    #[test]
    fn length_fast_path_requires_collection_types_and_pure_operands() {
        let plan=LogicalPlanBuilder::empty(true).project(vec![
            lit(7_i64).alias("number"),lit("é").alias("text"),
            datafusion::functions_nested::expr_fn::make_array(vec![lit(1_i64)]).alias("items"),
        ]).unwrap().build().unwrap();
        assert!(!length_operand(&IrExpr::Binding("number".into()),&plan,true).unwrap());
        assert!(length_operand(&IrExpr::Binding("text".into()),&plan,false).unwrap());
        assert!(length_operand(&IrExpr::Binding("items".into()),&plan,true).unwrap());
        assert!(!length_operand(&IrExpr::Binding("items".into()),&plan,false).unwrap());
        assert!(!candidate_predicate(&IrExpr::Call{name:"size".into(),args:vec![
            IrExpr::Call{name:"rand".into(),args:vec![]}
        ]}));
    }
    #[test]
    fn column_sources_with_nonimmutable_producers_are_rejected() {
        use datafusion::logical_expr::Volatility;
        for volatility in [Volatility::Volatile, Volatility::Stable, Volatility::Immutable] {
            let function=datafusion::logical_expr::create_udf("source_list",vec![],
                DataType::List(Arc::new(Field::new("item",DataType::Int64,true))), volatility,
                Arc::new(|_| panic!("must not evaluate diagnostic function")));
            let plan=LogicalPlanBuilder::empty(true).project(vec![function.call(vec![]).alias("xs")])
                .unwrap().build().unwrap();
            assert!(super::super::list_comparison::repeatable(&plan,&col_exact("xs")));
            assert_eq!(super::super::list_comparison::immutable_source_plan(&plan),volatility==Volatility::Immutable);
        }
    }
    #[test]
    fn constructor_nullability_does_not_trust_function_names() {
        let spoof=datafusion::logical_expr::create_udf("make_array",vec![],
            DataType::List(Arc::new(Field::new("item",DataType::Int64,true))),
            datafusion::logical_expr::Volatility::Immutable,
            Arc::new(|_| panic!("must not evaluate diagnostic function")));
        let plan=LogicalPlanBuilder::empty(true).build().unwrap();
        assert!(super::super::list_comparison::list_nullable(&plan,&spoof.call(vec![])).unwrap());
    }
    #[test]
    fn effectful_list_functions_are_not_repeatable_inputs() {
        let function=datafusion::logical_expr::create_udf("side_effect_list",vec![],
            DataType::List(Arc::new(Field::new("item",DataType::Int64,true))),
            datafusion::logical_expr::Volatility::Volatile,
            Arc::new(|_| panic!("must not evaluate diagnostic function")));
        let plan=LogicalPlanBuilder::empty(true).build().unwrap();
        assert!(!super::super::list_comparison::repeatable(&plan,&function.call(vec![])));
    }
