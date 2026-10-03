//! Query compilation must not depend on the host thread's stack size.
use orchiddb::ir::catalog::PropertyGraph;
use orchiddb::ir::rel::runtime::{execute, execute_rows_with_jvm};
use orchiddb::language::cypher::{parser::parse_query, planner::CypherPlanner};

fn on_worker_stack(stack_bytes: usize, test: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(stack_bytes)
        .spawn(test)
        .unwrap()
        .join()
        .unwrap();
}

fn creates(count: usize) -> String {
    (0..count)
        .map(|i| format!("CREATE (:Stress {{id: {i}}}) "))
        .collect()
}

#[test]
fn long_create_statement_executes_on_a_small_worker_stack() {
    for stack_bytes in [2 * 1024 * 1024, 16 * 1024 * 1024] {
        on_worker_stack(stack_bytes, || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async {
                for raw_rows in [false, true] {
                    let graph = PropertyGraph::new();
                    let ast = parse_query(&creates(500)).unwrap();
                    let plan = CypherPlanner::new().plan(&ast).unwrap();
                    if raw_rows {
                        execute_rows_with_jvm(&plan, &graph, Default::default())
                            .await
                            .unwrap();
                    } else {
                        execute(&plan, &graph, None).await.unwrap();
                    }
                    let ast = parse_query("MATCH (n:Stress) RETURN count(n)").unwrap();
                    let plan = CypherPlanner::new().plan(&ast).unwrap();
                    let (returned, _) = execute(&plan, &graph, None).await.unwrap();
                    assert_eq!(returned.batch.num_rows(), 1);
                    assert_eq!(
                        arrow::util::display::array_value_to_string(returned.batch.column(0), 0)
                            .unwrap(),
                        "500"
                    );
                }
            });
        });
    }
}

#[test]
fn excessive_plan_depth_returns_an_error_before_mutating_the_graph() {
    on_worker_stack(2 * 1024 * 1024, || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let graph = PropertyGraph::new();
            let ast = parse_query(&creates(600)).unwrap();
            let plan = CypherPlanner::new().plan(&ast).unwrap();
            let error = execute(&plan, &graph, None).await.unwrap_err();
            assert!(error.contains("query plan depth exceeds the execution limit of 512"));
            let error = execute_rows_with_jvm(&plan, &graph, Default::default())
                .await
                .unwrap_err();
            assert!(error.contains("query plan depth exceeds the execution limit of 512"));

            let ast = parse_query("MATCH (n) RETURN count(n)").unwrap();
            let plan = CypherPlanner::new().plan(&ast).unwrap();
            let (returned, _) = execute(&plan, &graph, None).await.unwrap();
            assert_eq!(
                arrow::util::display::array_value_to_string(returned.batch.column(0), 0).unwrap(),
                "0"
            );
        });
    });
}
