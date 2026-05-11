use crate::grammar::generated::cypher::cypherparser::{CypherParserContextType, OC_CypherContext};
use crate::grammar::generated::cypher::cyphervisitor::CypherVisitor;
use crate::language::cypher::ast::Query;
use crate::language::cypher::parser::{CypherParseError, Result};
use antlr4rust::tree::ParseTreeVisitor;

use super::{context::ParseLowering, statements};

#[derive(Debug, Default)]
pub(crate) struct AstLoweringVisitor {
    state: ParseLowering,
    query: Option<Query>,
}

impl AstLoweringVisitor {
    pub(crate) fn new() -> Self {
        Self {
            state: ParseLowering::new(),
            query: None,
        }
    }

    pub(crate) fn finish(self) -> Result<Query> {
        self.state.into_result()?;
        match self.query {
            Some(query) => Ok(query),
            None => Err(CypherParseError::Parse(
                "no Cypher query lowered".to_string(),
            )),
        }
    }
}

impl<'input> ParseTreeVisitor<'input, CypherParserContextType> for AstLoweringVisitor {}

impl<'input> CypherVisitor<'input> for AstLoweringVisitor {
    fn visit_oC_Cypher(&mut self, ctx: &OC_CypherContext<'input>) {
        match statements::lower_cypher(ctx) {
            Ok(query) => self.query = Some(query),
            Err(err) => self.state.fail(err),
        }
    }
}
