//! Compatibility for the no-argument `none()` step in Gremlin 3.x.
//! Reclassify only lexer tokens; strings, comments and `none(P)` remain intact.
use crate::grammar::generated::gremlin::gremlinparser::{
    Gremlin_DOT, Gremlin_K_DISCARD, Gremlin_K_NONE, Gremlin_LPAREN, Gremlin_RPAREN,
};
use antlr4rust::TokenSource;
use antlr4rust::int_stream::IntStream;
use antlr4rust::token::{CommonToken, TOKEN_DEFAULT_CHANNEL, TOKEN_EOF};
use antlr4rust::token_factory::CommonTokenFactory;
use std::collections::VecDeque;

pub(super) struct LegacyTokens<'input, S: TokenSource<'input, TF = CommonTokenFactory>> {
    source: S,
    pending: VecDeque<Box<CommonToken<'input>>>,
    previous: i32,
}
antlr4rust::tid! { impl<'input, S> TidAble<'input> for LegacyTokens<'input, S> where S: TokenSource<'input, TF = CommonTokenFactory> }
impl<'input, S: TokenSource<'input, TF = CommonTokenFactory>> LegacyTokens<'input, S> {
    pub(super) fn new(source: S) -> Self {
        Self {
            source,
            pending: VecDeque::new(),
            previous: 0,
        }
    }
}
impl<'input, S: TokenSource<'input, TF = CommonTokenFactory>> TokenSource<'input>
    for LegacyTokens<'input, S>
{
    type TF = CommonTokenFactory;
    fn next_token(&mut self) -> Box<CommonToken<'input>> {
        let mut token = self
            .pending
            .pop_front()
            .unwrap_or_else(|| self.source.next_token());
        // Java language translators qualify these enum constants. Strip only
        // the precise enum prefix, never string contents or arbitrary names.
        if token.text == "Scope" || token.text == "VertexProperty" {
            while self
                .pending
                .iter()
                .filter(|t| t.channel == TOKEN_DEFAULT_CHANNEL)
                .count()
                < 2
            {
                let next = self.source.next_token();
                let eof = next.token_type == TOKEN_EOF;
                self.pending.push_back(next);
                if eof {
                    break;
                }
            }
            let significant: Vec<_> = self
                .pending
                .iter()
                .filter(|t| t.channel == TOKEN_DEFAULT_CHANNEL)
                .take(2)
                .collect();
            let strip = significant.len() == 2
                && significant[0].token_type == Gremlin_DOT
                && ((token.text == "Scope"
                    && matches!(significant[1].text.as_ref(), "local" | "global"))
                    || (token.text == "VertexProperty" && significant[1].text == "Cardinality"));
            if strip {
                while let Some(next) = self.pending.pop_front() {
                    if next.token_type == Gremlin_DOT {
                        break;
                    }
                }
                return self.next_token();
            }
        }
        if token.token_type == Gremlin_K_NONE && self.previous == Gremlin_DOT {
            while self
                .pending
                .iter()
                .filter(|t| t.channel == TOKEN_DEFAULT_CHANNEL)
                .count()
                < 2
            {
                let next = self.source.next_token();
                let eof = next.token_type == TOKEN_EOF;
                self.pending.push_back(next);
                if eof {
                    break;
                }
            }
            let kinds: Vec<_> = self
                .pending
                .iter()
                .filter(|t| t.channel == TOKEN_DEFAULT_CHANNEL)
                .take(2)
                .map(|t| t.token_type)
                .collect();
            if kinds == [Gremlin_LPAREN, Gremlin_RPAREN] {
                token.token_type = Gremlin_K_DISCARD;
                token.text = std::borrow::Cow::Borrowed("discard");
            }
        }
        if token.channel == TOKEN_DEFAULT_CHANNEL {
            self.previous = token.token_type;
        }
        token
    }
    fn get_input_stream(&mut self) -> Option<&mut dyn IntStream> {
        self.source.get_input_stream()
    }
    fn get_source_name(&self) -> String {
        self.source.get_source_name()
    }
    fn get_token_factory(&self) -> &'input CommonTokenFactory {
        self.source.get_token_factory()
    }
    fn get_dfa_string(&self) -> String {
        self.source.get_dfa_string()
    }
}
