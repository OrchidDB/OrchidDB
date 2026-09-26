//! Structured execution diagnoses retained across DataFusion and public APIs.
use std::error::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeDiagnosis {
    ArgumentType,
    NumberOutOfRange,
    InvalidType,
    MapKeyType,
    InvalidValue,
    DeleteConnectedNode,
    MergeReadOwnWrites,
    NegativeIntegerArgument,
    InvalidSliceArgument,
    DeletedEntityAccess,
    InvalidPropertyType,
}

impl RuntimeDiagnosis {
    pub fn classification(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::DeletedEntityAccess => return ("EntityNotFound", "DeletedEntityAccess", "runtime"),
            Self::InvalidPropertyType => return ("TypeError", "InvalidPropertyType", "runtime"),
            Self::InvalidType => return ("TypeError", "InvalidArgumentType", "runtime"),
            Self::MapKeyType => return ("TypeError", "MapElementAccessByNonString", "runtime"),
            Self::InvalidValue => return ("TypeError", "InvalidArgumentValue", "runtime"),
            Self::DeleteConnectedNode => return ("ConstraintVerificationFailed", "DeleteConnectedNode", "runtime"),
            Self::MergeReadOwnWrites => return ("SemanticError", "MergeReadOwnWrites", "runtime"),
            Self::NegativeIntegerArgument => return ("SyntaxError", "NegativeIntegerArgument", "runtime"),
            Self::InvalidSliceArgument => return ("SyntaxError", "InvalidArgumentType", "runtime"),
            _ => {}
        }
        let detail = match self {
            Self::ArgumentType => "InvalidArgumentType",
            Self::NumberOutOfRange => "NumberOutOfRange",
            _ => unreachable!(),
        };
        ("ArgumentError", detail, "runtime")
    }
}

/// The message remains compatible with the string-returning engine APIs;
/// typed callers also receive the diagnosis made by the failing operator.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct QueryExecutionError {
    pub message: String,
    pub diagnosis: Option<RuntimeDiagnosis>,
}

impl QueryExecutionError {
    pub fn from_error(error: impl Error + 'static) -> Self {
        let message = error.to_string();
        let mut source: Option<&(dyn Error + 'static)> = Some(&error);
        let mut diagnosis = None;
        while let Some(error) = source {
            if matches!(error.downcast_ref::<crate::ir::catalog::CatalogError>(),
                Some(crate::ir::catalog::CatalogError::DeleteIntegrity(_))) {
                diagnosis = Some(RuntimeDiagnosis::DeleteConnectedNode);
                break;
            }
            if let Some(error) = error.downcast_ref::<Self>() {
                diagnosis = error.diagnosis;
                break;
            }
            if let Some(crate::ir::RuntimeError::Diagnosed { code, .. }) =
                error.downcast_ref::<crate::ir::RuntimeError>()
            {
                diagnosis = Some(*code);
                break;
            }
            source = error.source();
        }
        Self { message, diagnosis }
    }
}

impl From<String> for QueryExecutionError {
    fn from(message: String) -> Self {
        Self {
            message,
            diagnosis: None,
        }
    }
}
impl From<&str> for QueryExecutionError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}
