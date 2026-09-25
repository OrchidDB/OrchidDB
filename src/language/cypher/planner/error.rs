#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum CypherPlanError {
    #[error("unsupported cypher construct: {0}")]
    Unsupported(String),
    #[error("invalid cypher plan: {0}")]
    Invalid(String),
    /// A semantic diagnosis made by the validator that detected it. The
    /// wrapped error preserves the existing human-readable API message.
    #[error("{error}")]
    Classified {
        code: CypherSemanticError,
        #[source]
        error: Box<CypherPlanError>,
    },
}

pub type CypherPlanResult<T> = std::result::Result<T, CypherPlanError>;

/// Stable semantic categories, independent of diagnostic wording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CypherSemanticError {
    InvalidPropertyAccess,
    NoVariablesInScope,
    InvalidClauseComposition,
    IntegerOverflow,
    FloatingPointOverflow,
    NoSingleRelationshipType,
    RequiresDirectedRelationship,
    CreatingVarLength,
    InvalidDelete,
    ProcedureNotFound,
    InvalidNumberOfArguments,
    InvalidArgumentPassingMode,
    UnexpectedSyntax,
    InvalidParameterUse,
    RelationshipUniquenessViolation,
    ColumnNameConflict,
    DifferentColumnsInUnion,
    NoExpressionAlias,
    InvalidArgumentType,
    NonConstantExpression,
    NegativeIntegerArgument,
    UndefinedVariable,
    VariableAlreadyBound,
    VariableTypeConflict,
    InvalidAggregation,
    UnknownFunction,
    AmbiguousAggregationExpression,
    NestedAggregation,
}

impl CypherPlanError {
    pub fn classified(self, code: CypherSemanticError) -> Self {
        Self::Classified {
            code,
            error: Box::new(self),
        }
    }

    /// The classification is present only when an exact semantic validator
    /// supplies it. Unsupported functionality has no inferred classification.
    pub fn classification(&self) -> Option<(&'static str, &'static str)> {
        let Self::Classified { code, .. } = self else {
            return None;
        };
        let detail = match code {
            CypherSemanticError::NoVariablesInScope => "NoVariablesInScope",
            CypherSemanticError::InvalidClauseComposition => "InvalidClauseComposition",
            CypherSemanticError::ProcedureNotFound => return Some(("ProcedureError","ProcedureNotFound")),
            CypherSemanticError::InvalidNumberOfArguments => "InvalidNumberOfArguments",
            CypherSemanticError::InvalidArgumentPassingMode => "InvalidArgumentPassingMode",
            CypherSemanticError::UnexpectedSyntax => "UnexpectedSyntax",
            CypherSemanticError::InvalidParameterUse => "InvalidParameterUse",
            CypherSemanticError::InvalidPropertyAccess => return Some(("TypeError","InvalidArgumentType")),
            CypherSemanticError::IntegerOverflow => "IntegerOverflow",
            CypherSemanticError::FloatingPointOverflow => "FloatingPointOverflow",
            CypherSemanticError::NoSingleRelationshipType => "NoSingleRelationshipType",
            CypherSemanticError::RequiresDirectedRelationship => "RequiresDirectedRelationship",
            CypherSemanticError::CreatingVarLength => "CreatingVarLength",
            CypherSemanticError::InvalidDelete => "InvalidDelete",
            CypherSemanticError::RelationshipUniquenessViolation => "RelationshipUniquenessViolation",
            CypherSemanticError::ColumnNameConflict => "ColumnNameConflict",
            CypherSemanticError::DifferentColumnsInUnion => "DifferentColumnsInUnion",
            CypherSemanticError::NoExpressionAlias => "NoExpressionAlias",
            CypherSemanticError::InvalidArgumentType => "InvalidArgumentType",
            CypherSemanticError::NonConstantExpression => "NonConstantExpression",
            CypherSemanticError::NegativeIntegerArgument => "NegativeIntegerArgument",
            CypherSemanticError::UndefinedVariable => "UndefinedVariable",
            CypherSemanticError::VariableAlreadyBound => "VariableAlreadyBound",
            CypherSemanticError::VariableTypeConflict => "VariableTypeConflict",
            CypherSemanticError::InvalidAggregation => "InvalidAggregation",
            CypherSemanticError::UnknownFunction => "UnknownFunction",
            CypherSemanticError::AmbiguousAggregationExpression => "AmbiguousAggregationExpression",
            CypherSemanticError::NestedAggregation => "NestedAggregation",
        };
        Some(("SyntaxError", detail))
    }
}
