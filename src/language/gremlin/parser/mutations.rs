//! Graph mutation syntax supported by the Gremlin frontend.
use super::*;

impl LoweringVisitor {
    pub(super) fn lower_add_vertex<'input>(
        &mut self,
        ctx: &TraversalMethod_addVContextAll<'input>,
    ) {
        let label = match ctx {
            TraversalMethod_addVContextAll::TraversalMethod_addV_EmptyContext(_) => "vertex".into(),
            TraversalMethod_addVContextAll::TraversalMethod_addV_StringContext(c) => {
                let Some(arg) = c.stringArgument() else {
                    return;
                };
                let Some(label) = self.string_argument_text(&arg) else {
                    return;
                };
                label
            }
            _ => {
                self.fail(GremlinError::Unsupported("addV traversal label".into()));
                return;
            }
        };
        self.steps.push(Step::AddV { label });
    }

    pub(super) fn lower_add_edge<'input>(&mut self, ctx: &TraversalMethod_addEContextAll<'input>) {
        let TraversalMethod_addEContextAll::TraversalMethod_addE_StringContext(c) = ctx else {
            self.fail(GremlinError::Unsupported("addE traversal label".into()));
            return;
        };
        let Some(arg) = c.stringArgument() else {
            return;
        };
        let Some(label) = self.string_argument_text(&arg) else {
            return;
        };
        self.steps.push(Step::AddE {
            label,
            from: None,
            to: None,
        });
    }

    pub(super) fn lower_edge_endpoint(&mut self, raw: &str, source: bool) -> bool {
        let Some(index) = self
            .steps
            .iter()
            .rposition(|step| !matches!(step, Step::Property { .. }))
        else {
            return false;
        };
        if !matches!(self.steps[index], Step::AddE { .. }) {
            return false;
        }
        let argument = raw
            .split_once('(')
            .and_then(|(_, rest)| rest.strip_suffix(')'))
            .unwrap_or("");
        let label = match super::literals::decode_string_literal(argument) {
            Ok(label) => label,
            Err(_) => {
                self.fail(GremlinError::Unsupported(
                    "addE endpoint requires a path label".into(),
                ));
                return true;
            }
        };
        if let Step::AddE { from, to, .. } = &mut self.steps[index] {
            if source {
                *from = Some(label);
            } else {
                *to = Some(label);
            }
        }
        true
    }

    pub(super) fn lower_property_write<'input>(
        &mut self,
        ctx: &TraversalMethod_propertyContextAll<'input>,
    ) {
        let (key, value, extras) = match ctx {
            TraversalMethod_propertyContextAll::TraversalMethod_property_Object_Object_ObjectContext(c) =>
                (c.genericLiteral(), c.genericArgument(), c.genericArgumentVarargs()),
            TraversalMethod_propertyContextAll::TraversalMethod_property_Cardinality_Object_Object_ObjectContext(c) => {
                let cardinality = c.traversalCardinality().map(|c| c.get_text()).unwrap_or_default();
                if cardinality != "single" && cardinality != "Cardinality.single" {
                    self.fail(GremlinError::Unsupported("property cardinality requires single".into())); return;
                }
                (c.genericLiteral(), c.genericArgument(), c.genericArgumentVarargs())
            }
            _ => { self.fail(GremlinError::Unsupported("property map mutation".into())); return; }
        };
        if extras
            .as_ref()
            .is_some_and(|args| !args.genericArgument_all().is_empty())
        {
            self.fail(GremlinError::Unsupported("property meta-properties".into()));
            return;
        }
        let (Some(key), Some(value)) = (key, value) else {
            return;
        };
        self.visit_genericLiteral(&key);
        let Some(GValue::String(key)) = self.pop_value() else {
            self.fail(GremlinError::Unsupported(
                "property key must be a string".into(),
            ));
            return;
        };
        if let Some(nested) = value
            .genericLiteral()
            .and_then(|literal| literal.nestedTraversal())
        {
            let traversal = self.lower_nested_traversal(&nested);
            self.steps.push(Step::PropertyTraversal { key, traversal });
            return;
        }
        self.visit_genericArgument(&value);
        let Some(value) = self.pop_value() else {
            return;
        };
        self.steps.push(Step::Property { key, value });
    }
}
