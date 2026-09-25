//! Durable managed graphs with transactional writes and observable execution.
//!
//! DuckDB owns the committed checkpoint and incremental overlay records.
//! Queries lower to a relational DAG. DataFusion executes residual operators
//! and schedules explicit DuckDB regions.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use duckdb::{Connection, params};

use crate::ir::catalog::PropertyGraph;
use crate::ir::diagnostics::QueryExecutionError;
use crate::ir::catalog::incremental::IncrementalRecord;
use crate::ir::exec::{ExecStats, contains_mutation};
use crate::ir::interpreter::ReturnedBatches;
use crate::ir::plan::GraphPlan;
use crate::ir::rel::{RelBackend, sql};
use crate::ir::value::Value;
use crate::language::{cypher, gremlin, sparql};
use crate::storage::{decode_graph, encode_graph};

pub type EngineResult<T> = Result<T, String>;

/// Read execution policy. Strict SQL never silently invokes the interpreter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReadMode {
    #[default]
    Hybrid,
    SqlOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionBackend {
    DuckDb,
    Hybrid,
    GraphRuntime,
    DataFusion,
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub returned: ReturnedBatches,
    pub backend: ExecutionBackend,
    pub stats: ExecStats,
}

/// One graph and one transaction at a time. Separate engine instances use
/// DuckDB snapshot isolation; conflicting writes fail during persistence or commit.
pub struct GraphEngine {
    storage: Connection,
    database: Option<sql::SharedDatabase>,
    graph: PropertyGraph,
    loaded_revision: Option<i64>,
    in_transaction: bool,
    failed_transaction: bool,
    read_mode: ReadMode,
    backend: RelBackend,
    sql_timeout: Option<Duration>,
    strict_executor: sql::DuckDbExecutor,
    dag_session: crate::ir::rel::dag::DagSession,
}

// Discover pre-rename graph tables by their reserved schema and validate the
// checkpoint before renaming. This runs inside the initialization transaction.
fn migrate_storage_namespace(storage: &Connection) -> EngineResult<()> {
    let mut statement = storage.prepare(
        "SELECT table_name FROM information_schema.columns
         WHERE table_schema = current_schema()
           AND starts_with(table_name, '__') AND ends_with(table_name, '_state')
           AND table_name <> '__orchiddb_state'
         GROUP BY table_name
         HAVING count(*) FILTER (WHERE column_name IN ('singleton', 'format_version', 'payload')) = 3
            AND count(*) BETWEEN 3 AND 4"
    ).map_err(|e| e.to_string())?;
    let candidates = statement.query_map([], |row| row.get::<_, String>(0))
        .map_err(|e| e.to_string())?.collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if candidates.is_empty() { return Ok(()); }
    let canonical: i64 = storage.query_row(
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = '__orchiddb_state'",
        [], |row| row.get(0)).map_err(|e| e.to_string())?;
    if candidates.len() != 1 || canonical != 0 {
        return Err("ambiguous graph storage namespaces; keep a backup and resolve the duplicate graph tables before opening".into());
    }
    let state = &candidates[0];
    let records = format!("{}_records", state.strip_suffix("_state").unwrap());
    let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
    let payload: Vec<u8> = storage.query_row(
        &format!("SELECT payload FROM {} WHERE singleton = 1", quote(state)),
        [], |row| row.get(0)).map_err(|e| e.to_string())?;
    decode_graph(&payload)?;
    let records_exist: i64 = storage.query_row(
        "SELECT count(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = ?",
        params![records], |row| row.get(0)).map_err(|e| e.to_string())?;
    if records_exist != 0 {
        storage.execute_batch(&format!("ALTER TABLE {} RENAME TO __orchiddb_records", quote(&records)))
            .map_err(|e| e.to_string())?;
    }
    storage.execute_batch(&format!("ALTER TABLE {} RENAME TO __orchiddb_state", quote(state)))
        .map_err(|e| e.to_string())?;
    Ok(())
}

impl GraphEngine {
    pub fn open(path: impl AsRef<Path>) -> EngineResult<Self> {
        let (connection, database) = sql::open_shared(path.as_ref())?;
        let mut engine = Self::from_connection(connection)?;
        engine.database = database;
        Ok(engine)
    }

    pub fn in_memory() -> EngineResult<Self> {
        Self::from_connection(Connection::open_in_memory().map_err(|e| e.to_string())?)
    }

    fn from_connection(storage: Connection) -> EngineResult<Self> {
        // Schema creation and the v1 snapshot migration are atomic. Keeping the
        // checkpoint lets existing databases migrate without rewriting graph data.
        storage
            .execute_batch("BEGIN TRANSACTION")
            .map_err(|e| e.to_string())?;
        let initialize = (|| -> EngineResult<()> {
            migrate_storage_namespace(&storage)?;
            storage
                .execute_batch(
                    "CREATE TABLE IF NOT EXISTS __orchiddb_state (
                    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
                    format_version INTEGER NOT NULL,
                    payload BLOB NOT NULL,
                    revision BIGINT DEFAULT 0
                 );
                 ALTER TABLE __orchiddb_state ADD COLUMN IF NOT EXISTS revision BIGINT DEFAULT 0;
                 CREATE TABLE IF NOT EXISTS __orchiddb_records (
                    kind INTEGER NOT NULL,
                    name VARCHAR NOT NULL,
                    id BIGINT NOT NULL,
                    payload BLOB NOT NULL,
                    PRIMARY KEY(kind, name, id)
                 )",
                )
                .map_err(|e| e.to_string())?;
            let initial = encode_graph(&PropertyGraph::new())?;
            storage.execute(
                "INSERT INTO __orchiddb_state(singleton, format_version, payload, revision) VALUES (1, 2, ?, 0) ON CONFLICT DO NOTHING",
                params![initial],
            ).map_err(|e| e.to_string())?;
            let version: i32 = storage
                .query_row(
                    "SELECT format_version FROM __orchiddb_state WHERE singleton = 1",
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| e.to_string())?;
            match version {
                1 => {
                    let payload: Vec<u8> = storage
                        .query_row(
                            "SELECT payload FROM __orchiddb_state WHERE singleton = 1",
                            [],
                            |row| row.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    // Validate before making the migration durable. A bad
                    // legacy checkpoint must not leave a partially upgraded DB.
                    decode_graph(&payload)?;
                    storage
                        .execute_batch(
                            "UPDATE __orchiddb_state SET format_version = 2 WHERE singleton = 1",
                        )
                        .map_err(|e| e.to_string())?;
                }
                2 => {}
                other => return Err(format!("unsupported graph storage version {other}")),
            }
            storage.execute_batch("COMMIT").map_err(|e| e.to_string())?;
            Ok(())
        })();
        if let Err(error) = initialize {
            let _ = storage.execute_batch("ROLLBACK");
            return Err(error);
        }
        let mut engine = Self {
            storage,
            database: None,
            graph: PropertyGraph::new(),
            loaded_revision: None,
            in_transaction: false,
            failed_transaction: false,
            read_mode: ReadMode::Hybrid,
            backend: RelBackend::new(),
            sql_timeout: None,
            strict_executor: sql::DuckDbExecutor::new(),
            dag_session: crate::ir::rel::dag::DagSession::new(None),
        };
        engine.refresh()?;
        Ok(engine)
    }

    pub fn set_read_mode(&mut self, mode: ReadMode) {
        self.read_mode = mode;
    }

    /// Bound each DuckDB query and its setup. This does not impose a runtime
    /// deadline on residual DataFusion operators or snapshot serialization.
    pub fn set_sql_timeout(&mut self, timeout: Duration) {
        self.sql_timeout = Some(timeout);
        self.strict_executor = sql::DuckDbExecutor::with_timeout(timeout);
        self.dag_session = crate::ir::rel::dag::DagSession::new(Some(timeout));
    }

    pub fn in_transaction(&self) -> bool {
        self.in_transaction
    }

    pub fn begin(&mut self) -> EngineResult<()> {
        if self.in_transaction {
            return Err("a transaction is already active".into());
        }
        self.storage
            .execute_batch("BEGIN TRANSACTION")
            .map_err(|e| e.to_string())?;
        self.in_transaction = true;
        self.failed_transaction = false;
        if let Err(error) = self.refresh() {
            let _ = self.rollback();
            return Err(error);
        }
        Ok(())
    }

    pub fn commit(&mut self) -> EngineResult<()> {
        self.ensure_transaction()?;
        if self.failed_transaction {
            return Err("transaction failed; roll it back before continuing".into());
        }
        if let Err(error) = self.storage.execute_batch("COMMIT") {
            self.failed_transaction = true;
            return Err(error.to_string());
        }
        self.in_transaction = false;
        Ok(())
    }

    pub fn rollback(&mut self) -> EngineResult<()> {
        self.ensure_transaction()?;
        // Some database failures have already aborted the SQL transaction.
        // Recovery must still reset the graph cache and the failed state.
        if let Err(error) = self.storage.execute_batch("ROLLBACK") {
            if !error.to_string().contains("no transaction is active") {
                return Err(error.to_string());
            }
        }
        self.in_transaction = false;
        self.failed_transaction = false;
        self.loaded_revision = None;
        self.refresh()
    }

    fn ensure_transaction(&self) -> EngineResult<()> {
        if self.in_transaction {
            Ok(())
        } else {
            Err("no active transaction".into())
        }
    }

    fn refresh(&mut self) -> EngineResult<()> {
        // The revision, checkpoint, and overlay rows must all come from one
        // snapshot, even for an ordinary read outside an explicit transaction.
        let automatic = !self.in_transaction;
        if automatic {
            self.storage
                .execute_batch("BEGIN TRANSACTION")
                .map_err(|e| e.to_string())?;
        }
        let result = self.refresh_snapshot();
        if automatic {
            if result.is_ok() {
                if let Err(error) = self.storage.execute_batch("COMMIT") {
                    let _ = self.storage.execute_batch("ROLLBACK");
                    self.loaded_revision = None;
                    return Err(error.to_string());
                }
            } else {
                let _ = self.storage.execute_batch("ROLLBACK");
            }
        }
        result
    }

    fn refresh_snapshot(&mut self) -> EngineResult<()> {
        let (version, revision): (i32, i64) = self
            .storage
            .query_row(
                "SELECT format_version, revision FROM __orchiddb_state WHERE singleton = 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        if version != 2 {
            return Err(format!("unsupported graph storage version {version}"));
        }
        if self.loaded_revision == Some(revision) {
            return Ok(());
        }
        let payload: Vec<u8> = self
            .storage
            .query_row(
                "SELECT payload FROM __orchiddb_state WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        let mut graph = decode_graph(&payload)?;
        let mut statement = self
            .storage
            .prepare(
                "SELECT kind, name, id, payload FROM __orchiddb_records ORDER BY kind, name, id",
            )
            .map_err(|e| e.to_string())?;
        let records = statement
            .query_map([], |row| {
                Ok(IncrementalRecord {
                    kind: row.get(0)?,
                    name: row.get(1)?,
                    id: row.get(2)?,
                    payload: row.get(3)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        graph.apply_incremental_records(&records)?;
        graph.clear_pending_changes();
        self.graph = graph;
        self.loaded_revision = Some(revision);
        Ok(())
    }

    /// Lock the revision row before changing records. This deliberately retains
    /// graph-wide writer conflicts, so allocations and MERGE cannot race.
    fn advance_revision(&mut self) -> EngineResult<i64> {
        self.storage.query_row(
            "UPDATE __orchiddb_state SET revision = revision + 1 WHERE singleton = 1 RETURNING revision",
            [], |row| row.get(0),
        ).map_err(|e| e.to_string())
    }

    fn persist(&mut self) -> EngineResult<()> {
        let pending = self.graph.pending_changes();
        if pending.nodes.is_empty() && pending.edges.is_empty() {
            return Ok(());
        }
        let records = self
            .graph
            .incremental_records(&pending.nodes, &pending.edges)?;
        let result = (|| -> EngineResult<i64> {
            let revision = self.advance_revision()?;
            // Bind whole batches to one upsert. A prepared statement executed
            // once per element still starts thousands of DuckDB queries for
            // one CREATE, even though all writes share this transaction.
            for batch in records.chunks(256) {
                let placeholders = std::iter::repeat_n("(?, ?, ?, ?)", batch.len()).collect::<Vec<_>>().join(", ");
                let sql = format!("INSERT INTO __orchiddb_records VALUES {placeholders}
                    ON CONFLICT(kind, name, id) DO UPDATE SET payload = excluded.payload");
                let mut parameters: Vec<&dyn duckdb::ToSql> = Vec::with_capacity(batch.len() * 4);
                for record in batch {
                    parameters.extend([&record.kind as &dyn duckdb::ToSql, &record.name, &record.id, &record.payload]);
                }
                self.storage.execute(&sql, duckdb::params_from_iter(parameters)).map_err(|error| error.to_string())?;
            }
            Ok(revision)
        })();
        match result {
            Ok(revision) => {
                self.loaded_revision = Some(revision);
                self.graph.clear_pending_changes();
                Ok(())
            }
            Err(error) => {
                self.failed_transaction = true;
                Err(error)
            }
        }
    }

    fn persist_checkpoint(&mut self) -> EngineResult<()> {
        let payload = encode_graph(&self.graph)?;
        let result = (|| -> EngineResult<i64> {
            let revision = self.advance_revision()?;
            self.storage
                .execute(
                    "UPDATE __orchiddb_state SET payload = ? WHERE singleton = 1",
                    params![payload],
                )
                .map_err(|e| e.to_string())?;
            self.storage
                .execute_batch("DELETE FROM __orchiddb_records")
                .map_err(|e| e.to_string())?;
            Ok(revision)
        })();
        match result {
            Ok(revision) => {
                self.loaded_revision = Some(revision);
                self.graph.clear_pending_changes();
                Ok(())
            }
            Err(error) => {
                self.failed_transaction = true;
                Err(error)
            }
        }
    }

    /// Compact persisted overlay records into one checkpoint. Joins the active
    /// transaction, or commits atomically when called outside a transaction.
    pub fn checkpoint(&mut self) -> EngineResult<()> {
        if self.failed_transaction {
            return Err("transaction failed; roll it back".into());
        }
        let automatic = !self.in_transaction;
        if automatic {
            self.begin()?;
        }
        if let Err(error) = self.persist_checkpoint() {
            if automatic {
                let _ = self.rollback();
            }
            return Err(error);
        }
        if automatic {
            self.finish_automatic()?;
        }
        Ok(())
    }

    /// Import an Arrow-backed graph atomically, replacing the managed graph.
    pub fn replace_graph(&mut self, graph: PropertyGraph) -> EngineResult<()> {
        let automatic = !self.in_transaction;
        if automatic {
            self.begin()?;
        }
        if self.failed_transaction {
            return Err("transaction failed; roll it back".into());
        }
        let previous = std::mem::replace(&mut self.graph, graph);
        if let Err(error) = self.persist_checkpoint() {
            self.graph = previous;
            if automatic {
                let _ = self.rollback();
            }
            return Err(error);
        }
        if automatic {
            self.finish_automatic()?;
        }
        Ok(())
    }

    pub async fn cypher(&mut self, query: &str) -> EngineResult<QueryResult> {
        self.cypher_with_params(query, &BTreeMap::new()).await
    }

    pub fn register_table_procedure(&mut self, name: String, procedure: crate::ir::procedures::TableProcedure) -> EngineResult<()> {
        procedure.validate()?;
        std::sync::Arc::make_mut(&mut self.graph.procedures).insert(name,procedure);
        Ok(())
    }

    pub fn procedure_catalog(&self) -> &crate::ir::procedures::ProcedureCatalog {
        &self.graph.procedures
    }

    pub async fn cypher_with_params(
        &mut self,
        query: &str,
        parameters: &BTreeMap<String, Value>,
    ) -> EngineResult<QueryResult> {
        let mut parsed = cypher::parser::parse_query(query).map_err(|e| e.to_string())?;
        cypher::procedures::prepare(&mut parsed,self.procedure_catalog()).map_err(|e|e.to_string())?;
        cypher::parameters::bind_parameters(&mut parsed, parameters)?;
        cypher::procedures::prepare(&mut parsed,self.procedure_catalog()).map_err(|e|e.to_string())?;
        let plan = cypher::planner::CypherPlanner::new()
            .plan(&parsed)
            .map_err(|e| e.to_string())?;
        self.execute_plan(&plan).await
    }

    pub async fn gremlin(&mut self, query: &str) -> EngineResult<QueryResult> {
        self.gremlin_with_bindings(query, &std::collections::HashMap::new()).await
    }

    pub async fn gremlin_with_bindings(
        &mut self, query: &str,
        bindings: &std::collections::HashMap<String, gremlin::GremlinBinding>,
    ) -> EngineResult<QueryResult> {
        let (source, values) = gremlin::callables::prepare(query, bindings).map_err(|e| e.to_string())?;
        let parsed = gremlin::parse_traversal_with_bindings(&source, &values).map_err(|e| e.to_string())?;
        let plan = gremlin::GremlinPlanner::new()
            .plan(&parsed)
            .map_err(|e| e.to_string())?;
        self.execute_plan(&plan).await
    }

    pub async fn sparql(
        &mut self,
        query: &str,
        ontology: sparql::OntologyMapping,
    ) -> EngineResult<QueryResult> {
        let plan = sparql::SparqlPlanner::default()
            .with_ontology(ontology)
            .plan_str(query)
            .map_err(|e| e.to_string())?;
        self.execute_plan(&plan).await
    }

    async fn execute_dag(&mut self, plan: &GraphPlan) -> Result<(ReturnedBatches, crate::ir::rel::dag::DagStats), QueryExecutionError> {
        // Long clause chains create deep logical DAGs. Give each synchronous
        // poll enough stack for lowering/optimizer recursion while retaining
        // the same async executor, session, and wakeup behavior.
        let result = {
            let mut execution = Box::pin(crate::ir::rel::runtime::execute_with_session(
                plan, &self.graph, self.sql_timeout, Some(&self.dag_session)));
            futures::future::poll_fn(|cx| stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024,
                || std::future::Future::poll(execution.as_mut(), cx))).await
        };
        if result.is_err() {
            // Interruptions or failed SQL must not poison the next query.
            self.dag_session = crate::ir::rel::dag::DagSession::new(self.sql_timeout);
        }
        result
    }

    pub async fn execute_plan(&mut self, plan: &GraphPlan) -> EngineResult<QueryResult> {
        self.execute_plan_with_diagnostics(plan).await.map_err(|error|error.to_string())
    }

    /// Execute through the same SQL IR DAG, retaining structured runtime errors.
    pub async fn execute_plan_with_diagnostics(&mut self, plan: &GraphPlan) -> Result<QueryResult, QueryExecutionError> {
        if self.failed_transaction {
            return Err("transaction failed; roll it back".into());
        }
        if contains_mutation(&plan.root) {
            let automatic = !self.in_transaction;
            if automatic {
                self.begin()?;
            }
            // A failed statement cannot leave partially applied CREATE/SET/DELETE.
            let before = self.graph.clone();
            let (returned, dag_stats) =
                match self.execute_dag(plan).await {
                    Ok(result) => result,
                    Err(error) => {
                        self.graph = before;
                        if automatic {
                            let _ = self.rollback();
                        }
                        return Err(error);
                    }
                };
            if let Err(error) = self.persist() {
                self.graph = before;
                if automatic {
                    let _ = self.rollback();
                }
                return Err(error.into());
            }
            if automatic {
                self.finish_automatic()?;
            }
            return Ok(QueryResult {
                returned,
                backend: if dag_stats.duckdb_regions > 0 {
                    ExecutionBackend::Hybrid
                } else {
                    ExecutionBackend::DataFusion
                },
                stats: dag_stats.into(),
            });
        }
        if !self.in_transaction {
            self.refresh()?;
        }
        let (returned, stats) = match self.read_mode {
            ReadMode::Hybrid => {
                let (returned, stats) =
                    self.execute_dag(plan).await?;
                (returned, stats.into())
            }
            ReadMode::SqlOnly => {
                let lowered = self
                    .backend
                    .lower(plan, &self.graph)
                    .map_err(|e| e.to_string())?;
                let prepared = sql::prepare(&lowered, sql::SqlDialect::DuckDb)
                    .await
                    .map_err(|e| e.to_string())?;
                let returned = sql::execute_prepared(&mut self.strict_executor, &prepared)
                    .map_err(|e| e.to_string())?;
                let stats = ExecStats {
                    islands: 1,
                    island_rows: returned.batch.num_rows(),
                    ..ExecStats::default()
                };
                (returned, stats)
            }
        };
        let backend = if stats.datafusion_ops == 0 {
            ExecutionBackend::DuckDb
        } else if stats.islands > 0 {
            ExecutionBackend::Hybrid
        } else {
            ExecutionBackend::DataFusion
        };
        Ok(QueryResult {
            returned,
            backend,
            stats,
        })
    }

    fn finish_automatic(&mut self) -> EngineResult<()> {
        if let Err(error) = self.commit() {
            let _ = self.rollback();
            return Err(error);
        }
        Ok(())
    }
}

impl Drop for GraphEngine {
    fn drop(&mut self) {
        if self.in_transaction {
            let _ = self.storage.execute_batch("ROLLBACK");
        }
    }
}
