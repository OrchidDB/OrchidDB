//! Shared frontend preparation with the parser's original typed diagnostics.
use std::collections::BTreeMap;
use crate::ir::{plan::GraphPlan, procedures::ProcedureCatalog, value::Value};
use super::{parser, parameters, planner, procedures};

#[derive(Debug, Clone, serde::Serialize)]
pub struct Diagnostic {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub detail: &'static str,
    pub phase: &'static str,
}
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct PreparationError {
    pub message: String,
    pub classification: Option<Diagnostic>,
}
fn error(message: String, classification: Option<(&'static str, &'static str)>) -> PreparationError {
    PreparationError { message, classification: classification.map(|(kind, detail)| Diagnostic {
        kind, detail, phase: "compile time",
    }) }
}
pub fn prepare(query: &str, values: &BTreeMap<String, Value>, catalog: Option<&ProcedureCatalog>) -> Result<GraphPlan, PreparationError> {
    let mut parsed = parser::parse_query(query).map_err(|e| error(e.to_string(), e.classification()))?;
    let planning = |e: planner::CypherPlanError| error(e.to_string(), e.classification());
    if let Some(catalog) = catalog { procedures::prepare(&mut parsed, catalog).map_err(planning)?; }
    parameters::bind_parameters_with_diagnostics(&mut parsed, values).map_err(|e| error(e.to_string(), e.classification()))?;
    if let Some(catalog) = catalog { procedures::prepare(&mut parsed, catalog).map_err(planning)?; }
    planner::CypherPlanner::new().plan(&parsed).map_err(planning)
}
