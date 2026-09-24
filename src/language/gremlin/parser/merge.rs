//! Literal merge maps keep T tokens separate from ordinary string keys.
use super::*;
use crate::language::gremlin::ast::MergeVertexMap;

impl LoweringVisitor {
    fn parse_merge_map<'input>(
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
            if let Some(cardinality) = cardinality {
                if !allow_cardinality || cardinality.K_SINGLE().is_none() {
                    return Err(GremlinError::Unsupported(
                        "mergeV cardinality value requires single in an option map".into(),
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
                }
                result.properties.insert(key, value);
            } else if key.nakedKey().is_some() || key.keyword().is_some() {
                if single {
                    result.single_properties.insert(key.get_text());
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
    pub(super) fn lower_merge_vertex_option<'input>(
        &mut self,
        ctx: &TraversalMethod_optionContextAll<'input>,
    ) {
        let TraversalMethod_optionContextAll::TraversalMethod_option_Merge_MapContext(c) = ctx
        else {
            self.fail(GremlinError::Unsupported(
                "mergeV option requires a static map".into(),
            ));
            return;
        };
        let edge = matches!(self.steps.last(), Some(Step::MergeE { .. }));
        let option = c.traversalMerge().map(|t| t.get_text()).unwrap_or_default();
        let value = match self.parse_merge_map(c.genericMapNullableArgument(), !edge, edge) {
            Ok(value) => value,
            Err(error) => {
                self.fail(error);
                return;
            }
        };
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
