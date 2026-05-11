use antlr4rust::{
    InputStream,
    common_token_stream::CommonTokenStream,
    parser::Parser,
    token::{TOKEN_EOF, Token},
    token_stream::UnbufferedTokenStream,
    tree::ParseTree,
};

use crate::{
    GraphLanguage, GraphToken, ParsedGraphProgram, Result,
    grammar::generated::cypher::{
        cypherlexer::CypherLexer, cypherparser as c, cypherparser::CypherParser,
    },
    syntax::{error::SyntaxErrors, token::token_name},
};

pub fn parse_cypher(input: &str) -> Result<ParsedGraphProgram> {
    let errors = SyntaxErrors::default();
    let mut lexer = CypherLexer::new(InputStream::new(input));
    lexer.remove_error_listeners();
    lexer.add_error_listener(Box::new(errors.listener()));

    let token_stream = CommonTokenStream::new(lexer);
    let mut parser = CypherParser::new(token_stream);
    parser.remove_error_listeners();
    parser.add_error_listener(Box::new(errors.listener()));

    let root = parser
        .oC_Cypher()
        .map_err(|err| crate::GraphError::Parse(err.to_string()))?;
    errors.into_result()?;

    Ok(ParsedGraphProgram::new(
        GraphLanguage::Cypher,
        input,
        "oC_Cypher",
        root.to_string_tree(&*parser),
        tokenize_cypher(input)?,
    ))
}

fn tokenize_cypher(input: &str) -> Result<Vec<GraphToken>> {
    let errors = SyntaxErrors::default();
    let mut lexer = CypherLexer::new(InputStream::new(input));
    lexer.remove_error_listeners();
    lexer.add_error_listener(Box::new(errors.listener()));

    let mut token_stream = UnbufferedTokenStream::new_buffered(lexer);
    let mut tokens = Vec::new();
    for token in token_stream.token_iter() {
        let token_type = token.get_token_type();
        if token_type == TOKEN_EOF {
            break;
        }
        tokens.push(GraphToken {
            token_type,
            symbolic_name: token_name(&c::_SYMBOLIC_NAMES, token_type),
            literal_name: token_name(&c::_LITERAL_NAMES, token_type),
            text: token.get_text().to_string(),
            line: token.get_line(),
            column: token.get_column(),
            channel: token.get_channel(),
        });
    }
    errors.into_result()?;
    Ok(tokens)
}
