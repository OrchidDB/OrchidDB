//! Compatibility facade for the unified graph executor with mapped DuckDB storage.
//! Language semantics and mutations run through the same relational DAG and
//! typed graph kernels used by `GraphEngine`. The SQL compiler is also exposed
//! through `explain_cypher`; runtime islands resolve the same mapped sources.

mod update;

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::ir::catalog::PropertyGraph;
use crate::ir::functions::{OperatorTable, with_operator_table};
use crate::ir::plan::{GraphPlan, Node, ProcedureMode};
use crate::ir::rel::mapping::GraphMapping;
use crate::ir::rel::sql::{DuckDbExecutor, SqlExecutor, prepare_with_external};
use crate::ir::rel::{RelBackend, RelBackendOptions};
use crate::ir::runtime::ReturnedBatches;
use crate::ir::value::Value;
use crate::language::cypher::parser::parse_query;
use crate::language::cypher::planner::CypherPlanner;
use crate::language::sparql::OntologyMapping;

/// Owns the external SQL executor and the schema mapping that turns graph
/// labels/edge types into the user's relational tables.
pub struct MappedGraphEngine {
    executor: DuckDbExecutor,
    mapping: Arc<GraphMapping>,
    operator_table: Option<Arc<dyn OperatorTable>>,
    jvm_workers: crate::ir::jvm::JvmWorkerPool,
}

impl MappedGraphEngine {
    pub fn new(executor: DuckDbExecutor, mapping: Arc<GraphMapping>) -> Self {
        Self {
            executor,
            mapping,
            operator_table: None,
            jvm_workers: Default::default(),
        }
    }

    /// Install code-defined function mappings and the catalog used for this
    /// engine's queries. Register UDF implementations on `executor_mut()`'s
    /// connection first, then snapshot that connection with `DuckDbCatalog`.
    /// Selection is scoped internally to synchronous planning and lowering;
    /// callers can await query methods normally without thread-local setup.
    pub fn set_operator_table(&mut self, table: Arc<dyn OperatorTable>) -> Result<(), String> {
        if table.engine() != "duckdb" {
            return Err(format!(
                "mapped DuckDB engine cannot use `{}` operator table",
                table.engine()
            ));
        }
        self.operator_table = Some(table);
        Ok(())
    }

    fn with_functions<T>(&self, operation: impl FnOnce() -> T) -> T {
        match &self.operator_table {
            Some(table) => with_operator_table(table.clone(), operation),
            None => operation(),
        }
    }

    /// Immutable access to the underlying executor.
    pub fn executor(&self) -> &DuckDbExecutor {
        &self.executor
    }

    /// Mutable access to the underlying executor (e.g. to run raw SQL or
    /// tune timeouts).
    pub fn executor_mut(&mut self) -> &mut DuckDbExecutor {
        &mut self.executor
    }

    /// The schema mapping this engine resolves scans through.
    pub fn mapping(&self) -> &GraphMapping {
        &self.mapping
    }

    /// Apply raw SQL on every call; writes are never deduplicated as setup.
    pub fn execute_sql(&mut self, sql: &str) -> Result<(), String> {
        self.executor
            .execute_batch(sql)
            .map_err(|err| err.to_string())
    }

    /// Run a Cypher query or mutation against the mapped schema.
    pub async fn cypher(&mut self, query: &str) -> Result<ReturnedBatches, String> {
        self.cypher_with_params(query, &BTreeMap::new()).await
    }

    pub async fn cypher_with_params(
        &mut self,
        query: &str,
        parameters: &BTreeMap<String, Value>,
    ) -> Result<ReturnedBatches, String> {
        let plan = self.with_functions(|| {
            crate::engine::plan_cypher(query, parameters, &Default::default())
        })?;
        self.run_plan(&plan).await
    }

    /// Run a Gremlin traversal or mutation against the mapped schema.
    pub async fn gremlin(&mut self, query: &str) -> Result<ReturnedBatches, String> {
        self.gremlin_with_bindings(query, &Default::default()).await
    }

    pub async fn gremlin_with_bindings(
        &mut self,
        query: &str,
        bindings: &std::collections::HashMap<String, crate::language::gremlin::GremlinBinding>,
    ) -> Result<ReturnedBatches, String> {
        let plan = self.with_functions(|| crate::engine::plan_gremlin(query, bindings))?;
        self.run_plan(&plan).await
    }

    /// Run a SPARQL query against the mapped schema. The ontology is passed
    /// explicitly so vocabulary resolution is a caller decision, not hidden
    /// engine state.
    pub async fn sparql(
        &mut self,
        query: &str,
        ontology: OntologyMapping,
    ) -> Result<ReturnedBatches, String> {
        let mut mapping=(*self.mapping).clone();
        ontology.apply_to(&mut mapping,"default")?;
        let plan=crate::language::sparql::SparqlPlanner::new("default").plan_str(query).map_err(|e|e.to_string())?;
        let result=crate::engine::execute_mapped(&mut self.executor,Arc::new(mapping),&plan,self.jvm_workers.clone()).await?;
        crate::rdf_engine::legacy_columns(result)
    }

    /// Lower a Cypher read query and return the generated DuckDB SQL text.
    pub async fn explain_cypher(&mut self, query: &str) -> Result<String, String> {
        let plan = self.with_functions(|| {
            let parsed = parse_query(query).map_err(|err| err.to_string())?;
            CypherPlanner::new()
                .plan(&parsed)
                .map_err(|err| err.to_string())
        })?;
        self.sql_for_plan(&plan).await
    }

    fn backend(&self) -> RelBackend {
        RelBackend::with_options(RelBackendOptions {
            mapping: Some(self.mapping.clone()),
            ..RelBackendOptions::default()
        })
    }

    /// Lower a plan and return only the generated query text.
    async fn sql_for_plan(&self, plan: &GraphPlan) -> Result<String, String> {
        reject_mutations(plan)?;
        let lowered = self.with_functions(|| {
            self.backend()
                .lower(plan, &PropertyGraph::new())
                .map_err(|err| err.to_string())
        })?;
        let external = self.mapping.physical_table_names();
        let prepared = prepare_with_external(&lowered, self.executor.dialect(), &external)
            .await
            .map_err(|err| err.to_string())?;
        Ok(prepared.query)
    }

    /// Lower a plan, prepare it against the executor's dialect, and execute it.
    async fn run_plan(&mut self, plan: &GraphPlan) -> Result<ReturnedBatches, String> {
        let table=self.operator_table.clone();
        let mut execution=Box::pin(crate::engine::execute_mapped(&mut self.executor, self.mapping.clone(), plan, self.jvm_workers.clone()));
        futures::future::poll_fn(|cx| {
            let mut poll=|| std::future::Future::poll(execution.as_mut(),cx);
            match &table {Some(table)=>with_operator_table(table.clone(),poll),None=>poll()}
        }).await
    }
}

/// Reject mutation-shaped plans before they reach the relational lowering.
/// Read-only planning and SQL explain must not execute write boundaries.
fn reject_mutations(plan: &GraphPlan) -> Result<(), String> {
    match find_mutation(plan.root.as_ref()) {
        Some(kind) => Err(format!(
            "mapped SQL engine does not support mutation queries \
             (found `{kind}`); only read queries are allowed"
        )),
        None => Ok(()),
    }
}

fn mutation_kind(node: &Node) -> Option<&'static str> {
    match node {
        Node::GraphCreate { .. } => Some("CREATE"),
        Node::GraphMerge { .. } => Some("MERGE"),
        Node::GraphSetProperty { .. } => Some("SET"),
        Node::GraphDelete { .. } => Some("DELETE"),
        Node::GraphProcedureCall {
            mode: ProcedureMode::Write,
            ..
        } => Some("write procedure call"),
        _ => None,
    }
}

fn find_mutation(node: &Node) -> Option<&'static str> {
    if let Some(kind) = mutation_kind(node) {
        return Some(kind);
    }
    children(node).into_iter().find_map(find_mutation)
}

/// All child nodes of `node`, for recursive tree walks.
fn children(node: &Node) -> Vec<&Node> {
    use Node::*;
    match node {
        GraphMerge {
            input,
            match_arm,
            create_arm,
            ..
        } => vec![input, match_arm, create_arm],
        GraphReturn { input, .. }
        | GraphConstructTriples { input, .. }
        | GraphDescribe { input, .. }
        | GraphAsk { input, .. }
        | GraphBind { input, .. }
        | GraphPathPattern { input, .. }
        | GraphPathFilter { input, .. }
        | GraphCreate { input, .. }
        | GraphSetProperty { input, .. }
        | GraphDelete { input, .. }
        | GraphFilter { input, .. }
        | GraphCurrentProject { input, .. }
        | GraphJvm { input, .. }
        | GraphAggregate { input, .. }
        | GraphGroupMap { input, .. }
        | GraphGroupSideEffect { input, .. }
        | GraphGroupCountSideEffect { input, .. }
        | GraphSideEffect { input, .. }
        | GraphReadSideEffect { input, .. }
        | GraphCap { input, .. }
        | GraphShortestPath { input, .. }
        | GraphDistinct { input, .. }
        | GraphSort { input, .. }
        | GraphSlice { input, .. }
        | GraphSample { input, .. }
        | GraphSliceExpr { input, .. }
        | GraphBarrier { input, .. }
        | GraphUnwind { input, .. }
        | GraphQuantifier { input, .. }
        | GraphCollect { input, .. }
        | GraphListComprehension { input, .. }
        | GraphSelect { input, .. }
        | GraphExpand { input, .. }
        | GraphProject { input, .. } => vec![input],
        GraphJoin { left, right, .. }
        | GraphApply { left, right, .. }
        | GraphUnion { left, right, .. }
        | GraphSparqlMinus { left, right, .. } => vec![left, right],
        GraphRepeat {
            seed,
            body,
            until_traversal,
            prefix_traversal,
            ..
        } => {
            let mut out = vec![seed.as_ref(), body.as_ref()];
            if let Some(node) = until_traversal {
                out.push(node);
            }
            if let Some(node) = prefix_traversal {
                out.push(node);
            }
            out
        }
        GraphCoalesce { input, arms, .. } => {
            let mut out = vec![input.as_ref()];
            out.extend(arms.iter());
            out
        }
        GraphChoose {
            input,
            arms,
            default,
            ..
        } => {
            let mut out = vec![input.as_ref()];
            out.extend(arms.iter().map(|arm| &arm.body));
            if let Some(node) = default {
                out.push(node);
            }
            out
        }
        GraphProcedureCall { input, .. } => input.iter().map(|node| node.as_ref()).collect(),
        GraphExtension { inputs, .. } => inputs.iter().collect(),
        GraphNodeScan { .. }
        | GraphRelScan { .. }
        | GraphValues { .. }
        | GraphOneRow
        | GraphEmpty
        | GraphCorrelate { .. }
        | GraphSparqlTriplePattern { .. }
        | GraphSparqlGraphNames { .. }
        | GraphRdfPropertyPath { .. } => Vec::new(),
    }
}
