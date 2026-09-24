//! Projection modulators, selection, casts, and argument collection.

use crate::grammar::generated::gremlin::gremlinvisitor::GremlinVisitor;
use antlr4rust::tree::ParseTree;

use super::literals::{
    by_spec_from_raw_text, comparator_direction, contains_scope_local, first_boolean_arg,
    map_column_from_text, numeric_cast_from_token, order_token_direction, pop_from_text,
    select_label_from_constant_traversal,
};
use super::{
    BySpec, CastTarget, GValue, GremlinError, LoweringVisitor, Pop, Rc, Result, SortDir, Step,
};
use crate::grammar::generated::gremlin::gremlinparser::*;
#[allow(non_snake_case)]
impl LoweringVisitor {
    pub(super) fn dispatch_traversalMethod_by<'input>(
        &mut self,
        ctx: &TraversalMethod_byContextAll<'input>,
    ) {
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
    ///   * `__.values('k')` retains traversal identity (group reducers distinguish it from by('k')).
    /// Anything more complex (e.g. `__.bothE().count()`,
    /// `__.tail(Scope.local)`) is preserved as a sub-traversal on the
    /// `BySpec` so the planner can evaluate it per row instead of
    /// silently degrading to the current scalar.
    pub(super) fn bys_pec_from_nested<'input>(
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
        BySpec::traversal(inner)
    }

    pub(super) fn dispatch_traversalMethod_select<'input>(
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
                let inner = c
                    .nestedTraversal()
                    .map(|nested| self.lower_nested_traversal(&nested))
                    .unwrap_or_default();
                if let [Step::Select(label, _)] = inner.as_slice() {
                    // `select(__.select("a"))` — dynamic map-key select.
                    self.steps.push(Step::SelectMapValueBy(label.clone()));
                    return;
                }
                let labels = select_label_from_constant_traversal(&inner)
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

    pub(super) fn dispatch_traversalMethod_valueMap<'input>(
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

    pub(super) fn dispatch_traversalMethod_barrier<'input>(
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

    pub(super) fn dispatch_cast_traversalGType<'input>(
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

    pub(super) fn dispatch_cast_simple(&mut self, text: &str, target: CastTarget) {
        let step = Step::CastScalar(target);
        if contains_scope_local(text) {
            self.steps.push(Step::LocalScoped(Box::new(step)));
        } else {
            self.steps.push(step);
        }
    }

    pub(super) fn dispatch_traversalMethod_count<'input>(
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

    pub(super) fn collect_generic_argument_varargs<'input>(
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

    pub(super) fn collect_string_nullable_argument_varargs<'input>(
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

    pub(super) fn collect_string_nullable_literal_varargs<'input>(
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
}
