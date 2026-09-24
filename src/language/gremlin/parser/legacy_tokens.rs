//! Compatibility for the no-argument `none()` step in Gremlin 3.x.
//! Reclassify only lexer tokens; strings, comments and `none(P)` remain intact.
use crate::grammar::generated::gremlin::gremlinparser::{
    Gremlin_DOT, Gremlin_K_AGGREGATE, Gremlin_K_DISCARD, Gremlin_K_NONE, Gremlin_LPAREN,
    Gremlin_RPAREN,
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
    pub(super) literals:
        std::rc::Rc<std::cell::RefCell<std::collections::BTreeMap<isize, super::GValue>>>,
}
antlr4rust::tid! { impl<'input, S> TidAble<'input> for LegacyTokens<'input, S> where S: TokenSource<'input, TF = CommonTokenFactory> }
impl<'input, S: TokenSource<'input, TF = CommonTokenFactory>> LegacyTokens<'input, S> {
    pub(super) fn new(source: S) -> Self {
        Self {
            source,
            pending: VecDeque::new(),
            previous: 0,
            literals: Default::default(),
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
        // The bytecode translator spells the enum's enclosing class, while
        // the grammar accepts Barrier.normSack. Strip only the exact token
        // prefix, leaving quoted text and unrelated identifiers untouched.
        if token.text == "SackFunctions" {
            while self.pending.iter().filter(|t| t.channel == TOKEN_DEFAULT_CHANNEL).count() < 4 {
                let next = self.source.next_token();
                let eof = next.token_type == TOKEN_EOF;
                self.pending.push_back(next);
                if eof { break; }
            }
            let parts: Vec<_> = self.pending.iter().enumerate()
                .filter(|(_, t)| t.channel == TOKEN_DEFAULT_CHANNEL)
                .take(4).map(|(i, t)| (i, t.text.to_string())).collect();
            if parts.len() == 4 && parts[0].1 == "." && parts[1].1 == "Barrier"
                && parts[2].1 == "." && parts[3].1 == "normSack" {
                for _ in 0..=parts[0].0 { self.pending.pop_front(); }
                return self.next_token();
            }
        }
        // A native constructor is a typed lexical atom. Preserve its source
        // position in a side table; ordinary string values are never interpreted
        // as vertices. The generated grammar sees a literal-shaped token.
        if token.text == "new" {
            while self
                .pending
                .iter()
                .filter(|t| t.channel == TOKEN_DEFAULT_CHANNEL)
                .count()
                < 6
            {
                let next = self.source.next_token();
                let eof = next.token_type == TOKEN_EOF;
                self.pending.push_back(next);
                if eof {
                    break;
                }
            }
            let parts: Vec<_> = self
                .pending
                .iter()
                .enumerate()
                .filter(|(_, t)| t.channel == TOKEN_DEFAULT_CHANNEL)
                .take(6)
                .map(|(i, t)| (i, t.text.to_string()))
                .collect();
            if parts.len() == 6
                && parts[0].1 == "Vertex"
                && parts[1].1 == "("
                && parts[3].1 == ","
                && parts[5].1 == ")"
            {
                let id = super::literals::decode_string_literal(&parts[2].1)
                    .map(super::GValue::String)
                    .or_else(|_| super::literals::parse_typed_integer_literal(&parts[2].1));
                let label = super::literals::decode_string_literal(&parts[4].1);
                if let (Ok(id), Ok(label)) = (id, label) {
                    self.literals.borrow_mut().insert(
                        token.start,
                        super::GValue::VertexRef {
                            id: Box::new(id),
                            label,
                        },
                    );
                    let end = parts[5].0;
                    for _ in 0..=end {
                        self.pending.pop_front();
                    }
                    token.token_type = crate::grammar::generated::gremlin::gremlinparser::Gremlin_EmptyStringLiteral;
                    token.text = std::borrow::Cow::Owned(
                        String::from_utf8(vec![34, 34]).expect("literal quotes"),
                    );
                }
            }
        }
        // Gremlin 3.x scoped aggregate/store share aggregate(String) syntax in
        // the newer grammar. Preserve scope on the actual method token for AST lowering.
        if self.previous == Gremlin_DOT && matches!(token.text.as_ref(), "aggregate" | "store") {
            let is_store = token.text == "store";
            while self
                .pending
                .iter()
                .filter(|t| t.channel == TOKEN_DEFAULT_CHANNEL)
                .count()
                < 5
            {
                let next = self.source.next_token();
                let eof = next.token_type == TOKEN_EOF;
                self.pending.push_back(next);
                if eof {
                    break;
                }
            }
            let parts = self
                .pending
                .iter()
                .enumerate()
                .filter(|(_, t)| t.channel == TOKEN_DEFAULT_CHANNEL)
                .map(|(i, t)| (i, t.text.to_string()))
                .collect::<Vec<_>>();
            let scope = if parts.first().is_some_and(|(_, t)| t == "(") {
                if parts.get(1).is_some_and(|(_, t)| t == "Scope")
                    && parts.get(2).is_some_and(|(_, t)| t == ".")
                {
                    parts
                        .get(3)
                        .filter(|(_, t)| matches!(t.as_str(), "local" | "global"))
                        .and_then(|(_, scope)| {
                            parts
                                .get(4)
                                .filter(|(_, t)| t == ",")
                                .map(|(end, _)| (scope == "local", *end))
                        })
                } else {
                    parts
                        .get(1)
                        .filter(|(_, t)| matches!(t.as_str(), "local" | "global"))
                        .and_then(|(_, scope)| {
                            parts
                                .get(2)
                                .filter(|(_, t)| t == ",")
                                .map(|(end, _)| (scope == "local", *end))
                        })
                }
            } else {
                None
            };
            if let Some((local, end)) = scope {
                let start = parts[0].0 + 1;
                self.pending.drain(start..=end);
                if local {
                    token.text = std::borrow::Cow::Borrowed("aggregate_local");
                }
            }
            if is_store {
                token.token_type = Gremlin_K_AGGREGATE;
                token.text = std::borrow::Cow::Borrowed("aggregate_local");
            }
        }
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
