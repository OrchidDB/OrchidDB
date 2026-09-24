//! Ordering, slicing, string, and collection step lowering.

use crate::grammar::generated::gremlin::gremlinvisitor::GremlinVisitor;
use antlr4rust::tree::ParseTree;

use super::literals::{
    extract_top_level_args, extract_top_level_string_args, has_local_scope_arg, is_scope_local_arg,
    parse_format_template, parse_integer_literal, sack_op_from_text,
};
use super::{GValue, GremlinError, ListOpKind, LoweringVisitor, Rc, SackOp, Step, StringOp};
use crate::grammar::generated::gremlin::gremlinparser::*;
#[allow(non_snake_case)]
impl LoweringVisitor {
    pub(super) fn dispatch_traversalMethod_dedup<'input>(
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

    pub(super) fn dispatch_traversalMethod_order<'input>(
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

    pub(super) fn dispatch_traversalMethod_range<'input>(
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

    pub(super) fn dispatch_traversalMethod_skip<'input>(
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

    pub(super) fn dispatch_traversalMethod_tail<'input>(
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

    pub(super) fn dispatch_simple_string_op(&mut self, text: &str, op: StringOp) {
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

    pub(super) fn dispatch_traversalMethod_substring<'input>(
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

    pub(super) fn dispatch_traversalMethod_replace<'input>(
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

    pub(super) fn dispatch_traversalMethod_concat<'input>(
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

    pub(super) fn dispatch_traversalMethod_all<'input>(
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

    pub(super) fn dispatch_traversalMethod_any<'input>(
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

    pub(super) fn dispatch_traversalMethod_none<'input>(
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

    pub(super) fn dispatch_traversalMethod_format<'input>(
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

    pub(super) fn dispatch_traversalMethod_conjoin<'input>(
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

    pub(super) fn dispatch_traversalMethod_fold<'input>(
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

    pub(super) fn dispatch_list_op<'input>(
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

    pub(super) fn dispatch_traversalMethod_split<'input>(
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
}
