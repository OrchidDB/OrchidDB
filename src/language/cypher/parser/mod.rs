mod normalization;
use normalization::normalize_cypher_extensions;
mod visitor;

mod expression;

use crate::ParsedGraphProgram;
use crate::grammar::generated::cypher::cypherlexer::CypherLexer;
use crate::grammar::generated::cypher::cypherparser as c;
use crate::grammar::generated::cypher::cypherparser::*;
use crate::grammar::generated::cypher::cyphervisitor::CypherVisitor;
use crate::language::cypher::ast::*;
use antlr4rust::InputStream;
use antlr4rust::common_token_stream::CommonTokenStream;
use antlr4rust::error_listener::ErrorListener;
use antlr4rust::errors::ANTLRError;
use antlr4rust::parser::Parser;
use antlr4rust::recognizer::Recognizer;
use antlr4rust::token::{TOKEN_DEFAULT_CHANNEL, TOKEN_EOF, Token};
use antlr4rust::token_factory::TokenFactory;
use antlr4rust::token_stream::UnbufferedTokenStream;
use antlr4rust::tree::{ParseTree, ParseTreeVisitor};
use std::cell::RefCell;
use std::rc::Rc;

pub mod lowering;

#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum CypherParseError {
    #[error("parse: {0}")]
    Parse(String),
    #[error("unsupported cypher construct: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, CypherParseError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CypherToken {
    pub token_type: i32,
    pub symbolic_name: Option<&'static str>,
    pub literal_name: Option<&'static str>,
    pub text: String,
    pub line: isize,
    pub column: isize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CypherSyntax {
    pub parse_tree: String,
    pub tokens: Vec<CypherToken>,
}

pub fn parse_cypher(input: &str) -> Result<CypherProgram> {
    let query = parse_query(input)?;
    Ok(CypherProgram::new(
        ParsedGraphProgram {
            entry_rule: "oC_Cypher".to_string(),
        },
        query,
    ))
}

pub fn parse_query(input: &str) -> Result<Query> {
    let normalized = normalize_cypher_extensions(input);
    let (root, _syntax) = parse_root(&normalized)?;
    let mut visitor = lowering::visitor::AstLoweringVisitor::new();
    visitor.visit_oC_Cypher(&root);
    visitor.finish()
}

pub fn parse_syntax(input: &str) -> Result<CypherSyntax> {
    let normalized = normalize_cypher_extensions(input);
    let (_root, syntax) = parse_root(&normalized)?;
    Ok(syntax)
}

fn parse_root(input: &str) -> Result<(Rc<OC_CypherContextAll<'_>>, CypherSyntax)> {
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
        .map_err(|err| CypherParseError::Parse(err.to_string()))?;
    errors.into_result()?;

    let syntax = CypherSyntax {
        parse_tree: root.to_string_tree(&*parser),
        tokens: tokenize(input)?,
    };
    Ok((root, syntax))
}

pub fn tokenize(input: &str) -> Result<Vec<CypherToken>> {
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
        if token.get_channel() != TOKEN_DEFAULT_CHANNEL {
            continue;
        }
        tokens.push(CypherToken {
            token_type,
            symbolic_name: token_name(&c::_SYMBOLIC_NAMES, token_type),
            literal_name: token_name(&c::_LITERAL_NAMES, token_type),
            text: token.get_text().to_string(),
            line: token.get_line(),
            column: token.get_column(),
        });
    }
    errors.into_result()?;
    Ok(tokens)
}

fn token_name(names: &[Option<&'static str>], token_type: i32) -> Option<&'static str> {
    if token_type < 0 {
        return None;
    }
    names
        .get(token_type as usize)
        .and_then(|name| name.as_ref().copied())
}

#[derive(Clone, Default)]
struct SyntaxErrors {
    messages: Rc<RefCell<Vec<String>>>,
}

impl SyntaxErrors {
    fn listener(&self) -> Self {
        self.clone()
    }

    fn into_result(self) -> Result<()> {
        let messages = self.messages.borrow();
        if messages.is_empty() {
            Ok(())
        } else {
            Err(CypherParseError::Parse(messages.join("; ")))
        }
    }
}

impl<'a, T> ErrorListener<'a, T> for SyntaxErrors
where
    T: Recognizer<'a>,
{
    fn syntax_error(
        &self,
        _recognizer: &T,
        offending_symbol: Option<&<T::TF as TokenFactory<'a>>::Inner>,
        line: isize,
        column: isize,
        msg: &str,
        _error: Option<&ANTLRError>,
    ) {
        let offending = offending_symbol
            .map(ToString::to_string)
            .unwrap_or_else(|| "<unknown>".to_string());
        self.messages
            .borrow_mut()
            .push(format!("line {line}:{column} {msg} near {offending}"));
    }
}
