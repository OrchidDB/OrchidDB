
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
