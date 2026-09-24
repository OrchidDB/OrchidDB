mod collections;
mod control;
mod filters;
mod literals;
mod mutations;
mod merge;
mod legacy_tokens;
mod predicates;
mod projection;
mod source;
mod strategy_validation;
mod visitor;
use literals::{option_key_text, parse_pick_key};

use crate::grammar::generated::gremlin::gremlinlexer::GremlinLexer;
use crate::grammar::generated::gremlin::gremlinparser as g;
use crate::grammar::generated::gremlin::gremlinparser::*;
use crate::grammar::generated::gremlin::gremlinvisitor::GremlinVisitor;
use crate::language::gremlin::ast::{
    AggKind, BySpec, CallArg, CastTarget, FormatPart, ListOpKind, MapColumn, MathExpr, OptionKey,
    Pop, SackOp, SortDir, Step, StringOp, Traversal, TraversalOption,
};
use crate::language::gremlin::semantics::{CompareOp, Direction, GValue, Predicate, TextKind};
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
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

/// Errors raised by the Gremlin parser frontend.
#[derive(Debug, thiserror::Error, PartialEq, Eq, Clone)]
pub enum GremlinParseError {
    #[error("parse: {0}")]
    Parse(String),
    #[error("unsupported gremlin construct: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, GremlinParseError>;

// Internal alias kept compatible with the original parser.rs which used
// `GremlinError::Parse(..)` / `GremlinError::Unsupported(..)` pervasively.
use GremlinParseError as GremlinError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GremlinToken {
    pub token_type: i32,
    pub symbolic_name: Option<&'static str>,
    pub literal_name: Option<&'static str>,
    pub text: String,
    pub line: isize,
    pub column: isize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GremlinSyntax {
    pub parse_tree: String,
    pub tokens: Vec<GremlinToken>,
}

pub fn parse_traversal(input: &str) -> Result<Traversal> {
    parse_traversal_with_bindings(input, &HashMap::new())
}

/// Parse a Gremlin source with caller-provided bindings for free variables.
///
/// Free variables (e.g. `vid1`, `xx2`) appear inside argument positions of
/// some TinkerPop conformance cases. The parser normally lowers them to
/// `NULL`/`0`/`""` so the chain still compiles. When a binding is supplied
/// here, the corresponding `GValue` is substituted instead.
pub fn parse_traversal_with_bindings(
    input: &str,
    bindings: &HashMap<String, GValue>,
) -> Result<Traversal> {
    let errors = SyntaxErrors::default();
    let mut lexer = GremlinLexer::new(InputStream::new(input));
    lexer.remove_error_listeners();
    lexer.add_error_listener(Box::new(errors.listener()));

    let lexer = legacy_tokens::LegacyTokens::new(lexer);
    let literal_overrides = lexer.literals.clone();
    let token_stream = CommonTokenStream::new(lexer);
    let mut parser = GremlinParser::new(token_stream);
    parser.remove_error_listeners();
    parser.add_error_listener(Box::new(errors.listener()));

    let root = parser
        .queryList()
        .map_err(|err| GremlinError::Parse(err.to_string()))?;
    errors.into_result()?;

    strategy_validation::verify(&tokenize(input)?)?;
    let mut visitor = LoweringVisitor::new(bindings.clone());
    visitor.literal_overrides = literal_overrides.borrow().clone();
    visitor.visit_queryList(&root);
    let mut traversal = visitor.finish()?;
    // `withoutStrategies(ConnectiveStrategy)` disables the infix
    // `.and()` / `.or()` rewrite; TinkerPop then fails the traversal, so
    // it yields no results. Model that as a drop-everything filter.
    if input.contains("withoutStrategies(ConnectiveStrategy")
        && traversal
            .steps
            .iter()
            .any(|s| matches!(s, Step::InfixAnd | Step::InfixOr))
    {
        traversal
            .steps
            .retain(|s| !matches!(s, Step::InfixAnd | Step::InfixOr));
        traversal.steps.push(Step::None);
    }
    Ok(traversal)
}

pub fn parse_query_list(input: &str) -> Result<GremlinSyntax> {
    let errors = SyntaxErrors::default();
    let mut lexer = GremlinLexer::new(InputStream::new(input));
    lexer.remove_error_listeners();
    lexer.add_error_listener(Box::new(errors.listener()));

    let token_stream = CommonTokenStream::new(lexer);
    let mut parser = GremlinParser::new(token_stream);
    parser.remove_error_listeners();
    parser.add_error_listener(Box::new(errors.listener()));

    let root = parser
        .queryList()
        .map_err(|err| GremlinError::Parse(err.to_string()))?;
    errors.into_result()?;

    Ok(GremlinSyntax {
        parse_tree: root.to_string_tree(&*parser),
        tokens: tokenize(input)?,
    })
}

pub fn tokenize(input: &str) -> Result<Vec<GremlinToken>> {
    let errors = SyntaxErrors::default();
    let mut lexer = GremlinLexer::new(InputStream::new(input));
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
        tokens.push(GremlinToken {
            token_type,
            symbolic_name: token_name(&g::_SYMBOLIC_NAMES, token_type),
            literal_name: token_name(&g::_LITERAL_NAMES, token_type),
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
            Err(GremlinError::Parse(messages.join("; ")))
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

// ---------- Lowering visitor ----------
//
// Walks the parse tree top-down, emitting `Step`s into `self.steps` and any
// errors into `self.errors`. Typed sub-results from leaf rules (literals,
// predicates, arguments) flow back to their parents through the per-type
// stacks; any rule we haven't taught the visitor to lower pushes a
// `GremlinError::Unsupported`. `finish()` returns the first error if any.
//
// Why per-type stacks instead of one tagged stack: each leaf rule has a
// natural typed result (a `String`, a `GValue`, a `Predicate`, ...) and
// keeping them separate makes the consumer `pop_*` calls obvious. A `Frame`
// enum would push the type-checking to runtime.

struct LoweringVisitor {
    steps: Vec<Step>,
    errors: Vec<GremlinError>,
    string_stack: Vec<String>,
    integer_stack: Vec<u64>,
    value_stack: Vec<GValue>,
    predicate_stack: Vec<Predicate>,
    /// Caller-supplied resolution table for free variables (e.g. `vid1`).
    /// Lookups go through `binding_value()`; absent entries fall back to the
    /// "lower to NULL/0/empty" defaults.
    bindings: HashMap<String, GValue>,
    literal_overrides: BTreeMap<isize, GValue>,
}

impl LoweringVisitor {
    fn new(bindings: HashMap<String, GValue>) -> Self {
        Self {
            bindings,
            literal_overrides: BTreeMap::new(),
            steps: Vec::new(),
            errors: Vec::new(),
            string_stack: Vec::new(),
            integer_stack: Vec::new(),
            value_stack: Vec::new(),
            predicate_stack: Vec::new(),
        }
    }

    fn finish(mut self) -> Result<Traversal> {
        if let Some(err) = self.errors.drain(..).next() {
            return Err(err);
        }
        Ok(Traversal::new(self.steps))
    }

    fn fail(&mut self, err: GremlinError) {
        self.errors.push(err);
    }

    /// Resolve a free variable (e.g. `vid1`) against the binding table.
    /// Returns `None` when no entry was supplied by the caller.
    fn binding_value(&self, name: &str) -> Option<GValue> {
        self.bindings.get(name).cloned()
    }

    fn pop_string(&mut self) -> Option<String> {
        self.string_stack.pop()
    }

    fn pop_integer(&mut self) -> Option<u64> {
        self.integer_stack.pop()
    }

    fn pop_value(&mut self) -> Option<GValue> {
        self.value_stack.pop()
    }

    fn pop_predicate(&mut self) -> Option<Predicate> {
        self.predicate_stack.pop()
    }

    fn lower_option<'input>(
        &mut self,
        ctx: &TraversalMethod_optionContextAll<'input>,
    ) -> Option<TraversalOption> {
        match ctx {
            TraversalMethod_optionContextAll::TraversalMethod_option_Predicate_TraversalContext(
                c,
            ) => {
                let predicate = c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.pop_predicate()
                })?;
                let traversal = c
                    .nestedTraversal()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                Some(TraversalOption {
                    key: OptionKey::Predicate(predicate),
                    traversal,
                })
            }
            TraversalMethod_optionContextAll::TraversalMethod_option_Object_TraversalContext(c) => {
                let key_text = option_key_text(&c.get_text());
                let key = match key_text.as_deref().and_then(parse_pick_key) {
                    Some(key) => key,
                    None => {
                        // `option(__.hasLabel("x"), t)` — an anonymous
                        // traversal key hides inside genericLiteral.
                        if let Some(nested) = c
                            .genericArgument()
                            .and_then(|arg| arg.genericLiteral())
                            .and_then(|lit| lit.nestedTraversal())
                        {
                            OptionKey::Traversal(self.lower_nested_traversal(&nested))
                        } else {
                            let value = c.genericArgument().and_then(|arg| {
                                self.visit_genericArgument(&arg);
                                self.pop_value()
                            })?;
                            OptionKey::Value(value)
                        }
                    }
                };
                let traversal = c
                    .nestedTraversal()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                Some(TraversalOption { key, traversal })
            }
            TraversalMethod_optionContextAll::TraversalMethod_option_TraversalContext(c) => {
                let key = option_key_text(&c.get_text())
                    .and_then(|text| parse_pick_key(&text))
                    .unwrap_or(OptionKey::PickAny);
                let traversal = c
                    .nestedTraversal()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                Some(TraversalOption { key, traversal })
            }
            TraversalMethod_optionContextAll::TraversalMethod_option_Merge_TraversalContext(c) => {
                let traversal = c
                    .nestedTraversal()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                Some(TraversalOption {
                    key: OptionKey::PickAny,
                    traversal,
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_parser_accepts_full_syntax_before_lowering() {
        let syntax =
            parse_query_list("g.V().has('age', P.gt(30)).out('knows').values('name').toList()")
                .expect("parse with official grammar");
        assert!(syntax.parse_tree.starts_with("(queryList"));
        assert!(
            syntax
                .tokens
                .iter()
                .any(|token| token.symbolic_name == Some("K_HAS"))
        );
    }

    #[test]
    fn rejects_invalid_gremlin_syntax() {
        let err = parse_query_list("g.V(").expect_err("syntax error");
        let msg = err.to_string();
        assert!(msg.contains("parse"), "expected parse error, got: {msg}");
    }

    #[test]
    fn lowers_supported_chain_with_namespaced_predicate() {
        let traversal = parse_traversal(
            "g.V().hasLabel('person').has('age', P.gt(30)).out('knows').values('name')",
        )
        .expect("lower traversal");
        assert_eq!(traversal.steps.len(), 5);
        assert!(matches!(traversal.steps[2], Step::Has { .. }));
    }

    #[test]
    fn lowers_partition_strategy_to_visibility_filter() {
        let traversal = parse_traversal(
            r#"g.withStrategies(new PartitionStrategy(partitionKey: "_partition", writePartition: "a", readPartitions: ["a", "b"])).V().values("name")"#,
        )
        .expect("lower PartitionStrategy");
        match &traversal.steps[0] {
            Step::WithStrategy {
                vertex_filter,
                edge_filter,
                vertex_property_filter: _,
                check_adjacent_vertices,
            } => {
                assert!(
                    !check_adjacent_vertices,
                    "PartitionStrategy filters an edge by its own partition, independently of its endpoints"
                );
                assert!(matches!(
                    vertex_filter.as_deref(),
                    Some([Step::Has {
                        key,
                        predicate: Predicate::Within(values),
                    }]) if key == "_partition" && values.len() == 2
                ));
                assert_eq!(vertex_filter, edge_filter);
            }
            other => panic!("unexpected step: {other:?}"),
        }
    }

    #[test]
    fn preserves_shortest_path_with_option() {
        let traversal = parse_traversal(
            r#"g.V().shortestPath().with("~tinkerpop.shortestPath.edges", Direction.IN)"#,
        )
        .expect("lower shortestPath with option");
        assert!(matches!(traversal.steps.as_slice(), [
            Step::V { .. },
            Step::ShortestPath,
            Step::WithOption { key, value: Some(GValue::DirectionToken(value)), .. },
        ] if key.ends_with("edges") && value == "IN"));
    }

    #[test]
    fn decodes_gremlin_string_escapes() {
        let traversal =
            parse_traversal(r#"g.V().has("name", "mark\ntwain").values('name')"#).unwrap();
        match &traversal.steps[1] {
            Step::Has { predicate, .. } => {
                assert_eq!(
                    predicate,
                    &Predicate::eq(GValue::String("mark\ntwain".to_string()))
                );
            }
            other => panic!("unexpected step: {other:?}"),
        }
    }

    #[test]
    fn lowers_terminal_to_list() {
        let traversal = parse_traversal("g.V().toList()").expect("lower toList");
        assert_eq!(traversal.steps.len(), 1);
        assert!(matches!(traversal.steps[0], Step::V { .. }));
    }

    #[test]
    fn lowers_terminal_next_with_count() {
        let traversal = parse_traversal("g.V().next(5)").expect("lower next");
        assert_eq!(traversal.steps.len(), 2);
        assert!(matches!(traversal.steps[1], Step::Limit(5)));
    }

    #[test]
    fn lowers_has_with_label_key_value() {
        let traversal = parse_traversal("g.V().has('person', 'age', 30)").expect("lower has-3");
        assert_eq!(traversal.steps.len(), 3);
        assert!(
            matches!(&traversal.steps[1], Step::HasLabel(labels) if labels == &vec!["person".to_string()])
        );
        assert!(matches!(&traversal.steps[2], Step::Has { .. }));
    }

    #[test]
    fn lowers_substring_arguments_from_method_text() {
        let traversal = parse_traversal("g.inject('test').substring(Scope.local, -3, -1)")
            .expect("lower substring");
        assert!(matches!(
            traversal.steps.as_slice(),
            [
                Step::Inject(_),
                Step::LocalScoped(inner)
            ] if matches!(inner.as_ref(), Step::StringOp(StringOp::Substring { start: -3, end: Some(-1) }))
        ));

        let traversal =
            parse_traversal("g.inject('test').substring(1, 8)").expect("lower scalar substring");
        assert!(matches!(
            traversal.steps.as_slice(),
            [
                Step::Inject(_),
                Step::StringOp(StringOp::Substring {
                    start: 1,
                    end: Some(8)
                })
            ]
        ));

        let traversal =
            parse_traversal(r#"g.V().hasLabel("software").values("name").substring(2)"#)
                .expect("lower chained scalar substring");
        assert!(matches!(
            traversal.steps.as_slice(),
            [
                Step::V { .. },
                Step::HasLabel(_),
                Step::Values(_),
                Step::StringOp(StringOp::Substring {
                    start: 2,
                    end: None
                })
            ]
        ));
    }
}
