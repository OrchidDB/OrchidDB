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
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JvmMode {
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
}
impl Default for JvmExecution {
    fn default() -> Self {
        Self {
            config: JvmConfig::from_env(),
            cancelled: Arc::new(AtomicBool::new(false)),
            deadline: None,
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
struct Worker(Child);
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
const MAX_FRAME: u64 = 64 * 1024 * 1024;

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
    let request =
        json!({"script":operation.script,"mode":format!("{:?}",operation.mode),"rows":arguments});
    let frame = serde_json::to_string(&request).map_err(|e| error(e.to_string()))?;
    if frame.len() as u64 > MAX_FRAME {
        return Err(error("JVM input frame exceeds 64 MiB"));
    }
    // One deadline covers startup, native callbacks, output decoding and later JVM
    // nodes. Callers can supply an earlier deadline and a cancellation token.
    execution
        .deadline
        .get_or_insert_with(|| Instant::now() + Duration::from_secs(30));
    let mut worker = Worker(
        Command::new(&config.java)
            .arg("-cp")
            .arg(&config.classpath)
            .arg("io.crabgraph.gremlin.CrabIr")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| error(format!("Cannot start JVM IR worker: {e}")))?,
    );
    let stdout = worker.0.stdout.take().unwrap();
    let mut stdin = worker.0.stdin.take().unwrap();
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
                        if operation.mode != JvmMode::FlatMap && values.len() != 1 {
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
    drop(outgoing);
    drop(received);
    drop(worker); // Terminate before joining either potentially blocked pipe thread.
    let _ = writer.join();
    let _ = reader.join();
    result
}
