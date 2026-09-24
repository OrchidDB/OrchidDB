//! Calls, repeat and branch control, options, and side effects.

use crate::grammar::generated::gremlin::gremlinvisitor::GremlinVisitor;
use antlr4rust::tree::ParseTree;

use super::literals::{
    constant_value_from_steps, direction_from_text, has_local_scope_arg,
    parse_integer_literal_signed_unsigned, value_map_token_selection,
};
use super::{CallArg, Direction, GValue, LoweringVisitor, Predicate, Rc, Step};
use crate::grammar::generated::gremlin::gremlinparser::*;
#[allow(non_snake_case)]
impl LoweringVisitor {
    pub(super) fn lower_call_name<'input>(
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

    pub(super) fn lower_call<'input>(
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

    pub(super) fn lower_source_call<'input>(
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

    pub(super) fn dispatch_traversalMethod_hasValue<'input>(
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

    pub(super) fn dispatch_traversalMethod_toV<'input>(
        &mut self,
        ctx: &TraversalMethod_toVContext<'input>,
    ) {
        let direction = ctx
            .traversalDirection()
            .map(|d| direction_from_text(&d.get_text()))
            .unwrap_or(Direction::Both);
        self.steps.push(Step::EndpointVertex { direction });
    }

    pub(super) fn dispatch_traversalMethod_toE<'input>(
        &mut self,
        ctx: &TraversalMethod_toEContext<'input>,
    ) {
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

    pub(super) fn dispatch_traversalMethod_loops<'input>(
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

    pub(super) fn dispatch_traversalMethod_emit<'input>(
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

    pub(super) fn lower_predicate_as_filter<'input>(
        &mut self,
        predicate: &TraversalPredicateContextAll<'input>,
    ) -> Option<Vec<Step>> {
        self.visit_traversalPredicate(predicate);
        self.pop_predicate()
            .map(|predicate| vec![Step::Is { predicate }])
    }

    pub(super) fn dispatch_traversalMethod_until<'input>(
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

    pub(super) fn dispatch_traversalMethod_option<'input>(
        &mut self,
        ctx: &TraversalMethod_optionContextAll<'input>,
    ) {
        if matches!(self.steps.last(), Some(Step::MergeV { .. })) {
            self.lower_merge_vertex_option(ctx);
            return;
        }
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

    pub(super) fn dispatch_traversalMethod_aggregate<'input>(
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

    pub(super) fn dispatch_traversalMethod_with<'input>(
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

    pub(super) fn lower_with_option<'input>(
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

    pub(super) fn lower_with_string_key<'input>(
        &mut self,
        literal: Option<Rc<StringLiteralContextAll<'input>>>,
    ) -> Option<String> {
        let literal = literal?;
        self.visit_stringLiteral(&literal);
        self.pop_string()
    }

    pub(super) fn apply_value_map_with_option(
        &mut self,
        key: &str,
        value: Option<&GValue>,
    ) -> bool {
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

    pub(super) fn dispatch_traversalMethod_repeat<'input>(
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

    pub(super) fn dispatch_traversalMethod_choose<'input>(
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

    pub(super) fn dispatch_traversalMethod_sample<'input>(
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
}
