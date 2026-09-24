//! Property and identity filters, nested traversals, and adjacency helpers.

use crate::grammar::generated::gremlin::gremlinvisitor::GremlinVisitor;
use antlr4rust::tree::ParseTree;

use super::literals::{extract_first_string_arg, has_local_scope_arg};
use super::{
    AggKind, CompareOp, Direction, GValue, GremlinError, LoweringVisitor, Predicate, Rc, Step,
};
use crate::grammar::generated::gremlin::gremlinparser::*;
#[allow(non_snake_case)]
impl LoweringVisitor {
    pub(super) fn string_argument_text<'input>(
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
    pub(super) fn lower_nested_traversal<'input>(
        &mut self,
        ctx: &NestedTraversalContext<'input>,
    ) -> Vec<Step> {
        let baseline = self.steps.len();
        if let Some(chained) = ctx.chainedTraversal() {
            self.visit_chainedTraversal(&chained);
        }
        self.steps.split_off(baseline)
    }

    pub(super) fn collect_nested_traversal_list<'input>(
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

    pub(super) fn lower_expand_edge<'input>(
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
    pub(super) fn dispatch_traversalMethod_hasLabel<'input>(
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
    pub(super) fn dispatch_traversalMethod_hasKey<'input>(
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

    pub(super) fn push_has_key_filter(&mut self, mut keys: Vec<String>) {
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
    pub(super) fn dispatch_traversalMethod_has<'input>(
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
                if let Some(nested) = value_ctx.genericLiteral().and_then(|lit| lit.nestedTraversal()) {
                    let mut child = vec![Step::Values(vec![key])];
                    child.extend(self.lower_nested_traversal(&nested));
                    self.steps.push(Step::WhereTraversal(child));
                    return;
                }
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
                if let Some(nested) = value_ctx.genericLiteral().and_then(|lit| lit.nestedTraversal()) {
                    let mut child = vec![Step::Values(vec![key])];
                    child.extend(self.lower_nested_traversal(&nested));
                    self.steps.push(Step::HasLabel(vec![label]));
                    self.steps.push(Step::WhereTraversal(child));
                    return;
                }
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
                if let Some(nested) = c.genericArgument().and_then(|arg| arg.genericLiteral()).and_then(|lit| lit.nestedTraversal()) {
                    let projection = match token.as_str() {
                        "label" => Step::Label,
                        "id" => Step::Id,
                        "key" => Step::Values(vec!["key".into()]),
                        "value" => Step::Values(vec!["value".into()]),
                        _ => { self.fail(GremlinError::Parse("unknown element token".into())); return; }
                    };
                    let mut child = vec![projection];
                    child.extend(self.lower_nested_traversal(&nested));
                    self.steps.push(Step::WhereTraversal(child));
                    return;
                }
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

    pub(super) fn dispatch_traversalMethod_limit<'input>(
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

    pub(super) fn dispatch_traversalMethod_is<'input>(
        &mut self,
        ctx: &TraversalMethod_isContextAll<'input>,
    ) {
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

    pub(super) fn dispatch_traversalMethod_hasId<'input>(
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

    pub(super) fn lower_t_has(
        &mut self,
        token: &str,
        value: Option<GValue>,
        predicate: Option<Predicate>,
    ) {
        match token {
            "id" => self.lower_t_has_id(value, predicate),
            "label" => self.lower_t_has_label(value, predicate),
            _ => self.steps.push(Step::Identity),
        }
    }

    pub(super) fn lower_t_has_id(&mut self, value: Option<GValue>, predicate: Option<Predicate>) {
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

    pub(super) fn lower_t_has_label(
        &mut self,
        value: Option<GValue>,
        predicate: Option<Predicate>,
    ) {
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
    pub(super) fn push_aggregate_with_scope(&mut self, raw_text: &str, kind: AggKind) {
        let agg = Step::Aggregate(kind);
        if has_local_scope_arg(raw_text) {
            self.steps.push(Step::LocalScoped(Box::new(agg)));
        } else {
            self.steps.push(agg);
        }
    }

    pub(super) fn lower_has_id_predicate(&mut self, predicate: Option<Predicate>) {
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

    pub(super) fn dispatch_filter_or_where<'input>(
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

    pub(super) fn dispatch_traversalMethod_where<'input>(
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
                self.steps.push(Step::WhereString { label: "current".into(), predicate });
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
}
