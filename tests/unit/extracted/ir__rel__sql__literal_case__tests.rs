
    use super::*;
    use datafusion::logical_expr::expr::Case;
    fn case(conditions: Vec<(Expr, Expr)>, otherwise: Option<Expr>) -> Expr {
        Expr::Case(Case::new(None, conditions.into_iter().map(|(a,b)| (Box::new(a),Box::new(b))).collect(), otherwise.map(Box::new)))
    }
    #[test]
    fn false_and_null_conditions_do_not_select_a_branch() {
        let value = case(vec![(lit(false),lit(1_i64)), (lit(ScalarValue::Boolean(None)),lit(2_i64))], Some(lit(3_i64)));
        assert_eq!(simplify(value, &DFSchema::empty()).unwrap().data, lit(3_i64));
        let value = case(vec![(lit(false),lit(1_i64))],None);
        assert_eq!(simplify(value, &DFSchema::empty()).unwrap().data,lit(ScalarValue::Int64(None)));
    }
    #[test]
    fn true_condition_discards_only_later_branches() {
        let condition = lit(1_i64).eq(lit(2_i64));
        let value = case(vec![(condition.clone(),lit(1_i64)),(lit(true),lit(2_i64)),(lit(true),lit(3_i64))],Some(lit(4_i64)));
        assert_eq!(simplify(value,&DFSchema::empty()).unwrap().data,case(vec![(condition,lit(1_i64))],Some(lit(2_i64))));
    }
    #[test]
    fn simple_case_boolean_values_are_not_conditions() {
        let value = Expr::Case(Case::new(Some(Box::new(lit(false))),vec![(Box::new(lit(false)),Box::new(lit(1_i64)))],Some(Box::new(lit(2_i64)))));
        assert_eq!(simplify(value.clone(),&DFSchema::empty()).unwrap().data,value);
    }
