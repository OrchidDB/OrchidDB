//! Query-local cache of Orchid executable subplans.
use super::*;

#[derive(Debug)]
pub(super) struct PreparedSubplan {
    physical: Arc<dyn ExecutionPlan>,
    state: Arc<Mutex<State>>,
    task: Arc<TaskContext>,
    stats: super::super::dag::DagStats,
    cost: Arc<Mutex<crate::ir::QueryCost>>,
}
pub(super) type SubplanCache = Arc<Mutex<Option<PreparedSubplan>>>;

pub(super) fn run(
    plan: &LogicalPlan,
    cache: &SubplanCache,
    live: &mut State,
) -> IrResult<Vec<Row>> {
    let runtime = tokio::runtime::Handle::current();
    let error = |e: DataFusionError| RuntimeError::Runtime(e.to_string());
    // A query-local slot caches the physical operators, never their output.
    // Taking the slot permits reentrant calls to prepare another instance.
    let cached = cache
        .lock()
        .map_err(|_| RuntimeError::Runtime("Subplan cache poisoned".into()))?
        .take();
    let rule_context = live.context.rule_context.clone();
    let region_session = live.context.region_session.clone();
    let graph = live.graph.clone();
    let prepared = if let Some(prepared) = cached {
        *prepared
            .state
            .lock()
            .map_err(|_| RuntimeError::Runtime("Subplan state poisoned".into()))? =
            std::mem::take(live);
        prepared
    } else {
        let state = Arc::new(Mutex::new(std::mem::take(live)));
        #[cfg(feature = "duckdb")]
        let resources =
            graph
                .source
                .as_ref()
                .zip(graph.mapping.as_ref())
                .and_then(|(source, mapping)| {
                    source.executor().map(|executor| {
                        super::super::dag::DagSession::with_shared(
                            executor,
                            mapping.physical_table_names(),
                        )
                    })
                });
        #[cfg(not(feature = "duckdb"))]
        let resources: Option<super::super::dag::DagSession> = None;
        let cost = Arc::new(Mutex::new(crate::ir::QueryCost::default()));
        let result = if let Some(mut resources) = resources {
            resources.region_session = region_session.clone();
            runtime.block_on(super::super::dag::prepare_with_extensions(
                plan,
                vec![Arc::new(KernelPlanner {
                    state: state.clone(),
                })],
                &resources,
                cost.clone(),
                &rule_context,
            ))
        } else {
            let mut resources = super::super::dag::DagSession::new(
                state
                    .lock()
                    .map_err(|_| RuntimeError::Runtime("Subplan state poisoned".into()))?
                    .context
                    .sql_timeout,
            );
            resources.set_rule_context(rule_context.clone());
            resources.region_session = region_session;
            runtime.block_on(super::super::dag::prepare_with_extensions(
                plan,
                vec![Arc::new(KernelPlanner {
                    state: state.clone(),
                })],
                &resources,
                cost.clone(),
                &rule_context,
            ))
        };
        let (physical, task, stats) = match result {
            Ok(plan) => plan,
            Err(failure) => {
                *live = std::mem::take(&mut *state.lock().unwrap());
                return Err(RuntimeError::Runtime(failure.to_string()));
            }
        };
        PreparedSubplan {
            physical,
            state,
            task,
            stats,
            cost,
        }
    };
    if graph.source.is_some() {
        prepared
            .state
            .lock()
            .map_err(|_| RuntimeError::Runtime("Subplan state poisoned".into()))?
            .context
            .nested_dag_stats
            .merge_execution(&prepared.stats);
    }
    let result = runtime.block_on(super::super::engine_exec::collect(
        prepared.physical.clone(),
        prepared.task.clone(),
    ));
    let mut finished = std::mem::take(
        &mut *prepared
            .state
            .lock()
            .map_err(|_| RuntimeError::Runtime("Subplan state poisoned".into()))?,
    );
    finished.context.query_cost.add_work(&std::mem::take(
        &mut *prepared
            .cost
            .lock()
            .map_err(|_| RuntimeError::Runtime("Query cost poisoned".into()))?,
    ));
    *live = std::mem::take(&mut finished);
    // Empty query state prevents cache cycles through named group reducers,
    // and prevents one frontier's writes or bindings leaking into the next.
    *cache
        .lock()
        .map_err(|_| RuntimeError::Runtime("Subplan cache poisoned".into()))? = Some(prepared);
    let mut rows = Vec::new();
    for batch in result.map_err(error)? {
        rows.extend(decode_rows(&batch).map_err(error)?);
    }

    Ok(rows)
}
