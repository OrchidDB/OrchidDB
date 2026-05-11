mod cypher;
mod gql;
mod gremlin;

use crate::{GraphLanguage, ParsedGraphProgram, Result};

pub use cypher::parse_cypher;
pub use gql::parse_gql;
pub use gremlin::parse_gremlin;

pub fn parse_language(language: GraphLanguage, query: &str) -> Result<ParsedGraphProgram> {
    match language {
        GraphLanguage::Cypher => parse_cypher(query),
        GraphLanguage::Gremlin => parse_gremlin(query),
        GraphLanguage::Gql => parse_gql(query),
    }
}
