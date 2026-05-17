//! Recursive interpreter for the Graph IR.
//!
//! Consumes a `GraphPlan` plus a `PropertyGraph` and produces a stream
//! of rows. Each row is a `BTreeMap<String, Value>` plus an optional
//! hidden Gremlin `_bulk` count (default 1). Conversion to an Arrow
//! `RecordBatch` happens at `GraphReturn` boundaries.
//!
//! The interpreter is split across category modules:
//!
//! - [`run`]              top-level dispatcher (one match over `Node`).
//! - [`ops`]              one file per operator family (source, expand,
//!                        repeat, project, aggregate, distinct, sort,
//!                        slice, join, apply, unwind, coalesce, choose,
//!                        select, quantifier).
//! - [`expr`]             pure expression evaluation: `eval`, binary
//!                        ops, arithmetic, comparison, path predicates.
//! - [`runtime`]          the `eval_call` universe — casts, type checks,
//!                        string / list / path / property-object helpers,
//!                        reductions.
//! - [`arrow`]            Row stream → Arrow `RecordBatch`.

use std::collections::BTreeMap;

use arrow::array::RecordBatch;

use crate::ir::catalog::{CatalogError, PropertyGraph};
use crate::ir::plan::{GraphPlan, Node};
use crate::ir::policy::ResultForm;
use crate::ir::value::Value;

mod element_id;
mod expr;
mod ops;
mod output;
mod run;
mod runtime;

use crate::ir::interpreter::output::finalize_return;
use crate::ir::interpreter::run::run;

pub use crate::ir::interpreter::expr::eval;

#[derive(Debug, thiserror::Error)]
pub enum InterpretError {
    #[error("catalog: {0}")]
    Catalog(#[from] CatalogError),
    #[error("type error: {0}")]
    Type(String),
    #[error("{0}")]
    Runtime(String),
    #[error("unbound binding `{0}`")]
    Unbound(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("execution limit exceeded: {0}")]
    ExecutionLimit(String),
}

pub type IrResult<T> = Result<T, InterpretError>;

/// One row of the interpreter pipeline. Bindings flow forward; each
/// operator may add, remove, or rewrite them.
#[derive(Debug, Clone)]
pub struct Row {
    pub bindings: BTreeMap<String, Value>,
    /// Gremlin traverser bulk. Defaults to 1; observed by `countBulk` and
    /// reset by `dedup`.
    pub bulk: u64,
}

impl Row {
    pub fn new() -> Self {
        Self {
            bindings: BTreeMap::new(),
            bulk: 1,
        }
    }

    pub fn with(mut self, key: impl Into<String>, value: Value) -> Self {
        self.bindings.insert(key.into(), value);
        self
    }

    pub fn get(&self, key: &str) -> Value {
        self.bindings.get(key).cloned().unwrap_or(Value::Null)
    }
}

impl Default for Row {
    fn default() -> Self {
        Self::new()
    }
}

/// The Arrow-typed result of evaluating a `GraphReturn` boundary.
#[derive(Debug, Clone)]
pub struct ReturnedBatches {
    pub fields: Vec<String>,
    pub result_form: ResultForm,
    pub batch: RecordBatch,
}

pub fn execute(plan: &GraphPlan, graph: &PropertyGraph) -> IrResult<ReturnedBatches> {
    let policy = plan.policy.clone();
    let rows = match &*plan.root {
        Node::GraphReturn {
            fields,
            result_form,
            input,
        } => {
            let rows = run(input, graph)?;
            return finalize_return(fields, *result_form, rows, graph, &policy);
        }
        _ => run(&plan.root, graph)?,
    };
    // No explicit `Return` — emit a synthetic batch using all bindings of
    // the first row, in alphabetical order.
    let fields: Vec<String> = rows
        .first()
        .map(|row| row.bindings.keys().cloned().collect())
        .unwrap_or_default();
    finalize_return(&fields, ResultForm::RowSet, rows, graph, &policy)
}

/// Variant of `execute` that returns the raw `Row` stream — useful for
/// testing without fully Arrow-roundtripping.
pub fn execute_rows(plan: &GraphPlan, graph: &PropertyGraph) -> IrResult<Vec<Row>> {
    match &*plan.root {
        Node::GraphReturn { input, .. } => run(input, graph),
        _ => run(&plan.root, graph),
    }
}
