//! Ontology-backed basic graph pattern resolution.

use super::{
    ApplyKind, BTreeMap, BTreeSet, BinaryOp, ClassMapping, IrExpr, LabelExpr, Length, Lowered,
    MatchMode, NamedNodePattern, Node, OntologyMapping, OptionalMissing, PathMaterialization,
    PathMode, PathUpdate, PredicateMapping, ProjectErrorPolicy, ProjectMode, ProjectionItem,
    PropertyMissing, RdfTerm, SparqlError, TargetMode, TermPattern, TriplePattern, binding, eq,
    terms,
};
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Resolves a basic graph pattern through ontology metadata.
/// Returning `None` means the configured ontology does not cover the pattern.
pub(super) fn lower_ontology_bgp(
    patterns: &[TriplePattern],
    ontology: &OntologyMapping,
) -> Result<Option<Lowered>, SparqlError> {
    if patterns.is_empty() {
        return Ok(Some(Lowered {
            node: Node::GraphOneRow,
            variables: BTreeSet::new(),
            identity_variables: BTreeSet::new(),
            projection: None,
        }));
    }
    let mut typed_bindings = BTreeMap::<String, ClassMapping>::new();
    let mut root = None;
    for pattern in patterns {
        if !matches!(
            &pattern.predicate,
            NamedNodePattern::NamedNode(predicate) if predicate.as_str() == RDF_TYPE
        ) {
            continue;
        }
        let (TermPattern::Variable(subject), TermPattern::NamedNode(class)) =
            (&pattern.subject, &pattern.object)
        else {
            return Ok(None);
        };
        let Some(mapped) = ontology.class_for_iri(class.as_str()) else {
            return Ok(None);
        };
        let subject = binding(subject);
        if let Some(previous) = typed_bindings.get(&subject) {
            if previous.iri != mapped.iri {
                return Err(SparqlError::Unsupported(format!(
                    "multiple mapped rdf:type classes for {subject} require class intersection"
                )));
            }
        }
        root.get_or_insert_with(|| (subject.clone(), mapped.clone()));
        typed_bindings.insert(subject, mapped.clone());
    }
    let Some((root_binding, root_class)) = root else {
        return Ok(None);
    };
    let mut binding_labels = typed_bindings
        .iter()
        .map(|(binding, class)| (binding.clone(), class.label.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut node = Node::GraphNodeScan {
        graph: "mapped".into(),
        binding: root_binding.clone(),
        labels: LabelExpr::label(root_class.label.clone()),
    };
    let mut items = Vec::new();
    let mut conditions = Vec::new();
    let mut scalar_bindings = BTreeMap::<String, IrExpr>::new();
    let mut materialized_nodes = BTreeSet::from([root_binding.clone()]);
    let mut variables = BTreeSet::from([root_binding.clone()]);
    let mut pending = patterns
        .iter()
        .filter(|pattern| {
            !matches!(
                &pattern.predicate,
                NamedNodePattern::NamedNode(predicate) if predicate.as_str() == RDF_TYPE
            )
        })
        .collect::<Vec<_>>();
    while !pending.is_empty()
        || typed_bindings
            .keys()
            .any(|binding| !materialized_nodes.contains(binding))
    {
        let mut progressed = false;
        let mut remaining = Vec::new();
        for pattern in pending {
            let (TermPattern::Variable(subject), NamedNodePattern::NamedNode(predicate)) =
                (&pattern.subject, &pattern.predicate)
            else {
                return Ok(None);
            };
            let subject = binding(subject);
            if !materialized_nodes.contains(&subject) {
                remaining.push(pattern);
                continue;
            }
            let Some(subject_label) = binding_labels.get(&subject).cloned() else {
                return Ok(None);
            };
            match ontology.predicate_for_iri(predicate.as_str()) {
                Some(PredicateMapping::Property {
                    domain_label,
                    property,
                    ..
                }) if domain_label == &subject_label => {
                    let property_expr = IrExpr::Property {
                        binding: subject,
                        name: property.clone(),
                        policy: PropertyMissing::Unbound,
                    };
                    match &pattern.object {
                        TermPattern::Variable(object) => {
                            let object = binding(object);
                            if binding_labels.contains_key(&object)
                                || materialized_nodes.contains(&object)
                            {
                                return Ok(None);
                            }
                            if let Some(previous) = scalar_bindings.get(&object) {
                                conditions.push(eq(property_expr, previous.clone()));
                            } else {
                                conditions.push(IrExpr::IsNotNull(Box::new(property_expr.clone())));
                                scalar_bindings.insert(object.clone(), property_expr);
                                variables.insert(object);
                            }
                        }
                        TermPattern::NamedNode(value) => conditions.push(eq(
                            property_expr,
                            IrExpr::Lit(crate::ir::expr::Lit::String(value.as_str().into())),
                        )),
                        TermPattern::Literal(value) => {
                            let RdfTerm::Literal(literal) = terms::literal(value) else {
                                return Ok(None);
                            };
                            conditions.push(eq(property_expr, IrExpr::Lit(literal)));
                        }
                        TermPattern::BlankNode(_) => return Ok(None),
                    }
                    progressed = true;
                }
                Some(PredicateMapping::Relationship {
                    rel_type,
                    direction,
                    domain_label,
                    range_label,
                    ..
                }) if domain_label
                    .as_ref()
                    .is_none_or(|label| label == &subject_label) =>
                {
                    let TermPattern::Variable(object) = &pattern.object else {
                        return Ok(None);
                    };
                    let object = binding(object);
                    if scalar_bindings.contains_key(&object) {
                        return Ok(None);
                    }
                    let target_label = binding_labels
                        .get(&object)
                        .cloned()
                        .or_else(|| range_label.clone());
                    let target_labels = target_label
                        .as_ref()
                        .map(LabelExpr::label)
                        .unwrap_or(LabelExpr::Any);
                    let target_mode = if materialized_nodes.contains(&object) {
                        TargetMode::Existing
                    } else {
                        TargetMode::BindNew
                    };
                    node = Node::GraphExpand {
                        graph: "mapped".into(),
                        source: subject,
                        target: object.clone(),
                        target_mode,
                        target_labels,
                        rel_binding: None,
                        rel_types: LabelExpr::label(rel_type.clone()),
                        dir: *direction,
                        length: Length::ONE,
                        history: None,
                        path: None,
                        path_mode: PathMode::Walk,
                        match_mode: MatchMode::DifferentRelationships,
                        path_materialization: PathMaterialization::EndpointsOnly,
                        path_update: PathUpdate::None,
                        input: Box::new(node),
                    };
                    if let Some(label) = target_label {
                        binding_labels.insert(object.clone(), label);
                    }
                    materialized_nodes.insert(object.clone());
                    variables.insert(object);
                    progressed = true;
                }
                _ => return Ok(None),
            }
        }
        if !progressed {
            // A typed root with no edge from the current component is an
            // independent BGP factor. Scanning it also makes its properties
            // and outgoing relationships available in the next pass.
            let Some((binding, class)) = typed_bindings
                .iter()
                .find(|(binding, _)| !materialized_nodes.contains(*binding))
            else {
                return Ok(None);
            };
            node = Node::GraphApply {
                kind: ApplyKind::Inner,
                correlation: vec![],
                outputs: vec![binding.clone()],
                optional_missing: OptionalMissing::Unbound,
                left: Box::new(node),
                right: Box::new(Node::GraphNodeScan {
                    graph: "mapped".into(),
                    binding: binding.clone(),
                    labels: LabelExpr::label(class.label.clone()),
                }),
            };
            materialized_nodes.insert(binding.clone());
            variables.insert(binding.clone());
        }
        pending = remaining;
    }
    for (alias, expr) in scalar_bindings {
        items.push(ProjectionItem { alias, expr });
    }
    // Expose configured resource identities for every materialized class,
    // never Crabgraph's internal row ids unless the mapping requests that.
    for (binding, label) in &binding_labels {
        let identity = typed_bindings
            .get(binding)
            .or_else(|| ontology.class_for_label(label))
            .and_then(|class| class.identity_property.clone());
        items.push(ProjectionItem {
            alias: binding.clone(),
            expr: match identity {
                Some(name) => IrExpr::Property {
                    binding: binding.clone(),
                    name,
                    policy: PropertyMissing::Unbound,
                },
                None => IrExpr::Id(binding.clone()),
            },
        });
    }
    if let Some(condition) = conditions.into_iter().reduce(|lhs, rhs| IrExpr::Binary {
        op: BinaryOp::And,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }) {
        node = Node::GraphFilter {
            condition,
            input: Box::new(node),
        };
    }
    Ok(Some(Lowered {
        node: Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items,
            error_policy: ProjectErrorPolicy::UnboundOnExpressionError,
            input: Box::new(node),
        },
        variables,
        identity_variables: BTreeSet::new(),
        projection: None,
    }))
}
