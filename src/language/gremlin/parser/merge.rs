//! Literal merge maps keep T tokens separate from ordinary string keys.
use super::*;
use crate::language::gremlin::ast::{MergeVertexMap, MutationArgument};

impl LoweringVisitor {
    pub(super) fn parse_merge_map<'input>(
        &mut self,
        argument: Option<Rc<GenericMapNullableArgumentContextAll<'input>>>,
        allow_cardinality: bool,
        edge: bool,
    ) -> Result<Option<MergeVertexMap>> {
        let argument = argument.ok_or_else(|| GremlinError::Parse("missing merge map".into()))?;
        let literal = argument
            .genericMapNullableLiteral()
            .ok_or_else(|| GremlinError::Unsupported("mergeV variable map".into()))?;
        if literal.nullLiteral().is_some() {
            return Ok(None);
        }
        let map = literal
            .genericMapLiteral()
            .ok_or_else(|| GremlinError::Parse("missing merge map literal".into()))?;
        let mut result = MergeVertexMap::default();
        for entry in map.mapEntry_all() {
            let key = entry
                .mapKey()
                .ok_or_else(|| GremlinError::Parse("missing map key".into()))?;
            let value = entry
                .genericLiteral()
                .ok_or_else(|| GremlinError::Parse("missing map value".into()))?;
            if value.nestedTraversal().is_some() {
                return Err(GremlinError::Unsupported(
                    "mergeV map traversal value".into(),
                ));
            }
            let cardinality = value.traversalCardinality();
            let single = cardinality.is_some();
            let cardinality_name = cardinality.as_ref().map(|c| c.get_text().split('(').next().unwrap_or("list").rsplit('.').next().unwrap_or("list").to_string());
            if let Some(cardinality) = cardinality {
                if !allow_cardinality {
                    return Err(GremlinError::Unsupported(
                        "mergeV cardinality values require an option map".into(),
                    ));
                }
                let inner = cardinality.genericLiteral().ok_or_else(|| {
                    GremlinError::Unsupported("mergeV cardinality requires a value".into())
                })?;
                if inner.nestedTraversal().is_some() || inner.traversalCardinality().is_some() {
                    return Err(GremlinError::Unsupported(
                        "mergeV cardinality requires a literal value".into(),
                    ));
                }
                self.visit_genericLiteral(&inner);
            } else {
                self.visit_genericLiteral(&value);
            }
            let value = self
                .pop_value()
                .ok_or_else(|| GremlinError::Parse("invalid map value".into()))?;
            if let Some(token) = key
                .traversalT()
                .map(|t| t.get_text())
                .or_else(|| key.traversalTLong().map(|t| t.get_text()))
            {
                if single {
                    return Err(GremlinError::Unsupported(
                        "mergeV cardinality requires a property key".into(),
                    ));
                }
                match token.rsplit('.').next() {
                    Some("label") => result.label = Some(value),
                    Some("id") => result.id = Some(value),
                    _ => {
                        return Err(GremlinError::Unsupported(
                            "mergeV only supports T.id and T.label tokens".into(),
                        ));
                    }
                }
            } else if let Some(direction) = key
                .traversalDirection()
                .map(|d| d.get_text())
                .or_else(|| key.traversalDirectionLong().map(|d| d.get_text()))
            {
                if !edge {
                    return Err(GremlinError::Unsupported(
                        "mergeV does not accept direction keys".into(),
                    ));
                }
                match direction.rsplit('.').next() {
                    Some("OUT") | Some("from") => result.out_vertex = Some(value),
                    Some("IN") | Some("to") => result.in_vertex = Some(value),
                    _ => {
                        return Err(GremlinError::Parse(
                            "mergeE endpoint direction must be OUT or IN".into(),
                        ));
                    }
                }
            } else if let Some(string) = key.stringLiteral() {
                let key = super::literals::decode_string_literal(&string.get_text())?;
                if single {
                    result.single_properties.insert(key.clone());
                    result.cardinalities.insert(key.clone(),cardinality_name.clone().unwrap());
                }
                result.properties.insert(key, value);
            } else if key.nakedKey().is_some() || key.keyword().is_some() {
                if single {
                    result.single_properties.insert(key.get_text());
                    result.cardinalities.insert(key.get_text(),cardinality_name.clone().unwrap());
                }
                result.properties.insert(key.get_text(), value);
            } else {
                return Err(GremlinError::Unsupported(
                    "mergeV property keys must be strings".into(),
                ));
            }
        }
        Ok(Some(result))
    }
    pub(super) fn lower_merge_edge_map<'input>(
        &mut self,
        argument: Option<Rc<GenericMapNullableArgumentContextAll<'input>>>,
    ) {
        match self.parse_merge_map(argument, false, true) {
            Ok(criteria) => self.steps.push(Step::MergeE {
                criteria,
                on_create: None,
                on_match: None,
            }),
            Err(error) => self.fail(error),
        }
    }
    pub(super) fn lower_merge_vertex_map<'input>(
        &mut self,
        argument: Option<Rc<GenericMapNullableArgumentContextAll<'input>>>,
    ) {
        match self.parse_merge_map(argument, false, false) {
            Ok(criteria) => self.steps.push(Step::MergeV {
                criteria,
                on_create: None,
                on_match: None,
            }),
            Err(error) => self.fail(error),
        }
    }
    pub(super) fn dynamic_merge(&mut self, edge: bool, steps: Vec<Step>) {
        self.steps.push(Step::DynamicMerge {
            edge,
            criteria: MutationArgument::Traversal(steps),
            options: Default::default(),
        });
    }

    fn promote_merge(&mut self) {
        let Some(step) = self.steps.last().cloned() else {
            return;
        };
        let (edge, criteria, on_create, on_match) = match step {
            Step::MergeV {
                criteria,
                on_create,
                on_match,
            } => (false, criteria, on_create, on_match),
            Step::MergeE {
                criteria,
                on_create,
                on_match,
            } => (true, criteria, on_create, on_match),
            _ => return,
        };
        let mut options = BTreeMap::new();
        for (key, value) in [("onCreate", on_create), ("onMatch", on_match)] {
            if let Some(value) = value {
                if let Some(map)=&value {
                    options.insert(format!("{key}Cardinalities"),MutationArgument::Literal(GValue::Map(map.cardinalities.iter().map(|(k,v)|(k.clone(),GValue::String(v.clone()))).collect())));
                    if let Some(default)=&map.default_cardinality {options.insert(format!("{key}Cardinality"),MutationArgument::Literal(GValue::String(default.clone())));}
                }
                options.insert(
                    key.into(),
                    MutationArgument::Literal(value.map(|m| m.literal()).unwrap_or(GValue::Null)),
                );
            }
        }
        *self.steps.last_mut().unwrap() = Step::DynamicMerge {
            edge,
            criteria: MutationArgument::Literal(
                criteria.map(|m| m.literal()).unwrap_or(GValue::Null),
            ),
            options,
        };
    }

    pub(super) fn lower_merge_vertex_option<'input>(
        &mut self,
        ctx: &TraversalMethod_optionContextAll<'input>,
    ) {
        if matches!(
            self.steps.last(),
            Some(Step::DynamicMerge { .. } | Step::MergeE { .. })
        ) || matches!(
            ctx,
            TraversalMethod_optionContextAll::TraversalMethod_option_Merge_TraversalContext(_)
        ) {
            self.promote_merge();
            let (option, value) = match ctx {
                TraversalMethod_optionContextAll::TraversalMethod_option_Merge_MapContext(c) => {
                    let option = c.traversalMerge().map(|t| t.get_text()).unwrap_or_default();
                    let value =
                        match self.parse_merge_map(c.genericMapNullableArgument(), false, true) {
                            Ok(value) => MutationArgument::Literal(
                                value.map(|m| m.literal()).unwrap_or(GValue::Null),
                            ),
                            Err(error) => {
                                self.fail(error);
                                return;
                            }
                        };
                    (option, value)
                }
                TraversalMethod_optionContextAll::TraversalMethod_option_Merge_TraversalContext(
                    c,
                ) => {
                    let option = c.traversalMerge().map(|t| t.get_text()).unwrap_or_default();
                    let Some(nested) = c.nestedTraversal() else {
                        return;
                    };
                    (
                        option,
                        MutationArgument::Traversal(self.lower_nested_traversal(&nested)),
                    )
                }
                _ => {
                    self.fail(GremlinError::Unsupported("merge option form".into()));
                    return;
                }
            };
            if let Some(Step::DynamicMerge { options, .. }) = self.steps.last_mut() {
                options.insert(option.rsplit('.').next().unwrap_or("").into(), value);
            }
            return;
        }
        let (argument,option,default_cardinality)=match ctx {
            TraversalMethod_optionContextAll::TraversalMethod_option_Merge_MapContext(c)=>(c.genericMapNullableArgument(),c.traversalMerge().map(|t|t.get_text()).unwrap_or_default(),None),
            TraversalMethod_optionContextAll::TraversalMethod_option_Merge_Map_CardinalityContext(c)=>(c.genericMapNullableArgument(),c.traversalMerge().map(|t|t.get_text()).unwrap_or_default(),c.traversalCardinality().map(|c|c.get_text().rsplit('.').next().unwrap_or("list").to_string())),
            _=>{self.fail(GremlinError::Unsupported("mergeV option requires a map".into()));return}
        };
        let edge = matches!(self.steps.last(), Some(Step::MergeE { .. }));
        let mut value = match self.parse_merge_map(argument, !edge, edge) {
            Ok(value) => value,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
        if let Some(map)=value.as_mut(){map.default_cardinality=default_cardinality;}
        let Some(
            Step::MergeV {
                on_create,
                on_match,
                ..
            }
            | Step::MergeE {
                on_create,
                on_match,
                ..
            },
        ) = self.steps.last_mut()
        else {
            return;
        };
        match option.rsplit('.').next() {
            Some("onCreate") => *on_create = Some(value),
            Some("onMatch") => *on_match = Some(value),
            _ => self.fail(GremlinError::Unsupported(
                "mergeV option requires onCreate or onMatch".into(),
            )),
        }
    }
}
