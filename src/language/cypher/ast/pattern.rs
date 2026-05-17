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
    pub recursive: Option<RecursiveRelationshipPattern>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeLiteral {
    pub min: u32,
    pub max: Option<u32>,
    pub explicit: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecursiveRelationshipPattern {
    pub rel_variable: String,
    pub node_variable: String,
    pub predicate: Option<Expr>,
    pub rel_projection_keys: Option<Vec<String>>,
    pub node_projection_keys: Option<Vec<String>>,
}

impl Default for RangeLiteral {
    fn default() -> Self {
        Self {
            min: 1,
            max: Some(1),
            explicit: false,
        }
    }
}
