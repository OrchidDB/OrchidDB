use crate::ParsedGraphProgram;

use super::Query;

#[derive(Debug, Clone, PartialEq)]
pub struct CypherProgram {
    pub syntax: ParsedGraphProgram,
    pub query: Option<Query>,
}

impl CypherProgram {
    pub fn new(syntax: ParsedGraphProgram, query: Query) -> Self {
        Self {
            syntax,
            query: Some(query),
        }
    }

    pub fn syntax_only(syntax: ParsedGraphProgram) -> Self {
        Self {
            syntax,
            query: None,
        }
    }
}
