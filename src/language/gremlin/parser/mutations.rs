//! Graph mutation syntax supported by the Gremlin frontend.
use super::*;
use crate::language::gremlin::ast::MutationArgument;
use antlr4rust::parser_rule_context::ParserRuleContext;

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
            TraversalMethod_addVContextAll::TraversalMethod_addV_TraversalContext(c) => {
                if let Some(nested) = c.nestedTraversal() {
                    let traversal = self.lower_nested_traversal(&nested);
                    self.steps.push(Step::AddDynamicV {
                        label: MutationArgument::Traversal(traversal),
                    });
                }
                return;
            }
            _ => return,
        };
        self.steps.push(Step::AddV { label });
    }

    pub(super) fn lower_add_edge<'input>(&mut self, ctx: &TraversalMethod_addEContextAll<'input>) {
        let label = match ctx {
            TraversalMethod_addEContextAll::TraversalMethod_addE_StringContext(c) => c
                .stringArgument()
                .and_then(|a| self.string_argument_text(&a))
                .map(|s| MutationArgument::Literal(GValue::String(s))),
            TraversalMethod_addEContextAll::TraversalMethod_addE_TraversalContext(c) => c
                .nestedTraversal()
                .map(|n| MutationArgument::Traversal(self.lower_nested_traversal(&n))),
            _ => None,
        };
        if let Some(label) = label {
            self.steps.push(Step::AddDynamicE {
                label,
                from: None,
                to: None,
            });
        }
    }

    pub(super) fn lower_edge_endpoint(
        &mut self,
        raw: &str,
        source: bool,
        nested: Option<Vec<Step>>,
        reference: Option<GValue>,
    ) -> bool {
        let Some(index) = self.steps.iter().rposition(|step| {
            !matches!(
                step,
                Step::Property { .. }
                    | Step::PropertyTraversal { .. }
                    | Step::PropertyDynamic { .. }
                    | Step::PropertyNative { .. }
            )
        }) else {
            return false;
        };
        if !matches!(self.steps[index], Step::AddDynamicE { .. }) {
            return false;
        }
        let argument = if let Some(nested) = nested {
            MutationArgument::Traversal(nested)
        } else if let Some(reference) = reference {
            MutationArgument::Literal(reference)
        } else {
            let text = raw
                .split_once('(')
                .and_then(|(_, r)| r.strip_suffix(')'))
                .unwrap_or("");
            match super::literals::decode_string_literal(text) {
                Ok(label) => MutationArgument::Label(label),
                Err(error) => {
                    self.fail(error);
                    return true;
                }
            }
        };
        if let Step::AddDynamicE { from, to, .. } = &mut self.steps[index] {
            if source {
                *from = Some(argument);
            } else {
                *to = Some(argument);
            }
        }
        true
    }

    pub(super) fn lower_property_write<'input>(
        &mut self,
        ctx: &TraversalMethod_propertyContextAll<'input>,
    ) {
        let map_form=match ctx {
            TraversalMethod_propertyContextAll::TraversalMethod_property_ObjectContext(c)=>Some((c.genericMapNullableArgument(),"single".to_string())),
            TraversalMethod_propertyContextAll::TraversalMethod_property_Cardinality_ObjectContext(c)=>Some((c.genericMapNullableArgument(),c.traversalCardinality().map(|c|c.get_text().rsplit('.').next().unwrap_or("single").to_string()).unwrap_or_else(||"single".into()))),
            _=>None,
        };
        if let Some((argument,default))=map_form {
            match self.parse_merge_map(argument,true,false) {
                Ok(Some(map))=>{
                    for (key,value) in map.properties {let cardinality=map.cardinalities.get(&key).cloned().unwrap_or_else(||default.clone());self.steps.push(Step::PropertyNative{cardinality,key:MutationArgument::Literal(GValue::String(key)),value:MutationArgument::Literal(value),meta:vec![]});}
                    if let Some(id)=map.id {self.steps.push(Step::PropertyNative{cardinality:"single".into(),key:MutationArgument::Literal(GValue::Token("id".into())),value:MutationArgument::Literal(id),meta:vec![]});}
                    if map.label.is_some(){self.fail(GremlinError::Unsupported("property map label requires addV".into()));}
                }
                Ok(None)=>{},Err(error)=>self.fail(error),
            }
            return;
        }
        // Preserve omission so addV can distinguish folded parameters from explicit property steps.
        let mut cardinality = "default".to_string();
        let (key, value, extras) = match ctx {
            TraversalMethod_propertyContextAll::TraversalMethod_property_Object_Object_ObjectContext(c) =>
                (c.genericLiteral(), c.genericArgument(), c.genericArgumentVarargs()),
            TraversalMethod_propertyContextAll::TraversalMethod_property_Cardinality_Object_Object_ObjectContext(c) => {
                cardinality=c.traversalCardinality().map(|c|c.get_text().rsplit('.').next().unwrap_or("list").to_string()).unwrap_or_else(||"list".into());
                (c.genericLiteral(), c.genericArgument(), c.genericArgumentVarargs())
            }
            _ => { self.fail(GremlinError::Unsupported("property map mutation".into())); return; }
        };
        let (Some(key),Some(value))=(key,value) else{return};
        if key.traversalT().is_some_and(|t|t.get_text().ends_with("label")) {
            self.visit_genericArgument(&value);
            let Some(GValue::String(label))=self.pop_value() else {self.fail(GremlinError::Parse("Label must be a string".into()));return};
            if let Some(Step::AddV{label:current})=self.steps.last_mut(){*current=label;return}
            self.fail(GremlinError::Unsupported("T.label property requires addV".into()));return;
        }
        let key=if let Some(nested)=key.nestedTraversal(){MutationArgument::Traversal(self.lower_nested_traversal(&nested))}else{self.visit_genericLiteral(&key);let Some(key)=self.pop_value() else{return};MutationArgument::Literal(key)};
        let value=if let Some(nested)=value.genericLiteral().and_then(|v|v.nestedTraversal()){MutationArgument::Traversal(self.lower_nested_traversal(&nested))}else{self.visit_genericArgument(&value);let Some(value)=self.pop_value() else{return};MutationArgument::Literal(value)};
        let mut meta=vec![];
        if let Some(extras)=extras {
            let args=extras.genericArgument_all();
            if args.len()%2!=0 {self.fail(GremlinError::Parse("Meta-properties require key/value pairs".into()));return;}
            for pair in args.chunks(2) {
                self.visit_genericArgument(&pair[0]);let Some(GValue::String(key))=self.pop_value() else{self.fail(GremlinError::Parse("Meta-property key must be a string".into()));return};
                self.visit_genericArgument(&pair[1]);let Some(value)=self.pop_value() else{return};meta.push((key,value));
            }
        }
        self.steps.push(Step::PropertyNative{cardinality,key,value,meta});
    }
}
