
    use super::*;
    use serde_json::json;

    #[test]
    fn scalar_histograms_preserve_frequency_nulls_and_numeric_order() {
        let values=vec![Value::Null,json!(10),json!(2),json!(2),json!(-1)];
        let result=summarize(&values,"int64",true,&mut 0);
        assert_eq!((result.observations,result.nulls,result.sample_distinct),(5,1,3));
        assert_eq!(result.estimated_distinct,Some(3.0));
        assert_eq!(result.minimum.as_deref(),Some("-1"));
        assert_eq!(result.maximum.as_deref(),Some("10"));
        assert_eq!(result.histogram,vec!["-1","2","2","10"]);
        assert_eq!(result.frequent[0],("2".into(),2));
        assert_eq!(result.average_width,6.0/5.0);
        let empty=summarize(&[],"int64",false,&mut 0);
        assert_eq!(empty.estimated_distinct,None);assert_eq!(empty.average_width,0.0);
        assert!(empty.histogram.is_empty());assert!(empty.minimum.is_none());
        let many=summarize(&(0..130).map(|n|json!(n)).collect::<Vec<_>>(),"int64",false,&mut 0);
        assert_eq!(many.histogram.len(),64);assert_eq!(many.frequent.len(),32);
        assert_eq!(many.sample_distinct,130);
    }

    #[test]
    fn nested_list_summaries_count_parent_presence_and_respect_global_budget() {
        let values=vec![json!([1,1,null]),json!([]),Value::Null,json!([{"x":2},{"x":2,"y":3}])];
        let mut elements=0;
        let summary=summarize(&values,"list",true,&mut elements);
        assert!(summary.minimum.is_none());assert!(summary.maximum.is_none());
        assert!(summary.frequent.is_empty());assert!(summary.histogram.is_empty());
        let list=summary.list.unwrap();
        assert_eq!((list.parents,list.null_lists,list.empty_lists,list.total_lengths,list.observed_elements),(4,1,1,5,5));
        assert_eq!(elements,5);assert_eq!(list.lengths,vec![0,2,3]);
        assert_eq!(list.parents_containing["1"],1);assert_eq!(list.parents_containing["x:2"],1);
        assert_eq!(list.elements["$element"].nulls,1);assert_eq!(list.elements["x"].observations,2);
        let mut elements=MAX_ELEMENTS-1;
        let truncated=summarize(&[json!([1,2]),json!([3])],"list",true,&mut elements).list.unwrap();
        assert!(truncated.truncated);assert_eq!(elements,MAX_ELEMENTS);
        assert_eq!(truncated.total_lengths,3);assert_eq!(truncated.observed_elements,1);
        assert_eq!(truncated.elements["$element"].estimated_distinct,None);
        let bounded=summarize(&(0..130).map(|n|json!([n])).collect::<Vec<_>>(),"list",true,&mut 0).list.unwrap();
        assert_eq!(bounded.parents_containing.len(),128);assert_eq!(bounded.lengths.len(),64);
        assert!(bounded.lengths.iter().all(|n|*n==1));
    }

    #[test]
    fn tuple_summaries_treat_missing_values_as_null_and_keep_joint_counts() {
        let rows=[json!({"a":1,"b":2}),json!({"a":1,"b":2}),json!({"a":1}),json!({"a":1,"b":null})]
            .into_iter().map(|v|serde_json::from_value::<BTreeMap<String,Value>>(v).unwrap()).collect::<Vec<_>>();
        let summary=group(&rows,&["a".into(),"b".into()]);
        assert_eq!((summary.observations,summary.sample_distinct),(4,2));
        assert_eq!(summary.frequent,vec![(vec!["1".into(),"2".into()],2),(vec!["1".into(),"null".into()],2)]);
        assert_eq!(key_columns(&json!("a")),vec!["a"]);
        assert_eq!(key_columns(&json!(["a",1,"b"])),vec!["a","b"]);
        assert!(key_columns(&Value::Null).is_empty());
    }

    #[test]
    fn numeric_bounds_do_not_round_large_integers_or_decimal_neighbors() {
        use std::cmp::Ordering::{Less,Equal,Greater};
        for (left,right,expected) in [
            ("-9007199254740993","-9007199254740992",Less),
            ("9007199254740993","9007199254740992",Greater),
            ("1.00000000000000000001","1",Greater),
            ("1e100","9e99",Greater),("1.0","1",Equal),
            ("\"a\"","\"b\"",Less),("-Infinity","Infinity",Less),
        ] {assert_eq!(numeric_cmp(left,right),expected,"{left}, {right}");}
    }
