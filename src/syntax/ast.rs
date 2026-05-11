use crate::GraphLanguage;

use super::token::GraphToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGraphProgram {
    pub language: GraphLanguage,
    pub source: String,
    pub entry_rule: &'static str,
    pub ast: GraphAst,
}

impl ParsedGraphProgram {
    pub fn new(
        language: GraphLanguage,
        source: impl Into<String>,
        entry_rule: &'static str,
        parse_tree: impl Into<String>,
        tokens: Vec<GraphToken>,
    ) -> Self {
        Self {
            language,
            source: source.into(),
            entry_rule,
            ast: GraphAst {
                parse_tree: parse_tree.into(),
                tokens,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphAst {
    pub parse_tree: String,
    pub tokens: Vec<GraphToken>,
}
