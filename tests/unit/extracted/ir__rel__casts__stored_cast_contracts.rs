
    use super::*;
    use crate::ir::{catalog::PropertyGraph, expr::Lit, policy::{GraphPlanPolicy, Language}};
    use crate::ir::rel::RelBackendOptions;

    fn context(graph: &PropertyGraph) -> LoweringContext<'_> {
        LoweringContext {
            island_memo: None, graph, options: RelBackendOptions::default(),
            policy: GraphPlanPolicy::cypher(), language: Language::Cypher,
            scan_counter: 0, correlate_plan: None, rdf_typed_terms_used: false,
            scan_logical_labels: false, gremlin_label_binds: Default::default(),
            unordered_collects: Default::default(), unordered_distincts: Default::default(),
        }
    }

    fn add_values(graph: &mut PropertyGraph, label: &str, edge: bool, values: Vec<Value>) {
        use arrow::{array::{ArrayRef,Int64Array,RecordBatch,StringArray},datatypes::{Field,Schema}};
        use std::{sync::Arc,collections::HashMap};
        let field=Field::new("v",DataType::Utf8,true).with_metadata(HashMap::from([
            ("orchiddb.value_type".into(),"value".into())]));
        let mut fields=vec![field];
        let mut arrays:Vec<ArrayRef>=vec![Arc::new(StringArray::from(values.iter().map(|v|format!("{v:?}")).collect::<Vec<_>>()))];
        if edge {
            for name in ["__src_id","__dst_id"] {
                fields.push(Field::new(name,DataType::Int64,false));
                arrays.push(Arc::new(Int64Array::from(vec![0;values.len()])));
            }
            fields.rotate_left(1);
            arrays.rotate_left(1);
        }
        let batch=RecordBatch::try_new(Arc::new(Schema::new(fields)),arrays).unwrap();
        if edge {graph.add_edges(crate::ir::catalog::EdgeTable {rel_type:label.into(),src_label:"P".into(),dst_label:"P".into(),batch}).unwrap();}
        else {graph.add_nodes(crate::ir::catalog::NodeTable {label:label.into(),batch});}
    }

    #[test]
    fn stored_cast_proof_checks_node_and_edge_tables_and_duplicate_values() {
        for edge in [false, true] {
            let mut graph = PropertyGraph::new();
            add_values(&mut graph,"P",edge,vec![
                Value::List(vec![Value::Int(1),Value::Null,Value::Int(-2)]),
                Value::List(vec![Value::Int(1),Value::Null,Value::Int(-2)]),
                Value::List(vec![]),Value::Null]);
            for (target,rewrite) in [("INT64[]",ListTextRewrite::Identity),("DOUBLE[]",ListTextRewrite::IntegersToFloat)] {
                let args=vec![IrExpr::Binding("ignored".into()),IrExpr::Lit(Lit::String(target.into()))];
                context(&graph).verify_stored_list_cast("v","cast",&args,rewrite).unwrap();
            }
            add_values(&mut graph,"Other",edge,vec![Value::List(vec![Value::String("bad".into())])]);
            let args=vec![IrExpr::Binding("ignored".into()),IrExpr::Lit(Lit::String("INT64[]".into()))];
            assert!(context(&graph).verify_stored_list_cast("v","cast",&args,ListTextRewrite::Identity).is_err());
        }
    }

    #[test]
    fn list_cast_proof_rejects_display_changes_and_dynamic_float_lists() {
        let mut graph=PropertyGraph::new();
        add_values(&mut graph,"P",false,vec![Value::List(vec![Value::Int(1)])]);
        let context=context(&graph);
        let args=vec![IrExpr::Binding("v".into()),IrExpr::Lit(Lit::String("DOUBLE[]".into()))];
        assert!(context.verify_stored_list_cast("v","cast",&args,ListTextRewrite::Identity)
            .unwrap_err().to_string().contains("display text"));
        let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        assert!(context.lower_list_cast(&plan,"cast",&[],"INT64[]").unwrap_err().to_string().contains("arity"));
        assert!(context.lower_list_cast(&plan,"cast",&args,"DOUBLE[]").unwrap_err().to_string().contains("dynamic list"));
    }
    #[test]
    fn projected_union_tags_follow_values_and_do_not_invent_metadata() {
        let graph=PropertyGraph::new();
        let context=context(&graph);
        let plan=datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        let union=|tag:&str| IrExpr::Call {name:"map".into(),args:vec![
            IrExpr::Lit(Lit::String("__tag".into())),IrExpr::Lit(Lit::String(tag.into())),
            IrExpr::Lit(Lit::String("value".into())),IrExpr::Lit(Lit::Int(1))]};
        assert_eq!(context.projected_union_tags(&plan,&union("number")).unwrap(),Some(lit("number")));
        let list=IrExpr::List(vec![union("number"),IrExpr::Lit(Lit::Null),union("other")]);
        let expected=datafusion::functions_nested::expr_fn::make_array(vec![lit("number"),
            lit(datafusion::common::ScalarValue::Utf8(None)),lit("other")]);
        assert_eq!(context.projected_union_tags(&plan,&list).unwrap(),Some(expected));
        for expr in [IrExpr::List(vec![]),IrExpr::List(vec![union("number"),IrExpr::Lit(Lit::Int(1))]),
            IrExpr::Lit(Lit::Int(1)),IrExpr::Binding("missing".into()),
            IrExpr::Call{name:"does_not_exist".into(),args:vec![]},
            IrExpr::Call{name:"map".into(),args:vec![IrExpr::Lit(Lit::String("k".into())),IrExpr::Binding("x".into())]}] {
            assert!(context.projected_union_tags(&plan,&expr).unwrap().is_none(),"{expr:?}");
        }
        let column=value_union_tag_col("u");
        let plan=datafusion::logical_expr::LogicalPlanBuilder::from(plan)
            .project(vec![lit("number").alias(&column)]).unwrap().build().unwrap();
        assert_eq!(context.projected_union_tags(&plan,&IrExpr::Binding("u".into())).unwrap(),Some(col_exact(&column)));
        assert_eq!(context.lower_value_union_tag(&plan,&IrExpr::Binding("u".into())).unwrap(),Some(col_exact(&column)));
        for expr in [IrExpr::Binding("absent".into()),IrExpr::Lit(Lit::Int(1)),
            IrExpr::Call{name:"list_extract".into(),args:vec![IrExpr::Binding("u".into()),IrExpr::Lit(Lit::Int(0))]},
            IrExpr::Call{name:"list_extract".into(),args:vec![]}] {
            assert!(context.lower_value_union_tag(&plan,&expr).unwrap().is_none());
        }
    }

