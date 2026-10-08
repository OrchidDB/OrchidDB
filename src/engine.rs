//! Durable managed graphs with transactional writes and observable execution.
//!
//! DuckDB owns the committed checkpoint and incremental overlay records.
//! Queries lower to a relational DAG. DataFusion executes residual operators
//! and schedules explicit DuckDB regions.

mod mapped_storage;
mod rdf;
pub(crate) mod mapped_source;
mod snapshot;
mod checkpoint;
pub use snapshot::CypherStateSnapshot;
pub use checkpoint::ManagedGraphCheckpoint;
pub use crate::rdf_engine::{RdfTermValue, SparqlResults};

use std::collections::BTreeMap;
use std::sync::Arc;
use crate::ir::rel::mapping::GraphMapping;
use std::path::Path;
use std::time::Duration;

use duckdb::{Connection, params};

use crate::ir::catalog::PropertyGraph;
use crate::ir::diagnostics::QueryExecutionError;
use crate::ir::catalog::incremental::IncrementalRecord;
use crate::ir::exec::{ExecStats, contains_mutation};
use crate::ir::runtime::ReturnedBatches;
use crate::ir::plan::GraphPlan;
use crate::ir::rel::{RelBackend, sql};
use crate::ir::value::Value;
use crate::language::{cypher, gremlin, sparql};
use crate::storage::{decode_graph, encode_graph};

pub type EngineResult<T> = Result<T, String>;

/// Read execution policy. Strict SQL never silently switches to native execution.
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
    language_functions: bool,
    storage: Connection,
    mapping: Option<Arc<GraphMapping>>,
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
    jvm_workers: crate::ir::jvm::JvmWorkerPool,
}

// Restore caller-owned resources even when an async query future is dropped.
struct MappedConnectionLease<'a> {
    target: &'a mut Connection,
    executor: sql::DuckDbExecutor,
}
impl Drop for MappedConnectionLease<'_> {
    fn drop(&mut self) {if let Some(connection)=self.executor.take_connection() {*self.target=connection;}}
}
struct MappedExecutorLease<'a> {
    target: &'a mut sql::DuckDbExecutor,
    shared: Arc<std::sync::Mutex<sql::DuckDbExecutor>>,
    automatic: bool,
    finished: bool,
}
impl Drop for MappedExecutorLease<'_> {
    fn drop(&mut self) {
        let mut executor=self.shared.lock().unwrap_or_else(|e|e.into_inner());
        if self.automatic && !self.finished {let _=executor.rollback();}
        *self.target=std::mem::take(&mut *executor);
    }
}

/// Statement cleanup must also run when the awaiting caller cancels its future.
struct ManagedStatement<'a> {
    engine: &'a mut GraphEngine,
    before: Option<PropertyGraph>,
    automatic: bool,
}
impl Drop for ManagedStatement<'_> {
    fn drop(&mut self) {
        if let Some(before) = self.before.take() {
            self.engine.graph = before;
            let region_session = self.engine.dag_session.region_session.clone();
            self.engine.dag_session = crate::ir::rel::dag::DagSession::new(self.engine.sql_timeout);
            self.engine.dag_session.region_session = region_session;
            if self.automatic && self.engine.in_transaction {
                if self.engine.rollback().is_err() {
                    // Do not let later writes join a transaction that could not
                    // be recovered. The caller must explicitly roll it back.
                    self.engine.failed_transaction = true;
                }
            }
        }
    }
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
    /// Use caller-owned DuckDB tables with the common language executor.
    pub fn mapped(storage: Connection, mapping: Arc<GraphMapping>) -> EngineResult<Self> {
        let mut executor=sql::DuckDbExecutor::from_connection(storage);
        let in_transaction=executor.in_transaction();
        let storage=executor.take_connection().ok_or("missing mapped connection")?;
        mapped_storage::register(&storage)?;
        let mut engine = Self {
            storage, mapping: Some(mapping), database: None, graph: PropertyGraph::new(),
            loaded_revision: None, in_transaction, failed_transaction: false,
            read_mode: ReadMode::Hybrid, backend: RelBackend::new().with_language_functions(true), sql_timeout: None,
            strict_executor: sql::DuckDbExecutor::new(),
            dag_session: crate::ir::rel::dag::DagSession::new(None),
            jvm_workers: Default::default(),
            language_functions: true,
        };
        engine.refresh()?;
        Ok(engine)
    }

    /// Generate bounded statistics once and automatically use them for later plans.
    pub fn generate_statistics(&mut self) -> EngineResult<crate::ir::rel::statistics::StatisticsSnapshot> {
        let mapping=self.mapping.as_ref().ok_or("statistics generation requires a mapped graph")?;
        let request=mapping.statistics_request().map_err(|e|e.to_string())?;
        let snapshot=crate::ir::rel::statistics::generate_duckdb(&self.storage,request)?;
        self.set_statistics(snapshot.clone())?;
        Ok(snapshot)
    }
    pub fn set_statistics(&mut self, snapshot: crate::ir::rel::statistics::StatisticsSnapshot) -> EngineResult<()> {
        let mapping=self.mapping.as_ref().ok_or("statistics require a mapped graph")?;
        let mut mapping=(**mapping).clone();mapping.set_statistics(Arc::new(snapshot)).map_err(|e|e.to_string())?;
        self.mapping=Some(Arc::new(mapping));self.refresh_snapshot()
    }
    pub fn clear_statistics(&mut self) -> EngineResult<()> {
        if let Some(mapping)=&self.mapping {let mut mapping=(**mapping).clone();mapping.clear_statistics();self.mapping=Some(Arc::new(mapping));self.refresh_snapshot()?;}
        Ok(())
    }
    fn invalidate_statistics(&mut self) {
        if let Some(mapping) = &self.mapping {
            if mapping.statistics().is_some() {
                let mut mapping = (**mapping).clone();
                mapping.clear_statistics();
                self.mapping = Some(Arc::new(mapping));
            }
        }
    }
    pub fn statistics(&self) -> Option<&Arc<crate::ir::rel::statistics::StatisticsSnapshot>> {
        self.mapping.as_ref().and_then(|m|m.statistics())
    }

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
                    id BLOB NOT NULL,
                    payload BLOB NOT NULL,
                    PRIMARY KEY(kind, name, id)
                 )",
                )
                .map_err(|e| e.to_string())?;
            let key_type: String = storage.query_row("SELECT data_type FROM information_schema.columns WHERE table_name = '__orchiddb_records' AND column_name = 'id' AND table_schema = current_schema()", [], |row| row.get(0)).map_err(|e|e.to_string())?;
            if key_type != "BLOB" {
                let rows = {
                    let mut stmt = storage.prepare("SELECT kind, name, id, payload FROM __orchiddb_records").map_err(|e|e.to_string())?;
                    stmt.query_map([], |row| Ok((row.get::<_,i32>(0)?, row.get::<_,String>(1)?, row.get::<_,i64>(2)?, row.get::<_,Vec<u8>>(3)?))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?
                };
                storage.execute_batch("DROP TABLE __orchiddb_records; CREATE TABLE __orchiddb_records (kind INTEGER NOT NULL, name VARCHAR NOT NULL, id BLOB NOT NULL, payload BLOB NOT NULL, PRIMARY KEY(kind,name,id))").map_err(|e|e.to_string())?;
                for (kind,name,id,payload) in rows {
                    storage.execute("INSERT INTO __orchiddb_records VALUES (?, ?, ?, ?)", params![kind,name,crate::ir::ElementId::from(id),payload]).map_err(|e|e.to_string())?;
                }
            }
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
            mapping: None,
            database: None,
            graph: PropertyGraph::new(),
            loaded_revision: None,
            in_transaction: false,
            failed_transaction: false,
            read_mode: ReadMode::Hybrid,
            backend: RelBackend::new().with_language_functions(true),
            sql_timeout: None,
            strict_executor: sql::DuckDbExecutor::new(),
            dag_session: crate::ir::rel::dag::DagSession::new(None),
            jvm_workers: Default::default(),
            language_functions: true,
        };
        engine.refresh()?;
        Ok(engine)
    }

    /// Control in-process language UDFs and their SQL lowering (default: enabled).
    pub fn set_language_functions(&mut self, enabled: bool) -> EngineResult<()> {
        self.strict_executor.set_language_functions(enabled).map_err(|e| e.to_string())?;
        self.dag_session.executor()?.set_language_functions(enabled).map_err(|e| e.to_string())?;
        if enabled && !self.language_functions { sql::language_functions::register(&self.storage).map_err(|e| e.to_string())?; }
        self.backend = self.backend.clone().with_language_functions(enabled);
        self.language_functions = enabled;
        Ok(())
    }

    pub fn set_read_mode(&mut self, mode: ReadMode) {
        self.read_mode = mode;
    }

    /// Bound each DuckDB query and its setup. This does not impose a runtime
    /// deadline on residual DataFusion operators or snapshot serialization.
    pub fn set_sql_timeout(&mut self, timeout: Duration) {
        self.sql_timeout = Some(timeout);
        self.strict_executor = sql::DuckDbExecutor::with_timeout(timeout);
        let region_session = self.dag_session.region_session.clone();
        self.dag_session = crate::ir::rel::dag::DagSession::new(Some(timeout));
        self.dag_session.region_session = region_session;
    }

    /// Select the SQL session used by regions of the common execution DAG.
    /// The session receives read queries with bound inputs, never DDL.
    pub fn set_sql_region_session(&mut self, session: Box<dyn sql::region::RegionSession>) {
        self.dag_session.region_session = Some(Arc::new(std::sync::Mutex::new(session)));
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
        self.invalidate_statistics();
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
        self.invalidate_statistics();
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
        if let Some(mapping) = &self.mapping {
            let procedures = self.graph.procedures.clone();
            self.graph = mapped_storage::metadata(&self.storage, mapping.clone())?;
            self.graph.procedures = procedures;
            return Ok(());
        }

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
        if let Some(mapping) = &self.mapping {
            mapped_storage::persist(&self.storage, &self.graph, mapping)?;
            let procedures = self.graph.procedures.clone();
            self.graph = mapped_storage::metadata(&self.storage, mapping.clone())?;
            self.graph.procedures = procedures;
            return Ok(());
        }

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
        self.persist_checkpoint_payload(&payload)
    }

    fn persist_checkpoint_payload(&mut self, payload: &[u8]) -> EngineResult<()> {
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
        if self.mapping.is_some() { return Ok(()); }
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
        self.replace_graph_with_payload(graph, None)
    }

    fn replace_graph_with_payload(&mut self, graph: PropertyGraph, payload: Option<&[u8]>) -> EngineResult<()> {
        if self.mapping.is_some() { return Err("cannot replace a mapped schema with a managed graph".into()); }
        let automatic = !self.in_transaction;
        if automatic {
            self.begin()?;
        }
        if self.failed_transaction {
            return Err("transaction failed; roll it back".into());
        }
        let previous = std::mem::replace(&mut self.graph, graph);
        let persisted = match payload {
            Some(payload) => self.persist_checkpoint_payload(payload),
            None => self.persist_checkpoint(),
        };
        if let Err(error) = persisted {
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
        let plan = plan_cypher(query, parameters, self.procedure_catalog())?;
        self.execute_plan(&plan).await
    }

    pub async fn gremlin(&mut self, query: &str) -> EngineResult<QueryResult> {
        self.gremlin_with_bindings(query, &std::collections::HashMap::new()).await
    }

    pub async fn gremlin_with_bindings(
        &mut self, query: &str,
        bindings: &std::collections::HashMap<String, gremlin::GremlinBinding>,
    ) -> EngineResult<QueryResult> {
        let plan = plan_gremlin(query, bindings)?;
        self.execute_plan(&plan).await
    }

    pub async fn sparql(
        &mut self,
        query: &str,
        ontology: sparql::OntologyMapping,
    ) -> EngineResult<QueryResult> {
        if let Some(mapping)=&self.mapping {
            let mut mapping=(**mapping).clone();
            ontology.apply_to(&mut mapping,"default")?;
            let mut result=self.run_rdf(query,"default",None,false,Some(Arc::new(mapping.rdf_mapping()))).await?.ok_or("missing SPARQL result")?;
            result.returned=crate::rdf_engine::legacy_columns(result.returned)?;
            return Ok(result);
        }
        let plan = plan_sparql(query, ontology)?;
        self.execute_plan(&plan).await
    }

    async fn execute_dag(&mut self, plan: &GraphPlan) -> Result<(ReturnedBatches, crate::ir::rel::dag::DagStats), QueryExecutionError> {
        // Long clause chains create deep logical DAGs. Give each synchronous
        // poll enough stack for lowering/optimizer recursion while retaining
        // the same async executor, session, and wakeup behavior.
        let result = execute_graph_dag(plan, &self.graph, self.sql_timeout, Some(&self.dag_session), self.jvm_workers.clone()).await;
        if result.is_err() {
            // Interruptions or failed SQL must not poison the next query.
            let region_session = self.dag_session.region_session.clone();
            self.dag_session = crate::ir::rel::dag::DagSession::new(self.sql_timeout);
            self.dag_session.region_session = region_session;
        }
        result
    }

    pub async fn execute_plan(&mut self, plan: &GraphPlan) -> EngineResult<QueryResult> {
        self.execute_plan_with_diagnostics(plan).await.map_err(|error|error.to_string())
    }

    /// Execute through the same SQL IR DAG, retaining structured runtime errors.
    pub async fn execute_plan_with_diagnostics(&mut self, plan: &GraphPlan) -> Result<QueryResult, QueryExecutionError> {
        // Sessions may have been rebuilt after a timeout or rollback.
        self.set_language_functions(self.language_functions)?;
        if self.failed_transaction {
            return Err("transaction failed; roll it back".into());
        }
        if let Some(mapping) = self.mapping.clone() {
            let placeholder = Connection::open_in_memory().map_err(|e|e.to_string())?;
            let storage = std::mem::replace(&mut self.storage, placeholder);
            let mut lease=MappedConnectionLease {target:&mut self.storage,executor:sql::DuckDbExecutor::from_connection(storage)};
            lease.executor.set_language_functions(self.language_functions).map_err(|e| e.to_string())?;
            if let Some(timeout)=self.sql_timeout {lease.executor.set_timeouts(timeout,timeout);}
            if self.in_transaction {self.failed_transaction=true;}
            let result = execute_mapped_dag(&mut lease.executor, mapping, plan, self.graph.procedures.clone(), self.sql_timeout, self.jvm_workers.clone(), self.dag_session.region_session.clone()).await;
            if result.is_ok() {self.failed_transaction=false;}
            drop(lease);
            if result.is_ok() && contains_mutation(&plan.root) { self.invalidate_statistics(); }
            return result.map(|(returned, stats)| QueryResult {returned, backend: if stats.duckdb_regions + stats.postgres_regions + stats.other_sql_regions > 0 {ExecutionBackend::Hybrid} else {ExecutionBackend::DataFusion}, stats: stats.into()}).map_err(Into::into);
        }
        if contains_mutation(&plan.root) {
            let automatic = !self.in_transaction;
            if automatic {
                self.begin()?;
            }
            let before = self.graph.clone();
            let mut statement = ManagedStatement { engine: self, before: Some(before), automatic };
            let (returned, dag_stats) = statement.engine.execute_dag(plan).await?;
            statement.engine.persist()?;
            if automatic {
                statement.engine.commit()?;
            }
            statement.before = None;
            return Ok(QueryResult {
                returned,
                backend: if dag_stats.duckdb_regions + dag_stats.postgres_regions + dag_stats.other_sql_regions > 0 {
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
                let cost_started=std::time::Instant::now();
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
                    cost: crate::ir::QueryCost {
                        sql_executions: 1,
                        sql_output_rows: returned.batch.num_rows() as u64,
                        sql_output_bytes: returned.batch.get_array_memory_size() as u64,
                        result_rows: returned.batch.num_rows() as u64,
                        elapsed_micros: cost_started.elapsed().as_micros().min(u64::MAX as u128) as u64,
                        ..Default::default()
                    },
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

/// Compatibility entry point sharing the runtime and storage adapter while
/// retaining the caller's exact DuckDB connection and active transaction.
pub(crate) async fn execute_mapped(executor: &mut sql::DuckDbExecutor, mapping: Arc<GraphMapping>, plan: &GraphPlan, jvm_workers: crate::ir::jvm::JvmWorkerPool) -> EngineResult<ReturnedBatches> {
    execute_mapped_dag(executor, mapping, plan, Default::default(), None, jvm_workers, None).await.map(|(returned,_)|returned)
}

async fn execute_mapped_dag(executor: &mut sql::DuckDbExecutor, mapping: Arc<GraphMapping>, plan: &GraphPlan, procedures: Arc<crate::ir::procedures::ProcedureCatalog>, timeout: Option<Duration>, jvm_workers: crate::ir::jvm::JvmWorkerPool, region_session: Option<sql::region::SharedRegionSession>) -> EngineResult<(ReturnedBatches, crate::ir::rel::dag::DagStats)> {
    let automatic = !executor.in_transaction();
    if automatic { executor.begin().map_err(|e|e.to_string())?; }
    let shared=Arc::new(std::sync::Mutex::new(std::mem::take(executor)));
    let mut lease=MappedExecutorLease {target:executor,shared:shared.clone(),automatic,finished:false};
    let mut session = crate::ir::rel::dag::DagSession::with_shared(shared, mapping.physical_table_names());
    session.region_session = region_session;
    let result = async {
        let rdf=mapping.rdf_mapping();
        if crate::ir::rel::sparql::handles(plan) {
            let mut context=crate::rdf_engine::RdfSession {resources:&session,mapping:Arc::new(rdf),dataset:"default".into(),scalar_registered:false,last_query_stats:None};
            return context.execute_plan(plan).await;
        }
        let mut graph = mapped_source::attach(session.shared_executor(), mapping.clone())?;
        graph.procedures = procedures;
        let mut result = execute_graph_dag(plan, &graph, timeout, Some(&session), jvm_workers).await.map_err(|e|e.to_string())?;
        if contains_mutation(&plan.root) {
            // Populate only touched records before holding the SQL connection.
            let pending=graph.pending_changes();
            if let Some(source)=&graph.source {
                for (name,id) in pending.nodes { source.exists(false,&name,&id); }
                for (name,id) in pending.edges {
                    source.exists(true,&name,&id);
                    if let Some((child, _, _, _)) = mapping.edge(&name).and_then(|m| m.foreign_key_columns()) {
                        source.exists(false,child,&id);
                    }
                }
            }
            graph.check_source()?;
            let mut executor=session.executor()?;
            mapped_storage::persist(executor.connection().map_err(|e|e.to_string())?,&graph,&mapping)?;
        }
        if let Some(source)=&graph.source {let (rows,queries)=source.stats();result.1.native_source_rows=rows;result.1.native_source_queries=queries;result.1.optimizer_decisions.extend(source.access_decisions());}
        Ok(result)
    }.await;
    drop(session);
    lease.finished=true;
    drop(lease);
    if automatic {
        if result.is_ok(){executor.commit().map_err(|e|e.to_string())?;} else {let _=executor.rollback();}
    }
    result
}

/// Shared frontend preparation for every DuckDB storage layout.
pub(crate) fn plan_cypher(query: &str, parameters: &BTreeMap<String, Value>, catalog: &crate::ir::procedures::ProcedureCatalog) -> EngineResult<GraphPlan> {
        cypher::preparation::prepare(query, parameters, Some(catalog)).map_err(|e| e.to_string())
}

pub(crate) fn plan_gremlin(query: &str, bindings: &std::collections::HashMap<String, gremlin::GremlinBinding>) -> EngineResult<GraphPlan> {
        let (source, values) = gremlin::callables::prepare(query, bindings).map_err(|e| e.to_string())?;
        let parsed = gremlin::parse_traversal_with_bindings(&source, &values).map_err(|e| e.to_string())?;
        gremlin::GremlinPlanner::new()
            .plan(&parsed)
            .map_err(|e| e.to_string())
}

pub(crate) fn plan_sparql(query: &str, ontology: sparql::OntologyMapping) -> EngineResult<GraphPlan> {
        sparql::SparqlPlanner::default()
            .with_ontology(ontology)
            .plan_str(query)
            .map_err(|e| e.to_string())
}

async fn execute_graph_dag(plan: &GraphPlan, graph: &PropertyGraph, timeout: Option<Duration>, session: Option<&crate::ir::rel::dag::DagSession>, jvm_workers: crate::ir::jvm::JvmWorkerPool) -> Result<(ReturnedBatches, crate::ir::rel::dag::DagStats), QueryExecutionError> {
    let mut execution = Box::pin(crate::ir::rel::runtime::execute_with_session(plan, graph, timeout, session, jvm_workers));
    futures::future::poll_fn(|cx| stacker::maybe_grow(8 * 1024 * 1024, 64 * 1024 * 1024,
        || std::future::Future::poll(execution.as_mut(), cx))).await
}
