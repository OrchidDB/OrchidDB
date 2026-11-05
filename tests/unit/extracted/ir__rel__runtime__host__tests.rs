
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};



    // Test-only Hybrid reference harness for kernel semantics and transaction
    // behavior. Production scheduling is SQL-required and tested separately.
    pub(crate) fn reference_context() -> crate::ir::rules::RuleContext {
        let mut context = crate::ir::rules::RuleContext::default();
        context.lowering_policy = policy::LoweringPolicy::Hybrid;
        context
    }
    #[derive(Debug, Default)]
    pub(crate) struct HostRunner {
        calls: AtomicUsize,
    }
    impl SubplanRunner for HostRunner {
        fn run(&self, plan: &LogicalPlan, state: &mut KernelState) -> IrResult<Vec<Row>> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            let kernel = RowKernel::from_plan(plan).expect("native compiled descriptor");
            if let Some(mut cursor) = kernel.source_cursor() {
                let mut output = Vec::new();
                while let Some(rows) = cursor
                    .next(2, state)
                    .map_err(|e| RuntimeError::Runtime(e.to_string()))?
                {
                    output.extend(rows);
                }
                return Ok(output);
            }
            let inputs = kernel
                .inputs()
                .iter()
                .map(|input| self.run(input, state))
                .collect::<IrResult<Vec<_>>>()?;
            kernel
                .invoke(inputs, state)
                .map_err(|e| RuntimeError::Runtime(e.to_string()))
        }
    }
    #[test]
    fn nullable_optional_path_uses_existing_host_kernels() {
        for function in ["nodes", "relationships"] {
            let request = serde_json::from_value(serde_json::json!({
                "version": 1, "dialect": "duckdb", "language": "cypher",
                "query": format!("WITH null AS a OPTIONAL MATCH p = (a)-[r]->() RETURN {function}(p) AS path, {function}(null) AS literal"),
                "tables": [], "native_values": true
            })).unwrap();
            let prepared = crate::compiler::prepare_graph(&request).unwrap();
            let compiled = compile_for_host(
                &prepared.plan,
                &prepared.graph,
                CompileOptions {
                    sql_islands: false,
                    rule_context: reference_context(),
                    language_functions: true,
                    ..Default::default()
                },
            )
            .unwrap();
            let runner = Arc::new(HostRunner::default());
            let mut state =
                KernelState::for_host(prepared.graph, runner.clone(), &reference_context());
            let rows = runner.run(&compiled, &mut state).unwrap();
            assert_eq!(rows.len(), 1, "{function}");
            assert!(matches!(rows[0].bindings.get("path"), Some(Value::Null)));
            assert!(matches!(rows[0].bindings.get("literal"), Some(Value::Null)));
        }
    }

#[test]
fn cypher_host_kernels_preserve_collection_correlation_and_mutation_results() {
    use std::collections::BTreeMap;
    let cases: &[(&str,&[i64])]=&[
        ("UNWIND [1,null,2,2] AS x RETURN count(*) AS r",&[4]),
        ("UNWIND [1,null,2,2] AS x RETURN count(x) AS r",&[3]),
        ("UNWIND [1,null,2,2] AS x RETURN sum(x) AS r",&[5]),
        ("UNWIND [] AS x RETURN count(*) AS r",&[0]),
        ("UNWIND [1,2,2] AS x RETURN DISTINCT x AS r ORDER BY r DESC",&[2,1]),
        ("WITH [1,2,3] AS xs RETURN size([x IN xs WHERE x>1 | x*2]) AS r",&[2]),
        ("RETURN list_reduce([1,2,3],(a,x) -> a+x) AS r",&[6]),
        ("RETURN CASE WHEN any(x IN [null,false] WHERE x) IS NULL THEN 1 ELSE 0 END AS r",&[1]),
        ("CALL { RETURN 1 AS x UNION ALL RETURN 2 AS x } RETURN sum(x) AS r",&[3]),
        ("MATCH (n:Person) WHERE EXISTS { MATCH (n)-[:R]->() } RETURN n.id AS r ORDER BY r",&[1,2]),
        ("MATCH (n:Person) RETURN size([(n)-[:R]->(m) | m.id]) AS r ORDER BY n.id",&[1,1,0]),
        ("MATCH p=(a:Person)-[:R*0..2]->(b:Person) RETURN count(*) AS r",&[6]),
        ("MATCH (n:Person) OPTIONAL MATCH (n)-[e:R]->(m) RETURN n.id AS id, count(e) AS r ORDER BY id",&[1,1,0]),
        ("CREATE (n:Person {id:4}) WITH n MATCH (m:Person) RETURN count(m) AS r",&[4]),
        ("MERGE (n:Person {id:2}) ON MATCH SET n.score=5 RETURN n.score AS r",&[5]),
        ("MATCH (n:Person {id:3}) DETACH DELETE n RETURN count(*) AS r",&[1]),
    ];
    for (query,expected) in cases {
        for native_values in [false,true] {
            let graph=PropertyGraph::new();
            let nodes=(1..=3).map(|id|graph.insert_node("Person",BTreeMap::from([("id".into(),Value::Long(id))]))).collect::<Vec<_>>();
            graph.insert_edge("R",&nodes[0],&nodes[1],BTreeMap::new()).unwrap();
            graph.insert_edge("R",&nodes[1],&nodes[2],BTreeMap::new()).unwrap();
            let plan=crate::language::cypher::preparation::prepare(query,&BTreeMap::new(),Some(graph.procedures.as_ref())).unwrap_or_else(|e|panic!("{query}: {e:?}"));
            let compiled=compile_for_host(&plan,&graph,CompileOptions{
                sql_islands:false,rule_context:reference_context(),language_functions:true,native_values,..Default::default()
            }).unwrap_or_else(|e|panic!("{query}: {e:?}"));
            let runner=Arc::new(HostRunner::default());
            let mut state=KernelState::for_host(graph,runner.clone(),&reference_context());
            let rows=runner.run(&compiled,&mut state).unwrap_or_else(|e|panic!("{query}: {e}"));
            let actual=rows.iter().map(|row|row.get("r").as_i64().expect("numeric result")).collect::<Vec<_>>();
            assert_eq!(&actual,expected,"{query}; native={native_values}");
        }
    }
}

#[test]
fn sql_native_aggregate_policy_cannot_fall_back_to_conformant_host_aggregation() {
    let graph=PropertyGraph::new();
    let plan=crate::language::cypher::preparation::prepare("UNWIND [1,2] AS x RETURN sum(x) AS r",&Default::default(),Some(graph.procedures.as_ref())).unwrap();
    let mut context=reference_context();
    context.profile.cypher_aggregates=crate::ir::rules::CypherAggregatePolicy::SqlNative;
    let error=compile_for_host(&plan,&graph,CompileOptions{sql_islands:false,rule_context:context,..Default::default()}).unwrap_err();
    assert!(error.message.contains("SQL-native aggregate semantics require SQL lowering"),"{error:?}");
}

#[test]
fn compiled_kernel_rewrites_preserve_identity_and_validate_dependency_shape() {
    use datafusion::logical_expr::{UserDefinedLogicalNodeCore,lit};
    let graph=PropertyGraph::new();
    let plan=crate::language::cypher::preparation::prepare("RETURN 7 AS r",&Default::default(),Some(graph.procedures.as_ref())).unwrap();
    let compiled=compile_for_host(&plan,&graph,CompileOptions{sql_islands:false,rule_context:reference_context(),..Default::default()}).unwrap();
    let kernel=RowKernel::from_plan(&compiled).unwrap();
    let inputs=kernel.inputs().to_vec();
    assert!(kernel.with_exprs_and_inputs(vec![lit(1_i64)],inputs.clone()).unwrap_err().to_string().contains("Invalid kernel rewrite"));
    let mut wrong=inputs.clone();wrong.push(compiled.clone());
    assert!(kernel.with_exprs_and_inputs(vec![],wrong).is_err());
    let copy=kernel.with_exprs_and_inputs(vec![],inputs).unwrap();
    assert_eq!(&copy,kernel);
    assert_eq!(copy.partial_cmp(kernel),Some(std::cmp::Ordering::Equal));
    assert!(!format!("{copy:?}").is_empty());
}

#[test]
fn cypher_host_boundary_decodes_nullable_sql_columns_without_interpreting_aliases() {
    use arrow::{array::{ArrayRef, Int64Array, StringArray}, datatypes::{DataType, Field, Schema}};
    let graph = PropertyGraph::new();
    let plan = crate::language::cypher::preparation::prepare("RETURN null AS x", &Default::default(), Some(graph.procedures.as_ref())).unwrap();
    let compiled = compile_for_host(&plan, &graph, CompileOptions { sql_islands: false, rule_context: reference_context(), ..Default::default() }).unwrap();
    let mut kernel = RowKernel::from_plan(&compiled).unwrap().clone();
    let fields = vec!["x".to_string(), "__orchid_traverser_bulk".to_string()];
    kernel.relational_input = Some(fields.clone());
    kernel.relational_input_state = false;
    let schema = Arc::new(Schema::new(vec![Field::new(&fields[0], DataType::Int64, true), Field::new(&fields[1], DataType::Utf8, true)]));
    let batch = RecordBatch::try_new(schema.clone(), vec![
        Arc::new(Int64Array::from(vec![Some(7), None])) as ArrayRef,
        Arc::new(StringArray::from(vec![Some("user value"), None])) as ArrayRef,
    ]).unwrap();
    let decoded = kernel.decode_input(batch).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].get("x"), Value::Long(7));
    assert_eq!(decoded[1].get("x"), Value::Null);
    assert_eq!(decoded[0].get("__orchid_traverser_bulk"), Value::String("user value".into()));
    assert_eq!(decoded[1].get("__orchid_traverser_bulk"), Value::Null);
    assert!(decoded.iter().all(|row| row.bulk == 1));
    assert!(kernel.decode_input(RecordBatch::new_empty(schema)).unwrap().is_empty());
}

#[test]
fn cypher_host_boundary_round_trips_native_values_and_rejects_wrong_transport() {
    use arrow::datatypes::{DataType, Field, Schema};
    let graph = PropertyGraph::new();
    let plan = crate::language::cypher::preparation::prepare("RETURN [1,null,2] AS x", &Default::default(), Some(graph.procedures.as_ref())).unwrap();
    let compiled = compile_for_host(&plan, &graph, CompileOptions { sql_islands: false, rule_context: reference_context(), ..Default::default() }).unwrap();
    let mut kernel = RowKernel::from_plan(&compiled).unwrap().clone();
    kernel.relational_input = None;
    kernel.relational_output = None;
    let row = Row { bindings: [("x".into(), Value::List(vec![Value::Long(1), Value::Null, Value::Long(2)]))].into(), bulk: 1 };
    let batch = kernel.encode_output(vec![row.clone()], &KernelState::default()).unwrap();
    let decoded = kernel.decode_input(batch).unwrap();
    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].bindings, row.bindings);
    assert_eq!(decoded[0].bulk, 1);
    let invalid = RecordBatch::new_empty(Arc::new(Schema::new(vec![Field::new("x", DataType::Int64, true)])));
    assert!(kernel.decode_input(invalid).unwrap_err().to_string().contains("Invalid traverser batch schema"));
}
