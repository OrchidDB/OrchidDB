use crate::ParsedGraphProgram;

use super::Traversal;

#[derive(Debug, Clone, PartialEq)]
pub struct GremlinProgram {
    pub syntax: ParsedGraphProgram,
    pub traversal: Option<Traversal>,
}

impl GremlinProgram {
    pub fn new(syntax: ParsedGraphProgram, traversal: Traversal) -> Self {
        Self {
            syntax,
            traversal: Some(traversal),
        }
    }

    pub fn syntax_only(syntax: ParsedGraphProgram) -> Self {
        Self {
            syntax,
            traversal: None,
        }
    }
}
