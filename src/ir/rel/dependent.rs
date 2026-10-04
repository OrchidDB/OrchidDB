//! Relational functions and dependent operations, independent of search and engine.
use super::sql::lowering::SqlTemplate;
use datafusion::{
    common::{DFSchemaRef, DataFusionError, Result},
    logical_expr::{Expr, ExprSchemable, Extension, LogicalPlan, UserDefinedLogicalNodeCore},
};
use std::{fmt, sync::Arc};

/// Whether a function accepts SQL correlation or needs values at prepare time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum ArgumentBinding {
    Correlated,
    PrepareTime,
}

/// A row-producing operation. Predicates remain above this boundary unless an
/// adapter explicitly proves and implements a valid pushdown transformation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TableFunction {
    pub name: Vec<String>,
    pub arguments: Vec<Expr>,
    pub source: Option<Arc<LogicalPlan>>,
    pub binding: ArgumentBinding,
    pub output_schema: DFSchemaRef,
    pub schema: DFSchemaRef,
}
impl TableFunction {
    pub fn new(
        name: Vec<String>,
        arguments: Vec<Expr>,
        source: Option<Arc<LogicalPlan>>,
        binding: ArgumentBinding,
        output_schema: DFSchemaRef,
    ) -> Result<Self> {
        if name.is_empty() || name.iter().any(String::is_empty) {
            return Err(DataFusionError::Plan(
                "table function name must be nonempty".into(),
            ));
        }
        let empty = datafusion::common::DFSchema::empty();
        let input = source
            .as_ref()
            .map(|p| p.schema().as_ref())
            .unwrap_or(&empty);
        for arg in &arguments {
            arg.get_type(input)?;
        }
        // Bound SQL parameters and exchange output are addressed by flat
        // column names. Reject ambiguous schemas before a name can select the
        // wrong source value; callers can project explicit aliases first.
        let mut names = std::collections::BTreeSet::new();
        for field in input.fields().iter().chain(output_schema.fields()) {
            if !names.insert(field.name()) {
                return Err(DataFusionError::Plan("table function source and output columns must have unique names; project unique aliases before creating the function".into()));
            }
        }
        let schema = Arc::new(input.join(output_schema.as_ref())?);
        Ok(Self {
            name,
            arguments,
            source,
            binding,
            output_schema,
            schema,
        })
    }
    pub fn into_plan(self) -> LogicalPlan {
        LogicalPlan::Extension(Extension {
            node: Arc::new(self),
        })
    }
}
impl PartialOrd for TableFunction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(format!("{self:?}").cmp(&format!("{other:?}")))
    }
}
impl UserDefinedLogicalNodeCore for TableFunction {
    fn name(&self) -> &str {
        "TableFunction"
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        self.source.iter().map(AsRef::as_ref).collect()
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<Expr> {
        self.arguments.clone()
    }
    fn fmt_for_explain(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "TableFunction: {} {:?}, {:?}",
            self.name.join("."),
            self.arguments,
            self.binding
        )
    }
    fn with_exprs_and_inputs(&self, exprs: Vec<Expr>, inputs: Vec<LogicalPlan>) -> Result<Self> {
        if inputs.len() != usize::from(self.source.is_some()) || exprs.len() != self.arguments.len()
        {
            return Err(DataFusionError::Plan(
                "table function inputs changed".into(),
            ));
        }
        Self::new(
            self.name.clone(),
            exprs,
            inputs.into_iter().next().map(Arc::new),
            self.binding,
            self.output_schema.clone(),
        )
    }
}

/// Execute a typed SQL template once for each source row, using the operation's
/// owning engine session. Its output schema includes any source columns needed
/// by the parent; the template explicitly projects them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DependentOperation {
    pub source: Arc<LogicalPlan>,
    pub template: SqlTemplate,
    pub schema: DFSchemaRef,
}
impl PartialOrd for DependentOperation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(format!("{self:?}").cmp(&format!("{other:?}")))
    }
}
impl DependentOperation {
    pub fn into_plan(self) -> LogicalPlan {
        LogicalPlan::Extension(Extension {
            node: Arc::new(self),
        })
    }
}
impl UserDefinedLogicalNodeCore for DependentOperation {
    fn name(&self) -> &str {
        "DependentOperation"
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        vec![&self.source]
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<Expr> {
        vec![]
    }
    fn fmt_for_explain(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DependentOperation: {}", self.template.sql)
    }
    fn with_exprs_and_inputs(&self, exprs: Vec<Expr>, inputs: Vec<LogicalPlan>) -> Result<Self> {
        if !exprs.is_empty() || inputs.len() != 1 {
            return Err(DataFusionError::Plan(
                "dependent operation inputs changed".into(),
            ));
        }
        Ok(Self {
            source: Arc::new(inputs[0].clone()),
            ..self.clone()
        })
    }
}

/// Engine-independent DataFusion execution of bound relational operations.
pub mod exec;
