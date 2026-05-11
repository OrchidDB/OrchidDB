use crate::ir::plan::Direction;

use super::Expr;

#[derive(Debug, Clone, PartialEq)]
pub struct PatternPart {
    pub variable: Option<String>,
    pub element: PatternElement,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatternElement {
    pub start: NodePattern,
    pub chains: Vec<PatternElementChain>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatternElementChain {
    pub relationship: RelationshipPattern,
    pub node: NodePattern,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodePattern {
    pub variable: Option<String>,
    pub labels: Vec<String>,
    pub properties: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RelationshipPattern {
    pub variable: Option<String>,
    pub types: Vec<String>,
    pub range: RangeLiteral,
    pub direction: Direction,
    pub properties: Option<Expr>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeLiteral {
    pub min: u32,
    pub max: Option<u32>,
}

impl Default for RangeLiteral {
    fn default() -> Self {
        Self {
            min: 1,
            max: Some(1),
        }
    }
}
