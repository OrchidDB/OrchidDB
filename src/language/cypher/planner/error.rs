#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum CypherPlanError {
    #[error("unsupported cypher construct: {0}")]
    Unsupported(String),
    #[error("invalid cypher plan: {0}")]
    Invalid(String),
}

pub type CypherPlanResult<T> = std::result::Result<T, CypherPlanError>;
