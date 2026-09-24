//! Native JVM compute kernel for relational DataFusion execution. Graph state never leaves the native provider:
//! Java calls back into an execution-local native overlay, published on success.
use crate::ir::interpreter::{InterpretError, IrResult, Row, eval};
use crate::ir::{
    catalog::PropertyGraph,
    plan::{Node, ProjectionItem},
    value::Value,
};
use crate::jvm_bridge::Store;
use serde_json::{Value as Json, json};
use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JvmMode {
    Computer,
    Sort,
    Map,
    FlatMap,
    Filter,
}

/// Trusted application code, never generated from interpolated argument values.
/// A map emits one value (including null); flatMap consumes an iterable/traversal;
/// filter requires a Boolean. One evaluation per traverser, retaining its bulk.
#[derive(Debug, Clone, PartialEq)]
pub struct JvmOperation {
    pub script: String,
    pub arguments: Vec<ProjectionItem>,
    pub output: String,
    pub mode: JvmMode,
}

#[derive(Debug, Clone)]
pub struct JvmConfig {
    pub java: String,
    pub classpath: String,
}
impl JvmConfig {
    pub fn from_env() -> Option<Self> {
        std::env::var("CRABGRAPH_JVM_CLASSPATH")
            .ok()
            .map(|classpath| Self {
                java: std::env::var("CRABGRAPH_JAVA").unwrap_or_else(|_| "java".into()),
                classpath,
            })
    }
}

/// Shared across nodes, including DataFusion work between JVM calls.
#[derive(Debug, Clone)]
pub struct JvmExecution {
    pub config: Option<JvmConfig>,
    pub cancelled: Arc<AtomicBool>,
    pub deadline: Option<Instant>,
    pub worker: JvmWorkerPool,
}
impl Default for JvmExecution {
    fn default() -> Self {
        Self {
            config: JvmConfig::from_env(),
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: None,
            worker: Default::default(),
        }
    }
}
impl JvmExecution {
    pub fn check(&self) -> IrResult<()> {
        if self.cancelled.load(Ordering::Acquire) {
            return Err(error("execution cancelled"));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(error("execution deadline exceeded"));
        }
        Ok(())
    }
}
fn error(message: impl Into<String>) -> InterpretError {
    InterpretError::Runtime(message.into())
}
pub fn contains_jvm(node: &Node) -> bool {
    matches!(node, Node::GraphJvm { .. })
        || crate::ir::analysis::children(node)
            .into_iter()
            .any(contains_jvm)
}
/// Query-scoped worker shared by correlated and sequential relational operators.
#[derive(Debug, Clone, Default)]
pub struct JvmWorkerPool(Arc<Mutex<Option<Worker>>>);
#[derive(Debug)]
struct Worker {
    child: Child,
    outgoing: Option<mpsc::Sender<String>>,
    received: Option<mpsc::Receiver<Result<Json, String>>>,
    reader: Option<std::thread::JoinHandle<()>>,
    writer: Option<std::thread::JoinHandle<()>>,
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.outgoing.take();
        self.received.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(thread) = self.writer.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.reader.take() {
            let _ = thread.join();
        }
    }
}
const MAX_FRAME: u64 = 64 * 1024 * 1024;

impl Worker {
    fn start(config: &JvmConfig) -> IrResult<Self> {
        let mut child = Command::new(&config.java)
            .arg("-cp")
            .arg(&config.classpath)
            .arg("io.crabgraph.gremlin.CrabIr")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| error(format!("Cannot start JVM IR worker: {e}")))?;
        let stdout = child.stdout.take().unwrap();
        let mut stdin = child.stdin.take().unwrap();
        let (responses, received) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            let mut input = BufReader::new(stdout);
            loop {
                let mut line = String::new();
                let result = (&mut input).take(MAX_FRAME + 1).read_line(&mut line);
                let message = match result {
                    Ok(0) => Err("JVM IR worker disconnected".to_string()),
                    Ok(_) if line.len() as u64 > MAX_FRAME || !line.ends_with('\n') => {
                        Err("Invalid or oversized JVM frame".to_string())
                    }
                    Ok(_) => serde_json::from_str::<Json>(&line)
                        .map_err(|e| format!("Invalid JVM protocol: {e}")),
                    Err(e) => Err(e.to_string()),
                };
                let failed = message.is_err();
                if responses.send(message).is_err() || failed {
                    break;
                }
            }
        });
        // Writes run separately so a JVM that stops reading cannot block cancellation.
        let (outgoing, writes) = mpsc::channel::<String>();
        let writer = std::thread::spawn(move || {
            for frame in writes {
                if writeln!(stdin, "{frame}")
                    .and_then(|_| stdin.flush())
                    .is_err()
                {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            outgoing: Some(outgoing),
            received: Some(received),
            reader: Some(reader),
            writer: Some(writer),
        })
    }
}

pub(crate) fn execute(
    operation: &JvmOperation,
    rows: Vec<Row>,
    graph: &PropertyGraph,
    execution: &mut JvmExecution,
) -> IrResult<Vec<Row>> {
    execution.check()?;
    if rows.is_empty() {
        return Ok(rows);
    }
    let config = execution.config.clone().ok_or_else(|| {
        error("GraphJvm requires CRABGRAPH_JVM_CLASSPATH or an explicit JvmConfig")
    })?;
    let mut names = BTreeSet::new();
    for argument in &operation.arguments {
        if matches!(argument.alias.as_str(), "g" | "graph") || !names.insert(&argument.alias) {
            return Err(error("JVM arguments must have unique, non-reserved names"));
        }
    }
    // This checkpoint includes earlier statement writes and preserves the graph's
    // null policy. No commit/rollback is delegated to the Java application.
    let mut store = Store::from_execution_graph(graph.clone());
    let mut auxiliary = std::collections::BTreeMap::<u64, Store>::new();
    let arguments: Vec<Json> = rows
        .iter()
        .map(|row| {
            operation
                .arguments
                .iter()
                .map(|arg| {
                    Ok((
                        arg.alias.clone(),
                        store.encode(&eval(&arg.expr, row, graph)?).map_err(error)?,
                    ))
                })
                .collect::<IrResult<serde_json::Map<String, Json>>>()
                .map(Json::Object)
        })
        .collect::<IrResult<_>>()?;
    let traversers: Vec<Json> = rows
        .iter()
        .map(|row| {
            let state: serde_json::Map<String, Json> = row
                .bindings
                .iter()
                .map(|(key, value)| Ok((key.clone(), store.encode(value).map_err(error)?)))
                .collect::<IrResult<_>>()?;
            Ok(json!({"bindings":state,"bulk":row.bulk}))
        })
        .collect::<IrResult<_>>()?;
    let request = json!({"script":operation.script,"mode":format!("{:?}",operation.mode),"rows":arguments,"traversers":traversers});
    let frame = serde_json::to_string(&request).map_err(|e| error(e.to_string()))?;
    if frame.len() as u64 > MAX_FRAME {
        return Err(error("JVM input frame exceeds 64 MiB"));
    }
    // One deadline covers startup, native callbacks, output decoding and later JVM
    // nodes. Callers can supply an earlier deadline and a cancellation token.
    execution
        .deadline
        .get_or_insert_with(|| Instant::now() + Duration::from_secs(30));
    let pool = execution.worker.0.clone();
    let mut guard = pool.lock().map_err(|_| error("JVM worker lock poisoned"))?;
    if guard.is_none() {
        *guard = Some(Worker::start(&config)?);
    }
    let worker = guard.as_ref().unwrap();
    let outgoing = worker.outgoing.as_ref().unwrap();
    let received = worker.received.as_ref().unwrap();
    let result = (|| {
        outgoing.send(frame).map_err(|e| error(e.to_string()))?;
        loop {
            execution.check()?;
            let response = match received.recv_timeout(Duration::from_millis(10)) {
                Ok(response) => response.map_err(error)?,
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(_) => return Err(error("JVM IR worker disconnected")),
            };
            match response.get("kind").and_then(Json::as_str) {
                Some("native") => {
                    let request = &response["request"];
                    let op = request["op"].as_str().unwrap_or("");
                    // Statement/transaction ownership remains with the native query executor.
                    let graph_id = response.get("graph").and_then(Json::as_u64).unwrap_or(0);
                    let response = if graph_id == 0 && op == "createExecutionGraph" {
                        let id = auxiliary.len() as u64 + 1;
                        auxiliary.insert(id, Store::new());
                        json!({"ok":true,"value":id})
                    } else if graph_id != 0 {
                        auxiliary
                            .get_mut(&graph_id)
                            .map(|graph| graph.request(request))
                            .unwrap_or_else(
                                || json!({"ok":false,"error":"Unknown execution graph"}),
                            )
                    } else if matches!(op, "commit" | "rollback" | "close")
                        || request["committed"] == true
                    {
                        json!({"ok":false,"error":"IR execution owns the graph transaction"})
                    } else {
                        store.request(request)
                    };
                    outgoing
                        .send(response.to_string())
                        .map_err(|e| error(e.to_string()))?;
                }
                Some("result") => {
                    if response["ok"] != true {
                        return Err(error(format!("JVM IR: {}", response["error"])));
                    }
                    if operation.mode == JvmMode::Sort {
                        let indices = response["order"]
                            .as_array()
                            .ok_or_else(|| error("Missing JVM row order"))?;
                        let order: Vec<usize> = indices
                            .iter()
                            .map(|v| {
                                v.as_u64()
                                    .and_then(|i| usize::try_from(i).ok())
                                    .ok_or_else(|| error("Invalid JVM row index"))
                            })
                            .collect::<IrResult<_>>()?;
                        let unique: BTreeSet<usize> = order.iter().copied().collect();
                        if unique != (0..rows.len()).collect() || order.len() != rows.len() {
                            return Err(error("JVM sort must return a permutation of its input"));
                        }
                        execution.check()?;
                        store.validate_execution_finish().map_err(error)?;
                        graph.restore_execution_overlay(store.graph());
                        return Ok(order.into_iter().map(|index| rows[index].clone()).collect());
                    }
                    let groups = response["rows"]
                        .as_array()
                        .ok_or_else(|| error("JVM result must contain row groups"))?;
                    if groups.len() != rows.len() {
                        return Err(error("JVM result lost input row correspondence"));
                    }
                    let mut output = Vec::new();
                    for (row, group) in rows.iter().zip(groups) {
                        let values = group
                            .as_array()
                            .ok_or_else(|| error("Invalid JVM row group"))?;
                        if !matches!(operation.mode, JvmMode::FlatMap | JvmMode::Computer)
                            && values.len() != 1
                        {
                            return Err(error("Invalid JVM result cardinality"));
                        }
                        for value in values {
                            let value = store.decode(value).map_err(error)?;
                            let mut next = row.clone();
                            if operation.mode == JvmMode::Filter {
                                match value {
                                    Value::Bool(true) => {}
                                    Value::Bool(false) => continue,
                                    _ => return Err(error("JVM filter must return Boolean")),
                                }
                            } else {
                                next.bindings.insert(operation.output.clone(), value);
                            }
                            output.push(next);
                        }
                    }
                    execution.check()?;
                    store.validate_execution_finish().map_err(error)?;
                    graph.restore_execution_overlay(store.graph());
                    return Ok(output);
                }
                _ => return Err(error("Unknown JVM IR response")),
            }
        }
    })();
    // Any failure invalidates this protocol session. The statement overlay is
    // discarded by the caller; a failed worker cannot leak replies or mutations.
    if result.is_err() {
        guard.take();
    }
    result
}

/// GraphComputer results are a query-local graph view, never a durable graph mutation.
pub fn contains_computer(node: &Node) -> bool {
    matches!(node, Node::GraphJvm {operation, ..} if operation.mode==JvmMode::Computer)
        || crate::ir::analysis::children(node)
            .into_iter()
            .any(contains_computer)
}

/// Vertex-program traversals operate on a read snapshot. Reject ordinary graph
/// writes instead of silently discarding them with the computed graph view.
pub fn validate_computer_plan(node: &Node) -> Result<(), String> {
    if !contains_computer(node) {
        return Ok(());
    }
    let mut pending = vec![node];
    while let Some(node) = pending.pop() {
        if !matches!(node, Node::GraphJvm { .. })
            && crate::ir::analysis::node_effect(node) == crate::ir::analysis::Effect::SourceMutation
        {
            return Err("GraphComputer traversal cannot contain graph mutation operators".into());
        }
        pending.extend(crate::ir::analysis::children(node));
    }
    Ok(())
}
