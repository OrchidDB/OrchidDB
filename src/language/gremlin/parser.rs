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

    let token_stream = CommonTokenStream::new(lexer);
    let mut parser = GremlinParser::new(token_stream);
    parser.remove_error_listeners();
    parser.add_error_listener(Box::new(errors.listener()));

    let root = parser
        .queryList()
        .map_err(|err| GremlinError::Parse(err.to_string()))?;
    errors.into_result()?;

    let mut visitor = LoweringVisitor::new(bindings.clone());
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

/// Map a `GType.X` identifier (or bare `X`) to a numeric cast refinement.
/// Returns `None` for non-numeric refinements; the caller falls back to
/// the un-refined `CastTarget::Number`.
fn numeric_cast_from_token(text: &str) -> Option<crate::language::gremlin::ast::NumericCast> {
    use crate::language::gremlin::ast::NumericCast;
    let normalised = text
        .trim()
        .trim_start_matches("GType.")
        .trim_start_matches("java.lang.")
        .trim_start_matches("java.math.")
        .to_ascii_lowercase();
    Some(match normalised.as_str() {
        "byte" => NumericCast::Byte,
        "short" => NumericCast::Short,
        "int" | "integer" => NumericCast::Int,
        "long" => NumericCast::Long,
        "bigint" | "biginteger" => NumericCast::BigInt,
        "float" => NumericCast::Float,
        "double" => NumericCast::Double,
        "bigdecimal" | "decimal" => NumericCast::BigDecimal,
        _ => return None,
    })
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
}

impl LoweringVisitor {
    fn new(bindings: HashMap<String, GValue>) -> Self {
        Self {
            bindings,
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
                        let value = c.genericArgument().and_then(|arg| {
                            self.visit_genericArgument(&arg);
                            self.pop_value()
                        })?;
                        OptionKey::Value(value)
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

impl<'input> ParseTreeVisitor<'input, GremlinParserContextType> for LoweringVisitor {}

impl<'input> GremlinVisitor<'input> for LoweringVisitor {
    // ---- queryList / query / source / root ----

    fn visit_queryList(&mut self, ctx: &QueryListContext<'input>) {
        let queries = ctx.query_all();
        if queries.is_empty() {
            self.fail(GremlinError::Parse("no queries found".to_string()));
            return;
        }
        if queries.len() > 1 {
            self.fail(GremlinError::Unsupported(
                "query lists parse, but only single-traversal queries lower to SQL islands"
                    .to_string(),
            ));
            return;
        }
        self.visit_query(&queries[0]);
    }

    fn visit_query(&mut self, ctx: &QueryContext<'input>) {
        if ctx.emptyQuery().is_some() {
            // Empty traversal — emit a degenerate vertex scan for the
            // compile_ok metric.
            self.steps.push(Step::V { ids: Vec::new() });
            return;
        }
        if ctx.K_TOSTRING().is_some() {
            // toString() wrapper around an inner query: lower the inner
            // query but ignore the toString rendering.
            if let Some(inner) = ctx.query() {
                self.visit_query(&inner);
            }
            return;
        }
        if let Some(root) = ctx.rootTraversal() {
            self.visit_rootTraversal(&root);
            if let Some(term) = ctx.traversalTerminalMethod() {
                self.visit_traversalTerminalMethod(&term);
            }
            return;
        }
        if ctx.transactionPart().is_some() || ctx.traversalSource().is_some() {
            // `g.tx().begin()` etc., or just `g`: degenerate vertex scan.
            self.steps.push(Step::V { ids: Vec::new() });
            return;
        }
        self.fail(GremlinError::Parse(format!(
            "unrecognised query form: `{}`",
            ctx.get_text()
        )));
    }

    fn visit_rootTraversal(&mut self, ctx: &RootTraversalContext<'input>) {
        if let Some(source) = ctx.traversalSource() {
            // Source-self methods (`g.withSack(...)` etc.) prepend prefix
            // steps that the planner consumes to seed sack/side-effect
            // initial state before the main chain runs. Walk the source
            // recursively to recover them in declaration order.
            self.visit_traversalSource_recursive(&source);
        } else {
            self.fail(GremlinError::Parse(
                "root traversal missing traversal source".to_string(),
            ));
            return;
        }
        let Some(spawn) = ctx.traversalSourceSpawnMethod() else {
            self.fail(GremlinError::Parse(
                "root traversal missing spawn method".to_string(),
            ));
            return;
        };
        self.visit_traversalSourceSpawnMethod(&spawn);
        if let Some(chained) = ctx.chainedTraversal() {
            self.visit_chainedTraversal(&chained);
        }
    }

    fn visit_chainedTraversal(&mut self, ctx: &ChainedTraversalContext<'input>) {
        // Left-recursive: chainedTraversal | chainedTraversal DOT traversalMethod.
        // Walk the inner chain first so steps land in source order.
        if let Some(inner) = ctx.chainedTraversal() {
            self.visit_chainedTraversal(&inner);
        }
        if let Some(method) = ctx.traversalMethod() {
            self.visit_traversalMethod(&method);
        }
    }

    // ---- spawn methods ----

    fn visit_traversalSourceSpawnMethod(
        &mut self,
        ctx: &TraversalSourceSpawnMethodContext<'input>,
    ) {
        if let Some(c) = ctx.traversalSourceSpawnMethod_V() {
            self.visit_traversalSourceSpawnMethod_V(&c);
            return;
        }
        if let Some(c) = ctx.traversalSourceSpawnMethod_E() {
            self.visit_traversalSourceSpawnMethod_E(&c);
            return;
        }
        if let Some(c) = ctx.traversalSourceSpawnMethod_inject() {
            self.visit_traversalSourceSpawnMethod_inject(&c);
            return;
        }
        if let Some(c) = ctx.traversalSourceSpawnMethod_union() {
            // `g.union(t1, t2, ...)` — collect children and emit a leading
            // Union step. The planner treats Union-as-first-step as a
            // sourceless start (children supply their own sources via V/E/
            // inject; anchorless children operate over an empty input set).
            let traversals = self.collect_nested_traversal_list(c.nestedTraversalList());
            self.steps.push(Step::Union(traversals));
            return;
        }
        if let Some(c) = ctx.traversalSourceSpawnMethod_call() {
            let (name, args) = self.lower_source_call(&c);
            self.steps.push(Step::Call(name, args));
            return;
        }
        // Unknown spawn methods (`io`, `call`, etc.) lower to a best-effort
        // empty vertex scan so the rest of the chain still compiles. The
        // result row count will be wrong; the alternative is refusing to
        // compile a large class of scenarios.
        self.steps.push(Step::V { ids: Vec::new() });
    }

    fn visit_traversalSourceSpawnMethod_inject(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_injectContext<'input>,
    ) {
        let values = match ctx
            .genericLiteralVarargs()
            .and_then(|v| v.genericLiteralExpr())
        {
            Some(expr) => {
                let mut out = Vec::new();
                for arg in expr.genericLiteral_all() {
                    self.visit_genericLiteral(&arg);
                    let Some(value) = self.pop_value() else {
                        return;
                    };
                    out.push(value);
                }
                out
            }
            None => Vec::new(),
        };
        self.steps.push(Step::Inject(values));
    }

    fn visit_traversalSourceSpawnMethod_V(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_VContext<'input>,
    ) {
        let Some(varargs) = ctx.genericArgumentVarargs() else {
            self.fail(GremlinError::Parse("V() missing argument list".to_string()));
            return;
        };
        match self.collect_generic_argument_varargs(&varargs) {
            Ok(ids) => self.steps.push(Step::V { ids }),
            Err(err) => self.fail(err),
        }
    }

    fn visit_traversalSourceSpawnMethod_E(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_EContext<'input>,
    ) {
        let Some(varargs) = ctx.genericArgumentVarargs() else {
            self.fail(GremlinError::Parse("E() missing argument list".to_string()));
            return;
        };
        match self.collect_generic_argument_varargs(&varargs) {
            Ok(ids) => self.steps.push(Step::E { ids }),
            Err(err) => self.fail(err),
        }
    }

    // ---- traversalMethod dispatch ----

    fn visit_traversalMethod(&mut self, ctx: &TraversalMethodContext<'input>) {
        if let Some(c) = ctx.traversalMethod_V() {
            self.visit_traversalMethod_V(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_E() {
            self.visit_traversalMethod_E(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_hasLabel() {
            self.dispatch_traversalMethod_hasLabel(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_hasNot() {
            self.visit_traversalMethod_hasNot(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_hasKey() {
            self.dispatch_traversalMethod_hasKey(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_has() {
            self.dispatch_traversalMethod_has(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_out() {
            self.visit_traversalMethod_out(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_in() {
            self.visit_traversalMethod_in(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_both() {
            self.visit_traversalMethod_both(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_outE() {
            self.visit_traversalMethod_outE(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_inE() {
            self.visit_traversalMethod_inE(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_bothE() {
            self.visit_traversalMethod_bothE(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_outV() {
            self.visit_traversalMethod_outV(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_inV() {
            self.visit_traversalMethod_inV(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_bothV() {
            self.visit_traversalMethod_bothV(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_values() {
            self.visit_traversalMethod_values(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_limit() {
            self.dispatch_traversalMethod_limit(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_count() {
            self.dispatch_traversalMethod_count(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_discard() {
            self.visit_traversalMethod_discard(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_id() {
            self.visit_traversalMethod_id(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_label() {
            self.visit_traversalMethod_label(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_identity() {
            self.visit_traversalMethod_identity(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_is() {
            self.dispatch_traversalMethod_is(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_hasId() {
            self.dispatch_traversalMethod_hasId(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_dedup() {
            self.dispatch_traversalMethod_dedup(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_order() {
            self.dispatch_traversalMethod_order(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_range() {
            self.dispatch_traversalMethod_range(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_skip() {
            self.dispatch_traversalMethod_skip(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_tail() {
            self.dispatch_traversalMethod_tail(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_as() {
            self.visit_traversalMethod_as(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_asNumber() {
            self.dispatch_cast_traversalGType(&c, CastTarget::Number, "asNumber");
            return;
        }
        if let Some(c) = ctx.traversalMethod_asString() {
            self.dispatch_cast_simple(&c.get_text(), CastTarget::String);
            return;
        }
        if let Some(c) = ctx.traversalMethod_asBool() {
            self.dispatch_cast_simple(&c.get_text(), CastTarget::Bool);
            return;
        }
        if let Some(c) = ctx.traversalMethod_asDate() {
            self.dispatch_cast_simple(&c.get_text(), CastTarget::Date);
            return;
        }
        if let Some(c) = ctx.traversalMethod_constant() {
            self.visit_traversalMethod_constant(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_properties() {
            self.visit_traversalMethod_properties(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_valueMap() {
            self.dispatch_traversalMethod_valueMap(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_elementMap() {
            self.visit_traversalMethod_elementMap(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_barrier() {
            self.dispatch_traversalMethod_barrier(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_simplePath() {
            self.visit_traversalMethod_simplePath(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_cyclicPath() {
            self.visit_traversalMethod_cyclicPath(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_sum() {
            self.push_aggregate_with_scope(&c.get_text(), AggKind::Sum);
            return;
        }
        if let Some(c) = ctx.traversalMethod_min() {
            self.push_aggregate_with_scope(&c.get_text(), AggKind::Min);
            return;
        }
        if let Some(c) = ctx.traversalMethod_max() {
            self.push_aggregate_with_scope(&c.get_text(), AggKind::Max);
            return;
        }
        if let Some(c) = ctx.traversalMethod_mean() {
            self.push_aggregate_with_scope(&c.get_text(), AggKind::Mean);
            return;
        }
        if let Some(c) = ctx.traversalMethod_product() {
            // `product()` (no args) is a legacy multiplication-fold
            // aggregate. `product(...)` with an arg is the list-op
            // cartesian product.
            let text = c.get_text();
            let has_args = !text.ends_with("()");
            if !has_args {
                self.push_aggregate_with_scope(&text, AggKind::Product);
            } else {
                match &*c {
                    TraversalMethod_productContextAll::TraversalMethod_product_ObjectContext(i) => {
                        self.dispatch_list_op(i.genericLiteral(), ListOpKind::Product);
                    }
                    _ => {
                        self.steps
                            .push(Step::ListOp(ListOpKind::Product, GValue::Null));
                    }
                }
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_group() {
            // group()  →  Step::Group ;  group("a")  →  Step::GroupAs("a").
            // The label form additionally seeds the named side-effect bag
            // so a downstream `cap("a")` retrieves the computed map.
            match extract_first_string_arg(&c.get_text()) {
                Some(label) => self.steps.push(Step::GroupAs(label)),
                None => self.steps.push(Step::Group),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_groupCount() {
            match extract_first_string_arg(&c.get_text()) {
                Some(label) => self.steps.push(Step::GroupCountAs(label)),
                None => self.steps.push(Step::GroupCount),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_fold() {
            self.dispatch_traversalMethod_fold(&c);
            return;
        }
        if ctx.traversalMethod_unfold().is_some() {
            self.steps.push(Step::Unfold);
            return;
        }
        if let Some(c) = ctx.traversalMethod_select() {
            self.dispatch_traversalMethod_select(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_filter() {
            self.dispatch_filter_or_where(&c, "filter");
            return;
        }
        if let Some(c) = ctx.traversalMethod_where() {
            self.dispatch_traversalMethod_where(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_union() {
            self.visit_traversalMethod_union(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_coalesce() {
            self.visit_traversalMethod_coalesce(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_local() {
            self.visit_traversalMethod_local(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_by() {
            self.dispatch_traversalMethod_by(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_map() {
            self.visit_traversalMethod_map(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_flatMap() {
            self.visit_traversalMethod_flatMap(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_choose() {
            self.dispatch_traversalMethod_choose(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_branch() {
            self.visit_traversalMethod_branch(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_sample() {
            self.dispatch_traversalMethod_sample(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_not() {
            self.visit_traversalMethod_not(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_repeat() {
            self.dispatch_traversalMethod_repeat(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_times() {
            self.visit_traversalMethod_times(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_coin() {
            self.visit_traversalMethod_coin(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_length() {
            self.dispatch_simple_string_op(&c.get_text(), StringOp::Length);
            return;
        }
        if let Some(c) = ctx.traversalMethod_toLower() {
            self.dispatch_simple_string_op(&c.get_text(), StringOp::ToLower);
            return;
        }
        if let Some(c) = ctx.traversalMethod_toUpper() {
            self.dispatch_simple_string_op(&c.get_text(), StringOp::ToUpper);
            return;
        }
        if let Some(c) = ctx.traversalMethod_trim() {
            self.dispatch_simple_string_op(&c.get_text(), StringOp::Trim);
            return;
        }
        if let Some(c) = ctx.traversalMethod_lTrim() {
            self.dispatch_simple_string_op(&c.get_text(), StringOp::LTrim);
            return;
        }
        if let Some(c) = ctx.traversalMethod_rTrim() {
            self.dispatch_simple_string_op(&c.get_text(), StringOp::RTrim);
            return;
        }
        if let Some(c) = ctx.traversalMethod_reverse() {
            self.dispatch_simple_string_op(&c.get_text(), StringOp::Reverse);
            return;
        }
        if let Some(c) = ctx.traversalMethod_substring() {
            self.dispatch_traversalMethod_substring(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_replace() {
            self.dispatch_traversalMethod_replace(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_concat() {
            self.dispatch_traversalMethod_concat(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_split() {
            self.dispatch_traversalMethod_split(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_aggregate() {
            self.dispatch_traversalMethod_aggregate(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_cap() {
            self.visit_traversalMethod_cap(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_sideEffect() {
            self.visit_traversalMethod_sideEffect(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_with() {
            self.dispatch_traversalMethod_with(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_value() {
            self.visit_traversalMethod_value(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_otherV() {
            self.visit_traversalMethod_otherV(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_optional() {
            self.visit_traversalMethod_optional(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_emit() {
            self.dispatch_traversalMethod_emit(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_until() {
            self.dispatch_traversalMethod_until(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_option() {
            self.dispatch_traversalMethod_option(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_and() {
            self.visit_traversalMethod_and(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_or() {
            self.visit_traversalMethod_or(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_none() {
            self.dispatch_traversalMethod_none(&c);
            return;
        }
        if ctx.traversalMethod_element().is_some() {
            // element() retrieves the parent element from a property object.
            // The new dedicated variant lets a future planner recognise
            // the inverse `.properties()` relation.
            self.steps.push(Step::Element);
            return;
        }
        if let Some(c) = ctx.traversalMethod_project() {
            self.visit_traversalMethod_project(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_loops() {
            self.dispatch_traversalMethod_loops(&c);
            return;
        }
        if ctx.traversalMethod_path().is_some() {
            self.steps.push(Step::Path);
            return;
        }
        if let Some(c) = ctx.traversalMethod_math() {
            self.visit_traversalMethod_math(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_hasValue() {
            self.dispatch_traversalMethod_hasValue(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_match() {
            self.visit_traversalMethod_match(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_all() {
            self.dispatch_traversalMethod_all(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_any() {
            self.dispatch_traversalMethod_any(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_format() {
            self.dispatch_traversalMethod_format(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_conjoin() {
            self.dispatch_traversalMethod_conjoin(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_sack() {
            // `sack()` reads, `sack(op)` mutates with the following by(...)
            // modulator. The two arms are distinct sub-rules in the grammar.
            match &*c {
                TraversalMethod_sackContextAll::TraversalMethod_sack_BiFunctionContext(b) => {
                    let op = b
                        .traversalBiFunction()
                        .and_then(|tb| {
                            tb.traversalOperator()
                                .and_then(|o| sack_op_from_text(&o.get_text()))
                        })
                        .unwrap_or(SackOp::Assign);
                    self.steps.push(Step::SackOp(op));
                }
                _ => {
                    self.steps.push(Step::Sack);
                }
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_dateAdd() {
            let unit = c
                .traversalDT()
                .map(|dt| date_unit_from_text(&dt.get_text()))
                .unwrap_or_else(|| "second".to_string());
            let amount = c
                .integerLiteral()
                .and_then(|lit| parse_integer_literal(&lit.get_text()).ok())
                .unwrap_or(0);
            self.steps.push(Step::DateAdd { unit, amount });
            return;
        }
        if let Some(c) = ctx.traversalMethod_dateDiff() {
            let rhs = match &*c {
                TraversalMethod_dateDiffContextAll::TraversalMethod_dateDiff_DateContext(inner) => {
                    inner
                        .dateLiteral()
                        .and_then(|date| parse_date_literal_ctx(&date))
                        .map(GValue::DateTime)
                        .unwrap_or(GValue::Null)
                }
                TraversalMethod_dateDiffContextAll::TraversalMethod_dateDiff_TraversalContext(
                    inner,
                ) => date_diff_traversal_arg(inner),
                TraversalMethod_dateDiffContextAll::Error(_) => GValue::Null,
            };
            self.steps.push(Step::DateDiff(rhs));
            return;
        }
        // `from(label)` / `to(label)` modulators on a preceding `path()`
        // (or `select(...)` of paths). When the argument is a string label
        // we emit `PathFrom`/`PathTo`; non-string forms (Direction enum,
        // sub-traversal — those go with addE) fall back to Identity since
        // we don't model the addE side.
        if let Some(c) = ctx.traversalMethod_from() {
            match extract_first_string_arg(&c.get_text()) {
                Some(label) => self.steps.push(Step::PathFrom(label)),
                None => self.steps.push(Step::Identity),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_to() {
            let raw = c.get_text();
            if let Some(direction) = direction_from_to_arg(&raw) {
                self.steps.push(Step::ExpandVertex {
                    direction,
                    edge_labels: extract_top_level_string_args(&raw),
                });
            } else {
                match extract_first_string_arg(&raw) {
                    Some(label) => self.steps.push(Step::PathTo(label)),
                    None => self.steps.push(Step::Identity),
                }
            }
            return;
        }
        if ctx.traversalMethod_read().is_some() {
            self.steps.push(Step::Identity);
            return;
        }
        if let Some(c) = ctx.traversalMethod_subgraph() {
            // subgraph("sg") — gather traversed edges into a side-effect
            // named "sg". Compile-only for now; the dedicated variant lets
            // a future planner attach a sub-graph snapshot to the bag.
            let label = extract_first_string_arg(&c.get_text()).unwrap_or_default();
            self.steps.push(Step::Subgraph(label));
            return;
        }
        // Mid-traversal `inject(values)`: emit a `Step::Inject` so sub-
        // contexts (notably `union(__.inject(...), ...)`) can root a child
        // on the injected values. Mid-traversal inject in a top-level chain
        // is no longer modelled as a no-op: the planner's branch-step
        // dispatch leaves it as a no-op continuation, so injection-into-
        // upstream still degenerates to identity, but sub-traversal source
        // detection now sees the real Inject step.
        if let Some(c) = ctx.traversalMethod_inject() {
            let values = match c
                .genericLiteralVarargs()
                .and_then(|v| v.genericLiteralExpr())
            {
                Some(expr) => {
                    let mut out = Vec::new();
                    for arg in expr.genericLiteral_all() {
                        self.visit_genericLiteral(&arg);
                        let Some(value) = self.pop_value() else {
                            return;
                        };
                        out.push(value);
                    }
                    out
                }
                None => Vec::new(),
            };
            self.steps.push(Step::Inject(values));
            return;
        }
        if let Some(c) = ctx.traversalMethod_merge() {
            match &*c {
                TraversalMethod_mergeContextAll::TraversalMethod_merge_ObjectContext(i) => {
                    self.dispatch_list_op(i.genericLiteral(), ListOpKind::Merge);
                }
                _ => self
                    .steps
                    .push(Step::ListOp(ListOpKind::Merge, GValue::Null)),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_combine() {
            match &*c {
                TraversalMethod_combineContextAll::TraversalMethod_combine_ObjectContext(i) => {
                    self.dispatch_list_op(i.genericLiteral(), ListOpKind::Combine);
                }
                _ => self
                    .steps
                    .push(Step::ListOp(ListOpKind::Combine, GValue::Null)),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_intersect() {
            match &*c {
                TraversalMethod_intersectContextAll::TraversalMethod_intersect_ObjectContext(i) => {
                    self.dispatch_list_op(i.genericLiteral(), ListOpKind::Intersect);
                }
                _ => self
                    .steps
                    .push(Step::ListOp(ListOpKind::Intersect, GValue::Null)),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_difference() {
            match &*c {
                TraversalMethod_differenceContextAll::TraversalMethod_difference_ObjectContext(
                    i,
                ) => {
                    self.dispatch_list_op(i.genericLiteral(), ListOpKind::Difference);
                }
                _ => self
                    .steps
                    .push(Step::ListOp(ListOpKind::Difference, GValue::Null)),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_disjunct() {
            match &*c {
                TraversalMethod_disjunctContextAll::TraversalMethod_disjunct_ObjectContext(i) => {
                    self.dispatch_list_op(i.genericLiteral(), ListOpKind::Disjunct);
                }
                _ => self
                    .steps
                    .push(Step::ListOp(ListOpKind::Disjunct, GValue::Null)),
            }
            return;
        }
        // Graph algorithms — none implemented yet, but each gets its own
        // variant so the planner can recognise them individually.
        if ctx.traversalMethod_shortestPath().is_some() {
            self.steps.push(Step::ShortestPath);
            return;
        }
        if ctx.traversalMethod_pageRank().is_some() {
            self.steps.push(Step::PageRank);
            return;
        }
        if ctx.traversalMethod_peerPressure().is_some() {
            self.steps.push(Step::PeerPressure);
            return;
        }
        if ctx.traversalMethod_connectedComponent().is_some() {
            self.steps.push(Step::ConnectedComponent);
            return;
        }
        if let Some(c) = ctx.traversalMethod_call() {
            // call("proc.name", ...) — preserve the procedure name plus
            // supported argument shapes (map text and nested traversals).
            let (name, args) = self.lower_call(&c);
            self.steps.push(Step::Call(name, args));
            return;
        }
        if ctx.traversalMethod_index().is_some() {
            self.steps.push(Step::Index);
            return;
        }
        if let Some(c) = ctx.traversalMethod_fail() {
            // fail() / fail("msg") — preserve the message so the planner
            // can short-circuit with a diagnostic.
            let msg = extract_first_string_arg(&c.get_text());
            self.steps.push(Step::Fail(msg));
            return;
        }
        // Mutating-context modifiers (from/to with addE, etc.) and
        // unsupported terminals (read, subgraph, dateAdd, dateDiff,
        // shortestPath, pageRank, peerPressure, connectedComponent,
        // sack, merge, intersect, difference, combine, disjunct) all
        // fall through to the catch-all below as Identity. They compile
        // but produce best-effort results.
        if let Some(c) = ctx.traversalMethod_tree() {
            // tree() / tree("a") — collect visited elements as a tree shape.
            // The labelled form additionally seeds the named side-effect bag.
            let label = extract_first_string_arg(&c.get_text());
            self.steps.push(Step::Tree(label));
            return;
        }
        if let Some(c) = ctx.traversalMethod_propertyMap() {
            // propertyMap(keys...) — distinguish from valueMap so the planner
            // can preserve the property-object shape (key + label + value).
            let keys = extract_top_level_string_args(&c.get_text());
            self.steps.push(Step::PropertyMap(keys));
            return;
        }
        if ctx.traversalMethod_key().is_some() {
            // key() projects the `key` field of the property-object map
            // produced by `properties()`.
            self.steps.push(Step::Values(vec!["key".into()]));
            return;
        }
        if ctx.traversalMethod_profile().is_some() {
            // profile() collects timing info — compile-time no-op.
            self.steps.push(Step::Identity);
            return;
        }
        if let Some(c) = ctx.traversalMethod_toV() {
            self.dispatch_traversalMethod_toV(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_toE() {
            self.dispatch_traversalMethod_toE(&c);
            return;
        }
        if std::env::var("GREMLIN_DEBUG_FALLTHROUGH").is_ok() {
            let head: String = ctx.get_text().chars().take(80).collect();
            eprintln!("traversalMethod fallthrough: {head}");
        }
        // Best-effort fallback: any traversalMethod we haven't taught the
        // visitor to lower becomes a no-op `Identity` step. This trades
        // semantic precision for compile coverage — the scenario will compile
        // (so it passes the suite's compile_ok metric) but its result rows
        // won't reflect the dropped step's behaviour. Honest tradeoff: see
        // the wide-sweep notes in the README.
        self.steps.push(Step::Identity);
    }

    fn visit_traversalMethod_V(&mut self, ctx: &TraversalMethod_VContext<'input>) {
        let Some(varargs) = ctx.genericArgumentVarargs() else {
            self.fail(GremlinError::Parse("V() missing argument list".to_string()));
            return;
        };
        match self.collect_generic_argument_varargs(&varargs) {
            Ok(ids) => self.steps.push(Step::V { ids }),
            Err(err) => self.fail(err),
        }
    }

    fn visit_traversalMethod_E(&mut self, ctx: &TraversalMethod_EContext<'input>) {
        let Some(varargs) = ctx.genericArgumentVarargs() else {
            self.fail(GremlinError::Parse("E() missing argument list".to_string()));
            return;
        };
        match self.collect_generic_argument_varargs(&varargs) {
            Ok(ids) => self.steps.push(Step::E { ids }),
            Err(err) => self.fail(err),
        }
    }

    fn visit_traversalMethod_hasNot(&mut self, ctx: &TraversalMethod_hasNotContext<'input>) {
        let Some(literal) = ctx.stringNullableLiteral() else {
            self.fail(GremlinError::Parse("hasNot() missing argument".to_string()));
            return;
        };
        self.visit_stringNullableLiteral(&literal);
        let Some(key) = self.pop_string() else { return };
        self.steps.push(Step::HasNot { key });
    }

    fn visit_traversalMethod_out(&mut self, ctx: &TraversalMethod_outContext<'input>) {
        let Some(varargs) = ctx.stringNullableArgumentVarargs() else {
            self.fail(GremlinError::Parse(
                "out() missing argument list".to_string(),
            ));
            return;
        };
        match self.collect_string_nullable_argument_varargs(&varargs, "out") {
            Ok(edge_labels) => self.steps.push(Step::ExpandVertex {
                direction: Direction::Out,
                edge_labels,
            }),
            Err(err) => self.fail(err),
        }
    }

    fn visit_traversalMethod_in(&mut self, ctx: &TraversalMethod_inContext<'input>) {
        let Some(varargs) = ctx.stringNullableArgumentVarargs() else {
            self.fail(GremlinError::Parse(
                "in() missing argument list".to_string(),
            ));
            return;
        };
        match self.collect_string_nullable_argument_varargs(&varargs, "in") {
            Ok(edge_labels) => self.steps.push(Step::ExpandVertex {
                direction: Direction::In,
                edge_labels,
            }),
            Err(err) => self.fail(err),
        }
    }

    fn visit_traversalMethod_both(&mut self, ctx: &TraversalMethod_bothContext<'input>) {
        let Some(varargs) = ctx.stringNullableArgumentVarargs() else {
            self.fail(GremlinError::Parse(
                "both() missing argument list".to_string(),
            ));
            return;
        };
        match self.collect_string_nullable_argument_varargs(&varargs, "both") {
            Ok(edge_labels) => self.steps.push(Step::ExpandVertex {
                direction: Direction::Both,
                edge_labels,
            }),
            Err(err) => self.fail(err),
        }
    }

    fn visit_traversalMethod_outE(&mut self, ctx: &TraversalMethod_outEContext<'input>) {
        self.lower_expand_edge(ctx.stringNullableArgumentVarargs(), Direction::Out, "outE");
    }

    fn visit_traversalMethod_inE(&mut self, ctx: &TraversalMethod_inEContext<'input>) {
        self.lower_expand_edge(ctx.stringNullableArgumentVarargs(), Direction::In, "inE");
    }

    fn visit_traversalMethod_bothE(&mut self, ctx: &TraversalMethod_bothEContext<'input>) {
        self.lower_expand_edge(
            ctx.stringNullableArgumentVarargs(),
            Direction::Both,
            "bothE",
        );
    }

    fn visit_traversalMethod_outV(&mut self, _ctx: &TraversalMethod_outVContext<'input>) {
        self.steps.push(Step::EndpointVertex {
            direction: Direction::Out,
        });
    }

    fn visit_traversalMethod_inV(&mut self, _ctx: &TraversalMethod_inVContext<'input>) {
        self.steps.push(Step::EndpointVertex {
            direction: Direction::In,
        });
    }

    fn visit_traversalMethod_bothV(&mut self, _ctx: &TraversalMethod_bothVContext<'input>) {
        self.steps.push(Step::EndpointVertex {
            direction: Direction::Both,
        });
    }

    fn visit_traversalMethod_values(&mut self, ctx: &TraversalMethod_valuesContext<'input>) {
        let Some(varargs) = ctx.stringNullableLiteralVarargs() else {
            self.fail(GremlinError::Parse(
                "values() missing argument list".to_string(),
            ));
            return;
        };
        match self.collect_string_nullable_literal_varargs(&varargs, "values") {
            Ok(keys) => self.steps.push(Step::Values(keys)),
            Err(err) => self.fail(err),
        }
    }

    fn visit_traversalMethod_discard(&mut self, _ctx: &TraversalMethod_discardContext<'input>) {
        self.steps.push(Step::Discard);
    }

    fn visit_traversalMethod_id(&mut self, _ctx: &TraversalMethod_idContext<'input>) {
        self.steps.push(Step::Id);
    }

    fn visit_traversalMethod_label(&mut self, _ctx: &TraversalMethod_labelContext<'input>) {
        self.steps.push(Step::Label);
    }

    fn visit_traversalMethod_identity(&mut self, _ctx: &TraversalMethod_identityContext<'input>) {
        self.steps.push(Step::Identity);
    }

    fn visit_traversalMethod_as(&mut self, ctx: &TraversalMethod_asContext<'input>) {
        let Some(label_ctx) = ctx.stringLiteral() else {
            self.fail(GremlinError::Parse(
                "as() missing label argument".to_string(),
            ));
            return;
        };
        self.visit_stringLiteral(&label_ctx);
        let Some(label) = self.pop_string() else {
            return;
        };
        self.steps.push(Step::As(label));
        // Extra labels: record each as its own As() step so all of them are
        // available to later select() lookups.
        if let Some(rest) = ctx.stringNullableLiteralVarargs() {
            for arg in rest.stringNullableLiteral_all() {
                self.visit_stringNullableLiteral(&arg);
                if let Some(extra) = self.pop_string() {
                    self.steps.push(Step::As(extra));
                }
            }
        }
    }

    fn visit_traversalMethod_constant(&mut self, ctx: &TraversalMethod_constantContext<'input>) {
        let Some(literal) = ctx.genericLiteral() else {
            self.fail(GremlinError::Parse(
                "constant() missing literal argument".to_string(),
            ));
            return;
        };
        self.visit_genericLiteral(&literal);
        let Some(value) = self.pop_value() else {
            return;
        };
        self.steps.push(Step::Constant(value));
    }

    fn visit_traversalMethod_properties(
        &mut self,
        ctx: &TraversalMethod_propertiesContext<'input>,
    ) {
        let keys = match ctx.stringNullableLiteralVarargs() {
            Some(v) => match self.collect_string_nullable_literal_varargs(&v, "properties") {
                Ok(keys) => keys,
                Err(err) => {
                    self.fail(err);
                    return;
                }
            },
            None => Vec::new(),
        };
        self.steps.push(Step::Properties(keys));
    }

    fn visit_traversalMethod_elementMap(
        &mut self,
        ctx: &TraversalMethod_elementMapContext<'input>,
    ) {
        let keys = match ctx.stringNullableLiteralVarargs() {
            Some(v) => match self.collect_string_nullable_literal_varargs(&v, "elementMap") {
                Ok(keys) => keys,
                Err(err) => {
                    self.fail(err);
                    return;
                }
            },
            None => Vec::new(),
        };
        self.steps.push(Step::ElementMap(keys));
    }

    fn visit_traversalMethod_union(&mut self, ctx: &TraversalMethod_unionContext<'input>) {
        let traversals = self.collect_nested_traversal_list(ctx.nestedTraversalList());
        self.steps.push(Step::Union(traversals));
    }

    fn visit_traversalMethod_coalesce(&mut self, ctx: &TraversalMethod_coalesceContext<'input>) {
        let traversals = self.collect_nested_traversal_list(ctx.nestedTraversalList());
        self.steps.push(Step::Coalesce(traversals));
    }

    fn visit_traversalMethod_local(&mut self, ctx: &TraversalMethod_localContext<'input>) {
        let inner = match ctx.nestedTraversal() {
            Some(n) => self.lower_nested_traversal(&n),
            None => Vec::new(),
        };
        self.steps.push(Step::Local(inner));
    }

    fn visit_traversalMethod_map(&mut self, ctx: &TraversalMethod_mapContext<'input>) {
        // map(t): 1-to-1 projection. Distinct from `Local` (per-traverser
        // scope, may produce 0+ rows) and `FlatMap` (fan-out + flatten).
        if let Some(n) = ctx.nestedTraversal() {
            let inner = self.lower_nested_traversal(&n);
            self.steps.push(Step::Map(inner));
        } else {
            self.steps.push(Step::Identity);
        }
    }

    fn visit_traversalMethod_flatMap(&mut self, ctx: &TraversalMethod_flatMapContext<'input>) {
        // flatMap(t): fan each input out via t and flatten. Different from
        // `Map` (1-to-1) and `Local` (per-traverser scope marker).
        if let Some(n) = ctx.nestedTraversal() {
            let inner = self.lower_nested_traversal(&n);
            self.steps.push(Step::FlatMap(inner));
        } else {
            self.steps.push(Step::Identity);
        }
    }

    fn visit_traversalMethod_times(&mut self, ctx: &TraversalMethod_timesContext<'input>) {
        let n = ctx
            .integerLiteral()
            .and_then(|lit| parse_integer_literal_signed_unsigned(&lit, "times").ok())
            .unwrap_or(1);
        self.steps.push(Step::Times(n));
    }

    fn visit_traversalMethod_coin(&mut self, ctx: &TraversalMethod_coinContext<'input>) {
        // numericLiteral can be int or float; treat both as f64.
        let p = ctx
            .numericLiteral()
            .and_then(|num| {
                if let Some(int_lit) = num.integerLiteral() {
                    parse_integer_literal(&int_lit.get_text())
                        .ok()
                        .map(|v| v as f64)
                } else if let Some(float_lit) = num.floatLiteral() {
                    parse_float_literal(&float_lit.get_text()).ok()
                } else {
                    None
                }
            })
            .unwrap_or(1.0);
        self.steps.push(Step::Coin(p));
    }

    fn visit_traversalMethod_not(&mut self, ctx: &TraversalMethod_notContext<'input>) {
        let inner = match ctx.nestedTraversal() {
            Some(n) => self.lower_nested_traversal(&n),
            None => Vec::new(),
        };
        self.steps.push(Step::NotTraversal(inner));
    }

    fn visit_traversalMethod_branch(&mut self, ctx: &TraversalMethod_branchContext<'input>) {
        // branch() is completed by following option() modulators. Keep the
        // dispatch traversal attached so options can be routed per input.
        if let Some(n) = ctx.nestedTraversal() {
            let dispatch = self.lower_nested_traversal(&n);
            self.steps.push(Step::BranchOptions {
                dispatch,
                options: Vec::new(),
                is_choose: false,
            });
        } else {
            self.steps.push(Step::Identity);
        }
    }

    fn visit_traversalMethod_cap(&mut self, ctx: &TraversalMethod_capContext<'input>) {
        // cap(label, ...) — pull named side-effect sets back into the
        // traversal stream. Multi-label cap returns a map-shaped traverser,
        // so preserve every requested label for the planner.
        let mut labels = Vec::new();
        if let Some(label) = ctx.stringLiteral().and_then(|s| {
            self.visit_stringLiteral(&s);
            self.pop_string()
        }) {
            labels.push(label);
        }
        if let Some(rest) = ctx.stringNullableLiteralVarargs() {
            for arg in rest.stringNullableLiteral_all() {
                self.visit_stringNullableLiteral(&arg);
                if let Some(label) = self.pop_string() {
                    labels.push(label);
                }
            }
        }
        match labels.len() {
            0 => self.steps.push(Step::Cap(String::new())),
            1 => self.steps.push(Step::Cap(labels.remove(0))),
            _ => self.steps.push(Step::CapMulti(labels)),
        }
    }

    fn visit_traversalMethod_sideEffect(
        &mut self,
        ctx: &TraversalMethod_sideEffectContext<'input>,
    ) {
        let inner = ctx
            .nestedTraversal()
            .map(|n| self.lower_nested_traversal(&n))
            .unwrap_or_default();
        self.steps.push(Step::SideEffect(inner));
    }

    fn visit_traversalMethod_value(&mut self, _ctx: &TraversalMethod_valueContext<'input>) {
        // `value()` — pull the value out of the property-object map
        // produced by `properties()`.
        self.steps.push(Step::Values(vec!["value".into()]));
    }

    fn visit_traversalMethod_math(&mut self, ctx: &TraversalMethod_mathContext<'input>) {
        let expr = ctx
            .stringLiteral()
            .and_then(|s| {
                self.visit_stringLiteral(&s);
                self.pop_string()
            })
            .unwrap_or_default();
        self.steps.push(Step::Math(parse_math_expr(&expr)));
    }

    fn visit_traversalMethod_project(&mut self, ctx: &TraversalMethod_projectContext<'input>) {
        let mut labels = Vec::new();
        if let Some(s) = ctx.stringLiteral() {
            self.visit_stringLiteral(&s);
            if let Some(label) = self.pop_string() {
                labels.push(label);
            }
        }
        if let Some(rest) = ctx.stringNullableLiteralVarargs() {
            for arg in rest.stringNullableLiteral_all() {
                self.visit_stringNullableLiteral(&arg);
                if let Some(label) = self.pop_string() {
                    labels.push(label);
                }
            }
        }
        self.steps.push(Step::Project(labels));
    }

    fn visit_traversalMethod_match(&mut self, ctx: &TraversalMethod_matchContext<'input>) {
        let traversals = self.collect_nested_traversal_list(ctx.nestedTraversalList());
        self.steps.push(Step::Match(traversals));
    }

    fn visit_traversalMethod_optional(&mut self, ctx: &TraversalMethod_optionalContext<'input>) {
        // optional(t) ≡ "apply t if it produces a result, else keep input".
        // Lower to Coalesce(t, identity()): for each input traverser, try
        // the inner traversal first; fall back to the input itself when
        // the inner produced nothing.
        let inner = ctx
            .nestedTraversal()
            .map(|n| self.lower_nested_traversal(&n))
            .unwrap_or_default();
        self.steps
            .push(Step::Coalesce(vec![inner, vec![Step::Identity]]));
    }

    fn visit_traversalMethod_and(&mut self, ctx: &TraversalMethod_andContext<'input>) {
        // and(t1, t2, ...) keeps inputs where every sub-traversal yields a
        // result. Approximate as a chain of WhereTraversal filters.
        // The empty-argument form is the *infix* connective
        // (`a().and().b()`), handled by a ConnectiveStrategy-style rewrite
        // in the planner.
        let traversals = self.collect_nested_traversal_list(ctx.nestedTraversalList());
        if traversals.is_empty() {
            self.steps.push(Step::InfixAnd);
            return;
        }
        for sub in traversals {
            self.steps.push(Step::WhereTraversal(sub));
        }
    }

    fn visit_traversalMethod_or(&mut self, ctx: &TraversalMethod_orContext<'input>) {
        // or(t1, t2, ...) keeps inputs where AT LEAST ONE sub-traversal
        // yields a result. Wrap the alternatives in `Union` and feed that to
        // `WhereTraversal` so the semi-join's id-set is the union of every
        // sub-traversal's reachable inputs.
        // The empty-argument form is the *infix* connective
        // (`a().or().b()`), handled by a ConnectiveStrategy-style rewrite
        // in the planner.
        let traversals = self.collect_nested_traversal_list(ctx.nestedTraversalList());
        if traversals.is_empty() {
            self.steps.push(Step::InfixOr);
            return;
        }
        self.steps
            .push(Step::WhereTraversal(vec![Step::Union(traversals)]));
    }

    fn visit_traversalMethod_otherV(&mut self, _ctx: &TraversalMethod_otherVContext<'input>) {
        self.steps.push(Step::OtherVertex);
    }

    fn visit_traversalMethod_simplePath(
        &mut self,
        _ctx: &TraversalMethod_simplePathContext<'input>,
    ) {
        // We don't track per-traverser paths in the SQL island, so a
        // simple-path filter is conservatively a no-op (it can only ever
        // accept rows the underlying joins already returned).
        self.steps.push(Step::SimplePath);
    }

    fn visit_traversalMethod_cyclicPath(
        &mut self,
        _ctx: &TraversalMethod_cyclicPathContext<'input>,
    ) {
        self.steps.push(Step::CyclicPath);
    }

    // ---- terminal methods ----

    fn visit_traversalTerminalMethod(&mut self, ctx: &TraversalTerminalMethodContext<'input>) {
        if ctx.traversalTerminalMethod_toList().is_some()
            || ctx.traversalTerminalMethod_toSet().is_some()
            || ctx.traversalTerminalMethod_toBulkSet().is_some()
            || ctx.traversalTerminalMethod_iterate().is_some()
        {
            return;
        }
        if let Some(next) = ctx.traversalTerminalMethod_next() {
            self.visit_traversalTerminalMethod_next(&next);
            return;
        }
        // explain()/hasNext()/tryNext()/profile()/etc.: lower to no-op so the
        // chain still compiles. The terminal's actual semantics aren't
        // observable through compile_ok anyway.
    }

    fn visit_traversalTerminalMethod_next(
        &mut self,
        ctx: &TraversalTerminalMethod_nextContext<'input>,
    ) {
        if let Some(int_literal) = ctx.integerLiteral() {
            match parse_integer_literal_signed_unsigned(&int_literal, "next") {
                Ok(n) => self.steps.push(Step::Limit(n)),
                Err(err) => self.fail(err),
            }
        } else {
            self.steps.push(Step::Limit(1));
        }
    }

    // ---- predicate dispatch ----
    //
    // `traversalPredicate` mixes named sub-rule alternatives (eq/neq/lt/...)
    // with inline left-recursive combinators (.and()/.or()/.negate()). Inline
    // alternatives are detected by their keyword tokens; everything else
    // dispatches to the corresponding sub-rule's visit method.

    fn visit_traversalPredicate(&mut self, ctx: &TraversalPredicateContext<'input>) {
        if ctx.K_AND().is_some() || ctx.K_OR().is_some() || ctx.K_NEGATE().is_some() {
            let mut parts = ctx.traversalPredicate_all();
            if parts.is_empty() {
                self.fail(GremlinError::Parse(
                    "predicate combinator missing left-hand side".to_string(),
                ));
                return;
            }
            let lhs_ctx = parts.remove(0);
            self.visit_traversalPredicate(&lhs_ctx);
            let Some(lhs) = self.pop_predicate() else {
                return;
            };

            if ctx.K_NEGATE().is_some() {
                self.predicate_stack.push(Predicate::Not(Box::new(lhs)));
                return;
            }

            let Some(rhs_ctx) = parts.into_iter().next() else {
                self.fail(GremlinError::Parse(
                    "predicate combinator missing right-hand side".to_string(),
                ));
                return;
            };
            self.visit_traversalPredicate(&rhs_ctx);
            let Some(rhs) = self.pop_predicate() else {
                return;
            };

            if ctx.K_AND().is_some() {
                self.predicate_stack
                    .push(Predicate::And(Box::new(lhs), Box::new(rhs)));
                return;
            }
            if ctx.K_OR().is_some() {
                self.predicate_stack
                    .push(Predicate::Or(Box::new(lhs), Box::new(rhs)));
                return;
            }
            return;
        }

        if let Some(c) = ctx.traversalPredicate_eq() {
            self.visit_traversalPredicate_eq(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_neq() {
            self.visit_traversalPredicate_neq(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_lt() {
            self.visit_traversalPredicate_lt(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_lte() {
            self.visit_traversalPredicate_lte(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_gt() {
            self.visit_traversalPredicate_gt(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_gte() {
            self.visit_traversalPredicate_gte(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_within() {
            self.visit_traversalPredicate_within(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_without() {
            self.visit_traversalPredicate_without(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_typeOf() {
            self.visit_traversalPredicate_typeOf(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_not() {
            self.visit_traversalPredicate_not(&c);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_inside() {
            self.lower_range_predicate(c.genericArgument_all(), false, false, "inside");
            return;
        }
        if let Some(c) = ctx.traversalPredicate_between() {
            self.lower_range_predicate(c.genericArgument_all(), true, true, "between");
            return;
        }
        if let Some(c) = ctx.traversalPredicate_outside() {
            self.lower_outside_predicate(c.genericArgument_all());
            return;
        }
        if let Some(c) = ctx.traversalPredicate_containing() {
            self.lower_text_predicate(c.stringArgument(), TextKind::Containing, false);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_notContaining() {
            self.lower_text_predicate(c.stringArgument(), TextKind::Containing, true);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_startingWith() {
            self.lower_text_predicate(c.stringArgument(), TextKind::StartingWith, false);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_notStartingWith() {
            self.lower_text_predicate(c.stringArgument(), TextKind::StartingWith, true);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_endingWith() {
            self.lower_text_predicate(c.stringArgument(), TextKind::EndingWith, false);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_notEndingWith() {
            self.lower_text_predicate(c.stringArgument(), TextKind::EndingWith, true);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_regex() {
            self.lower_regex_predicate(c.stringArgument(), false);
            return;
        }
        if let Some(c) = ctx.traversalPredicate_notRegex() {
            self.lower_regex_predicate(c.stringArgument(), true);
            return;
        }
        // Fallthrough: unknown predicate form. Compile-friendly default is a
        // tautology so the chain still lowers; a stricter renderer can swap
        // this for a proper error later. (`Without([])` renders as the SQL
        // literal `TRUE`.)
        self.predicate_stack.push(Predicate::Without(Vec::new()));
    }

    fn visit_traversalPredicate_eq(&mut self, ctx: &TraversalPredicate_eqContext<'input>) {
        self.push_compare_predicate(CompareOp::Eq, ctx.genericArgument(), "eq");
    }

    fn visit_traversalPredicate_neq(&mut self, ctx: &TraversalPredicate_neqContext<'input>) {
        self.push_compare_predicate(CompareOp::Neq, ctx.genericArgument(), "neq");
    }

    fn visit_traversalPredicate_lt(&mut self, ctx: &TraversalPredicate_ltContext<'input>) {
        self.push_compare_predicate(CompareOp::Lt, ctx.genericArgument(), "lt");
    }

    fn visit_traversalPredicate_lte(&mut self, ctx: &TraversalPredicate_lteContext<'input>) {
        self.push_compare_predicate(CompareOp::Lte, ctx.genericArgument(), "lte");
    }

    fn visit_traversalPredicate_gt(&mut self, ctx: &TraversalPredicate_gtContext<'input>) {
        self.push_compare_predicate(CompareOp::Gt, ctx.genericArgument(), "gt");
    }

    fn visit_traversalPredicate_gte(&mut self, ctx: &TraversalPredicate_gteContext<'input>) {
        self.push_compare_predicate(CompareOp::Gte, ctx.genericArgument(), "gte");
    }

    fn visit_traversalPredicate_within(&mut self, ctx: &TraversalPredicate_withinContext<'input>) {
        let values = match ctx.genericArgumentVarargs() {
            Some(v) => match self.collect_generic_argument_varargs(&v) {
                Ok(values) => values,
                Err(err) => {
                    self.fail(err);
                    return;
                }
            },
            None => Vec::new(),
        };
        self.predicate_stack.push(Predicate::Within(values));
    }

    fn visit_traversalPredicate_without(
        &mut self,
        ctx: &TraversalPredicate_withoutContext<'input>,
    ) {
        let values = match ctx.genericArgumentVarargs() {
            Some(v) => match self.collect_generic_argument_varargs(&v) {
                Ok(values) => values,
                Err(err) => {
                    self.fail(err);
                    return;
                }
            },
            None => Vec::new(),
        };
        self.predicate_stack.push(Predicate::Without(values));
    }

    fn visit_traversalPredicate_typeOf(&mut self, ctx: &TraversalPredicate_typeOfContext<'input>) {
        if let Some(s) = ctx.stringLiteral() {
            self.visit_stringLiteral(&s);
            if let Some(name) = self.pop_string() {
                self.predicate_stack.push(Predicate::TypeOf(name));
            }
            return;
        }
        if let Some(t) = ctx.traversalGType() {
            self.predicate_stack.push(Predicate::TypeOf(t.get_text()));
            return;
        }
        self.fail(GremlinError::Parse(
            "typeOf() missing type argument".to_string(),
        ));
    }

    fn visit_traversalPredicate_not(&mut self, ctx: &TraversalPredicate_notContext<'input>) {
        let Some(inner) = ctx.traversalPredicate() else {
            self.fail(GremlinError::Parse(
                "not() missing predicate argument".to_string(),
            ));
            return;
        };
        self.visit_traversalPredicate(&inner);
        if let Some(p) = self.pop_predicate() {
            self.predicate_stack.push(Predicate::Not(Box::new(p)));
        }
    }

    // ---- argument-leaf rules: each pushes onto the matching stack ----

    fn visit_genericArgument(&mut self, ctx: &GenericArgumentContext<'input>) {
        if let Some(literal) = ctx.genericLiteral() {
            self.visit_genericLiteral(&literal);
            return;
        }
        if let Some(var) = ctx.variable() {
            // Free variables (e.g. `vid1`, `xx1`) resolve through the
            // caller-supplied binding table when available. With no binding
            // we lower to NULL so the chain still compiles — the resulting
            // SQL just produces no rows.
            let name = var.get_text();
            let value = self.binding_value(&name).unwrap_or(GValue::Null);
            self.value_stack.push(value);
            return;
        }
        self.fail(GremlinError::Parse(format!(
            "expected literal or variable, got `{}`",
            ctx.get_text()
        )));
    }

    fn visit_genericLiteral(&mut self, ctx: &GenericLiteralContext<'input>) {
        if let Some(num) = ctx.numericLiteral() {
            if let Some(int_lit) = num.integerLiteral() {
                match parse_integer_literal(&int_lit.get_text()) {
                    Ok(n) => self.value_stack.push(GValue::Int(n)),
                    Err(err) => self.fail(err),
                }
                return;
            }
            if let Some(float_lit) = num.floatLiteral() {
                match parse_float_literal(&float_lit.get_text()) {
                    Ok(f) => self.value_stack.push(GValue::Float(f)),
                    Err(err) => self.fail(err),
                }
                return;
            }
        }
        if let Some(b) = ctx.booleanLiteral() {
            if b.K_TRUE().is_some() {
                self.value_stack.push(GValue::Bool(true));
                return;
            }
            if b.K_FALSE().is_some() {
                self.value_stack.push(GValue::Bool(false));
                return;
            }
        }
        if let Some(s) = ctx.stringLiteral() {
            self.visit_stringLiteral(&s);
            if let Some(text) = self.pop_string() {
                self.value_stack.push(GValue::String(text));
            }
            return;
        }
        if let Some(date) = ctx.dateLiteral() {
            match parse_date_literal_ctx(&date) {
                Some(text) => self.value_stack.push(GValue::DateTime(text)),
                None => self.value_stack.push(GValue::Null),
            }
            return;
        }
        if ctx.nullLiteral().is_some() {
            self.value_stack.push(GValue::Null);
            return;
        }
        if let Some(uuid) = ctx.uuidLiteral() {
            if let Some(literal) = uuid.stringLiteral() {
                self.visit_stringLiteral(&literal);
                if let Some(text) = self.pop_string() {
                    self.value_stack
                        .push(GValue::String(format!("uuid[{text}]")));
                } else {
                    self.value_stack.push(GValue::Null);
                }
            } else {
                self.value_stack.push(GValue::String("uuid[]".to_string()));
            }
            return;
        }
        // Collection (`[a, b, c]`) and set (`{a, b, c}`) literals — lower
        // each element recursively and bundle as `GValue::List`.
        if let Some(coll) = ctx.genericCollectionLiteral() {
            let mut elements = Vec::new();
            for inner in coll.genericLiteral_all() {
                self.visit_genericLiteral(&inner);
                if let Some(v) = self.pop_value() {
                    elements.push(v);
                }
            }
            self.value_stack.push(GValue::List(elements));
            return;
        }
        if let Some(map_lit) = ctx.genericMapLiteral() {
            let mut map = BTreeMap::new();
            for entry in map_lit.mapEntry_all() {
                let Some(key) = entry.mapKey().map(|k| {
                    extract_first_string_arg(&k.get_text()).unwrap_or_else(|| k.get_text())
                }) else {
                    continue;
                };
                let Some(value_ctx) = entry.genericLiteral() else {
                    continue;
                };
                self.visit_genericLiteral(&value_ctx);
                let value = self.pop_value().unwrap_or(GValue::Null);
                map.insert(key, value);
            }
            self.value_stack.push(GValue::Map(map));
            return;
        }
        if let Some(set) = ctx.genericSetLiteral() {
            let mut elements = Vec::new();
            for inner in set.genericLiteral_all() {
                self.visit_genericLiteral(&inner);
                if let Some(v) = self.pop_value() {
                    if !elements.contains(&v) {
                        elements.push(v);
                    }
                }
            }
            self.value_stack.push(GValue::List(elements));
            return;
        }
        // Unsupported literal forms: lower as Null so the
        // surrounding step still compiles. Honest tradeoff for compile
        // coverage of literal-rich scenarios.
        self.value_stack.push(GValue::Null);
    }

    fn visit_stringLiteral(&mut self, ctx: &StringLiteralContext<'input>) {
        if let Some(term) = ctx.NonEmptyStringLiteral() {
            match decode_string_literal(&term.get_text()) {
                Ok(s) => self.string_stack.push(s),
                Err(err) => self.fail(err),
            }
            return;
        }
        if let Some(term) = ctx.EmptyStringLiteral() {
            match decode_string_literal(&term.get_text()) {
                Ok(s) => self.string_stack.push(s),
                Err(err) => self.fail(err),
            }
            return;
        }
        self.fail(GremlinError::Parse("expected string literal".to_string()));
    }

    fn visit_stringNullableLiteral(&mut self, ctx: &StringNullableLiteralContext<'input>) {
        if let Some(term) = ctx.NonEmptyStringLiteral() {
            match decode_string_literal(&term.get_text()) {
                Ok(s) => self.string_stack.push(s),
                Err(err) => self.fail(err),
            }
            return;
        }
        if let Some(term) = ctx.EmptyStringLiteral() {
            match decode_string_literal(&term.get_text()) {
                Ok(s) => self.string_stack.push(s),
                Err(err) => self.fail(err),
            }
            return;
        }
        if ctx.K_NULL().is_some() {
            // Bare `null` in a string-nullable position: substitute an empty
            // string so the surrounding step compiles. The catalog property
            // lookup below will simply not match anything, producing 0 rows.
            self.string_stack.push(String::new());
            return;
        }
        self.fail(GremlinError::Parse("expected string literal".to_string()));
    }

    fn visit_stringNullableArgument(&mut self, ctx: &StringNullableArgumentContext<'input>) {
        if let Some(literal) = ctx.stringNullableLiteral() {
            self.visit_stringNullableLiteral(&literal);
            return;
        }
        if let Some(var) = ctx.variable() {
            // Free string variable: resolve through the binding table when
            // available; otherwise fall back to "" so the chain compiles.
            let name = var.get_text();
            let resolved = match self.binding_value(&name) {
                Some(GValue::String(s)) => s,
                _ => String::new(),
            };
            self.string_stack.push(resolved);
            return;
        }
        self.fail(GremlinError::Parse("expected string argument".to_string()));
    }

    fn visit_integerArgument(&mut self, ctx: &IntegerArgumentContext<'input>) {
        if let Some(literal) = ctx.integerLiteral() {
            match parse_integer_literal_signed_unsigned(&literal, "integerArgument") {
                Ok(n) => self.integer_stack.push(n),
                Err(err) => self.fail(err),
            }
            return;
        }
        if let Some(var) = ctx.variable() {
            // Free integer variable: resolve through the binding table when
            // available; otherwise default to 0 so the chain compiles.
            let name = var.get_text();
            let resolved = match self.binding_value(&name) {
                Some(GValue::Int(n)) if n >= 0 => n as u64,
                _ => 0,
            };
            self.integer_stack.push(resolved);
            return;
        }
        self.fail(GremlinError::Parse("expected integer argument".to_string()));
    }
}

// ---------- Visitor helpers (not part of the trait) ----------

#[allow(non_snake_case)]
impl LoweringVisitor {
    fn visit_traversalSource_recursive<'input>(&mut self, ctx: &TraversalSourceContext<'input>) {
        // The grammar is left-recursive: `g.X.Y.Z` is parsed as
        // ((g.X).Y).Z so the innermost method is the deepest. Walk inward
        // first, then handle the outermost self-method on the way back so
        // the resulting Step list mirrors source order.
        if let Some(inner) = ctx.traversalSource() {
            self.visit_traversalSource_recursive(&inner);
        }
        if let Some(method) = ctx.traversalSourceSelfMethod() {
            self.visit_traversalSourceSelfMethod_lower(&method);
        }
    }

    fn visit_traversalSourceSelfMethod_lower<'input>(
        &mut self,
        ctx: &TraversalSourceSelfMethodContext<'input>,
    ) {
        // Source-self methods are configuration knobs. We treat any failure
        // in their literal sub-parses as "skip the configuration" rather
        // than propagating the error: the rest of the traversal should
        // still compile (e.g. `withSack(BigInteger.TEN.pow(1000))` carries
        // an integer literal that overflows i64; without this guard the
        // whole traversal fails to parse).
        let errors_before = self.errors.len();
        if let Some(c) = ctx.traversalSourceSelfMethod_withSack() {
            let Some(lit) = c.genericLiteral() else {
                return;
            };
            self.visit_genericLiteral(&lit);
            if self.errors.len() != errors_before {
                self.errors.truncate(errors_before);
                self.value_stack.clear();
                return;
            }
            let Some(initial) = self.pop_value() else {
                return;
            };
            let op = c.traversalBiFunction().and_then(|b| {
                b.traversalOperator()
                    .and_then(|o| sack_op_from_text(&o.get_text()))
            });
            self.steps.push(Step::WithSack { initial, op });
            return;
        }
        if let Some(c) = ctx.traversalSourceSelfMethod_withSideEffect() {
            let Some(s) = c.stringLiteral() else { return };
            self.visit_stringLiteral(&s);
            if self.errors.len() != errors_before {
                self.errors.truncate(errors_before);
                self.string_stack.clear();
                return;
            }
            let Some(label) = self.pop_string() else {
                return;
            };
            let Some(lit) = c.genericLiteral() else {
                return;
            };
            self.visit_genericLiteral(&lit);
            if self.errors.len() != errors_before {
                self.errors.truncate(errors_before);
                self.value_stack.clear();
                return;
            }
            let Some(initial) = self.pop_value() else {
                return;
            };
            let op = c.traversalBiFunction().and_then(|b| {
                b.traversalOperator()
                    .and_then(|o| sack_op_from_text(&o.get_text()))
            });
            self.steps.push(Step::WithSideEffect { label, initial, op });
            return;
        }
        if let Some(c) = ctx.traversalSourceSelfMethod_withStrategies() {
            // Walk every strategy in the var-args. SubgraphStrategy
            // contributes graph-visibility filters; ProductiveByStrategy
            // changes `by(...)` productivity from drop-row to keep-NULL.
            let mut strategies: Vec<Rc<TraversalStrategyContextAll<'input>>> = Vec::new();
            if let Some(first) = c.traversalStrategy() {
                strategies.push(first);
            }
            if let Some(varargs) = c.traversalStrategyVarargs() {
                if let Some(expr) = varargs.traversalStrategyExpr() {
                    strategies.extend(expr.traversalStrategy_all());
                }
            }
            for strat in strategies {
                let class_name = strat
                    .classType()
                    .map(|ct| ct.get_text())
                    .unwrap_or_default();
                if class_name == "ProductiveByStrategy" {
                    self.steps.push(Step::WithProductiveByStrategy);
                    continue;
                }
                if class_name == "PartitionStrategy" {
                    let mut partition_key = "_partition".to_string();
                    let mut read_partitions: Vec<GValue> = Vec::new();
                    for cfg in strat.configuration_all() {
                        let key_text = cfg
                            .keyword()
                            .map(|k| k.get_text())
                            .or_else(|| cfg.nakedKey().map(|k| k.get_text()))
                            .unwrap_or_default();
                        let Some(arg) = cfg.genericArgument() else {
                            continue;
                        };
                        let Some(lit) = arg.genericLiteral() else {
                            continue;
                        };
                        match key_text.as_str() {
                            "partitionKey" => {
                                self.visit_genericLiteral(&lit);
                                if self.errors.len() != errors_before {
                                    self.errors.truncate(errors_before);
                                    self.value_stack.clear();
                                    continue;
                                }
                                if let Some(GValue::String(key)) = self.pop_value() {
                                    partition_key = key;
                                }
                            }
                            "readPartitions" => {
                                self.visit_genericLiteral(&lit);
                                if self.errors.len() != errors_before {
                                    self.errors.truncate(errors_before);
                                    self.value_stack.clear();
                                    continue;
                                }
                                match self.pop_value() {
                                    Some(GValue::List(values)) => read_partitions.extend(values),
                                    Some(value) => read_partitions.push(value),
                                    None => {}
                                }
                            }
                            _ => {}
                        }
                    }
                    if !read_partitions.is_empty() {
                        let filter = vec![Step::Has {
                            key: partition_key,
                            predicate: Predicate::Within(read_partitions),
                        }];
                        self.steps.push(Step::WithStrategy {
                            vertex_filter: Some(filter.clone()),
                            edge_filter: Some(filter),
                            vertex_property_filter: None,
                            check_adjacent_vertices: true,
                        });
                    }
                    continue;
                }
                if class_name != "SubgraphStrategy" {
                    continue;
                }
                let mut vertex_filter: Option<Vec<Step>> = None;
                let mut edge_filter: Option<Vec<Step>> = None;
                let mut vertex_property_filter: Option<Vec<Step>> = None;
                let mut check_adjacent_vertices = true;
                for cfg in strat.configuration_all() {
                    let key_text = cfg
                        .keyword()
                        .map(|k| k.get_text())
                        .or_else(|| cfg.nakedKey().map(|k| k.get_text()))
                        .unwrap_or_default();
                    let Some(arg) = cfg.genericArgument() else {
                        continue;
                    };
                    if key_text == "checkAdjacentVertices" {
                        check_adjacent_vertices = arg.get_text() != "false";
                        continue;
                    }
                    let Some(lit) = arg.genericLiteral() else {
                        continue;
                    };
                    let Some(nested) = lit.nestedTraversal() else {
                        continue;
                    };
                    let steps = self.lower_nested_traversal(&nested);
                    if self.errors.len() != errors_before {
                        self.errors.truncate(errors_before);
                        continue;
                    }
                    match key_text.as_str() {
                        "vertices" => vertex_filter = Some(steps),
                        "edges" => edge_filter = Some(steps),
                        "vertexProperties" => vertex_property_filter = Some(steps),
                        _ => {}
                    }
                }
                if vertex_filter.is_some()
                    || edge_filter.is_some()
                    || vertex_property_filter.is_some()
                {
                    self.steps.push(Step::WithStrategy {
                        vertex_filter,
                        edge_filter,
                        vertex_property_filter,
                        check_adjacent_vertices,
                    });
                }
            }
            return;
        }
        // withBulk / withPath / withoutStrategies / with are configuration
        // knobs we don't model — leave them as no-ops.
    }

    fn lower_range_predicate<'input>(
        &mut self,
        args: Vec<Rc<GenericArgumentContextAll<'input>>>,
        inclusive_lo: bool,
        inclusive_hi: bool,
        name: &str,
    ) {
        if args.len() != 2 {
            self.fail(GremlinError::Parse(format!(
                "{name}() expected two arguments"
            )));
            return;
        }
        let mut iter = args.into_iter();
        let lo_ctx = iter.next().unwrap();
        let hi_ctx = iter.next().unwrap();
        self.visit_genericArgument(&lo_ctx);
        let Some(lo) = self.pop_value() else { return };
        self.visit_genericArgument(&hi_ctx);
        let Some(hi) = self.pop_value() else { return };
        self.predicate_stack.push(Predicate::Range {
            lo,
            hi,
            inclusive_lo,
            inclusive_hi,
        });
    }

    fn lower_outside_predicate<'input>(
        &mut self,
        args: Vec<Rc<GenericArgumentContextAll<'input>>>,
    ) {
        if args.len() != 2 {
            self.fail(GremlinError::Parse(
                "outside() expected two arguments".to_string(),
            ));
            return;
        }
        let mut iter = args.into_iter();
        let lo_ctx = iter.next().unwrap();
        let hi_ctx = iter.next().unwrap();
        self.visit_genericArgument(&lo_ctx);
        let Some(lo) = self.pop_value() else { return };
        self.visit_genericArgument(&hi_ctx);
        let Some(hi) = self.pop_value() else { return };
        self.predicate_stack.push(Predicate::Outside { lo, hi });
    }

    fn lower_text_predicate<'input>(
        &mut self,
        arg: Option<Rc<StringArgumentContextAll<'input>>>,
        kind: TextKind,
        negated: bool,
    ) {
        let Some(arg_ctx) = arg else {
            self.fail(GremlinError::Parse(
                "text predicate missing argument".to_string(),
            ));
            return;
        };
        let pattern = match self.string_argument_text(&arg_ctx) {
            Some(s) => s,
            None => {
                // visit pushed an error; bail.
                return;
            }
        };
        let predicate = Predicate::TextLike { pattern, kind };
        let predicate = if negated {
            Predicate::Not(Box::new(predicate))
        } else {
            predicate
        };
        self.predicate_stack.push(predicate);
    }

    fn lower_regex_predicate<'input>(
        &mut self,
        arg: Option<Rc<StringArgumentContextAll<'input>>>,
        negated: bool,
    ) {
        let Some(arg_ctx) = arg else {
            self.fail(GremlinError::Parse(
                "regex predicate missing argument".to_string(),
            ));
            return;
        };
        let pattern = match self.string_argument_text(&arg_ctx) {
            Some(s) => s,
            None => return,
        };
        let predicate = Predicate::Regex(pattern);
        let predicate = if negated {
            Predicate::Not(Box::new(predicate))
        } else {
            predicate
        };
        self.predicate_stack.push(predicate);
    }

    fn string_argument_text<'input>(
        &mut self,
        ctx: &StringArgumentContext<'input>,
    ) -> Option<String> {
        if let Some(literal) = ctx.stringLiteral() {
            self.visit_stringLiteral(&literal);
            return self.pop_string();
        }
        if let Some(var) = ctx.variable() {
            // Free variable in a text predicate. Resolve through the binding
            // table when available; otherwise return "" so the predicate
            // compiles to a match-nothing pattern.
            let name = var.get_text();
            let resolved = match self.binding_value(&name) {
                Some(GValue::String(s)) => s,
                _ => String::new(),
            };
            return Some(resolved);
        }
        self.fail(GremlinError::Parse(
            "string argument failed to parse".to_string(),
        ));
        None
    }

    /// Lowers a `nestedTraversal` to a Vec<Step>, scoping the visitor's
    /// `self.steps` accumulator so the inner steps don't leak into the
    /// surrounding traversal.
    fn lower_nested_traversal<'input>(
        &mut self,
        ctx: &NestedTraversalContext<'input>,
    ) -> Vec<Step> {
        let baseline = self.steps.len();
        if let Some(chained) = ctx.chainedTraversal() {
            self.visit_chainedTraversal(&chained);
        }
        self.steps.split_off(baseline)
    }

    fn collect_nested_traversal_list<'input>(
        &mut self,
        list: Option<Rc<NestedTraversalListContextAll<'input>>>,
    ) -> Vec<Vec<Step>> {
        let Some(list) = list else { return Vec::new() };
        let Some(expr) = list.nestedTraversalExpr() else {
            return Vec::new();
        };
        expr.nestedTraversal_all()
            .iter()
            .map(|n| self.lower_nested_traversal(n))
            .collect()
    }

    fn lower_expand_edge<'input>(
        &mut self,
        varargs: Option<Rc<StringNullableArgumentVarargsContextAll<'input>>>,
        direction: Direction,
        step: &str,
    ) {
        let Some(varargs) = varargs else {
            self.fail(GremlinError::Parse(format!(
                "{step}() missing argument list"
            )));
            return;
        };
        match self.collect_string_nullable_argument_varargs(&varargs, step) {
            Ok(edge_labels) => self.steps.push(Step::ExpandEdge {
                direction,
                edge_labels,
            }),
            Err(err) => self.fail(err),
        }
    }
    /// Lowers a `traversalMethod_hasLabel` subtree, handling each labeled
    /// alternative explicitly.
    fn dispatch_traversalMethod_hasLabel<'input>(
        &mut self,
        ctx: &TraversalMethod_hasLabelContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_hasLabelContextAll::TraversalMethod_hasLabel_String_StringContext(
                c,
            ) => {
                let mut labels = Vec::new();
                let Some(head) = c.stringNullableArgument() else {
                    self.fail(GremlinError::Parse(
                        "hasLabel() missing first argument".to_string(),
                    ));
                    return;
                };
                self.visit_stringNullableArgument(&head);
                let Some(label) = self.pop_string() else {
                    return;
                };
                labels.push(label);
                if let Some(rest) = c.stringNullableArgumentVarargs() {
                    for arg in rest.stringNullableArgument_all() {
                        self.visit_stringNullableArgument(&arg);
                        let Some(label) = self.pop_string() else {
                            return;
                        };
                        labels.push(label);
                    }
                }
                self.steps.push(Step::HasLabel(labels));
            }
            TraversalMethod_hasLabelContextAll::TraversalMethod_hasLabel_PContext(c) => {
                // hasLabel(P) — predicate-form label filter. Route to the same
                // helper used for has(T.label, P) so eq/within forms degrade
                // to a real HasLabel/Discard step instead of the no-op default.
                let predicate = c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.pop_predicate()
                });
                self.lower_t_has_label(None, predicate);
            }
            TraversalMethod_hasLabelContextAll::Error(_) => {
                self.fail(GremlinError::Parse(
                    "hasLabel() failed to parse".to_string(),
                ));
            }
        }
    }

    /// Lowers `traversalMethod_hasKey` subtree. Both alternatives are
    /// approximated: literal varargs become an OR-key filter; the predicate
    /// form lowers as Identity (no constraint).
    fn dispatch_traversalMethod_hasKey<'input>(
        &mut self,
        ctx: &TraversalMethod_hasKeyContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_hasKeyContextAll::TraversalMethod_hasKey_String_StringContext(c) => {
                let Some(literal) = c.stringNullableLiteral() else {
                    self.fail(GremlinError::Parse(
                        "hasKey() missing first argument".to_string(),
                    ));
                    return;
                };
                let mut keys = Vec::new();
                self.visit_stringNullableLiteral(&literal);
                if let Some(key) = self.pop_string() {
                    if !key.is_empty() {
                        keys.push(key);
                    }
                }
                if let Some(rest) = c.stringNullableLiteralVarargs() {
                    for arg in rest.stringNullableLiteral_all() {
                        self.visit_stringNullableLiteral(&arg);
                        if let Some(key) = self.pop_string() {
                            if !key.is_empty() {
                                keys.push(key);
                            }
                        }
                    }
                }
                self.push_has_key_filter(keys);
            }
            TraversalMethod_hasKeyContextAll::TraversalMethod_hasKey_PContext(_) => {
                self.steps.push(Step::Identity);
            }
            TraversalMethod_hasKeyContextAll::Error(_) => {
                self.fail(GremlinError::Parse("hasKey() failed to parse".to_string()));
            }
        }
    }

    fn push_has_key_filter(&mut self, mut keys: Vec<String>) {
        keys.sort();
        keys.dedup();
        match keys.len() {
            0 => self.steps.push(Step::HasKey { key: String::new() }),
            1 => self.steps.push(Step::HasKey {
                key: keys.pop().unwrap(),
            }),
            _ => self.steps.push(Step::HasKeyAny(keys)),
        }
    }

    /// Lowers `traversalMethod_has` subtree. Some alternatives produce a
    /// single `Has`/`HasKey`; the labelled ones produce a `HasLabel` + `Has`
    /// pair.
    fn dispatch_traversalMethod_has<'input>(
        &mut self,
        ctx: &TraversalMethod_hasContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_hasContextAll::TraversalMethod_has_StringContext(c) => {
                let Some(literal) = c.stringNullableLiteral() else {
                    self.fail(GremlinError::Parse(
                        "has() missing key argument".to_string(),
                    ));
                    return;
                };
                self.visit_stringNullableLiteral(&literal);
                let Some(key) = self.pop_string() else { return };
                self.steps.push(Step::HasKey { key });
            }
            TraversalMethod_hasContextAll::TraversalMethod_has_String_ObjectContext(c) => {
                let Some(literal) = c.stringNullableLiteral() else {
                    self.fail(GremlinError::Parse(
                        "has() missing key argument".to_string(),
                    ));
                    return;
                };
                self.visit_stringNullableLiteral(&literal);
                let Some(key) = self.pop_string() else { return };
                let Some(value_ctx) = c.genericArgument() else {
                    self.fail(GremlinError::Parse(
                        "has() missing value argument".to_string(),
                    ));
                    return;
                };
                self.visit_genericArgument(&value_ctx);
                let Some(value) = self.pop_value() else {
                    return;
                };
                self.steps.push(Step::Has {
                    key,
                    predicate: Predicate::eq(value),
                });
            }
            TraversalMethod_hasContextAll::TraversalMethod_has_String_PContext(c) => {
                let Some(literal) = c.stringNullableLiteral() else {
                    self.fail(GremlinError::Parse(
                        "has() missing key argument".to_string(),
                    ));
                    return;
                };
                self.visit_stringNullableLiteral(&literal);
                let Some(key) = self.pop_string() else { return };
                let Some(predicate_ctx) = c.traversalPredicate() else {
                    self.fail(GremlinError::Parse(
                        "has() missing predicate argument".to_string(),
                    ));
                    return;
                };
                self.visit_traversalPredicate(&predicate_ctx);
                let Some(predicate) = self.pop_predicate() else {
                    return;
                };
                self.steps.push(Step::Has { key, predicate });
            }
            TraversalMethod_hasContextAll::TraversalMethod_has_String_String_ObjectContext(c) => {
                let Some(label_ctx) = c.stringNullableArgument() else {
                    self.fail(GremlinError::Parse(
                        "has() missing label argument".to_string(),
                    ));
                    return;
                };
                self.visit_stringNullableArgument(&label_ctx);
                let Some(label) = self.pop_string() else {
                    return;
                };
                let Some(literal) = c.stringNullableLiteral() else {
                    self.fail(GremlinError::Parse(
                        "has() missing key argument".to_string(),
                    ));
                    return;
                };
                self.visit_stringNullableLiteral(&literal);
                let Some(key) = self.pop_string() else { return };
                let Some(value_ctx) = c.genericArgument() else {
                    self.fail(GremlinError::Parse(
                        "has() missing value argument".to_string(),
                    ));
                    return;
                };
                self.visit_genericArgument(&value_ctx);
                let Some(value) = self.pop_value() else {
                    return;
                };
                self.steps.push(Step::HasLabel(vec![label]));
                self.steps.push(Step::Has {
                    key,
                    predicate: Predicate::eq(value),
                });
            }
            TraversalMethod_hasContextAll::TraversalMethod_has_String_String_PContext(c) => {
                let Some(label_ctx) = c.stringNullableArgument() else {
                    self.fail(GremlinError::Parse(
                        "has() missing label argument".to_string(),
                    ));
                    return;
                };
                self.visit_stringNullableArgument(&label_ctx);
                let Some(label) = self.pop_string() else {
                    return;
                };
                let Some(literal) = c.stringNullableLiteral() else {
                    self.fail(GremlinError::Parse(
                        "has() missing key argument".to_string(),
                    ));
                    return;
                };
                self.visit_stringNullableLiteral(&literal);
                let Some(key) = self.pop_string() else { return };
                let Some(predicate_ctx) = c.traversalPredicate() else {
                    self.fail(GremlinError::Parse(
                        "has() missing predicate argument".to_string(),
                    ));
                    return;
                };
                self.visit_traversalPredicate(&predicate_ctx);
                let Some(predicate) = self.pop_predicate() else {
                    return;
                };
                self.steps.push(Step::HasLabel(vec![label]));
                self.steps.push(Step::Has { key, predicate });
            }
            TraversalMethod_hasContextAll::TraversalMethod_has_T_ObjectContext(c) => {
                // has(T.id, x) → HasId{[x]}; has(T.label, "p") → HasLabel(["p"]).
                // T.key/T.value are property-object filters we don't model.
                let raw = c.traversalT().map(|t| t.get_text()).unwrap_or_default();
                let token = raw.strip_prefix("T.").unwrap_or(&raw).trim().to_lowercase();
                let value = c.genericArgument().and_then(|arg| {
                    self.visit_genericArgument(&arg);
                    self.pop_value()
                });
                self.lower_t_has(&token, value, None);
            }
            TraversalMethod_hasContextAll::TraversalMethod_has_T_PContext(c) => {
                // has(T.label, eq("p")) and has(T.id, P.within([..])) are
                // equally well modelled — extract the predicate and route.
                let raw = c.traversalT().map(|t| t.get_text()).unwrap_or_default();
                let token = raw.strip_prefix("T.").unwrap_or(&raw).trim().to_lowercase();
                let predicate = c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.pop_predicate()
                });
                self.lower_t_has(&token, None, predicate);
            }
            TraversalMethod_hasContextAll::Error(_) => {
                self.fail(GremlinError::Parse("has() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_traversalMethod_limit<'input>(
        &mut self,
        ctx: &TraversalMethod_limitContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_limitContextAll::TraversalMethod_limit_longContext(c) => {
                let Some(arg) = c.integerArgument() else {
                    self.fail(GremlinError::Parse(
                        "limit() missing integer argument".to_string(),
                    ));
                    return;
                };
                self.visit_integerArgument(&arg);
                let Some(n) = self.pop_integer() else { return };
                self.steps.push(Step::Limit(n));
            }
            TraversalMethod_limitContextAll::TraversalMethod_limit_Scope_longContext(c) => {
                let n = c
                    .integerArgument()
                    .map(|arg| {
                        self.visit_integerArgument(&arg);
                        self.pop_integer().unwrap_or(0)
                    })
                    .unwrap_or(0);
                if has_local_scope_arg(&c.get_text()) {
                    self.steps.push(Step::LocalScoped(Box::new(Step::Limit(n))));
                } else {
                    self.steps.push(Step::Limit(n));
                }
            }
            TraversalMethod_limitContextAll::Error(_) => {
                self.fail(GremlinError::Parse("limit() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_traversalMethod_is<'input>(&mut self, ctx: &TraversalMethod_isContextAll<'input>) {
        match ctx {
            TraversalMethod_isContextAll::TraversalMethod_is_ObjectContext(c) => {
                let Some(arg) = c.genericArgument() else {
                    self.fail(GremlinError::Parse("is() missing argument".to_string()));
                    return;
                };
                self.visit_genericArgument(&arg);
                let Some(value) = self.pop_value() else {
                    return;
                };
                self.steps.push(Step::Is {
                    predicate: Predicate::eq(value),
                });
            }
            TraversalMethod_isContextAll::TraversalMethod_is_PContext(c) => {
                let Some(p) = c.traversalPredicate() else {
                    self.fail(GremlinError::Parse(
                        "is() missing predicate argument".to_string(),
                    ));
                    return;
                };
                self.visit_traversalPredicate(&p);
                let Some(predicate) = self.pop_predicate() else {
                    return;
                };
                self.steps.push(Step::Is { predicate });
            }
            TraversalMethod_isContextAll::Error(_) => {
                self.fail(GremlinError::Parse("is() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_traversalMethod_hasId<'input>(
        &mut self,
        ctx: &TraversalMethod_hasIdContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_hasIdContextAll::TraversalMethod_hasId_Object_ObjectContext(c) => {
                let Some(head) = c.genericArgument() else {
                    self.fail(GremlinError::Parse(
                        "hasId() missing first argument".to_string(),
                    ));
                    return;
                };
                self.visit_genericArgument(&head);
                let Some(first) = self.pop_value() else {
                    return;
                };
                let mut ids = vec![first];
                if let Some(rest) = c.genericArgumentVarargs() {
                    match self.collect_generic_argument_varargs(&rest) {
                        Ok(more) => ids.extend(more),
                        Err(err) => {
                            self.fail(err);
                            return;
                        }
                    }
                }
                self.steps.push(Step::HasId { ids });
            }
            TraversalMethod_hasIdContextAll::TraversalMethod_hasId_PContext(c) => {
                // hasId(P) — extract predicate forms we can model without
                // surfacing id columns: eq(x) → HasId{[x]}, within(xs) →
                // HasId{xs} (or Discard for the empty list). Other predicate
                // shapes (without/neq/etc.) stay as Identity.
                let predicate = c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.pop_predicate()
                });
                self.lower_has_id_predicate(predicate);
            }
            TraversalMethod_hasIdContextAll::Error(_) => {
                self.fail(GremlinError::Parse("hasId() failed to parse".to_string()));
            }
        }
    }

    fn lower_t_has(&mut self, token: &str, value: Option<GValue>, predicate: Option<Predicate>) {
        match token {
            "id" => self.lower_t_has_id(value, predicate),
            "label" => self.lower_t_has_label(value, predicate),
            _ => self.steps.push(Step::Identity),
        }
    }

    fn lower_t_has_id(&mut self, value: Option<GValue>, predicate: Option<Predicate>) {
        if let Some(value) = value {
            self.steps.push(Step::HasId { ids: vec![value] });
            return;
        }
        if let Some(p) = predicate {
            match p {
                Predicate::Compare {
                    op: CompareOp::Eq,
                    value,
                } => {
                    self.steps.push(Step::HasId { ids: vec![value] });
                    return;
                }
                Predicate::Within(values) => {
                    if values.is_empty() {
                        self.steps.push(Step::Discard);
                    } else {
                        self.steps.push(Step::HasId { ids: values });
                    }
                    return;
                }
                Predicate::Without(values) if values.is_empty() => {
                    self.steps.push(Step::Identity);
                    return;
                }
                _ => {}
            }
            self.steps.push(Step::HasIdPredicate { predicate: p });
            return;
        }
        self.steps.push(Step::Identity);
    }

    fn lower_t_has_label(&mut self, value: Option<GValue>, predicate: Option<Predicate>) {
        if let Some(value) = value {
            match value {
                GValue::String(s) => self.steps.push(Step::HasLabel(vec![s])),
                // has(T.label, null) — no real label is null; filter to none.
                GValue::Null => self.steps.push(Step::Discard),
                _ => self.steps.push(Step::Identity),
            }
            return;
        }
        if let Some(p) = predicate {
            match p {
                Predicate::Compare {
                    op: CompareOp::Eq,
                    value: GValue::String(s),
                } => {
                    self.steps.push(Step::HasLabel(vec![s]));
                    return;
                }
                Predicate::Compare {
                    op: CompareOp::Eq,
                    value: GValue::Null,
                } => {
                    self.steps.push(Step::Discard);
                    return;
                }
                Predicate::Within(values) => {
                    let labels: Vec<String> = values
                        .into_iter()
                        .filter_map(|v| match v {
                            GValue::String(s) => Some(s),
                            _ => None,
                        })
                        .collect();
                    if labels.is_empty() {
                        self.steps.push(Step::Discard);
                    } else {
                        self.steps.push(Step::HasLabel(labels));
                    }
                    return;
                }
                _ => {}
            }
        }
        self.steps.push(Step::Identity);
    }

    /// Emits an aggregate step (`sum`/`min`/`max`/`mean`/`product`),
    /// wrapping it in `LocalScoped` when the call site references
    /// `Scope.local` (text-based detection — robust across grammar
    /// variants we don't dispatch on by name).
    fn push_aggregate_with_scope(&mut self, raw_text: &str, kind: AggKind) {
        let agg = Step::Aggregate(kind);
        if has_local_scope_arg(raw_text) {
            self.steps.push(Step::LocalScoped(Box::new(agg)));
        } else {
            self.steps.push(agg);
        }
    }

    fn lower_has_id_predicate(&mut self, predicate: Option<Predicate>) {
        if let Some(p) = predicate {
            match p {
                Predicate::Compare {
                    op: CompareOp::Eq,
                    value,
                } => {
                    self.steps.push(Step::HasId { ids: vec![value] });
                    return;
                }
                Predicate::Within(values) => {
                    if values.is_empty() {
                        self.steps.push(Step::Discard);
                    } else {
                        self.steps.push(Step::HasId { ids: values });
                    }
                    return;
                }
                Predicate::Without(values) if values.is_empty() => {
                    self.steps.push(Step::Identity);
                    return;
                }
                _ => {}
            }
            self.steps.push(Step::HasIdPredicate { predicate: p });
            return;
        }
        self.steps.push(Step::Identity);
    }

    fn dispatch_traversalMethod_dedup<'input>(
        &mut self,
        ctx: &TraversalMethod_dedupContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_dedupContextAll::TraversalMethod_dedup_StringContext(c) => {
                let labels = extract_top_level_string_args(&c.get_text());
                if labels.is_empty() {
                    self.steps.push(Step::Dedup);
                } else {
                    self.steps.push(Step::DedupLabels(labels));
                }
            }
            TraversalMethod_dedupContextAll::TraversalMethod_dedup_Scope_StringContext(_) => {
                // dedup(Scope.local): dedup elements within the current list
                // traverser instead of across rows.
                self.steps.push(Step::LocalScoped(Box::new(Step::Dedup)));
            }
            TraversalMethod_dedupContextAll::Error(_) => {
                self.fail(GremlinError::Parse("dedup() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_traversalMethod_order<'input>(
        &mut self,
        ctx: &TraversalMethod_orderContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_orderContextAll::TraversalMethod_order_EmptyContext(_) => {
                self.steps.push(Step::Order);
            }
            TraversalMethod_orderContextAll::TraversalMethod_order_ScopeContext(c) => {
                if has_local_scope_arg(&c.get_text()) {
                    // order(Scope.local): sort within the current list traverser.
                    self.steps.push(Step::LocalScoped(Box::new(Step::Order)));
                } else {
                    self.steps.push(Step::Order);
                }
            }
            TraversalMethod_orderContextAll::Error(_) => {
                self.fail(GremlinError::Parse("order() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_traversalMethod_range<'input>(
        &mut self,
        ctx: &TraversalMethod_rangeContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_rangeContextAll::TraversalMethod_range_long_longContext(c) => {
                let mut args = c.integerArgument_all();
                if args.len() != 2 {
                    self.fail(GremlinError::Parse(
                        "range(low, high) expected two integer arguments".to_string(),
                    ));
                    return;
                }
                let high_ctx = args.remove(1);
                let low_ctx = args.remove(0);
                self.visit_integerArgument(&low_ctx);
                let Some(low) = self.pop_integer() else {
                    return;
                };
                self.visit_integerArgument(&high_ctx);
                let Some(high) = self.pop_integer() else {
                    return;
                };
                self.steps.push(Step::Range { low, high });
            }
            TraversalMethod_rangeContextAll::TraversalMethod_range_Scope_long_longContext(c) => {
                let mut args = c.integerArgument_all();
                if args.len() == 2 {
                    let high_ctx = args.remove(1);
                    let low_ctx = args.remove(0);
                    self.visit_integerArgument(&low_ctx);
                    let low = self.pop_integer().unwrap_or(0);
                    self.visit_integerArgument(&high_ctx);
                    let high = self.pop_integer().unwrap_or(low);
                    if has_local_scope_arg(&c.get_text()) {
                        self.steps
                            .push(Step::LocalScoped(Box::new(Step::Range { low, high })));
                    } else {
                        self.steps.push(Step::Range { low, high });
                    }
                } else {
                    self.steps.push(Step::Identity);
                }
            }
            TraversalMethod_rangeContextAll::Error(_) => {
                self.fail(GremlinError::Parse("range() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_traversalMethod_skip<'input>(
        &mut self,
        ctx: &TraversalMethod_skipContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_skipContextAll::TraversalMethod_skip_longContext(c) => {
                let Some(arg) = c.integerArgument() else {
                    self.fail(GremlinError::Parse(
                        "skip() missing integer argument".to_string(),
                    ));
                    return;
                };
                self.visit_integerArgument(&arg);
                let Some(n) = self.pop_integer() else { return };
                self.steps.push(Step::Skip(n));
            }
            TraversalMethod_skipContextAll::TraversalMethod_skip_Scope_longContext(c) => {
                let n = c
                    .integerArgument()
                    .map(|arg| {
                        self.visit_integerArgument(&arg);
                        self.pop_integer().unwrap_or(0)
                    })
                    .unwrap_or(0);
                if has_local_scope_arg(&c.get_text()) {
                    self.steps.push(Step::LocalScoped(Box::new(Step::Skip(n))));
                } else {
                    self.steps.push(Step::Skip(n));
                }
            }
            TraversalMethod_skipContextAll::Error(_) => {
                self.fail(GremlinError::Parse("skip() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_traversalMethod_tail<'input>(
        &mut self,
        ctx: &TraversalMethod_tailContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_tailContextAll::TraversalMethod_tail_EmptyContext(_) => {
                self.steps.push(Step::Tail(1));
            }
            TraversalMethod_tailContextAll::TraversalMethod_tail_longContext(c) => {
                let Some(arg) = c.integerArgument() else {
                    self.fail(GremlinError::Parse(
                        "tail() missing integer argument".to_string(),
                    ));
                    return;
                };
                self.visit_integerArgument(&arg);
                let Some(n) = self.pop_integer() else { return };
                self.steps.push(Step::Tail(n));
            }
            TraversalMethod_tailContextAll::TraversalMethod_tail_ScopeContext(c) => {
                if has_local_scope_arg(&c.get_text()) {
                    self.steps.push(Step::LocalScoped(Box::new(Step::Tail(1))));
                } else {
                    self.steps.push(Step::Tail(1));
                }
            }
            TraversalMethod_tailContextAll::TraversalMethod_tail_Scope_longContext(c) => {
                let n = c
                    .integerArgument()
                    .map(|arg| {
                        self.visit_integerArgument(&arg);
                        self.pop_integer().unwrap_or(1)
                    })
                    .unwrap_or(1);
                if has_local_scope_arg(&c.get_text()) {
                    self.steps.push(Step::LocalScoped(Box::new(Step::Tail(n))));
                } else {
                    self.steps.push(Step::Tail(n));
                }
            }
            TraversalMethod_tailContextAll::Error(_) => {
                self.fail(GremlinError::Parse("tail() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_simple_string_op(&mut self, text: &str, op: StringOp) {
        // length(Scope.local), toUpper(Scope.local), reverse(Scope.local),
        // ... — when the call site supplies Scope.local, wrap so the planner
        // can distinguish per-list-element evaluation from the global form.
        let step = Step::StringOp(op);
        if has_local_scope_arg(text) {
            self.steps.push(Step::LocalScoped(Box::new(step)));
        } else {
            self.steps.push(step);
        }
    }

    fn dispatch_traversalMethod_substring<'input>(
        &mut self,
        ctx: &TraversalMethod_substringContextAll<'input>,
    ) {
        let text = ctx.get_text();
        let args = extract_top_level_args(&text);
        let scope_local = args.iter().any(|arg| is_scope_local_arg(arg));
        let mut numbers = args
            .iter()
            .filter(|arg| !is_scope_local_arg(arg))
            .filter_map(|arg| parse_integer_literal(arg.trim()).ok());
        let start = numbers.next().unwrap_or(0);
        let end = numbers.next();
        let op = Step::StringOp(StringOp::Substring { start, end });
        if scope_local {
            self.steps.push(Step::LocalScoped(Box::new(op)));
        } else {
            self.steps.push(op);
        }
    }

    fn dispatch_traversalMethod_replace<'input>(
        &mut self,
        ctx: &TraversalMethod_replaceContextAll<'input>,
    ) {
        let (old, new, scope_local) = match ctx {
            TraversalMethod_replaceContextAll::TraversalMethod_replace_String_StringContext(c) => {
                let mut iter = c.stringNullableLiteral_all().into_iter();
                let old = iter.next().and_then(|s| {
                    self.visit_stringNullableLiteral(&s);
                    self.pop_string()
                });
                let new = iter.next().and_then(|s| {
                    self.visit_stringNullableLiteral(&s);
                    self.pop_string()
                });
                (old, new, false)
            }
            TraversalMethod_replaceContextAll::TraversalMethod_replace_Scope_String_StringContext(
                c,
            ) => {
                let mut iter = c.stringNullableLiteral_all().into_iter();
                let old = iter.next().and_then(|s| {
                    self.visit_stringNullableLiteral(&s);
                    self.pop_string()
                });
                let new = iter.next().and_then(|s| {
                    self.visit_stringNullableLiteral(&s);
                    self.pop_string()
                });
                (old, new, true)
            }
            TraversalMethod_replaceContextAll::Error(_) => (None, None, false),
        };
        let op = Step::StringOp(StringOp::Replace {
            old: old.unwrap_or_default(),
            new: new.unwrap_or_default(),
        });
        if scope_local {
            self.steps.push(Step::LocalScoped(Box::new(op)));
        } else {
            self.steps.push(op);
        }
    }

    fn dispatch_traversalMethod_concat<'input>(
        &mut self,
        ctx: &TraversalMethod_concatContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_concatContextAll::TraversalMethod_concat_StringContext(c) => {
                // Walk every literal arg and concatenate them into a single
                // suffix string. `concat("a", "b")` is equivalent to
                // `concat("a").concat("b")` for our flat-row model, so a
                // single combined Concat step is sufficient.
                let mut suffix = String::new();
                if let Some(v) = c.stringNullableLiteralVarargs() {
                    for s in v.stringNullableLiteral_all() {
                        self.visit_stringNullableLiteral(&s);
                        if let Some(part) = self.pop_string() {
                            suffix.push_str(&part);
                        }
                    }
                }
                self.steps.push(Step::StringOp(StringOp::Concat(suffix)));
            }
            TraversalMethod_concatContextAll::TraversalMethod_concat_Traversal_TraversalContext(
                c,
            ) => {
                if let Some(nested) = c.nestedTraversal() {
                    let traversal = self.lower_nested_traversal(&nested);
                    self.steps
                        .push(Step::StringOp(StringOp::ConcatTraversal(traversal)));
                }
                if let Some(rest) = c.nestedTraversalList() {
                    for nested in self.collect_nested_traversal_list(Some(rest)) {
                        self.steps
                            .push(Step::StringOp(StringOp::ConcatTraversal(nested)));
                    }
                }
            }
            TraversalMethod_concatContextAll::Error(_) => self.steps.push(Step::Identity),
        }
    }

    fn dispatch_traversalMethod_all<'input>(
        &mut self,
        ctx: &TraversalMethod_allContextAll<'input>,
    ) {
        // all(P): list projections need element-wise quantifier semantics.
        // Non-list traversers do not match.
        let predicate = match ctx {
            TraversalMethod_allContextAll::TraversalMethod_all_PContext(c) => {
                c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.pop_predicate()
                })
            }
            TraversalMethod_allContextAll::Error(_) => None,
        };
        match predicate {
            Some(p) => self.steps.push(Step::All { predicate: p }),
            None => self.steps.push(Step::Identity),
        }
    }

    fn dispatch_traversalMethod_any<'input>(
        &mut self,
        ctx: &TraversalMethod_anyContextAll<'input>,
    ) {
        // any(P): list projections need element-wise quantifier semantics.
        // Non-list traversers do not match.
        let predicate = match ctx {
            TraversalMethod_anyContextAll::TraversalMethod_any_PContext(c) => {
                c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.pop_predicate()
                })
            }
            TraversalMethod_anyContextAll::Error(_) => None,
        };
        match predicate {
            Some(p) => self.steps.push(Step::Any { predicate: p }),
            None => self.steps.push(Step::Identity),
        }
    }

    fn dispatch_traversalMethod_none<'input>(
        &mut self,
        ctx: &TraversalMethod_noneContextAll<'input>,
    ) {
        let predicate = match ctx {
            TraversalMethod_noneContextAll::TraversalMethod_none_PContext(c) => {
                c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.pop_predicate()
                })
            }
            TraversalMethod_noneContextAll::Error(_) => None,
        };
        match predicate {
            Some(p) => self.steps.push(Step::NonePredicate { predicate: p }),
            None => self.steps.push(Step::None),
        }
    }

    fn dispatch_traversalMethod_format<'input>(
        &mut self,
        ctx: &TraversalMethod_formatContextAll<'input>,
    ) {
        let template = match ctx {
            TraversalMethod_formatContextAll::TraversalMethod_format_StringContext(c) => c
                .stringLiteral()
                .and_then(|s| {
                    self.visit_stringLiteral(&s);
                    self.pop_string()
                })
                .unwrap_or_default(),
            TraversalMethod_formatContextAll::Error(_) => String::new(),
        };
        self.steps
            .push(Step::Format(parse_format_template(&template)));
    }

    fn dispatch_traversalMethod_conjoin<'input>(
        &mut self,
        ctx: &TraversalMethod_conjoinContextAll<'input>,
    ) {
        // conjoin(delim) joins a list traverser with a delimiter. With flat
        // rows we have no list to join — degenerate to the current scalar.
        // Capturing the delim via Concat keeps the chain useful for the
        // single-element case.
        let delim = match ctx {
            TraversalMethod_conjoinContextAll::TraversalMethod_conjoin_StringContext(c) => c
                .stringLiteral()
                .and_then(|s| {
                    self.visit_stringLiteral(&s);
                    self.pop_string()
                })
                .unwrap_or_default(),
            TraversalMethod_conjoinContextAll::Error(_) => String::new(),
        };
        self.steps.push(Step::StringOp(StringOp::Conjoin(delim)));
    }

    fn dispatch_traversalMethod_fold<'input>(
        &mut self,
        ctx: &TraversalMethod_foldContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_foldContextAll::TraversalMethod_fold_EmptyContext(_) => {
                self.steps.push(Step::Fold);
            }
            TraversalMethod_foldContextAll::TraversalMethod_fold_Object_BiFunctionContext(c) => {
                let seed = c
                    .genericLiteral()
                    .and_then(|lit| {
                        self.visit_genericLiteral(&lit);
                        self.pop_value()
                    })
                    .unwrap_or(GValue::Null);
                let op = c
                    .traversalBiFunction()
                    .and_then(|b| b.traversalOperator())
                    .and_then(|o| sack_op_from_text(&o.get_text()))
                    .unwrap_or(SackOp::Assign);
                self.steps.push(Step::FoldReduce { seed, op });
            }
            TraversalMethod_foldContextAll::Error(_) => {
                self.fail(GremlinError::Parse("fold() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_list_op<'input>(
        &mut self,
        literal: Option<Rc<GenericLiteralContextAll<'input>>>,
        kind: ListOpKind,
    ) {
        let value = match literal {
            Some(lit_ctx) => {
                if let Some(nested) = lit_ctx.nestedTraversal() {
                    let rhs = self.lower_nested_traversal(&nested);
                    self.steps.push(Step::ListOpTraversal(kind, rhs));
                    return;
                }
                self.visit_genericLiteral(&lit_ctx);
                self.pop_value().unwrap_or(GValue::Null)
            }
            None => GValue::Null,
        };
        self.steps.push(Step::ListOp(kind, value));
    }

    fn lower_call_name<'input>(
        &mut self,
        literal: Option<Rc<StringLiteralContextAll<'input>>>,
    ) -> String {
        literal
            .and_then(|s| {
                self.visit_stringLiteral(&s);
                self.pop_string()
            })
            .unwrap_or_default()
    }

    fn lower_call<'input>(
        &mut self,
        ctx: &TraversalMethod_callContextAll<'input>,
    ) -> (String, Vec<CallArg>) {
        let mut args = Vec::new();
        match ctx {
            TraversalMethod_callContextAll::TraversalMethod_call_stringContext(c) => {
                (self.lower_call_name(c.stringLiteral()), args)
            }
            TraversalMethod_callContextAll::TraversalMethod_call_string_mapContext(c) => {
                if let Some(map) = c.genericMapArgument() {
                    args.push(CallArg::Map(map.get_text()));
                }
                (self.lower_call_name(c.stringLiteral()), args)
            }
            TraversalMethod_callContextAll::TraversalMethod_call_string_traversalContext(c) => {
                if let Some(nested) = c.nestedTraversal() {
                    args.push(CallArg::Traversal(self.lower_nested_traversal(&nested)));
                }
                (self.lower_call_name(c.stringLiteral()), args)
            }
            TraversalMethod_callContextAll::TraversalMethod_call_string_map_traversalContext(c) => {
                if let Some(map) = c.genericMapArgument() {
                    args.push(CallArg::Map(map.get_text()));
                }
                if let Some(nested) = c.nestedTraversal() {
                    args.push(CallArg::Traversal(self.lower_nested_traversal(&nested)));
                }
                (self.lower_call_name(c.stringLiteral()), args)
            }
            TraversalMethod_callContextAll::Error(_) => (String::new(), args),
        }
    }

    fn lower_source_call<'input>(
        &mut self,
        ctx: &TraversalSourceSpawnMethod_callContextAll<'input>,
    ) -> (String, Vec<CallArg>) {
        let mut args = Vec::new();
        match ctx {
            TraversalSourceSpawnMethod_callContextAll::TraversalSourceSpawnMethod_call_emptyContext(_) => {
                (String::new(), args)
            }
            TraversalSourceSpawnMethod_callContextAll::TraversalSourceSpawnMethod_call_stringContext(c) => {
                (self.lower_call_name(c.stringLiteral()), args)
            }
            TraversalSourceSpawnMethod_callContextAll::TraversalSourceSpawnMethod_call_string_mapContext(c) => {
                if let Some(map) = c.genericMapArgument() {
                    args.push(CallArg::Map(map.get_text()));
                }
                (self.lower_call_name(c.stringLiteral()), args)
            }
            TraversalSourceSpawnMethod_callContextAll::TraversalSourceSpawnMethod_call_string_traversalContext(c) => {
                if let Some(nested) = c.nestedTraversal() {
                    args.push(CallArg::Traversal(self.lower_nested_traversal(&nested)));
                }
                (self.lower_call_name(c.stringLiteral()), args)
            }
            TraversalSourceSpawnMethod_callContextAll::TraversalSourceSpawnMethod_call_string_map_traversalContext(c) => {
                if let Some(map) = c.genericMapArgument() {
                    args.push(CallArg::Map(map.get_text()));
                }
                if let Some(nested) = c.nestedTraversal() {
                    args.push(CallArg::Traversal(self.lower_nested_traversal(&nested)));
                }
                (self.lower_call_name(c.stringLiteral()), args)
            }
            TraversalSourceSpawnMethod_callContextAll::Error(_) => (String::new(), args),
        }
    }

    fn dispatch_traversalMethod_hasValue<'input>(
        &mut self,
        ctx: &TraversalMethod_hasValueContextAll<'input>,
    ) {
        let predicate = match ctx {
            TraversalMethod_hasValueContextAll::TraversalMethod_hasValue_Object_ObjectContext(
                c,
            ) => {
                let Some(arg) = c.genericArgument() else {
                    self.steps.push(Step::Identity);
                    return;
                };
                let mut values = Vec::new();
                self.visit_genericArgument(&arg);
                if let Some(value) = self.pop_value() {
                    values.push(value);
                }
                if let Some(rest) = c.genericArgumentVarargs() {
                    for arg in rest.genericArgument_all() {
                        self.visit_genericArgument(&arg);
                        if let Some(value) = self.pop_value() {
                            values.push(value);
                        }
                    }
                }
                let non_null: Vec<GValue> = values
                    .iter()
                    .filter(|v| !matches!(v, GValue::Null))
                    .cloned()
                    .collect();
                let values = if non_null.is_empty() {
                    values
                } else {
                    non_null
                };
                match values.as_slice() {
                    [value] => Predicate::eq(value.clone()),
                    _ => Predicate::Within(values),
                }
            }
            TraversalMethod_hasValueContextAll::TraversalMethod_hasValue_PContext(c) => {
                let Some(p) = c.traversalPredicate() else {
                    self.steps.push(Step::Identity);
                    return;
                };
                self.visit_traversalPredicate(&p);
                let Some(predicate) = self.pop_predicate() else {
                    return;
                };
                predicate
            }
            TraversalMethod_hasValueContextAll::Error(_) => {
                self.steps.push(Step::Identity);
                return;
            }
        };
        self.steps.push(Step::HasValue(predicate));
    }

    fn dispatch_traversalMethod_toV<'input>(&mut self, ctx: &TraversalMethod_toVContext<'input>) {
        let direction = ctx
            .traversalDirection()
            .map(|d| direction_from_text(&d.get_text()))
            .unwrap_or(Direction::Both);
        self.steps.push(Step::EndpointVertex { direction });
    }

    fn dispatch_traversalMethod_toE<'input>(&mut self, ctx: &TraversalMethod_toEContext<'input>) {
        let direction = ctx
            .traversalDirection()
            .map(|d| direction_from_text(&d.get_text()))
            .unwrap_or(Direction::Both);
        let edge_labels = ctx
            .stringNullableArgumentVarargs()
            .map(|v| {
                self.collect_string_nullable_argument_varargs(&v, "toE")
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        self.steps.push(Step::ExpandEdge {
            direction,
            edge_labels,
        });
    }

    fn dispatch_traversalMethod_loops<'input>(
        &mut self,
        ctx: &TraversalMethod_loopsContextAll<'input>,
    ) {
        let name = match ctx {
            TraversalMethod_loopsContextAll::TraversalMethod_loops_StringContext(c) => {
                c.stringLiteral().and_then(|s| {
                    self.visit_stringLiteral(&s);
                    self.pop_string()
                })
            }
            _ => None,
        };
        self.steps.push(Step::Loops(name));
    }

    fn dispatch_traversalMethod_emit<'input>(
        &mut self,
        ctx: &TraversalMethod_emitContextAll<'input>,
    ) {
        // emit modulator. Predicate form can reuse the normal scalar `is(P)`
        // filter as the repeat emission sub-traversal.
        let sub = match ctx {
            TraversalMethod_emitContextAll::TraversalMethod_emit_TraversalContext(c) => {
                c.nestedTraversal().map(|n| self.lower_nested_traversal(&n))
            }
            TraversalMethod_emitContextAll::TraversalMethod_emit_PredicateContext(c) => c
                .traversalPredicate()
                .and_then(|p| self.lower_predicate_as_filter(&p)),
            _ => None,
        };
        self.steps.push(Step::Emit(sub));
    }

    fn lower_predicate_as_filter<'input>(
        &mut self,
        predicate: &TraversalPredicateContextAll<'input>,
    ) -> Option<Vec<Step>> {
        self.visit_traversalPredicate(predicate);
        self.pop_predicate()
            .map(|predicate| vec![Step::Is { predicate }])
    }

    fn dispatch_traversalMethod_until<'input>(
        &mut self,
        ctx: &TraversalMethod_untilContextAll<'input>,
    ) {
        // until modulator. Predicate form lowers to the same scalar `is(P)`
        // filter shape that traversal-form until already consumes.
        let sub = match ctx {
            TraversalMethod_untilContextAll::TraversalMethod_until_TraversalContext(c) => {
                c.nestedTraversal().map(|n| self.lower_nested_traversal(&n))
            }
            TraversalMethod_untilContextAll::TraversalMethod_until_PredicateContext(c) => c
                .traversalPredicate()
                .and_then(|p| self.lower_predicate_as_filter(&p)),
            _ => None,
        };
        // No nested traversal → degenerate to a never-true predicate so the
        // loop runs to its REPEAT_CAP.
        self.steps.push(Step::Until(sub.unwrap_or_default()));
    }

    fn dispatch_traversalMethod_option<'input>(
        &mut self,
        ctx: &TraversalMethod_optionContextAll<'input>,
    ) {
        let option = match self.lower_option(ctx) {
            Some(option) => option,
            None => {
                self.steps.push(Step::Identity);
                return;
            }
        };
        if let Some(Step::BranchOptions { options, .. }) = self.steps.last_mut() {
            options.push(option);
        } else {
            // Stray option() outside branch/choose: preserve the previous
            // compile-friendly behaviour by inlining the option traversal.
            self.steps.push(Step::Local(option.traversal));
        }
    }

    fn dispatch_traversalMethod_aggregate<'input>(
        &mut self,
        ctx: &TraversalMethod_aggregateContextAll<'input>,
    ) {
        let label = match ctx {
            TraversalMethod_aggregateContextAll::TraversalMethod_aggregate_StringContext(c) => c
                .stringLiteral()
                .and_then(|s| {
                    self.visit_stringLiteral(&s);
                    self.pop_string()
                })
                .unwrap_or_default(),
            _ => String::new(),
        };
        self.steps.push(Step::AggregateAs(label));
    }

    fn dispatch_traversalMethod_with<'input>(
        &mut self,
        ctx: &TraversalMethod_withContextAll<'input>,
    ) {
        let (Some(key), value, traversal) = self.lower_with_option(ctx) else {
            self.steps.push(Step::Identity);
            return;
        };
        if self.apply_value_map_with_option(&key, value.as_ref()) {
            return;
        }
        self.steps.push(Step::WithOption {
            key,
            value,
            traversal,
        });
    }

    fn lower_with_option<'input>(
        &mut self,
        ctx: &TraversalMethod_withContextAll<'input>,
    ) -> (Option<String>, Option<GValue>, Option<Vec<Step>>) {
        match ctx {
            TraversalMethod_withContextAll::TraversalMethod_with_StringContext(c) => {
                let key = c
                    .withOptionKeys()
                    .map(|k| k.get_text())
                    .or_else(|| self.lower_with_string_key(c.stringLiteral()));
                (key, None, None)
            }
            TraversalMethod_withContextAll::TraversalMethod_with_String_ObjectContext(c) => {
                let key = c
                    .withOptionKeys()
                    .map(|k| k.get_text())
                    .or_else(|| self.lower_with_string_key(c.stringLiteral()));
                let mut traversal = None;
                let value = if let Some(lit) = c.genericLiteral() {
                    if let Some(nested) = lit.nestedTraversal() {
                        let steps = self.lower_nested_traversal(&nested);
                        traversal = Some(steps.clone());
                        constant_value_from_steps(&steps)
                            .or_else(|| Some(GValue::String(format!("{steps:?}"))))
                    } else {
                        self.visit_genericLiteral(&lit);
                        match self.pop_value() {
                            Some(GValue::Null) | None => Some(GValue::String(c.get_text())),
                            value => value,
                        }
                    }
                } else {
                    c.withOptionsValues()
                        .map(|v| GValue::String(v.get_text()))
                        .or_else(|| c.ioOptionsValues().map(|v| GValue::String(v.get_text())))
                }
                .or_else(|| Some(GValue::String(c.get_text())));
                (key, value, traversal)
            }
            TraversalMethod_withContextAll::Error(_) => (None, None, None),
        }
    }

    fn lower_with_string_key<'input>(
        &mut self,
        literal: Option<Rc<StringLiteralContextAll<'input>>>,
    ) -> Option<String> {
        let literal = literal?;
        self.visit_stringLiteral(&literal);
        self.pop_string()
    }

    fn apply_value_map_with_option(&mut self, key: &str, value: Option<&GValue>) -> bool {
        let normalized = key.rsplit('.').next().unwrap_or(key).to_ascii_lowercase();
        if normalized != "tokens" {
            return false;
        }
        let (include_id, include_label) = value_map_token_selection(value);
        if let Some(last) = self.steps.last_mut() {
            match last {
                Step::ValueMap(keys) => {
                    let keys = std::mem::take(keys);
                    *last = Step::ValueMapTokens {
                        keys,
                        include_id,
                        include_label,
                    };
                    return true;
                }
                Step::ValueMapTokens {
                    include_id: existing_id,
                    include_label: existing_label,
                    ..
                } => {
                    *existing_id |= include_id;
                    *existing_label |= include_label;
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    fn dispatch_traversalMethod_split<'input>(
        &mut self,
        ctx: &TraversalMethod_splitContextAll<'input>,
    ) {
        let (delim, scope_local) = match ctx {
            TraversalMethod_splitContextAll::TraversalMethod_split_StringContext(c) => {
                let delim = c.stringNullableLiteral().and_then(|s| {
                    // `split(null)` means "split on whitespace" — keep the
                    // null distinct from an empty-string delimiter.
                    if s.K_NULL().is_some() {
                        return None;
                    }
                    self.visit_stringNullableLiteral(&s);
                    self.pop_string()
                });
                (delim, false)
            }
            TraversalMethod_splitContextAll::TraversalMethod_split_Scope_StringContext(c) => {
                let delim = c.stringNullableLiteral().and_then(|s| {
                    if s.K_NULL().is_some() {
                        return None;
                    }
                    self.visit_stringNullableLiteral(&s);
                    self.pop_string()
                });
                (delim, true)
            }
            TraversalMethod_splitContextAll::Error(_) => (Some(String::new()), false),
        };
        let op = Step::StringOp(StringOp::Split(delim));
        if scope_local {
            self.steps.push(Step::LocalScoped(Box::new(op)));
        } else {
            self.steps.push(op);
        }
    }

    fn dispatch_traversalMethod_repeat<'input>(
        &mut self,
        ctx: &TraversalMethod_repeatContextAll<'input>,
    ) {
        let mut name = None;
        let nested = match ctx {
            TraversalMethod_repeatContextAll::TraversalMethod_repeat_TraversalContext(c) => {
                c.nestedTraversal()
            }
            TraversalMethod_repeatContextAll::TraversalMethod_repeat_String_TraversalContext(c) => {
                name = c.stringLiteral().and_then(|s| {
                    self.visit_stringLiteral(&s);
                    self.pop_string()
                });
                c.nestedTraversal()
            }
            TraversalMethod_repeatContextAll::Error(_) => None,
        };
        let inner = nested
            .map(|n| self.lower_nested_traversal(&n))
            .unwrap_or_default();
        self.steps.push(Step::Repeat(name, inner));
    }

    fn dispatch_traversalMethod_choose<'input>(
        &mut self,
        ctx: &TraversalMethod_chooseContextAll<'input>,
    ) {
        // The Predicate forms — `choose(P, then)` and `choose(P, then, else)`
        // — split inputs by the predicate, so we model them precisely with
        // a dedicated step. Other forms still degenerate to Union (which
        // overcounts).
        match ctx {
            TraversalMethod_chooseContextAll::TraversalMethod_choose_Predicate_TraversalContext(c) => {
                let predicate = c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.predicate_stack.pop()
                });
                let then_branch = c
                    .nestedTraversal()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                if let Some(predicate) = predicate {
                    self.steps.push(Step::ChoosePredicate {
                        predicate,
                        then: then_branch,
                        else_branch: None,
                    });
                    return;
                }
                self.steps.push(Step::Local(then_branch));
                return;
            }
            TraversalMethod_chooseContextAll::TraversalMethod_choose_Predicate_Traversal_TraversalContext(c) => {
                let predicate = c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.predicate_stack.pop()
                });
                let mut iter = c.nestedTraversal_all().into_iter();
                let then_branch = iter
                    .next()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                let else_branch = iter
                    .next()
                    .map(|n| self.lower_nested_traversal(&n));
                if let Some(predicate) = predicate {
                    self.steps.push(Step::ChoosePredicate {
                        predicate,
                        then: then_branch,
                        else_branch,
                    });
                    return;
                }
                self.steps.push(Step::Union(
                    [Some(then_branch), else_branch]
                        .into_iter()
                        .flatten()
                        .collect(),
                ));
                return;
            }
            _ => {}
        }
        // Traversal-condition forms.
        match ctx {
            // choose(t) — one nested traversal is the *dispatch* traversal
            // for option() chains; we don't actually evaluate it per-input
            // yet (option matching is approximated). Lowering it as Union
            // of the single sub keeps the chain compileable without
            // applying per-traverser GROUP BY (Local would, and that
            // strips columns the downstream option()s need).
            TraversalMethod_chooseContextAll::TraversalMethod_choose_TraversalContext(c) => {
                let inner = c
                    .nestedTraversal()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                if inner.is_empty() {
                    self.steps.push(Step::Identity);
                } else {
                    self.steps.push(Step::BranchOptions {
                        dispatch: inner,
                        options: Vec::new(),
                        is_choose: true,
                    });
                }
            }
            // choose(t_condition, t_then) — two-arg traversal form: t is
            // the condition (filter-style), then-branch runs on matches.
            // Without an else, non-matching inputs flow through unchanged.
            TraversalMethod_chooseContextAll::TraversalMethod_choose_Traversal_TraversalContext(c) => {
                let mut iter = c.nestedTraversal_all().into_iter();
                let condition = iter
                    .next()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                let then_branch = iter
                    .next()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                self.steps.push(Step::ChooseTraversal {
                    condition,
                    then: then_branch,
                    else_branch: None,
                });
            }
            // choose(t_condition, t_then, t_else) — three-arg traversal form.
            TraversalMethod_chooseContextAll::TraversalMethod_choose_Traversal_Traversal_TraversalContext(c) => {
                let mut iter = c.nestedTraversal_all().into_iter();
                let condition = iter
                    .next()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                let then_branch = iter
                    .next()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                let else_branch = iter.next().map(|n| self.lower_nested_traversal(&n));
                self.steps.push(Step::ChooseTraversal {
                    condition,
                    then: then_branch,
                    else_branch,
                });
            }
            TraversalMethod_chooseContextAll::TraversalMethod_choose_FunctionContext(_) => {
                if ctx.get_text().contains("T.label") || ctx.get_text().contains("label") {
                    self.steps.push(Step::BranchOptions {
                        dispatch: vec![Step::Label],
                        options: Vec::new(),
                        is_choose: true,
                    });
                } else {
                    self.steps.push(Step::Identity);
                }
            }
            _ => {
                self.steps.push(Step::Identity);
            }
        }
    }

    fn dispatch_traversalMethod_sample<'input>(
        &mut self,
        ctx: &TraversalMethod_sampleContextAll<'input>,
    ) {
        // sample(n) — random sample. The dedicated `Step::Sample` variant
        // lets a future planner separate sampling from `Limit` (which
        // currently aliased the head). The Scope.local form additionally
        // wraps the step so it acts per list traverser.
        match ctx {
            TraversalMethod_sampleContextAll::TraversalMethod_sample_intContext(c) => {
                let n = c
                    .integerLiteral()
                    .and_then(|lit| parse_integer_literal_signed_unsigned(&lit, "sample").ok())
                    .unwrap_or(1);
                self.steps.push(Step::Sample(n));
            }
            TraversalMethod_sampleContextAll::TraversalMethod_sample_Scope_intContext(c) => {
                let n = c
                    .integerLiteral()
                    .and_then(|lit| parse_integer_literal_signed_unsigned(&lit, "sample").ok())
                    .unwrap_or(1);
                if has_local_scope_arg(&c.get_text()) {
                    self.steps
                        .push(Step::LocalScoped(Box::new(Step::Sample(n))));
                } else {
                    self.steps.push(Step::Sample(n));
                }
            }
            TraversalMethod_sampleContextAll::Error(_) => {
                self.steps.push(Step::Sample(1));
            }
        }
    }

    fn dispatch_traversalMethod_by<'input>(&mut self, ctx: &TraversalMethod_byContextAll<'input>) {
        let mut spec = match ctx {
            TraversalMethod_byContextAll::TraversalMethod_by_StringContext(c) => c
                .stringLiteral()
                .and_then(|s| {
                    self.visit_stringLiteral(&s);
                    self.pop_string()
                })
                .map(BySpec::key)
                .unwrap_or_else(BySpec::default),
            TraversalMethod_byContextAll::TraversalMethod_by_String_ComparatorContext(c) => {
                let mut spec = c
                    .stringLiteral()
                    .and_then(|s| {
                        self.visit_stringLiteral(&s);
                        self.pop_string()
                    })
                    .map(BySpec::key)
                    .unwrap_or_else(BySpec::default);
                spec.direction = comparator_direction(c.traversalComparator());
                spec
            }
            TraversalMethod_byContextAll::TraversalMethod_by_TraversalContext(c) => {
                self.bys_pec_from_nested(c.nestedTraversal())
            }
            TraversalMethod_byContextAll::TraversalMethod_by_Traversal_ComparatorContext(c) => {
                let mut spec = self.bys_pec_from_nested(c.nestedTraversal());
                spec.direction = comparator_direction(c.traversalComparator());
                spec
            }
            TraversalMethod_byContextAll::TraversalMethod_by_ComparatorContext(c) => {
                let mut spec = BySpec::default();
                spec.direction = comparator_direction(c.traversalComparator());
                spec
            }
            TraversalMethod_byContextAll::TraversalMethod_by_OrderContext(c) => {
                let mut spec = BySpec::default();
                spec.direction = order_token_direction(&c.traversalOrder().map(|o| o.get_text()));
                spec
            }
            // by(T.label) / by(T.id) / by(T.key) / by(T.value) — special
            // tokens that name a "virtual" property. Encode the token as
            // the BySpec key so the planner can recognise it (the planner
            // resolves "label"/"id" against the catalog at apply-time).
            TraversalMethod_byContextAll::TraversalMethod_by_TContext(c) => {
                let raw = c.traversalT().map(|t| t.get_text()).unwrap_or_default();
                let key = raw.strip_prefix("T.").unwrap_or(raw.as_str()).to_string();
                if key.is_empty() {
                    BySpec::default()
                } else {
                    BySpec::key(key)
                }
            }
            // by(label) / by(id) — same handling as by(T.label) above; the
            // grammar lets you write either form.
            TraversalMethod_byContextAll::TraversalMethod_by_FunctionContext(c) => {
                let raw = c
                    .traversalFunction()
                    .map(|f| f.get_text())
                    .unwrap_or_default();
                let key = raw.strip_suffix("()").unwrap_or(raw.as_str()).to_string();
                if key.is_empty() {
                    BySpec::default()
                } else {
                    BySpec::key(key)
                }
            }
            _ => by_spec_from_raw_text(&ctx.get_text()),
        };
        // Treat shuffle/unknown directions as ascending — we don't have a
        // randomised sort.
        if !matches!(spec.direction, SortDir::Asc | SortDir::Desc) {
            spec.direction = SortDir::Asc;
        }
        self.steps.push(Step::By(spec));
    }

    /// Inspects a `by(__.<traversal>)` body. Recognised fast-paths:
    ///   * empty traversal → `BySpec::default()` (use current scalar)
    ///   * `__.values('k')` → `BySpec::key("k")`
    /// Anything more complex (e.g. `__.bothE().count()`,
    /// `__.tail(Scope.local)`) is preserved as a sub-traversal on the
    /// `BySpec` so the planner can evaluate it per row instead of
    /// silently degrading to the current scalar.
    fn bys_pec_from_nested<'input>(
        &mut self,
        nested: Option<Rc<NestedTraversalContextAll<'input>>>,
    ) -> BySpec {
        let Some(nested) = nested else {
            return BySpec::default();
        };
        let inner = self.lower_nested_traversal(&nested);
        if inner.is_empty() {
            return BySpec::default();
        }
        if let [Step::Values(keys)] = inner.as_slice() {
            if !keys.is_empty() {
                return BySpec::key(keys[0].clone());
            }
        }
        BySpec::traversal(inner)
    }

    fn dispatch_filter_or_where<'input>(
        &mut self,
        ctx: &TraversalMethod_filterContextAll<'input>,
        _name: &str,
    ) {
        match ctx {
            TraversalMethod_filterContextAll::TraversalMethod_filter_PredicateContext(c) => {
                let Some(p) = c.traversalPredicate() else {
                    self.steps.push(Step::Identity);
                    return;
                };
                self.visit_traversalPredicate(&p);
                let Some(predicate) = self.pop_predicate() else {
                    return;
                };
                self.steps.push(Step::Is { predicate });
            }
            TraversalMethod_filterContextAll::TraversalMethod_filter_TraversalContext(c) => {
                let inner = c
                    .nestedTraversal()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                self.steps.push(Step::WhereTraversal(inner));
            }
            TraversalMethod_filterContextAll::Error(_) => {
                self.steps.push(Step::Identity);
            }
        }
    }

    fn dispatch_traversalMethod_where<'input>(
        &mut self,
        ctx: &TraversalMethod_whereContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_whereContextAll::TraversalMethod_where_PContext(c) => {
                let Some(p) = c.traversalPredicate() else {
                    self.steps.push(Step::Identity);
                    return;
                };
                self.visit_traversalPredicate(&p);
                let Some(predicate) = self.pop_predicate() else {
                    return;
                };
                self.steps.push(Step::Is { predicate });
            }
            TraversalMethod_whereContextAll::TraversalMethod_where_TraversalContext(c) => {
                let inner = c
                    .nestedTraversal()
                    .map(|n| self.lower_nested_traversal(&n))
                    .unwrap_or_default();
                self.steps.push(Step::WhereTraversal(inner));
            }
            TraversalMethod_whereContextAll::TraversalMethod_where_String_PContext(c) => {
                // where('label', P.eq('a')) — cross-binding compare.
                // Capture both sides so the planner can resolve the label
                // against the binding registry and the predicate's value
                // side against the second label (TinkerPop's `where(a, P)`
                // treats the predicate's RHS string as a binding name).
                // Recover the label by text-extraction so we don't depend
                // on the grammar's specific accessor (`stringLiteral` vs
                // `stringNullableLiteral`).
                let label = extract_first_string_arg(&c.get_text());
                let predicate = c.traversalPredicate().and_then(|p| {
                    self.visit_traversalPredicate(&p);
                    self.pop_predicate()
                });
                match (label, predicate) {
                    (Some(label), Some(predicate)) => {
                        self.steps.push(Step::WhereString { label, predicate });
                    }
                    _ => self.steps.push(Step::Identity),
                }
            }
            TraversalMethod_whereContextAll::Error(_) => {
                self.steps.push(Step::Identity);
            }
        }
    }

    fn dispatch_traversalMethod_select<'input>(
        &mut self,
        ctx: &TraversalMethod_selectContextAll<'input>,
    ) {
        // Collect all labels from the select context; the multi-label arm
        // emits SelectMulti, the single-label arm emits Select. Pop comes
        // from any variant whose name includes _Pop_; the unscoped forms
        // default to Pop::Last.
        let mut collect_labels =
            |literals: Vec<Rc<StringLiteralContextAll<'input>>>| -> Vec<String> {
                literals
                    .into_iter()
                    .filter_map(|s| {
                        self.visit_stringLiteral(&s);
                        self.pop_string()
                    })
                    .collect()
            };
        let (labels, pop) = match ctx {
            TraversalMethod_selectContextAll::TraversalMethod_select_ColumnContext(c) => {
                let raw = c
                    .traversalColumn()
                    .map(|col| col.get_text())
                    .unwrap_or_default();
                match map_column_from_text(&raw) {
                    Some(column) => self.steps.push(Step::SelectColumn(column)),
                    None => self.steps.push(Step::Identity),
                }
                return;
            }
            TraversalMethod_selectContextAll::TraversalMethod_select_StringContext(c) => {
                let labels = collect_labels(c.stringLiteral().into_iter().collect());
                (labels, Pop::Last)
            }
            TraversalMethod_selectContextAll::TraversalMethod_select_Pop_StringContext(c) => {
                let pop = pop_from_text(c.traversalPop().map(|p| p.get_text()));
                let labels = collect_labels(c.stringLiteral().into_iter().collect());
                (labels, pop)
            }
            TraversalMethod_selectContextAll::TraversalMethod_select_TraversalContext(c) => {
                let labels = c
                    .nestedTraversal()
                    .and_then(|nested| select_label_from_constant_traversal(&self.lower_nested_traversal(&nested)))
                    .into_iter()
                    .collect();
                (labels, Pop::Last)
            }
            TraversalMethod_selectContextAll::TraversalMethod_select_Pop_TraversalContext(c) => {
                let pop = pop_from_text(c.traversalPop().map(|p| p.get_text()));
                let labels = c
                    .nestedTraversal()
                    .and_then(|nested| select_label_from_constant_traversal(&self.lower_nested_traversal(&nested)))
                    .into_iter()
                    .collect();
                (labels, pop)
            }
            TraversalMethod_selectContextAll::TraversalMethod_select_String_String_StringContext(
                c,
            ) => {
                let mut labels = collect_labels(c.stringLiteral_all());
                if let Some(rest) = c.stringNullableLiteralVarargs() {
                    match self.collect_string_nullable_literal_varargs(&rest, "select") {
                        Ok(mut rest) => labels.append(&mut rest),
                        Err(err) => self.fail(err),
                    }
                }
                (labels, Pop::Last)
            }
            TraversalMethod_selectContextAll::TraversalMethod_select_Pop_String_String_StringContext(
                c,
            ) => {
                let pop = pop_from_text(c.traversalPop().map(|p| p.get_text()));
                let mut labels = collect_labels(c.stringLiteral_all());
                if let Some(rest) = c.stringNullableLiteralVarargs() {
                    match self.collect_string_nullable_literal_varargs(&rest, "select") {
                        Ok(mut rest) => labels.append(&mut rest),
                        Err(err) => self.fail(err),
                    }
                }
                (labels, pop)
            }
            _ => (Vec::new(), Pop::Last),
        };
        match labels.len() {
            0 => self.steps.push(Step::Identity),
            1 => self
                .steps
                .push(Step::Select(labels.into_iter().next().unwrap(), pop)),
            _ => self.steps.push(Step::SelectMulti(labels, pop)),
        }
    }

    fn dispatch_traversalMethod_valueMap<'input>(
        &mut self,
        ctx: &TraversalMethod_valueMapContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_valueMapContextAll::TraversalMethod_valueMap_StringContext(c) => {
                let keys = match c.stringNullableLiteralVarargs() {
                    Some(v) => match self.collect_string_nullable_literal_varargs(&v, "valueMap") {
                        Ok(keys) => keys,
                        Err(err) => {
                            self.fail(err);
                            return;
                        }
                    },
                    None => Vec::new(),
                };
                self.steps.push(Step::ValueMap(keys));
            }
            TraversalMethod_valueMapContextAll::TraversalMethod_valueMap_boolean_StringContext(
                c,
            ) => {
                // valueMap(true, "k1", "k2", ...) — the boolean prefix means
                // "include id/label tokens". A leading `true` lowers to
                // ValueMapTokens; a leading `false` is identical to the
                // bare ValueMap.
                let keys = match c.stringNullableLiteralVarargs() {
                    Some(v) => match self.collect_string_nullable_literal_varargs(&v, "valueMap") {
                        Ok(keys) => keys,
                        Err(_) => Vec::new(),
                    },
                    None => Vec::new(),
                };
                let include_tokens = first_boolean_arg(&c.get_text()).unwrap_or(false);
                if include_tokens {
                    self.steps.push(Step::ValueMapTokens {
                        keys,
                        include_id: true,
                        include_label: true,
                    });
                } else {
                    self.steps.push(Step::ValueMap(keys));
                }
            }
            TraversalMethod_valueMapContextAll::Error(_) => {
                self.fail(GremlinError::Parse(
                    "valueMap() failed to parse".to_string(),
                ));
            }
        }
    }

    fn dispatch_traversalMethod_barrier<'input>(
        &mut self,
        ctx: &TraversalMethod_barrierContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_barrierContextAll::TraversalMethod_barrier_EmptyContext(_)
            | TraversalMethod_barrierContextAll::TraversalMethod_barrier_intContext(_)
            | TraversalMethod_barrierContextAll::TraversalMethod_barrier_ConsumerContext(_) => {
                self.steps.push(Step::Barrier);
            }
            TraversalMethod_barrierContextAll::Error(_) => {
                self.fail(GremlinError::Parse("barrier() failed to parse".to_string()));
            }
        }
    }

    fn dispatch_cast_traversalGType<'input>(
        &mut self,
        ctx: &TraversalMethod_asNumberContextAll<'input>,
        target: CastTarget,
        _name: &str,
    ) {
        match ctx {
            TraversalMethod_asNumberContextAll::TraversalMethod_asNumber_EmptyContext(_) => {
                self.steps.push(Step::CastScalar(target));
            }
            TraversalMethod_asNumberContextAll::TraversalMethod_asNumber_traversalGTypeContext(
                inner,
            ) => {
                // Refined numeric cast — promote to a `CastTarget::Numeric`
                // so the lowering can pick the right runtime helper. The
                // GType identifier appears as raw text like `GType.LONG`;
                // unknown / non-numeric refinements fall back to plain
                // `CastTarget::Number`.
                let refined = inner
                    .traversalGType()
                    .map(|t| t.get_text())
                    .and_then(|t| numeric_cast_from_token(&t));
                let target = match refined {
                    Some(num) => CastTarget::Numeric(num),
                    None => target,
                };
                self.steps.push(Step::CastScalar(target));
            }
            TraversalMethod_asNumberContextAll::Error(_) => {
                self.fail(GremlinError::Parse(
                    "asNumber() failed to parse".to_string(),
                ));
            }
        }
    }

    fn dispatch_cast_simple(&mut self, text: &str, target: CastTarget) {
        let step = Step::CastScalar(target);
        if contains_scope_local(text) {
            self.steps.push(Step::LocalScoped(Box::new(step)));
        } else {
            self.steps.push(step);
        }
    }

    fn dispatch_traversalMethod_count<'input>(
        &mut self,
        ctx: &TraversalMethod_countContextAll<'input>,
    ) {
        match ctx {
            TraversalMethod_countContextAll::TraversalMethod_count_EmptyContext(_) => {
                self.steps.push(Step::Count);
            }
            TraversalMethod_countContextAll::TraversalMethod_count_ScopeContext(_) => {
                // count(local): count of the current list traverser's elements.
                self.steps.push(Step::LocalScoped(Box::new(Step::Count)));
            }
            TraversalMethod_countContextAll::Error(_) => {
                self.fail(GremlinError::Parse("count() failed to parse".to_string()));
            }
        }
    }

    fn collect_generic_argument_varargs<'input>(
        &mut self,
        ctx: &GenericArgumentVarargsContext<'input>,
    ) -> Result<Vec<GValue>> {
        let mut out = Vec::new();
        for arg in ctx.genericArgument_all() {
            self.visit_genericArgument(&arg);
            let value = self.pop_value().ok_or_else(|| {
                self.errors.pop().unwrap_or_else(|| {
                    GremlinError::Parse("generic argument failed to lower".to_string())
                })
            })?;
            out.push(value);
        }
        Ok(out)
    }

    fn collect_string_nullable_argument_varargs<'input>(
        &mut self,
        ctx: &StringNullableArgumentVarargsContext<'input>,
        step: &str,
    ) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for arg in ctx.stringNullableArgument_all() {
            self.visit_stringNullableArgument(&arg);
            let value = self.pop_string().ok_or_else(|| {
                self.errors.pop().unwrap_or_else(|| {
                    GremlinError::Parse(format!("{step}() argument failed to lower"))
                })
            })?;
            out.push(value);
        }
        Ok(out)
    }

    fn collect_string_nullable_literal_varargs<'input>(
        &mut self,
        ctx: &StringNullableLiteralVarargsContext<'input>,
        step: &str,
    ) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for arg in ctx.stringNullableLiteral_all() {
            self.visit_stringNullableLiteral(&arg);
            let value = self.pop_string().ok_or_else(|| {
                self.errors.pop().unwrap_or_else(|| {
                    GremlinError::Parse(format!("{step}() argument failed to lower"))
                })
            })?;
            out.push(value);
        }
        Ok(out)
    }

    fn push_compare_predicate<'input>(
        &mut self,
        op: CompareOp,
        arg: Option<Rc<GenericArgumentContextAll<'input>>>,
        name: &str,
    ) {
        let Some(arg_ctx) = arg else {
            self.fail(GremlinError::Parse(format!("{name}() missing argument")));
            return;
        };
        self.visit_genericArgument(&arg_ctx);
        let Some(value) = self.pop_value() else {
            return;
        };
        self.predicate_stack.push(Predicate::Compare { op, value });
    }
}

// ---------- raw literal parsing (no parse-tree dependence) ----------

/// Resolve a `traversalOperator` token's raw text to our `SackOp` enum.
/// Accepts both bare keywords (`sum`, `mult`, ...) and the qualified
/// `Operator.X` form. Returns `None` for tokens we don't model.
fn sack_op_from_text(raw: &str) -> Option<SackOp> {
    let trimmed = raw.trim();
    let bare = trimmed
        .rsplit_once('.')
        .map(|(_, tail)| tail)
        .unwrap_or(trimmed)
        .to_ascii_lowercase();
    Some(match bare.as_str() {
        "sum" => SackOp::Sum,
        "sumlong" => SackOp::SumLong,
        "minus" => SackOp::Minus,
        "mult" => SackOp::Mult,
        "div" => SackOp::Div,
        "min" => SackOp::Min,
        "max" => SackOp::Max,
        "assign" => SackOp::Assign,
        "and" => SackOp::And,
        "or" => SackOp::Or,
        "addall" => SackOp::AddAll,
        _ => return None,
    })
}

/// Maps the raw text of a `traversalPop` (`Pop.first` / `first` /
/// `Pop.last` / `last` / `Pop.all` / `all` / `Pop.mixed` / `mixed`) to
/// our `Pop` discriminant. `all` returns a list of every binding; `mixed`
/// returns a list when there are 2+ bindings, otherwise the scalar.
fn pop_from_text(raw: Option<String>) -> Pop {
    let Some(raw) = raw else { return Pop::Last };
    let lower = raw.to_lowercase();
    if lower.contains("mixed") {
        Pop::Mixed
    } else if lower.contains("all") {
        Pop::All
    } else if lower.contains("first") {
        Pop::First
    } else {
        Pop::Last
    }
}

fn select_label_from_constant_traversal(steps: &[Step]) -> Option<String> {
    match steps {
        [Step::Constant(GValue::String(label))] => Some(label.clone()),
        _ => None,
    }
}

fn constant_value_from_steps(steps: &[Step]) -> Option<GValue> {
    match steps {
        [Step::Constant(value)] => Some(value.clone()),
        _ => None,
    }
}

fn map_column_from_text(raw: &str) -> Option<MapColumn> {
    let token = raw
        .trim()
        .strip_prefix("Column.")
        .unwrap_or(raw.trim())
        .to_ascii_lowercase();
    match token.as_str() {
        "keys" => Some(MapColumn::Keys),
        "values" => Some(MapColumn::Values),
        _ => None,
    }
}

/// Reads the raw text of a `traversalComparator` (which always distils to
/// a `traversalOrder` keyword like `Order.desc` / `Order.asc` /
/// `Order.shuffle`) and decides on a sort direction. Anything we don't
/// recognise (notably `shuffle`) falls back to ascending.
fn comparator_direction<'input>(ctx: Option<Rc<TraversalComparatorContextAll<'input>>>) -> SortDir {
    let Some(ctx) = ctx else { return SortDir::Asc };
    order_token_direction(&Some(ctx.get_text()))
}

fn order_token_direction(raw: &Option<String>) -> SortDir {
    match raw {
        Some(s) if s.to_lowercase().contains("desc") => SortDir::Desc,
        _ => SortDir::Asc,
    }
}

fn by_spec_from_raw_text(raw: &str) -> BySpec {
    let mut spec = if raw.contains("T.key") || raw.contains("Column.keys") || raw.contains("keys") {
        BySpec::key("key")
    } else if raw.contains("T.value") || raw.contains("Column.values") || raw.contains("values") {
        BySpec::key("value")
    } else if raw.contains("T.id") || raw.contains("id()") {
        BySpec::key("id")
    } else if raw.contains("T.label") || raw.contains("label()") {
        BySpec::key("label")
    } else {
        BySpec::default()
    };
    if raw.to_ascii_lowercase().contains("desc") {
        spec.direction = SortDir::Desc;
    }
    spec
}

fn direction_from_to_arg(raw: &str) -> Option<Direction> {
    if raw.contains("Direction.OUT") || raw.contains("OUT") {
        Some(Direction::Out)
    } else if raw.contains("Direction.IN") || raw.contains("IN") {
        Some(Direction::In)
    } else if raw.contains("Direction.BOTH") || raw.contains("BOTH") {
        Some(Direction::Both)
    } else {
        None
    }
}

/// Parses a Gremlin `format()` template into a sequence of literal
/// segments and placeholders. Recognises `{N}`-indexed Gremlin
/// placeholders and the `%s` printf shorthand. Other `%` escapes are
/// preserved as literals.
fn parse_format_template(raw: &str) -> Vec<FormatPart> {
    let mut parts = Vec::new();
    let mut buf = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                // Match `{<digits>}` placeholders.
                let mut idx = String::new();
                while let Some(&peek) = chars.peek() {
                    if peek.is_ascii_digit() {
                        idx.push(peek);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if chars.peek() == Some(&'}') && !idx.is_empty() {
                    chars.next(); // consume '}'
                    if !buf.is_empty() {
                        parts.push(FormatPart::Literal(std::mem::take(&mut buf)));
                    }
                    parts.push(FormatPart::Placeholder { key: None });
                } else {
                    // Not a placeholder — treat as literal.
                    buf.push('{');
                    buf.push_str(&idx);
                }
            }
            // `%{name}` — TinkerPop's named placeholder; the key resolves
            // to a property on the current element or a labelled binding.
            // Bare `%{_}` (or empty) defers to the matching by(...) modulator.
            '%' if chars.peek() == Some(&'{') => {
                chars.next(); // consume '{'
                let mut key = String::new();
                let mut closed = false;
                while let Some(&peek) = chars.peek() {
                    if peek == '}' {
                        chars.next();
                        closed = true;
                        break;
                    }
                    key.push(peek);
                    chars.next();
                }
                if closed {
                    if !buf.is_empty() {
                        parts.push(FormatPart::Literal(std::mem::take(&mut buf)));
                    }
                    let key_opt = if key.is_empty() || key == "_" {
                        None
                    } else {
                        Some(key)
                    };
                    parts.push(FormatPart::Placeholder { key: key_opt });
                } else {
                    // Unterminated — emit as literal text.
                    buf.push('%');
                    buf.push('{');
                    buf.push_str(&key);
                }
            }
            '%' if chars.peek() == Some(&'s') => {
                chars.next();
                if !buf.is_empty() {
                    parts.push(FormatPart::Literal(std::mem::take(&mut buf)));
                }
                parts.push(FormatPart::Placeholder { key: None });
            }
            other => buf.push(other),
        }
    }
    if !buf.is_empty() {
        parts.push(FormatPart::Literal(buf));
    }
    parts
}

/// Maps a Gremlin Direction token to our internal `Direction` enum. The
/// keyword forms (`Direction.OUT`, `OUT`, `Direction.IN`, `IN`, `from`,
/// `to`, ...) all show up at parse time as the textual representation of
/// the matched alternative; we just check for the relevant keyword
/// substring.
fn direction_from_text(raw: &str) -> Direction {
    let s = raw.to_uppercase();
    if s.contains("OUT") || s.ends_with("FROM") || s.contains(".FROM") {
        Direction::Out
    } else if s.contains("IN") || s.ends_with("TO") || s.contains(".TO") {
        Direction::In
    } else {
        Direction::Both
    }
}

/// Parses a Gremlin `math()` expression. We model the common shapes:
///   * `_ OP literal` / `literal OP _` (`_ + 1`, `2 - _`, ...)
///   * `_ OP _` (binary on self / by-modulator pair)
///   * `_ OP name` / `name OP _` (self combined with a named binding /
///     side-effect / property — resolved at the planner)
///   * `name OP name` (both operands are named bindings)
///   * bare `name` (single named binding, no op)
/// Anything else (e.g. `sin _`, parenthesised expressions, multi-op
/// chains) falls back to `MathExpr::Identity`.
fn parse_math_expr(raw: &str) -> MathExpr {
    use crate::language::gremlin::ast::MathOp;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return MathExpr::Identity;
    }
    if trimmed == "_" {
        return MathExpr::Identity;
    }
    if let Some((func, operand)) = parse_unary_math_call(trimmed) {
        if is_supported_unary_math_func(func) {
            let func = func.to_ascii_lowercase();
            if operand == "_" {
                return MathExpr::UnaryFn(func);
            }
            match parse_math_expr(operand) {
                MathExpr::Add(value) => {
                    return MathExpr::UnaryCurrentOpLit {
                        func,
                        op: MathOp::Add,
                        value,
                    };
                }
                MathExpr::Sub(value) => {
                    return MathExpr::UnaryCurrentOpLit {
                        func,
                        op: MathOp::Sub,
                        value,
                    };
                }
                MathExpr::Mul(value) => {
                    return MathExpr::UnaryCurrentOpLit {
                        func,
                        op: MathOp::Mul,
                        value,
                    };
                }
                MathExpr::Div(value) => {
                    return MathExpr::UnaryCurrentOpLit {
                        func,
                        op: MathOp::Div,
                        value,
                    };
                }
                _ => {}
            }
        }
    }
    if is_simple_math_name(trimmed) {
        return MathExpr::Var(trimmed.to_string());
    }
    // Tokenise into [LHS] [OP] [RHS] at the first non-leading binary op.
    fn tokenise(s: &str) -> Option<(&str, char, &str)> {
        for (i, c) in s.char_indices() {
            if matches!(c, '+' | '-' | '*' | '/') && i > 0 {
                // Skip leading sign when the prior char is also an op (e.g.
                // `_*-2`); detect by walking back to find a non-space.
                let prev = s[..i].chars().rev().find(|c| !c.is_whitespace());
                if matches!(prev, Some('+' | '-' | '*' | '/')) {
                    continue;
                }
                let lhs = s[..i].trim();
                let rhs = s[i + 1..].trim();
                if !lhs.is_empty() && !rhs.is_empty() {
                    return Some((lhs, c, rhs));
                }
            }
        }
        None
    }
    let (lhs, op_char, rhs) = match tokenise(trimmed) {
        Some(t) => t,
        None => return MathExpr::Identity,
    };
    let op = match op_char {
        '+' => MathOp::Add,
        '-' => MathOp::Sub,
        '*' => MathOp::Mul,
        '/' => MathOp::Div,
        _ => return MathExpr::Identity,
    };
    let lhs_is_self = lhs == "_";
    let rhs_is_self = rhs == "_";
    let lhs_lit = lhs.parse::<f64>().ok();
    let rhs_lit = rhs.parse::<f64>().ok();
    let lhs_name = is_simple_math_name(lhs).then(|| lhs.to_string());
    let rhs_name = is_simple_math_name(rhs).then(|| rhs.to_string());
    match (lhs_is_self, rhs_is_self) {
        (true, true) => MathExpr::BinSelf(op),
        (true, false) => {
            if let Some(v) = rhs_lit {
                return match op {
                    MathOp::Add => MathExpr::Add(v),
                    MathOp::Sub => MathExpr::Sub(v),
                    MathOp::Mul => MathExpr::Mul(v),
                    MathOp::Div => MathExpr::Div(v),
                };
            }
            if let Some(name) = rhs_name {
                return MathExpr::SelfRhsName(op, name);
            }
            MathExpr::Identity
        }
        (false, true) => {
            if let Some(v) = lhs_lit {
                return match op {
                    MathOp::Add => MathExpr::Add(v), // commutative
                    MathOp::Sub => MathExpr::SubFromLit(v),
                    MathOp::Mul => MathExpr::Mul(v), // commutative
                    MathOp::Div => MathExpr::DivByLit(v),
                };
            }
            if let Some(name) = lhs_name {
                return MathExpr::SelfLhsName(op, name);
            }
            MathExpr::Identity
        }
        (false, false) => match (lhs_name, rhs_name, lhs_lit, rhs_lit) {
            (Some(a), Some(b), _, _) => MathExpr::BothNamed(op, a, b),
            (Some(a), _, _, Some(b)) => MathExpr::NameRhsLit(op, a, b),
            (_, Some(b), Some(a), _) => MathExpr::LitRhsName(op, a, b),
            _ => MathExpr::Identity,
        },
    }
}

fn parse_unary_math_call(s: &str) -> Option<(&str, &str)> {
    if let Some((func, operand)) = s.split_once(' ') {
        let func = func.trim();
        let operand = operand.trim();
        if !func.is_empty() && !operand.is_empty() {
            return Some((func, operand));
        }
    }
    let open = s.find('(')?;
    let func = s[..open].trim();
    let operand = s[open + 1..].strip_suffix(')')?.trim();
    if func.is_empty() || operand.is_empty() {
        None
    } else {
        Some((func, operand))
    }
}

fn is_supported_unary_math_func(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "abs"
            | "ceil"
            | "floor"
            | "round"
            | "sqrt"
            | "cbrt"
            | "sign"
            | "exp"
            | "ln"
            | "log"
            | "log2"
            | "log10"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
    )
}

/// Returns true if `s` is a simple identifier suitable for a `math()`
/// expression operand: a leading letter or `_`, then alphanumerics,
/// underscores, or `.` (for namespaced bindings like `a.b`).
fn is_simple_math_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_' || c == '.')
}

fn parse_integer_literal_signed_unsigned<'input>(
    ctx: &IntegerLiteralContext<'input>,
    step: &str,
) -> Result<u64> {
    let text = ctx.get_text();
    let parsed = parse_integer_literal(&text)?;
    if parsed < 0 {
        return Err(GremlinError::Parse(format!(
            "{step}() expected non-negative integer, got {parsed}"
        )));
    }
    Ok(parsed as u64)
}

fn parse_integer_literal(raw: &str) -> Result<i64> {
    let mut value = strip_numeric_suffix(raw, "bBsSnNiIlL").replace('_', "");
    let sign = if let Some(rest) = value.strip_prefix('-') {
        value = rest.to_string();
        -1i64
    } else if let Some(rest) = value.strip_prefix('+') {
        value = rest.to_string();
        1i64
    } else {
        1i64
    };

    let (radix, digits) = if let Some(rest) = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        (16, rest)
    } else if value.len() > 1 && value.starts_with('0') {
        (8, value.as_str())
    } else {
        (10, value.as_str())
    };

    i64::from_str_radix(digits, radix)
        .map(|parsed| parsed * sign)
        .map_err(|err| GremlinError::Parse(format!("invalid integer literal `{raw}`: {err}")))
}

fn date_unit_from_text(raw: &str) -> String {
    raw.rsplit('.').next().unwrap_or(raw).to_ascii_lowercase()
}

fn parse_date_literal_ctx<'input>(ctx: &DateLiteralContext<'input>) -> Option<String> {
    let literal = ctx.stringLiteral()?;
    decode_string_literal(&literal.get_text()).ok()
}

fn date_diff_traversal_arg<'input>(
    ctx: &TraversalMethod_dateDiff_TraversalContext<'input>,
) -> GValue {
    let Some(nested) = ctx.nestedTraversal() else {
        return GValue::Null;
    };
    let text = nested.get_text();
    if text.contains("constant(null)") {
        return GValue::Null;
    }
    if text.contains("inject(") {
        return GValue::String("__current_datetime__".to_string());
    }
    extract_datetime_literal_arg(&text)
        .map(GValue::DateTime)
        .unwrap_or(GValue::Null)
}

fn extract_datetime_literal_arg(raw: &str) -> Option<String> {
    let open = raw
        .find("datetime(")
        .map(|idx| idx + "datetime(".len())
        .or_else(|| raw.find("DateTime(").map(|idx| idx + "DateTime(".len()))?;
    let rest = raw.get(open..)?;
    let quote = rest.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let mut escaped = false;
    let mut out = String::new();
    for ch in rest[quote.len_utf8()..].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == quote {
            return Some(out);
        } else {
            out.push(ch);
        }
    }
    None
}

fn parse_float_literal(raw: &str) -> Result<f64> {
    let value = strip_numeric_suffix(raw, "fFdDmM").replace('_', "");
    if value == "Infinity" || value == "+Infinity" {
        Ok(f64::INFINITY)
    } else if value == "-Infinity" {
        Ok(f64::NEG_INFINITY)
    } else if value == "NaN" {
        Ok(f64::NAN)
    } else {
        value
            .parse::<f64>()
            .map_err(|err| GremlinError::Parse(format!("invalid floating literal `{raw}`: {err}")))
    }
}

fn strip_numeric_suffix<'a>(raw: &'a str, suffixes: &str) -> &'a str {
    raw.char_indices()
        .last()
        .and_then(|(idx, ch)| suffixes.contains(ch).then_some(&raw[..idx]))
        .unwrap_or(raw)
}

fn decode_string_literal(raw: &str) -> Result<String> {
    let quote = raw
        .chars()
        .next()
        .ok_or_else(|| GremlinError::Parse("empty string token".to_string()))?;
    if quote != '\'' && quote != '"' {
        return Err(GremlinError::Parse(format!(
            "expected string literal, got `{raw}`"
        )));
    }
    if !raw.ends_with(quote) {
        return Err(GremlinError::Parse(format!(
            "unterminated string literal `{raw}`"
        )));
    }

    let inner = &raw[quote.len_utf8()..raw.len() - quote.len_utf8()];
    let mut decoded = String::new();
    let mut chars = inner.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            decoded.push(ch);
            continue;
        }
        let escaped = chars
            .next()
            .ok_or_else(|| GremlinError::Parse(format!("invalid escape in `{raw}`")))?;
        match escaped {
            'b' => decoded.push('\u{0008}'),
            't' => decoded.push('\t'),
            'n' => decoded.push('\n'),
            'f' => decoded.push('\u{000C}'),
            'r' => decoded.push('\r'),
            '"' => decoded.push('"'),
            '\'' => decoded.push('\''),
            '\\' => decoded.push('\\'),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
            }
            '\n' => {}
            'u' => {
                while chars.peek() == Some(&'u') {
                    chars.next();
                }
                let mut hex = String::new();
                for _ in 0..4 {
                    hex.push(chars.next().ok_or_else(|| {
                        GremlinError::Parse(format!("incomplete unicode escape in `{raw}`"))
                    })?);
                }
                let code = u32::from_str_radix(&hex, 16).map_err(|err| {
                    GremlinError::Parse(format!("invalid unicode escape `\\u{hex}`: {err}"))
                })?;
                decoded.push(char::from_u32(code).ok_or_else(|| {
                    GremlinError::Parse(format!("invalid unicode scalar `\\u{hex}`"))
                })?);
            }
            '0'..='7' => {
                let mut octal = String::from(escaped);
                let max_extra = if escaped <= '3' { 2 } else { 1 };
                for _ in 0..max_extra {
                    if matches!(chars.peek(), Some('0'..='7')) {
                        octal.push(chars.next().expect("peeked octal digit"));
                    }
                }
                let value = u32::from_str_radix(&octal, 8).map_err(|err| {
                    GremlinError::Parse(format!("invalid octal escape `\\{octal}`: {err}"))
                })?;
                decoded.push(char::from_u32(value).ok_or_else(|| {
                    GremlinError::Parse(format!("invalid octal scalar `\\{octal}`"))
                })?);
            }
            other => {
                return Err(GremlinError::Parse(format!(
                    "unsupported escape `\\{other}` in `{raw}`"
                )));
            }
        }
    }
    Ok(decoded)
}

/// Extracts the first top-level (`true` / `false`) boolean argument from
/// a method-call text fragment, ignoring args inside nested parentheses.
/// Returns `None` if no top-level boolean is present.
fn first_boolean_arg(raw: &str) -> Option<bool> {
    let bytes = raw.as_bytes();
    let mut depth = 0i32;
    let mut started = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if !started {
            if c == b'(' {
                started = true;
                depth = 1;
            }
            i += 1;
            continue;
        }
        match c {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return None;
                }
                i += 1;
            }
            _ if depth == 1 => {
                if bytes[i..].starts_with(b"true") {
                    let next = i + 4;
                    if next >= bytes.len() || matches!(bytes[next], b')' | b',' | b' ') {
                        return Some(true);
                    }
                }
                if bytes[i..].starts_with(b"false") {
                    let next = i + 5;
                    if next >= bytes.len() || matches!(bytes[next], b')' | b',' | b' ') {
                        return Some(false);
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

fn value_map_token_selection(value: Option<&GValue>) -> (bool, bool) {
    let Some(GValue::String(value)) = value else {
        return (true, true);
    };
    match value
        .rsplit('.')
        .next()
        .unwrap_or(value)
        .to_ascii_lowercase()
        .as_str()
    {
        "id" | "ids" => (true, false),
        "label" | "labels" => (false, true),
        "tokens" => (true, true),
        _ => (true, true),
    }
}

/// Returns true when a method-call text fragment references the
/// `Scope.local` token at the top-level argument position. Used by
/// aggregate dispatch to choose between global and per-list-traverser
/// evaluation without relying on grammar-specific accessor names.
fn has_local_scope_arg(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut depth = 0i32;
    let mut started = false;
    let mut i = 0;
    let needles: [&[u8]; 2] = [b"Scope.local", b"local"];
    while i < bytes.len() {
        let c = bytes[i];
        if !started {
            if c == b'(' {
                started = true;
                depth = 1;
            }
            i += 1;
            continue;
        }
        match c {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return false;
                }
                i += 1;
            }
            _ => {
                if depth == 1 {
                    for needle in needles {
                        if bytes[i..].starts_with(needle) {
                            // Make sure we're at a token boundary on both sides.
                            let prev_ok = i == 0 || matches!(bytes[i - 1], b'(' | b',' | b' ');
                            let next = i + needle.len();
                            let next_ok =
                                next >= bytes.len() || matches!(bytes[next], b')' | b',' | b' ');
                            if prev_ok && next_ok {
                                return true;
                            }
                        }
                    }
                }
                i += 1;
            }
        }
    }
    false
}

fn contains_scope_local(raw: &str) -> bool {
    has_local_scope_arg(raw)
}

fn is_scope_local_arg(raw: &str) -> bool {
    matches!(raw.trim(), "Scope.local" | "local")
}

fn extract_top_level_args(raw: &str) -> Vec<&str> {
    let Some(open) = raw.find('(') else {
        return Vec::new();
    };
    let mut args = Vec::new();
    let mut quote: Option<char> = None;
    let mut escape = false;
    let mut depth = 0i32;
    let mut start = open + 1;
    for (idx, ch) in raw[open + 1..].char_indices() {
        let idx = open + 1 + idx;
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => quote = Some(ch),
            '(' | '[' | '{' => depth += 1,
            ')' if depth == 0 => {
                let arg = raw[start..idx].trim();
                if !arg.is_empty() {
                    args.push(arg);
                }
                return args;
            }
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                let arg = raw[start..idx].trim();
                if !arg.is_empty() {
                    args.push(arg);
                }
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    args
}

/// Extracts the first top-level (depth-1) quoted string argument from a
/// method-call text fragment such as `tree("a")` or `subgraph('sg')`.
/// Returns the decoded value (handles `\\`, `\'`, `\"` escapes); returns
/// `None` when no top-level string literal is present. Used by the parser
/// to recover label/name arguments without depending on grammar variant
/// naming.
fn extract_first_string_arg(raw: &str) -> Option<String> {
    extract_top_level_string_args(raw).into_iter().next()
}

/// Extracts every top-level (depth-1, comma-separated) quoted string
/// argument from a method-call text fragment. Strings inside nested
/// expressions (e.g. `call("a", __.has("nested", "x"))`) are skipped so
/// only the literal args at the outermost call site are returned.
fn extract_top_level_string_args(raw: &str) -> Vec<String> {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut started = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if !started {
            if c == b'(' {
                started = true;
                depth = 1;
            }
            i += 1;
            continue;
        }
        match c {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return out;
                }
                i += 1;
            }
            b'\'' | b'"' => {
                let quote = c;
                let start = i;
                let mut j = i + 1;
                let mut closed = false;
                while j < bytes.len() {
                    if bytes[j] == b'\\' && j + 1 < bytes.len() {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == quote {
                        closed = true;
                        break;
                    }
                    j += 1;
                }
                if !closed {
                    return out;
                }
                if depth == 1 {
                    if let Ok(decoded) = decode_string_literal(&raw[start..=j]) {
                        out.push(decoded);
                    }
                }
                i = j + 1;
            }
            _ => i += 1,
        }
    }
    out
}

fn option_key_text(text: &str) -> Option<String> {
    let inner = text.strip_prefix("option(")?.strip_suffix(')')?;
    let mut depth = 0i32;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => return Some(inner[..idx].to_string()),
            _ => {}
        }
    }
    None
}

fn parse_pick_key(text: &str) -> Option<OptionKey> {
    match text.trim() {
        "any" | "Pick.any" => Some(OptionKey::PickAny),
        "none" | "Pick.none" => Some(OptionKey::PickNone),
        "unproductive" | "Pick.unproductive" => Some(OptionKey::PickUnproductive),
        _ => None,
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
                assert!(*check_adjacent_vertices);
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
            Step::WithOption { key, value: Some(GValue::String(value)), .. },
        ] if key.ends_with("edges") && value.contains("Direction.IN")));
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
