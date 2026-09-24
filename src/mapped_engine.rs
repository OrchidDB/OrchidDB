//! Public mapped-table graph query engine.
//!
//! `MappedGraphEngine` is the integration seam between the language
//! frontends and an external SQL engine for "bring your own schema" graphs.
//! It owns a [`DuckDbExecutor`] and an `Arc<GraphMapping>`; every query is
//! parsed, lowered to Graph IR, lowered again through the relational backend
//! with the mapping installed (so node/edge scans resolve against the user's
//! own tables and views), unparsed to DuckDB SQL, and executed there.
//!
//! Cypher and Gremlin mutations resolve identities and properties through the
//! same mapping as reads. SQL read regions supply the rows for transactional
//! INSERT, UPDATE, and DELETE statements on the mapped physical tables.
//! No managed property-graph overlay is used for mapped writes.
//!
//! [`MappedGraphEngine::cypher_update`] also provides affected-row counts for
//! individual property assignments.

mod update;
mod write;

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::ir::catalog::PropertyGraph;
use crate::ir::functions::{OperatorTable, with_operator_table};
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::plan::{GraphPlan, Node, ProcedureMode};
use crate::ir::rel::mapping::GraphMapping;
use crate::ir::rel::sql::{DuckDbExecutor, SqlExecutor, execute_prepared, prepare_with_external};
use crate::ir::rel::{RelBackend, RelBackendOptions};
use crate::ir::value::Value;
use crate::language::cypher::parser::parse_query;
use crate::language::cypher::planner::CypherPlanner;
use crate::language::gremlin::parser::parse_traversal;
use crate::language::gremlin::planner::GremlinPlanner;
use crate::language::sparql::{OntologyMapping, SparqlPlanner};

/// Owns the external SQL executor and the schema mapping that turns graph
/// labels/edge types into the user's relational tables.
pub struct MappedGraphEngine {
    executor: DuckDbExecutor,
    mapping: Arc<GraphMapping>,
    operator_table: Option<Arc<dyn OperatorTable>>,
}

impl MappedGraphEngine {
    pub fn new(executor: DuckDbExecutor, mapping: Arc<GraphMapping>) -> Self {
        Self {
            executor,
            mapping,
            operator_table: None,
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
            let mut parsed = parse_query(query).map_err(|err| err.to_string())?;
            crate::language::cypher::parameters::bind_parameters(&mut parsed, parameters)?;
            CypherPlanner::new()
                .plan(&parsed)
                .map_err(|err| err.to_string())
        })?;
        self.run_plan(&plan).await
    }

    /// Run a Gremlin traversal or mutation against the mapped schema.
    pub async fn gremlin(&mut self, query: &str) -> Result<ReturnedBatches, String> {
        let plan = self.with_functions(|| {
            let traversal = parse_traversal(query).map_err(|err| err.to_string())?;
            GremlinPlanner::new()
                .plan(&traversal)
                .map_err(|err| err.to_string())
        })?;
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
        let plan = self.with_functions(|| {
            SparqlPlanner::new("mapped")
                .with_ontology(ontology)
                .plan_str(query)
                .map_err(|err| err.to_string())
        })?;
        self.run_plan(&plan).await
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
        if find_mutation(&plan.root).is_some() { self.run_mutations(plan).await }
        else { self.run_read_plan(plan).await }
    }

    async fn run_read_plan(&mut self, plan: &GraphPlan) -> Result<ReturnedBatches, String> {
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
        execute_prepared(&mut self.executor, &prepared).map_err(|err| err.to_string())
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
        | GraphProject { input, .. }
        | GraphService { input, .. } => vec![input],
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
        | GraphRdfPropertyPath { .. } => Vec::new(),
    }
}
