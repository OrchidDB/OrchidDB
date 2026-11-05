
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

    #[test]
    fn mapped_endpoint_pruning_respects_direction_partitions_and_dynamic_labels() {
        use crate::ir::rel::mapping::{GraphMapping, NodeMapping, EdgeMapping};
        let graph = PropertyGraph::new();
        let mut ctx = context(&graph);
        let labels = LabelExpr::AnyOf(vec!["C".into(), "A".into(), "B".into(), "A".into()]);
        assert_eq!(ctx.mapped_endpoint_labels(&labels, &LabelExpr::Any, Direction::Out).unwrap(), labels);
        let mut mapping = GraphMapping::new();
        for label in ["A", "B", "C", "Unrelated"] {
            mapping.map_node(NodeMapping::table(label, label, "id"));
        }
        mapping.map_edge(EdgeMapping::table("R", "ab", "src", "dst", "A", "B"));
        mapping.map_edge_partition(EdgeMapping::table("R", "cb", "src", "dst", "C", "B"));
        ctx.options.mapping = Some(Arc::new(mapping.clone()));
        for (direction, expected) in [
            (Direction::Out, vec!["B"]), (Direction::In, vec!["A", "C"]),
            (Direction::Both, vec!["A", "B", "C"]),
        ] {
            assert_eq!(ctx.mapped_endpoint_labels(&labels, &LabelExpr::label("R"), direction).unwrap(),
                LabelExpr::AnyOf(expected.into_iter().map(str::to_owned).collect()));
        }
        assert_eq!(ctx.mapped_endpoint_labels(&LabelExpr::label("Unrelated"), &LabelExpr::Any, Direction::Both).unwrap(), LabelExpr::AnyOf(vec![]));
        assert_eq!(ctx.mapped_endpoint_labels(&labels, &LabelExpr::label("Missing"), Direction::Both).unwrap(), LabelExpr::AnyOf(vec![]));
        assert!(ctx.mapped_endpoint_labels(&LabelExpr::Not(Box::new(LabelExpr::label("A"))), &LabelExpr::Any, Direction::Out).is_err());
        assert!(ctx.mapped_endpoint_labels(&labels, &LabelExpr::AllOf(vec!["R".into(), "S".into()]), Direction::Out).is_err());
        mapping.map_node(NodeMapping::table("Dynamic", "dynamic", "id").labels_column("labels"));
        ctx.options.mapping = Some(Arc::new(mapping));
        // Logical memberships cannot be inferred from physical endpoint namespaces.
        assert_eq!(ctx.mapped_endpoint_labels(&labels, &LabelExpr::label("R"), Direction::Out).unwrap(), labels);
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn existing_endpoint_label_restriction_preserves_identity_and_columns() {
        let graph = PropertyGraph::new();
        graph.insert_node("A", BTreeMap::from([("value".into(), Value::Int(11))]));
        graph.insert_node("B", BTreeMap::from([("value".into(), Value::Int(22))]));
        graph.insert_node("A", BTreeMap::from([("value".into(), Value::Int(33))]));
        for (labels, mut expected) in [
            (LabelExpr::Any, vec![("A", 0_i64), ("B", 0), ("A", 1)]),
            (LabelExpr::label("A"), vec![("A", 0), ("A", 1)]),
            (LabelExpr::label("B"), vec![("B", 0)]),
            (LabelExpr::label("Missing"), vec![]),
            (LabelExpr::AnyOf(vec!["A".into(), "B".into()]), vec![("A", 0), ("B", 0), ("A", 1)]),
        ] {
            let mut ctx = context(&graph);
            let input = ctx.lower_node_scan("n", &LabelExpr::Any).unwrap();
            let schema = input.plan.schema().clone();
            let restricted = ctx.restrict_existing_target(input, "n", &labels).unwrap();
            assert_eq!(restricted.plan.schema().as_arrow(), schema.as_arrow());
            let plan = LogicalPlanBuilder::from(restricted.plan).project(vec![col_exact(label_col("n")).alias("label"), col_exact(id_col("n")).alias("id")]).unwrap().build().unwrap();
            let lowered = LoweredPlan { plan, fields: vec!["label".into(), "id".into()], result_form: ResultForm::RowSet,
                rule_context: Default::default(), islands: restricted.islands };
            let mut executor = sql::DuckDbExecutor::default();
            let batch = sql::execute_lowered_sql(&mut executor, &lowered).await.unwrap().batch;
            let mut actual = (0..batch.num_rows()).map(|row| (
                ScalarValue::try_from_array(batch.column(0), row).unwrap().to_string(),
                ScalarValue::try_from_array(batch.column(1), row).unwrap().to_string().parse::<i64>().unwrap(),
            )).collect::<Vec<_>>();
            let mut expected = expected.drain(..).map(|(label, id)| (label.to_owned(), id)).collect::<Vec<_>>();
            actual.sort(); expected.sort();
            assert_eq!(actual, expected, "{labels:?}");
        }
    }

    #[test]
    fn native_node_rebinding_preserves_existing_and_non_element_bindings() {
        let graph = PropertyGraph::new();
        graph.insert_node("A", BTreeMap::new());
        let mut ctx = context(&graph);
        let input = ctx.lower_node_scan("n", &LabelExpr::Any).unwrap();
        let original = input.plan.clone();
        for name in ["n", "missing"] {
            let rebound = ctx.bind_native_node(input.clone(), name).unwrap();
            assert_eq!(rebound.plan, original);
        }
        let scalar = input.with_plan(LogicalPlanBuilder::from(original).project(vec![lit(42_i64).alias("scalar")]).unwrap().build().unwrap());
        let rebound = ctx.bind_native_node(scalar.clone(), "scalar").unwrap();
        assert_eq!(rebound.plan, scalar.plan);
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn fixed_relationship_history_compares_label_qualified_identity() {
        let graph = PropertyGraph::new();
        let ctx = context(&graph);
        for (old_label, old_id, new_label, new_id, count) in [
            ("A", "1", "A", "1", 0),
            ("A", "1", "A", "2", 1),
            ("A", "1", "B", "1", 1),
            ("A:B", "c", "A", "B:c", 1),
            ("雪", "x:y", "雪", "x:y", 0),
            ("", "", "", "", 0),
        ] {
            for retain_typed_ids in [true, false] {
                let plan = LogicalPlanBuilder::empty(true).project(vec![
                    lit(old_id).alias(id_col("r")), lit(old_label).alias(label_col("r")),
                ]).unwrap().build().unwrap();
                let first = ctx.track_relationship(LoweredNode::new(plan), "trail", "r").unwrap();
                let mut columns = vec![col_exact("trail"), lit(new_id).alias(id_col("r")), lit(new_label).alias(label_col("r"))];
                if retain_typed_ids {
                    columns.extend([col_exact("trail__e0_id"), col_exact("trail__e0_label")]);
                }
                let second_input = first.clone().with_plan(LogicalPlanBuilder::from(first.plan).project(columns).unwrap().build().unwrap());
                let second = ctx.track_relationship(second_input, "trail", "r").unwrap();
                let plan = LogicalPlanBuilder::from(second.plan).project(vec![lit(1_i64).alias("present")]).unwrap().build().unwrap();
                let lowered = LoweredPlan { plan, fields: vec!["present".into()], result_form: ResultForm::RowSet,
                    rule_context: Default::default(), islands: second.islands };
                let mut executor = sql::DuckDbExecutor::default();
                let result = sql::execute_lowered_sql(&mut executor, &lowered).await.unwrap().batch;
                assert_eq!(result.num_rows(), count, "{old_label}/{old_id} -> {new_label}/{new_id}, typed={retain_typed_ids}");
            }
        }
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn native_node_rebinding_recovers_label_qualified_identity_and_properties() {
        let graph = PropertyGraph::new();
        graph.insert_node("A", BTreeMap::from([("value".into(), Value::Int(11))]));
        graph.insert_node("B", BTreeMap::from([("value".into(), Value::Int(22))]));
        graph.insert_node("A", BTreeMap::from([("value".into(), Value::Int(33))]));
        for (labels, expected) in [
            (LabelExpr::Any, vec![("A", 0_i64, 11_i64), ("A", 1, 33), ("B", 0, 22)]),
            (LabelExpr::label("A"), vec![("A", 0, 11), ("A", 1, 33)]),
            (LabelExpr::label("B"), vec![("B", 0, 22)]),
        ] {
            let mut ctx = context(&graph);
            let input = ctx.lower_node_scan("n", &LabelExpr::Any).unwrap();
            let native = ctx.native_expr(&input.plan, &IrExpr::binding("n")).unwrap();
            let plan = LogicalPlanBuilder::from(input.plan).project(vec![native.alias("value")]).unwrap().build().unwrap();
            let input = LoweredNode::new(plan);
            assert!(has_binding_shape(&input.plan, "value").is_none());
            let rebound = ctx.bind_native_node_with_labels(input, "value", &labels).unwrap();
            assert_eq!(has_binding_shape(&rebound.plan, "value"), Some(BindingShape::Node));
            assert!(has_exact_col(&rebound.plan, "value"));
            assert!(!rebound.plan.schema().fields().iter().any(|field| field.name().starts_with("__orchid_value_node_")));
            let plan = LogicalPlanBuilder::from(rebound.plan).project(vec![
                col_exact(label_col("value")).alias("label"), col_exact(id_col("value")).alias("id"),
                col_exact(prop_col("value", "value")).alias("property"),
            ]).unwrap().build().unwrap();
            let lowered = LoweredPlan { plan, fields: vec!["label".into(), "id".into(), "property".into()],
                result_form: ResultForm::RowSet, rule_context: Default::default(), islands: rebound.islands };
            let mut executor = sql::DuckDbExecutor::default();
            executor.set_language_functions(true).unwrap();
            let batch = sql::execute_lowered_sql(&mut executor, &lowered).await.unwrap().batch;
            let mut actual = (0..batch.num_rows()).map(|row| (
                ScalarValue::try_from_array(batch.column(0), row).unwrap().to_string(),
                ScalarValue::try_from_array(batch.column(1), row).unwrap().to_string().parse::<i64>().unwrap(),
                ScalarValue::try_from_array(batch.column(2), row).unwrap().to_string().parse::<i64>().unwrap(),
            )).collect::<Vec<_>>();
            actual.sort();
            assert_eq!(actual, expected.into_iter().map(|(label, id, value)| (label.to_owned(), id, value)).collect::<Vec<_>>(), "{labels:?}");
        }
    }

    #[test]
    fn native_node_rebinding_does_not_open_computed_sources() {
        use crate::ir::rel::mapping::{GraphMapping, NodeMapping};
        let graph = PropertyGraph::new();
        graph.insert_node("A", BTreeMap::new());
        let mut ctx = context(&graph);
        let input = ctx.lower_node_scan("n", &LabelExpr::Any).unwrap();
        let native = ctx.native_expr(&input.plan, &IrExpr::binding("n")).unwrap();
        let plan = LogicalPlanBuilder::from(input.plan).project(vec![native.alias("value")]).unwrap().build().unwrap();
        let mut mapping = GraphMapping::new();
        mapping.map_node(NodeMapping::query("External", "SELECT id FROM external_service", "id"));
        ctx.options.mapping = Some(Arc::new(mapping));
        let before = ctx.scan_counter;
        let rebound = ctx.bind_native_node(LoweredNode::new(plan.clone()), "value").unwrap();
        assert_eq!(rebound.plan, plan);
        assert_eq!(ctx.scan_counter, before);
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn native_relationship_history_preserves_membership_after_list_transport() {
        let graph = PropertyGraph::new();
        let ctx = context(&graph);
        for (new_label, new_id, expected_rows) in [("A", "1", 0), ("B", "1", 1), ("A", "2", 1)] {
            for packed_list in [true, false] {
                let plan = LogicalPlanBuilder::empty(true).project(vec![
                    lit("1").alias(id_col("r")), lit("A").alias(label_col("r")),
                ]).unwrap().build().unwrap();
                let first = ctx.track_relationship(LoweredNode::new(plan), "trail", "r").unwrap();
                let packed = native_values::typed::call("pack", vec![col_exact("trail")]);
                let history = if packed_list { packed } else { native_values::typed::call("items", vec![packed]) };
                let plan = LogicalPlanBuilder::from(first.plan).project(vec![
                    history.alias("trail"), lit(new_id).alias(id_col("r")), lit(new_label).alias(label_col("r")),
                ]).unwrap().build().unwrap();
                let second = ctx.track_relationship(LoweredNode::new(plan), "trail", "r").unwrap();
                let plan = LogicalPlanBuilder::from(second.plan).project(vec![
                    datafusion::functions_nested::expr_fn::array_length(col_exact("trail")).alias("length"),
                ]).unwrap().build().unwrap();
                let lowered = LoweredPlan { plan, fields: vec!["length".into()], result_form: ResultForm::RowSet,
                    rule_context: Default::default(), islands: second.islands };
                let mut executor = sql::DuckDbExecutor::default();
                executor.set_language_functions(true).unwrap();
                let batch = sql::execute_lowered_sql(&mut executor, &lowered).await.unwrap().batch;
                assert_eq!(batch.num_rows(), expected_rows, "{new_label}/{new_id}, packed={packed_list}");
                if expected_rows > 0 {
                    assert_eq!(ScalarValue::try_from_array(batch.column(0), 0).unwrap().to_string(), "2");
                }
            }
        }
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn native_node_rebinding_keeps_duplicates_and_rejects_non_node_values() {
        let graph = PropertyGraph::new();
        graph.insert_node("A", BTreeMap::new());
        let mut ctx = context(&graph);
        let input = ctx.lower_node_scan("n", &LabelExpr::Any).unwrap();
        let native = ctx.native_expr(&input.plan, &IrExpr::binding("n")).unwrap();
        let one = LogicalPlanBuilder::from(input.plan).project(vec![native.alias("v")]).unwrap().build().unwrap();
        let duplicate = LogicalPlanBuilder::from(one.clone()).union(one).unwrap().build().unwrap();
        for (plan, expected_rows) in std::iter::once((duplicate, 2)).chain([
            lit(ScalarValue::Null), lit(0_i64), lit("A:0"), lit(false),
        ].into_iter().map(|value| (LogicalPlanBuilder::empty(true).project(vec![
            native_values::typed::call("pack", vec![value]).alias("v"),
        ]).unwrap().build().unwrap(), 0))) {
            let rebound = ctx.bind_native_node(LoweredNode::new(plan), "v").unwrap();
            let plan = LogicalPlanBuilder::from(rebound.plan).project(vec![col_exact(id_col("v")).alias("id")]).unwrap().build().unwrap();
            let lowered = LoweredPlan { plan, fields: vec!["id".into()], result_form: ResultForm::RowSet,
                rule_context: Default::default(), islands: rebound.islands };
            let mut executor = sql::DuckDbExecutor::default();
            executor.set_language_functions(true).unwrap();
            let batch = sql::execute_lowered_sql(&mut executor, &lowered).await.unwrap().batch;
            assert_eq!(batch.num_rows(), expected_rows);
            for row in 0..batch.num_rows() {
                assert_eq!(ScalarValue::try_from_array(batch.column(0), row).unwrap().to_string(), "0");
            }
        }
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn relationship_history_retains_binary_and_composite_identity_domains() {
        let graph = PropertyGraph::new();
        let ctx = context(&graph);
        let composite = |number, text| df_core::named_struct(vec![lit("number"), lit(number), lit("text"), lit(text)]);
        for (old_id, other_id) in [
            (lit(ScalarValue::Binary(Some(vec![0, 255, 58]))), lit(ScalarValue::Binary(Some(vec![0, 255, 59])))),
            (lit(ScalarValue::Binary(Some(vec![]))), lit(ScalarValue::Binary(Some(vec![0])))),
            (composite(1_i64, "a:b"), composite(1_i64, "a:c")),
            (composite(1_i64, "same"), composite(2_i64, "same")),
        ] {
            for (new_label, new_id, expected_rows) in [
                ("A", old_id.clone(), 0), ("A", other_id, 1), ("B", old_id.clone(), 1),
            ] {
                for typed_history in [true, false] {
                    let plan = LogicalPlanBuilder::empty(true).project(vec![
                        old_id.clone().alias(id_col("r")), lit("A").alias(label_col("r")),
                    ]).unwrap().build().unwrap();
                    let first = ctx.track_relationship(LoweredNode::new(plan), "trail", "r").unwrap();
                    let mut columns = vec![col_exact("trail"), new_id.clone().alias(id_col("r")), lit(new_label).alias(label_col("r"))];
                    if typed_history { columns.extend([col_exact("trail__e0_id"), col_exact("trail__e0_label")]); }
                    let plan = LogicalPlanBuilder::from(first.plan).project(columns).unwrap().build().unwrap();
                    let second = ctx.track_relationship(LoweredNode::new(plan), "trail", "r").unwrap();
                    let plan = LogicalPlanBuilder::from(second.plan).project(vec![
                        datafusion::functions_nested::expr_fn::array_length(col_exact("trail")).alias("length"),
                    ]).unwrap().build().unwrap();
                    let lowered = LoweredPlan { plan, fields: vec!["length".into()], result_form: ResultForm::RowSet,
                        rule_context: Default::default(), islands: second.islands };
                    let mut executor = sql::DuckDbExecutor::default();
                    executor.set_language_functions(true).unwrap();
                    let batch = sql::execute_lowered_sql(&mut executor, &lowered).await.unwrap().batch;
                    assert_eq!(batch.num_rows(), expected_rows, "old={old_id:?}, new={new_id:?}, label={new_label}, typed={typed_history}");
                    if expected_rows > 0 {
                        assert_eq!(ScalarValue::try_from_array(batch.column(0), 0).unwrap().to_string(), "2");
                    }
                }
            }
        }
    }

    #[cfg(feature = "duckdb")]
    #[tokio::test]
    async fn cypher_fixed_paths_keep_edges_for_internal_looking_user_names() {
        let graph = PropertyGraph::new();
        let first = graph.insert_node("A", BTreeMap::new());
        let last = graph.insert_node("A", BTreeMap::new());
        graph.insert_edge("R", &first, &last, BTreeMap::new()).unwrap();
        for (path, target) in [("p", "m"), ("__path", "m"), ("__path", "__edge_other"), ("p", "__edge_other")] {
            let mut ctx = context(&graph);
            let input = ctx.lower_node_scan("n", &LabelExpr::Any).unwrap();
            let rel = "r".to_owned();
            let expanded = ctx.lower_expand_direction(input, "n", target, TargetMode::BindNew,
                &LabelExpr::Any, Some(&rel), &LabelExpr::Any, Direction::Out).unwrap();
            let expanded = ctx.materialize_fixed_path(expanded, path, "n", target, &rel).unwrap();
            let plan = LogicalPlanBuilder::from(expanded.plan).project(vec![col_exact(path).alias("value")]).unwrap().build().unwrap();
            let lowered = LoweredPlan { plan, fields: vec!["value".into()], result_form: ResultForm::RowSet,
                rule_context: Default::default(), islands: expanded.islands };
            let mut executor = sql::DuckDbExecutor::default();
            executor.set_language_functions(true).unwrap();
            let batch = sql::execute_lowered_sql(&mut executor, &lowered).await.unwrap().batch;
            let rows = native_values::decode_rows(&batch, &["value".into()], &graph).unwrap();
            assert_eq!(rows.len(), 1);
            let Value::Path(items) = rows[0].get("value") else { panic!("expected path for {path}/{target}") };
            assert_eq!(items.len(), 3, "path={path}, target={target}: {items:?}");
            assert!(matches!(&items[0], Value::Node { .. }));
            assert!(matches!(&items[1], Value::Edge { .. }));
            assert!(matches!(&items[2], Value::Node { .. }));
        }
    }
