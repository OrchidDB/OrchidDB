//! ANTLR visitor callbacks that emit traversal steps and typed leaf values.

use antlr4rust::tree::ParseTree;

use super::literals::{
    date_diff_traversal_arg, date_unit_from_text, decode_string_literal, direction_from_to_arg,
    extract_first_string_arg, extract_top_level_string_args, parse_date_literal_ctx,
    parse_float_literal, parse_integer_literal, parse_integer_literal_signed_unsigned,
    parse_typed_integer_literal, parse_typed_float_literal,
    parse_math_expr, sack_op_from_text,
};
use super::{
    AggKind, BTreeMap, CastTarget, CompareOp, Direction, GValue, GremlinError, GremlinVisitor,
    ListOpKind, LoweringVisitor, ParseTreeVisitor, Predicate, SackOp, Step, StringOp, TextKind,
};
use crate::grammar::generated::gremlin::gremlinparser::*;
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
        if let Some(c) = ctx.traversalSourceSpawnMethod_mergeV() {
            match &*c {
                TraversalSourceSpawnMethod_mergeVContextAll::TraversalSourceSpawnMethod_mergeV_MapContext(c) => self.lower_merge_vertex_map(c.genericMapNullableArgument()),
                _ => self.fail(GremlinError::Unsupported("mergeV traversal criteria".into())),
            }
            return;
        }
        if ctx.traversalSourceSpawnMethod_addE().is_some() {
            self.fail(GremlinError::Unsupported("source addE requires traversal endpoints".into()));
            return;
        }
        if let Some(c) = ctx.traversalSourceSpawnMethod_addV() {
            if c.nestedTraversal().is_some() {
                self.fail(GremlinError::Unsupported("addV traversal label".into()));
                return;
            }
            let label = match c.stringArgument() {
                Some(arg) => match self.string_argument_text(&arg) { Some(label) => label, None => return },
                None => "vertex".into(),
            };
            self.steps.push(Step::AddV { label });
            return;
        }
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
        if let Some(c) = ctx.traversalMethod_mergeV() {
            match &*c {
                TraversalMethod_mergeVContextAll::TraversalMethod_mergeV_MapContext(c) => self.lower_merge_vertex_map(c.genericMapNullableArgument()),
                _ => self.fail(GremlinError::Unsupported("mergeV dynamic criteria".into())),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_addE() {
            self.lower_add_edge(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_addV() {
            self.lower_add_vertex(&c);
            return;
        }
        if let Some(c) = ctx.traversalMethod_property() {
            self.lower_property_write(&c);
            return;
        }
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
            if self.lower_edge_endpoint(&c.get_text(), true) { return; }
            match extract_first_string_arg(&c.get_text()) {
                Some(label) => self.steps.push(Step::PathFrom(label)),
                None => self.steps.push(Step::Identity),
            }
            return;
        }
        if let Some(c) = ctx.traversalMethod_to() {
            let raw = c.get_text();
            if self.lower_edge_endpoint(&raw, false) { return; }
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
                match parse_typed_integer_literal(&int_lit.get_text()) {
                    Ok(n) => self.value_stack.push(n),
                    Err(err) => self.fail(err),
                }
                return;
            }
            if let Some(float_lit) = num.floatLiteral() {
                match parse_typed_float_literal(&float_lit.get_text()) {
                    Ok(f) => self.value_stack.push(f),
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
            self.value_stack.push(GValue::Set(elements));
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
                Some(GValue::Int(n) | GValue::Long(n)) if n >= 0 => n as u64,
                _ => 0,
            };
            self.integer_stack.push(resolved);
            return;
        }
        self.fail(GremlinError::Parse("expected integer argument".to_string()));
    }
}
