
    use super::*;
    use arrow::{array::Int64Array, datatypes::DataType};

    fn encoded(batch: &RecordBatch) -> Vec<u8> {
        let mut bytes=vec![];
        let mut writer=arrow::ipc::writer::StreamWriter::try_new(&mut bytes,&batch.schema()).unwrap();
        writer.write(batch).unwrap(); writer.finish().unwrap(); drop(writer); bytes
    }
    fn batch(values: Vec<Option<i64>>) -> RecordBatch {
        RecordBatch::try_new(Arc::new(Schema::new(vec![Field::new("v",DataType::Int64,true)])),
            vec![Arc::new(Int64Array::from(values))]).unwrap()
    }

    #[test]
    fn scalar_ipc_requires_exactly_one_cell_and_preserves_nulls_and_integers() {
        for value in [None,Some(i64::MIN),Some(i64::MAX)] {
            let bytes=encoded(&batch(vec![value]));
            assert_eq!(literal(&bytes).unwrap(),ScalarValue::Int64(value));
            let expr=LocalExpr::Literal {ipc:bytes,json:None,data_type:"int64".into()};
            validate_expr(&expr,&Functions::new(),0).unwrap();
            for rows in [0,3] {
                let result=evaluate(&expr,&batch(vec![Some(0);rows]),&Functions::new()).unwrap();
                assert_eq!(result.len(),rows);
                for row in 0..rows {assert_eq!(ScalarValue::try_from_array(&result,row).unwrap(),ScalarValue::Int64(value));}
            }
        }
        for values in [vec![],vec![Some(1),Some(2)]] {
            assert_eq!(literal(&encoded(&batch(values))).unwrap_err(),"invalid literal shape");
        }
        assert_eq!(literal(&schema_ipc(&Schema::empty()).unwrap()).unwrap_err(),"missing literal");
        assert!(literal(b"invalid IPC").is_err());
    }

    struct Output(ArrayRef);
    impl LocalFunction for Output {
        fn call(&self,_: &[ArrayRef],_: usize)->Result<ArrayRef,String>{Ok(self.0.clone())}
    }
    #[test]
    fn local_calls_validate_nested_inputs_and_enforce_result_shape() {
        let input=batch(vec![Some(3),None]);
        let expr=LocalExpr::Call {function:"f".into(),arguments:vec![LocalExpr::Column{index:0}],returns:"int64".into()};
        assert!(validate_expr(&expr,&Functions::new(),1).unwrap_err().contains("missing local function"));
        for output in [Arc::new(Int64Array::from(vec![1])) as ArrayRef,
            Arc::new(arrow::array::StringArray::from(vec!["a","b"])) as ArrayRef] {
            let functions=Functions::from([("f".into(),Arc::new(Output(output)) as Arc<dyn LocalFunction>)]);
            validate_expr(&expr,&functions,1).unwrap();
            assert!(evaluate(&expr,&input,&functions).unwrap_err().contains("invalid type or row count"));
            assert!(validate_expr(&expr,&functions,0).unwrap_err().contains("out of bounds"));
        }
        let functions=Functions::from([("f".into(),Arc::new(Output(input.column(0).clone())) as Arc<dyn LocalFunction>)]);
        assert_eq!(evaluate(&expr,&input,&functions).unwrap().to_data(),input.column(0).to_data());
        let cast=LocalExpr::Cast{input:Box::new(LocalExpr::Column{index:0}),data_type:"float64".into()};
        validate_expr(&cast,&functions,1).unwrap();
        assert_eq!(evaluate(&cast,&input,&functions).unwrap().data_type(),&DataType::Float64);
        let descriptor=LocalExpr::GraphElement{input:Box::new(LocalExpr::Column{index:0}),data_type:"string".into()};
        assert_eq!(evaluate(&descriptor,&input,&functions).unwrap_err(),"graph descriptor schema mismatch");
        let invalid=LocalExpr::Cast{input:Box::new(LocalExpr::Column{index:0}),data_type:"not_a_type".into()};
        assert!(validate_expr(&invalid,&functions,1).is_err());
    }

    #[test]
    fn local_input_pruning_remaps_nested_columns_and_retains_literal_payloads() {
        let literal=LocalExpr::Literal{ipc:encoded(&batch(vec![Some(7)])),json:Some(7.into()),data_type:"int64".into()};
        let mut expr=LocalExpr::Call{function:"f".into(),returns:"int64".into(),arguments:vec![
            LocalExpr::Cast{input:Box::new(LocalExpr::Column{index:8}),data_type:"int64".into()},
            LocalExpr::GraphElement{input:Box::new(LocalExpr::Column{index:3}),data_type:"string".into()},literal]};
        let mut demand=BTreeSet::new();expr.column_demand(&mut demand);
        assert_eq!(demand,BTreeSet::from([3,8]));
        expr.remap_columns(&BTreeMap::from([(3,0),(8,1)]));
        demand.clear();expr.column_demand(&mut demand);assert_eq!(demand,BTreeSet::from([0,1]));
        let LocalExpr::Call{arguments,..}=expr else{unreachable!()};
        let LocalExpr::Literal{ipc,json,..}=&arguments[2] else{unreachable!()};
        assert_eq!(literal_value(ipc),7);assert_eq!(*json,Some(7.into()));
    }
    fn literal_value(ipc:&[u8])->i64 {let ScalarValue::Int64(Some(value))=literal(ipc).unwrap() else{panic!("wrong literal type")};value}
