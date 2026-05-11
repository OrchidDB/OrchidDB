//! Errors raised when lowering a Gremlin traversal into Graph IR.

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GremlinPlanError {
    #[error("parse: {0}")]
    Parse(String),
    #[error("unsupported gremlin construct: {0}")]
    Unsupported(String),
    #[error("plan: {0}")]
    Plan(String),
}

pub type GremlinPlanResult<T> = Result<T, GremlinPlanError>;
