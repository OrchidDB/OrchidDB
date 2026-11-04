
    use super::*;

    fn context(graph: &PropertyGraph) -> LoweringContext<'_> {
        LoweringContext {
            island_memo: None, graph, options: RelBackendOptions::default(),
            policy: GraphPlanPolicy::cypher(), language: Language::Cypher,
            scan_counter: 0, correlate_plan: None, rdf_typed_terms_used: false,
            scan_logical_labels: false, gremlin_label_binds: Default::default(),
            unordered_collects: Default::default(), unordered_distincts: Default::default(),
        }
    }

    fn walks(start: i64, direction: Direction) -> Vec<(i64,i64,i64)> {
        fn visit(start: i64, current: i64, direction: Direction, used: &mut Vec<usize>, out: &mut Vec<(i64,i64,i64)>) {
            out.push((start,current,used.len() as i64));
            if used.len() == 3 { return; }
            for (edge,(from,to)) in [(0,1),(1,2),(2,0),(1,1)].into_iter().enumerate() {
                if used.contains(&edge) { continue; }
                let next = match direction {
                    Direction::Out if from == current => Some(to),
                    Direction::In if to == current => Some(from),
                    Direction::Both if from == current => Some(to),
                    Direction::Both if to == current => Some(from),
                    _ => None,
                };
                if let Some(next) = next {
                    used.push(edge); visit(start,next,direction,used,out); used.pop();
                }
            }
        }
        let mut out=Vec::new(); visit(start,start,direction,&mut Vec::new(),&mut out); out
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn bounded_compatibility_paths_preserve_trails_and_zero_hop_identity() {
        let graph = PropertyGraph::new();
        let nodes = (0..3).map(|_| graph.insert_node("P", BTreeMap::new())).collect::<Vec<_>>();
        for (from,to) in [(0,1),(1,2),(2,0),(1,1)] {
            graph.insert_edge("R", &nodes[from], &nodes[to], BTreeMap::new()).unwrap();
        }
        for direction in [Direction::Out,Direction::In,Direction::Both] {
            for (existing, payload) in [(false,false),(false,true),(true,false),(true,true)] {
                let mut ctx = context(&graph);
                let input = ctx.lower_node_scan("n", &LabelExpr::Any).unwrap();
                let target = if existing { "n" } else { "m" };
                let rel = "path".to_string();
                let lowered = ctx.lower_expand_varlen_unrolled(input,"n",target,
                    if existing {TargetMode::Existing} else {TargetMode::BindNew},
                    &LabelExpr::Any,payload.then_some(&rel),&LabelExpr::Any,direction,
                    &crate::ir::plan::Length{min:0,max:Some(3)},None).unwrap();
                let plan = LogicalPlanBuilder::from(lowered.plan).project(vec![
                    col_exact(id_col("n")).alias("source"),
                    col_exact(id_col(target)).alias("target"),
                    if payload { col_exact(path_len_col(&rel)).alias("length") } else { lit(0_i64).alias("length") },
                ]).unwrap().build().unwrap();
                let lowered = LoweredPlan { plan, fields: vec!["source".into(),"target".into(),"length".into()],
                    result_form: ResultForm::RowSet, rule_context: Default::default(), islands: Default::default() };
                let mut executor = sql::DuckDbExecutor::default();
                executor.set_language_functions(true).unwrap();
                let batches = vec![sql::execute_lowered_sql(&mut executor, &lowered).await.unwrap().batch];
                let mut actual = Vec::new();
                for batch in batches {
                    for row in 0..batch.num_rows() {
                        let numbers = (0..3).map(|column| ScalarValue::try_from_array(batch.column(column),row).unwrap().to_string().parse::<i64>().unwrap()).collect::<Vec<_>>();
                        actual.push((numbers[0],numbers[1],numbers[2]));
                    }
                }
                let mut expected = (0..3).flat_map(|start|walks(start,direction))
                    .filter(|(start,end,_)|!existing || start==end)
                    .map(|(start,end,length)|(start,end,if payload {length} else {0})).collect::<Vec<_>>();
                actual.sort(); expected.sort();
                assert_eq!(actual,expected,"{direction:?}, existing={existing}, payload={payload}");
            }
        }
    }

    #[test]
    fn bounded_fallback_rejects_unbounded_oversized_and_empty_ranges() {
        let graph = PropertyGraph::new();
        graph.insert_node("P",BTreeMap::new());
        for (length,message) in [
            (crate::ir::plan::Length{min:1,max:None}, "cannot safely unroll"),
            (crate::ir::plan::Length{min:1,max:Some(7)}, "exceeds unroll cap"),
            (crate::ir::plan::Length{min:2,max:Some(1)}, "had no branches"),
        ] {
            let mut ctx=context(&graph);
            let input=ctx.lower_node_scan("n",&LabelExpr::Any).unwrap();
            let error=ctx.lower_expand_varlen_unrolled(input,"n","m",TargetMode::BindNew,
                &LabelExpr::Any,None,&LabelExpr::Any,Direction::Out,&length,None).unwrap_err();
            assert!(error.to_string().contains(message),"{error}");
        }
    }
