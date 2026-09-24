//! Static vertex merge through the normal graph mutation operators.
use super::context::CURRENT;
use super::literals::gvalue_to_expr;
use crate::ir::expr::{BinaryOp, IrExpr};
use crate::ir::plan::LabelExpr;
use crate::ir::plan::{BindKind, CreateNode, Node, SetMode, SetPropertyItem};
use crate::ir::policy::PropertyMissing;
use crate::language::gremlin::ast::MergeVertexMap;
use crate::language::gremlin::planner::error::{GremlinPlanError, GremlinPlanResult};
use crate::language::gremlin::semantics::GValue;

fn validate(map: &MergeVertexMap) -> GremlinPlanResult<()> {
    if map.id.is_some() {
        return Err(GremlinPlanError::Unsupported(
            "mergeV user-supplied T.id requires an element identity allocator".into(),
        ));
    }
    match map.label.as_ref() {
        Some(GValue::Null) => {
            return Err(GremlinPlanError::Parse(
                "mergeV() does not allow null Map values - check: label".into(),
            ));
        }
        Some(GValue::String(label)) if label.is_empty() => {
            return Err(GremlinPlanError::Parse("Label can not be empty".into()));
        }
        Some(GValue::String(label)) if label.starts_with('~') => {
            return Err(GremlinPlanError::Parse(format!(
                "Label can not be a hidden key: {label}"
            )));
        }
        Some(GValue::String(_)) | None => (),
        _ => {
            return Err(GremlinPlanError::Parse(
                "mergeV() and option(onCreate) args expect T.label value to be of String".into(),
            ));
        }
    }
    for key in map.properties.keys() {
        if key.is_empty() {
            return Err(GremlinPlanError::Parse(
                "Property key can not be empty".into(),
            ));
        }
        if key.starts_with('~') {
            return Err(GremlinPlanError::Parse(format!(
                "Property key can not be a hidden key: {key}"
            )));
        }
    }
    Ok(())
}

pub(super) fn lower_merge_vertex(
    input: Node,
    criteria: Option<&MergeVertexMap>,
    on_create: Option<&Option<MergeVertexMap>>,
    on_match: Option<&Option<MergeVertexMap>>,
    lo: &mut super::context::Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node> {
    for map in criteria
        .into_iter()
        .chain(on_create.and_then(|m| m.as_ref()))
        .chain(on_match.and_then(|m| m.as_ref()))
    {
        validate(map)?;
    }
    // MergeStep.materializeMap in the pinned TinkerPop release treats null
    // as an empty map, including onCreate. Historical feature comments differ.
    let empty = MergeVertexMap::default();
    let criteria = Some(criteria.unwrap_or(&empty));
    let match_arm = if let Some(criteria) = criteria {
        let scan = Node::GraphNodeScan {
            graph: "default".into(),
            binding: CURRENT.into(),
            labels: LabelExpr::Any,
        };
        let mut node = Node::GraphBind {
            bind: CURRENT.into(),
            kind: BindKind::Node,
            expr: None,
            input: scan.boxed(),
        };
        node = super::subgraph_strategy::apply_vertex_subgraph(node, lo, ctx)?;
        let mut conditions = Vec::new();
        if let Some(GValue::String(label)) = &criteria.label {
            conditions.push(IrExpr::HasLabel {
                binding: CURRENT.into(),
                label: label.clone(),
            });
        }
        for (key, value) in &criteria.properties {
            conditions.push(IrExpr::Binary {
                op: BinaryOp::Eq,
                lhs: Box::new(IrExpr::property(
                    CURRENT,
                    key,
                    PropertyMissing::NullOnMissing,
                )),
                rhs: Box::new(gvalue_to_expr(value)?),
            });
        }
        if !conditions.is_empty() {
            node = Node::GraphFilter {
                condition: IrExpr::and(conditions),
                input: node.boxed(),
            };
        }
        if let Some(Some(updates)) = on_match {
            if updates.label.is_some() || updates.id.is_some() {
                return Err(GremlinPlanError::Unsupported(
                    "option(onMatch) cannot update element tokens".into(),
                ));
            }
            node = Node::GraphSetProperty {
                items: updates
                    .properties
                    .iter()
                    .map(|(key, value)| {
                        Ok(SetPropertyItem {
                            target: IrExpr::Binding(CURRENT.into()),
                            key: key.clone(),
                            mode: SetMode::Property,
                            value: gvalue_to_expr(value)?,
                        })
                    })
                    .collect::<GremlinPlanResult<Vec<_>>>()?,
                input: node.boxed(),
            };
        }
        node
    } else {
        Node::GraphEmpty
    };
    let create_arm = match on_create {
        _ => {
            let mut merged = criteria.cloned().unwrap_or_default();
            if let Some(Some(create)) = on_create {
                if merged.label.is_some() && create.label.is_some() && merged.label != create.label
                {
                    return Err(GremlinPlanError::Parse(
                        "option(onCreate) cannot override values from merge() argument".into(),
                    ));
                }
                if create.label.is_some() {
                    merged.label = create.label.clone();
                }
                for (key, value) in &create.properties {
                    if merged.properties.get(key).is_some_and(|previous| {
                        previous != value || create.single_properties.contains(key)
                    }) {
                        return Err(GremlinPlanError::Parse(
                            "option(onCreate) cannot override values from merge() argument".into(),
                        ));
                    }
                    merged.properties.insert(key.clone(), value.clone());
                }
            }
            if let Some((key, value)) = &lo.partition_write {
                merged.properties.insert(key.clone(), value.clone());
            }
            let label = match merged.label {
                Some(GValue::String(label)) => label,
                _ => "vertex".into(),
            };
            Node::GraphCreate {
                graph: "default".into(),
                nodes: vec![CreateNode {
                    bind: Some(CURRENT.into()),
                    label,
                    properties: Some(gvalue_to_expr(&GValue::Map(merged.properties))?),
                }],
                edges: vec![],
                input: Node::GraphCorrelate {
                    bindings: vec![CURRENT.into()],
                }
                .boxed(),
            }
        }
    };
    Ok(Node::GraphMerge {
        correlation: vec![CURRENT.into()],
        outputs: vec![CURRENT.into()],
        input: input.boxed(),
        match_arm: match_arm.boxed(),
        create_arm: create_arm.boxed(),
    })
}

/// Edge merge keeps endpoint identity distinct from ordinary properties.
pub(super) fn lower_merge_edge(
    input: Node,
    criteria: Option<&MergeVertexMap>,
    on_create: Option<&Option<MergeVertexMap>>,
    on_match: Option<&Option<MergeVertexMap>>,
    lo: &mut super::context::Lowerer,
    ctx: &super::context::TraversalContext,
) -> GremlinPlanResult<Node> {
    use crate::ir::plan::{ApplyKind, CreateEdge, Direction};
    use crate::ir::policy::OptionalMissing;
    for map in criteria
        .into_iter()
        .chain(on_create.and_then(|m| m.as_ref()))
        .chain(on_match.and_then(|m| m.as_ref()))
    {
        validate(map)?;
        if matches!(map.out_vertex, Some(GValue::Null))
            || matches!(map.in_vertex, Some(GValue::Null))
        {
            return Err(GremlinPlanError::Parse(
                "mergeE() does not allow null Map values for endpoints".into(),
            ));
        }
    }
    let empty = MergeVertexMap::default();
    let criteria = criteria.unwrap_or(&empty);
    let scan = Node::GraphBind {
        bind: CURRENT.into(),
        kind: BindKind::Edge,
        expr: None,
        input: Node::GraphRelScan {
            graph: "default".into(),
            binding: CURRENT.into(),
            types: LabelExpr::Any,
            dir: Direction::Out,
        }
        .boxed(),
    };
    let mut matching = super::subgraph_strategy::apply_edge_subgraph(scan, lo, ctx)?;
    let mut conditions = vec![];
    if let Some(label) = &criteria.label {
        conditions.push(equal(IrExpr::Label(CURRENT.into()), gvalue_to_expr(label)?));
    }
    for (key, value) in &criteria.properties {
        conditions.push(equal(
            IrExpr::property(CURRENT, key, PropertyMissing::NullOnMissing),
            gvalue_to_expr(value)?,
        ));
    }
    for (endpoint, function) in [
        (&criteria.out_vertex, "edge_src"),
        (&criteria.in_vertex, "edge_dst"),
    ] {
        if let Some(id) = endpoint {
            conditions.push(equal(
                call(
                    "gremlin_id",
                    vec![call(function, vec![IrExpr::Binding(CURRENT.into())])],
                ),
                endpoint_id_expr(id)?,
            ));
        }
    }
    if !conditions.is_empty() {
        matching = Node::GraphFilter {
            condition: IrExpr::and(conditions),
            input: matching.boxed(),
        };
    }
    if let Some(Some(updates)) = on_match {
        if updates.label.is_some()
            || updates.id.is_some()
            || updates.out_vertex.is_some()
            || updates.in_vertex.is_some()
        {
            return Err(GremlinPlanError::Parse(
                "option(onMatch) expects keys in Map to be of String".into(),
            ));
        }
        matching = Node::GraphSetProperty {
            items: updates
                .properties
                .iter()
                .map(|(key, value)| {
                    Ok(SetPropertyItem {
                        target: IrExpr::Binding(CURRENT.into()),
                        key: key.clone(),
                        value: gvalue_to_expr(value)?,
                        mode: SetMode::Property,
                    })
                })
                .collect::<GremlinPlanResult<_>>()?,
            input: matching.boxed(),
        };
    }
    let mut creation = criteria.clone();
    if let Some(Some(create)) = on_create {
        for (old, new) in [
            (&criteria.label, &create.label),
            (&criteria.out_vertex, &create.out_vertex),
            (&criteria.in_vertex, &create.in_vertex),
        ] {
            if old.is_some() && new.is_some() && old != new {
                return Err(GremlinPlanError::Parse(
                    "option(onCreate) cannot override values from merge() argument".into(),
                ));
            }
        }
        if create.label.is_some() {
            creation.label = create.label.clone();
        }
        if create.out_vertex.is_some() {
            creation.out_vertex = create.out_vertex.clone();
        }
        if create.in_vertex.is_some() {
            creation.in_vertex = create.in_vertex.clone();
        }
        for (key, value) in &create.properties {
            if creation.properties.get(key).is_some_and(|old| old != value) {
                return Err(GremlinPlanError::Parse(
                    "option(onCreate) cannot override values from merge() argument".into(),
                ));
            }
            creation.properties.insert(key.clone(), value.clone());
        }
    }
    let mut create_input = Node::GraphCorrelate {
        bindings: vec![CURRENT.into()],
    };
    let src = lo.fresh("merge_src");
    let dst = lo.fresh("merge_dst");
    for (binding, id) in [(&src, &creation.out_vertex), (&dst, &creation.in_vertex)] {
        let endpoint = if let Some(id) = id {
            Node::GraphFilter {
                condition: equal(
                    call("gremlin_id", vec![IrExpr::Binding(binding.clone())]),
                    endpoint_id_expr(id)?,
                ),
                input: Node::GraphNodeScan {
                    graph: "default".into(),
                    binding: binding.clone(),
                    labels: LabelExpr::Any,
                }
                .boxed(),
            }
        } else {
            Node::GraphEmpty
        };
        create_input = Node::GraphApply {
            kind: ApplyKind::Optional,
            correlation: vec![],
            outputs: vec![binding.clone()],
            optional_missing: OptionalMissing::Null,
            left: create_input.boxed(),
            right: endpoint.boxed(),
        };
    }
    if let Some((key, value)) = &lo.partition_write {
        creation.properties.insert(key.clone(), value.clone());
    }
    let label = match creation.label {
        Some(GValue::String(label)) => label,
        _ => "edge".into(),
    };
    let create = Node::GraphCreate {
        graph: "default".into(),
        nodes: vec![],
        edges: vec![CreateEdge {
            bind: Some(CURRENT.into()),
            rel_type: label,
            src,
            dst,
            properties: Some(gvalue_to_expr(&GValue::Map(creation.properties))?),
        }],
        input: create_input.boxed(),
    };
    Ok(Node::GraphMerge {
        correlation: vec![CURRENT.into()],
        outputs: vec![CURRENT.into()],
        input: input.boxed(),
        match_arm: matching.boxed(),
        create_arm: create.boxed(),
    })
}

fn call(name: &str, args: Vec<IrExpr>) -> IrExpr {
    IrExpr::Call {
        name: name.into(),
        args,
    }
}
fn equal(lhs: IrExpr, rhs: IrExpr) -> IrExpr {
    IrExpr::Binary {
        op: BinaryOp::Eq,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

fn endpoint_id_expr(value: &GValue) -> GremlinPlanResult<IrExpr> {
    match value {
        GValue::VertexRef { .. } => Ok(call("gremlin_id", vec![gvalue_to_expr(value)?])),
        _ => gvalue_to_expr(value),
    }
}
