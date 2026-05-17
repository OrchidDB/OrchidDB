use crate::ir::expr::{BinaryOp as IrBinaryOp, IrExpr, Lit};
use crate::ir::plan::{
    ApplyKind, Direction, Length, Node, PathMaterialization, PathUpdate, ProjectErrorPolicy,
    ProjectMode, ProjectionItem, TargetMode,
};
use crate::ir::policy::{MatchMode, OptionalMissing, PathMode};
use crate::language::cypher::ast::{
    Expr, Literal, NodePattern, PatternPart, QuantifierKind, RangeLiteral,
    RecursiveRelationshipPattern, RelationshipPattern,
};
use crate::language::cypher::planner::error::{CypherPlanError, CypherPlanResult};
use crate::language::cypher::planner::lowering::{
    Lowerer,
    context::{BindingKind, CypherTraversalKind},
    project, sources,
};
use crate::language::cypher::semantics::DEFAULT_GRAPH;
use std::collections::{BTreeMap, BTreeSet};

pub fn lower_pattern_part(
    lowerer: &mut Lowerer,
    input: Node,
    part: &PatternPart,
    optional: bool,
    history: Option<&str>,
    history_available: bool,
) -> CypherPlanResult<Node> {
    let traversal_kind = if optional {
        CypherTraversalKind::OptionalMatchPattern
    } else {
        CypherTraversalKind::MatchPattern
    };
    let (node, outputs, output_kinds) =
        lowerer.with_child_traversal(traversal_kind, |lowerer| {
            let outer_visible: BTreeSet<_> = lowerer.visible_set();
            let mut property_correlation = pattern_property_correlation(part, &outer_visible);
            property_correlation.extend(
                outer_visible
                    .iter()
                    .filter(|binding| pattern_mentions(part, binding))
                    .cloned(),
            );
            if let Some(history) = history.filter(|_| history_available) {
                property_correlation.insert(history.to_string());
            }
            let mut outputs = Vec::new();
            let mut output_kinds = BTreeMap::new();
            let mut pattern_kinds = binding_kinds(lowerer, &outer_visible);
            validate_path_binding(part, &outer_visible)?;
            if let Some(path) = &part.variable {
                if !outer_visible.contains(path) {
                    push_output(
                        &mut outputs,
                        &mut output_kinds,
                        path.clone(),
                        BindingKind::RecursiveRelationship,
                    );
                    pattern_kinds.insert(path.clone(), BindingKind::RecursiveRelationship);
                }
            }
            let mut source = node_binding(lowerer, &part.element.start);
            validate_node_binding(&source, &pattern_kinds)?;
            let mut traversal = lower_node_start(
                lowerer,
                &part.element.start,
                &source,
                &outer_visible,
                &property_correlation,
                &mut outputs,
            )?;
            let mut pattern_visible = outer_visible.clone();
            if part.element.start.variable.is_some() {
                if !outer_visible.contains(&source) {
                    output_kinds.insert(source.clone(), BindingKind::Node);
                }
                pattern_visible.insert(source.clone());
                pattern_kinds.insert(source.clone(), BindingKind::Node);
            }
            let shared_path_binding = part.variable.clone();

            for chain in &part.element.chains {
                let target = node_binding(lowerer, &chain.node);
                let target_exists = pattern_visible.contains(&target);
                let variable_length = is_variable_length(&chain.relationship.range);
                let user_rel_binding = chain.relationship.variable.clone();
                if chain.node.variable.is_some() {
                    pattern_kinds
                        .entry(target.clone())
                        .or_insert(BindingKind::Node);
                }
                if let Some(rel) = &user_rel_binding {
                    let expected = if variable_length {
                        BindingKind::RecursiveRelationship
                    } else {
                        BindingKind::Relationship
                    };
                    validate_relationship_binding(rel, expected, &pattern_kinds)?;
                }
                let mut chain_kinds = pattern_kinds.clone();
                if let Some(rel) = &user_rel_binding {
                    let kind = if variable_length {
                        BindingKind::RecursiveRelationship
                    } else {
                        BindingKind::Relationship
                    };
                    chain_kinds.insert(rel.clone(), kind);
                }
                if chain.node.variable.is_some() {
                    validate_node_binding(&target, &chain_kinds)?;
                }
                let path_binding = shared_path_binding
                    .clone()
                    .or_else(|| variable_length.then(|| lowerer.synthetic("path")));
                let rel_binding = expand_relationship_binding(
                    lowerer,
                    &chain.relationship,
                    &pattern_visible,
                    variable_length && matches!(chain.relationship.direction, Direction::Both),
                );
                if let Some(rel) = &user_rel_binding {
                    if !pattern_visible.contains(rel) {
                        let kind = if variable_length {
                            BindingKind::RecursiveRelationship
                        } else {
                            BindingKind::Relationship
                        };
                        push_output(&mut outputs, &mut output_kinds, rel.clone(), kind);
                    }
                }
                if !target_exists && chain.node.variable.is_some() {
                    push_output(
                        &mut outputs,
                        &mut output_kinds,
                        target.clone(),
                        BindingKind::Node,
                    );
                }
                traversal = lower_expand(
                    traversal,
                    &source,
                    &target,
                    target_exists,
                    &chain.relationship,
                    rel_binding.as_deref(),
                    &chain.node,
                    path_binding.clone(),
                    history.map(ToString::to_string),
                    variable_length,
                );
                if variable_length {
                    if let (Some(recursive), Some(path)) =
                        (&chain.relationship.recursive, path_binding.as_deref())
                    {
                        let mut allowed = pattern_visible.clone();
                        allowed.insert(source.clone());
                        allowed.insert(path.to_string());
                        traversal = apply_recursive_relationship_predicate(
                            lowerer, traversal, path, &source, recursive, &allowed,
                        )?;
                        traversal =
                            apply_recursive_relationship_projection(traversal, path, recursive);
                    }
                }
                if let Some(rel) = &user_rel_binding {
                    traversal = apply_relationship_binding_projection(
                        traversal,
                        rel,
                        rel_binding.as_deref(),
                        path_binding.as_deref(),
                        &source,
                        pattern_visible.contains(rel),
                        variable_length,
                    );
                }
                if let Some(properties) = &chain.relationship.properties {
                    if variable_length {
                        if let Some(path) = &path_binding {
                            let mut allowed = pattern_visible.clone();
                            allowed.insert(source.clone());
                            allowed.insert(path.clone());
                            if let Some(rel) = &user_rel_binding {
                                allowed.insert(rel.clone());
                            }
                            traversal = apply_variable_length_property_filters(
                                lowerer, traversal, path, &source, properties, &allowed,
                            )?;
                        }
                    } else if let Some(rel) = rel_binding.as_deref().or(user_rel_binding.as_deref())
                    {
                        let mut allowed = pattern_visible.clone();
                        allowed.insert(rel.to_string());
                        traversal =
                            apply_property_filters(lowerer, traversal, rel, properties, &allowed)?;
                    }
                }
                let mut node_allowed = pattern_visible.clone();
                node_allowed.insert(target.clone());
                if let Some(rel) = &user_rel_binding {
                    node_allowed.insert(rel.clone());
                }
                traversal =
                    apply_node_filters(lowerer, traversal, &target, &chain.node, &node_allowed)?;
                if let Some(rel) = &user_rel_binding {
                    pattern_visible.insert(rel.clone());
                    let kind = if variable_length {
                        BindingKind::RecursiveRelationship
                    } else {
                        BindingKind::Relationship
                    };
                    pattern_kinds.insert(rel.clone(), kind);
                }
                if chain.node.variable.is_some() {
                    pattern_visible.insert(target.clone());
                    pattern_kinds.insert(target.clone(), BindingKind::Node);
                }
                source = target;
            }
            if let Some(path) = &part.variable {
                if part.element.chains.is_empty() && !outer_visible.contains(path) {
                    traversal = Node::GraphProject {
                        mode: ProjectMode::PreserveVisible,
                        items: vec![ProjectionItem {
                            alias: path.clone(),
                            expr: IrExpr::Call {
                                name: "path_or_self".to_string(),
                                args: vec![IrExpr::Lit(Lit::Null), IrExpr::Binding(source.clone())],
                            },
                        }],
                        error_policy: ProjectErrorPolicy::PropagateError,
                        input: traversal.boxed(),
                    };
                }
            }

            outputs.sort();
            outputs.dedup();
            let correlation: Vec<String> = outer_visible
                .iter()
                .filter(|binding| {
                    pattern_mentions(part, binding) || property_correlation.contains(*binding)
                })
                .cloned()
                .collect();
            let mut correlation = correlation;
            if let Some(history) = history.filter(|_| history_available) {
                if !correlation.iter().any(|binding| binding == history) {
                    correlation.push(history.to_string());
                }
            }
            let kind = if optional {
                ApplyKind::Optional
            } else {
                ApplyKind::Inner
            };
            let mut apply_outputs = outputs.clone();
            if let Some(history) = history {
                if !part.element.chains.is_empty()
                    && !apply_outputs.iter().any(|binding| binding == history)
                {
                    apply_outputs.push(history.to_string());
                }
            }
            let node = Node::GraphApply {
                kind,
                correlation: correlation.clone(),
                outputs: apply_outputs,
                optional_missing: OptionalMissing::Null,
                left: input.boxed(),
                right: traversal.boxed(),
            };
            lowerer.record_current_imports(outer_visible.iter().cloned());
            lowerer.record_current_correlation(correlation.clone());
            lowerer.record_current_outputs(outputs.clone());
            if optional {
                lowerer.record_current_nullable(outputs.clone());
            }
            Ok((node, outputs, output_kinds))
        })?;
    for output in outputs {
        if optional {
            lowerer.add_nullable(output.clone());
        }
        let kind = output_kinds
            .get(&output)
            .copied()
            .unwrap_or(BindingKind::Unknown);
        lowerer.add_visible_kind(output, kind);
    }
    Ok(node)
}

fn expand_relationship_binding(
    lowerer: &mut Lowerer,
    rel: &RelationshipPattern,
    outer_visible: &BTreeSet<String>,
    variable_length: bool,
) -> Option<String> {
    if variable_length {
        return None;
    }
    if let Some(binding) = &rel.variable {
        if outer_visible.contains(binding) {
            Some(lowerer.synthetic("rel"))
        } else {
            Some(binding.clone())
        }
    } else if rel.properties.is_some() {
        Some(lowerer.synthetic("rel"))
    } else {
        None
    }
}

fn binding_kinds(lowerer: &Lowerer, visible: &BTreeSet<String>) -> BTreeMap<String, BindingKind> {
    visible
        .iter()
        .filter_map(|binding| {
            lowerer
                .binding_kind(binding)
                .map(|kind| (binding.clone(), kind))
        })
        .collect()
}

fn push_output(
    outputs: &mut Vec<String>,
    output_kinds: &mut BTreeMap<String, BindingKind>,
    binding: String,
    kind: BindingKind,
) {
    outputs.push(binding.clone());
    output_kinds.insert(binding, kind);
}

fn validate_path_binding(
    part: &PatternPart,
    outer_visible: &BTreeSet<String>,
) -> CypherPlanResult<()> {
    let Some(path) = &part.variable else {
        return Ok(());
    };
    if outer_visible.contains(path) || pattern_element_declares(&part.element, path) {
        return Err(CypherPlanError::Invalid(
            "SyntaxError: VariableAlreadyBound".to_string(),
        ));
    }
    Ok(())
}

fn validate_node_binding(
    binding: &str,
    kinds: &BTreeMap<String, BindingKind>,
) -> CypherPlanResult<()> {
    match kinds.get(binding).copied() {
        Some(kind) if !matches!(kind, BindingKind::Unknown | BindingKind::Node) => {
            Err(CypherPlanError::Invalid(format!(
                "Binder exception: Cannot bind {binding} as node pattern."
            )))
        }
        _ => Ok(()),
    }
}

fn validate_relationship_binding(
    binding: &str,
    expected: BindingKind,
    kinds: &BTreeMap<String, BindingKind>,
) -> CypherPlanResult<()> {
    match kinds.get(binding).copied() {
        Some(kind) if kind != expected && kind != BindingKind::Unknown => {
            Err(CypherPlanError::Invalid(format!(
                "Binder exception: {binding} has data type {} but {} was expected.",
                kind.cypher_type_name(),
                expected.cypher_type_name()
            )))
        }
        _ => Ok(()),
    }
}

fn pattern_element_declares(
    element: &crate::language::cypher::ast::PatternElement,
    binding: &str,
) -> bool {
    element.start.variable.as_deref() == Some(binding)
        || element.chains.iter().any(|chain| {
            chain.node.variable.as_deref() == Some(binding)
                || chain.relationship.variable.as_deref() == Some(binding)
        })
}

fn lower_node_start(
    lowerer: &mut Lowerer,
    node: &NodePattern,
    binding: &str,
    outer_visible: &BTreeSet<String>,
    property_correlation: &BTreeSet<String>,
    outputs: &mut Vec<String>,
) -> CypherPlanResult<Node> {
    let mut source = if outer_visible.contains(binding) {
        let mut bindings = property_correlation.clone();
        bindings.insert(binding.to_string());
        Node::GraphCorrelate {
            bindings: bindings.into_iter().collect(),
        }
    } else {
        if node.variable.is_some() {
            outputs.push(binding.to_string());
        }
        let scan = sources::node_scan(binding.to_string(), node.labels.clone());
        if property_correlation.is_empty() {
            scan
        } else {
            Node::GraphApply {
                kind: ApplyKind::Inner,
                correlation: property_correlation.iter().cloned().collect(),
                outputs: vec![binding.to_string()],
                optional_missing: OptionalMissing::Null,
                left: Node::GraphCorrelate {
                    bindings: property_correlation.iter().cloned().collect(),
                }
                .boxed(),
                right: scan.boxed(),
            }
        }
    };
    if outer_visible.contains(binding) {
        for label in &node.labels {
            source = Node::GraphFilter {
                condition: IrExpr::HasLabel {
                    binding: binding.to_string(),
                    label: label.clone(),
                },
                input: source.boxed(),
            };
        }
    }
    let mut allowed = outer_visible.clone();
    allowed.insert(binding.to_string());
    apply_node_filters(lowerer, source, binding, node, &allowed)
}

fn lower_expand(
    input: Node,
    source: &str,
    target: &str,
    target_exists: bool,
    rel: &RelationshipPattern,
    rel_binding: Option<&str>,
    target_node: &NodePattern,
    path: Option<String>,
    history: Option<String>,
    repeatable_elements: bool,
) -> Node {
    let variable_length = is_variable_length(&rel.range);
    Node::GraphExpand {
        graph: DEFAULT_GRAPH.to_string(),
        source: source.to_string(),
        target: target.to_string(),
        target_mode: if target_exists {
            TargetMode::Existing
        } else {
            TargetMode::BindNew
        },
        target_labels: sources::label_expr(target_node.labels.clone()),
        rel_binding: rel_binding.map(ToString::to_string),
        rel_types: sources::rel_types_expr(rel.types.clone()),
        dir: rel.direction,
        length: Length {
            min: rel.range.min,
            max: rel.range.max,
        },
        history,
        path: path.clone(),
        path_mode: PathMode::Walk,
        match_mode: if repeatable_elements && variable_length {
            MatchMode::RepeatableElements
        } else {
            MatchMode::DifferentRelationships
        },
        path_materialization: if path.is_some() {
            PathMaterialization::NodesAndRelationships
        } else {
            PathMaterialization::None
        },
        path_update: PathUpdate::None,
        input: input.boxed(),
    }
}

fn apply_node_filters(
    lowerer: &mut Lowerer,
    mut input: Node,
    binding: &str,
    node: &NodePattern,
    allowed_bindings: &BTreeSet<String>,
) -> CypherPlanResult<Node> {
    if let Some(properties) = &node.properties {
        input = apply_property_filters(lowerer, input, binding, properties, allowed_bindings)?;
    }
    Ok(input)
}

fn apply_relationship_binding_projection(
    input: Node,
    user_binding: &str,
    expanded_binding: Option<&str>,
    path_binding: Option<&str>,
    source_binding: &str,
    already_visible: bool,
    variable_length: bool,
) -> Node {
    if variable_length {
        let Some(path_binding) = path_binding else {
            return input;
        };
        let recursive_rel = recursive_relationship_expr(path_binding, source_binding);
        if already_visible {
            return Node::GraphFilter {
                condition: IrExpr::Binary {
                    op: IrBinaryOp::Eq,
                    lhs: Box::new(IrExpr::Binding(user_binding.to_string())),
                    rhs: Box::new(recursive_rel),
                },
                input: input.boxed(),
            };
        }
        return Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items: vec![ProjectionItem {
                alias: user_binding.to_string(),
                expr: recursive_rel,
            }],
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        };
    }

    if already_visible {
        if let Some(expanded_binding) = expanded_binding {
            return Node::GraphFilter {
                condition: IrExpr::Binary {
                    op: IrBinaryOp::Eq,
                    lhs: Box::new(IrExpr::Binding(expanded_binding.to_string())),
                    rhs: Box::new(IrExpr::Binding(user_binding.to_string())),
                },
                input: input.boxed(),
            };
        }
    }
    input
}

fn apply_recursive_relationship_predicate(
    lowerer: &mut Lowerer,
    input: Node,
    path_binding: &str,
    source_binding: &str,
    recursive: &RecursiveRelationshipPattern,
    allowed_bindings: &BTreeSet<String>,
) -> CypherPlanResult<Node> {
    let Some(predicate) = &recursive.predicate else {
        return Ok(input);
    };
    let pair = lowerer.synthetic("recursive_pair");
    let predicate = rewrite_recursive_filter_vars(
        predicate,
        &recursive.rel_variable,
        &recursive.node_variable,
        &pair,
    );
    let quantifier = Expr::Quantifier {
        kind: QuantifierKind::All,
        variable: pair,
        collection: Box::new(path_pair_collection_expr(path_binding, source_binding)),
        predicate: Box::new(predicate),
    };
    let (input, condition) =
        lower_property_expr_with_allowed(lowerer, input, &quantifier, allowed_bindings)?;
    Ok(Node::GraphFilter {
        condition,
        input: input.boxed(),
    })
}

fn apply_recursive_relationship_projection(
    input: Node,
    path_binding: &str,
    recursive: &RecursiveRelationshipPattern,
) -> Node {
    let Some(keys) = &recursive.rel_projection_keys else {
        return input;
    };
    Node::GraphProject {
        mode: ProjectMode::PreserveVisible,
        items: vec![ProjectionItem {
            alias: path_binding.to_string(),
            expr: IrExpr::Call {
                name: "path_project_edges".to_string(),
                args: vec![
                    IrExpr::Binding(path_binding.to_string()),
                    IrExpr::List(
                        keys.iter()
                            .map(|key| IrExpr::Lit(Lit::String(key.clone())))
                            .collect(),
                    ),
                ],
            },
        }],
        error_policy: ProjectErrorPolicy::PropagateError,
        input: input.boxed(),
    }
}

fn apply_variable_length_property_filters(
    lowerer: &mut Lowerer,
    mut input: Node,
    path_binding: &str,
    source_binding: &str,
    expr: &Expr,
    allowed_bindings: &BTreeSet<String>,
) -> CypherPlanResult<Node> {
    match expr {
        Expr::Map(items) => {
            for (key, value) in items {
                project::validate_expression_refs(
                    value,
                    allowed_bindings,
                    "variable-length relationship property expression",
                )?;
                let item = lowerer.synthetic("rel");
                let predicate = Expr::Binary {
                    op: crate::language::cypher::ast::BinaryOp::Eq,
                    lhs: Box::new(Expr::Property {
                        target: Box::new(Expr::Variable(item.clone())),
                        key: key.clone(),
                    }),
                    rhs: Box::new(value.clone()),
                };
                let (next, condition) = lower_property_expr_with_allowed(
                    lowerer,
                    input,
                    &relationship_quantifier(path_binding, source_binding, item, predicate),
                    allowed_bindings,
                )?;
                input = Node::GraphFilter {
                    condition,
                    input: next.boxed(),
                };
            }
            Ok(input)
        }
        Expr::Parameter(name) => {
            let item = lowerer.synthetic("rel");
            let predicate = Expr::Function {
                name: "cypher_properties_match".to_string(),
                distinct: false,
                args: vec![Expr::Variable(item.clone()), Expr::Parameter(name.clone())],
            };
            let (input, condition) = lower_property_expr_with_allowed(
                lowerer,
                input,
                &relationship_quantifier(path_binding, source_binding, item, predicate),
                allowed_bindings,
            )?;
            Ok(Node::GraphFilter {
                condition,
                input: input.boxed(),
            })
        }
        other => {
            project::validate_expression_refs(
                other,
                allowed_bindings,
                "variable-length relationship property expression",
            )?;
            let item = lowerer.synthetic("rel");
            let (input, condition) = lower_property_expr_with_allowed(
                lowerer,
                input,
                &relationship_quantifier(path_binding, source_binding, item, other.clone()),
                allowed_bindings,
            )?;
            Ok(Node::GraphFilter {
                condition,
                input: input.boxed(),
            })
        }
    }
}

fn pattern_property_correlation(
    part: &PatternPart,
    outer_visible: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();
    if let Some(properties) = &part.element.start.properties {
        refs.extend(project::expression_candidate_refs(
            properties,
            outer_visible,
        ));
    }
    for chain in &part.element.chains {
        if let Some(properties) = &chain.relationship.properties {
            refs.extend(project::expression_candidate_refs(
                properties,
                outer_visible,
            ));
        }
        if let Some(properties) = &chain.node.properties {
            refs.extend(project::expression_candidate_refs(
                properties,
                outer_visible,
            ));
        }
    }
    refs
}

fn relationship_quantifier(
    path_binding: &str,
    source_binding: &str,
    item: String,
    predicate: Expr,
) -> Expr {
    Expr::Quantifier {
        kind: QuantifierKind::All,
        variable: item,
        collection: Box::new(relationship_collection_expr(path_binding, source_binding)),
        predicate: Box::new(predicate),
    }
}

fn relationship_collection_expr(path_binding: &str, source_binding: &str) -> Expr {
    Expr::Function {
        name: "relationships".to_string(),
        distinct: false,
        args: vec![path_segment_ast_expr(path_binding, source_binding)],
    }
}

fn path_pair_collection_expr(path_binding: &str, source_binding: &str) -> Expr {
    Expr::Function {
        name: "path_pairs".to_string(),
        distinct: false,
        args: vec![path_segment_ast_expr(path_binding, source_binding)],
    }
}

fn path_segment_ast_expr(path_binding: &str, source_binding: &str) -> Expr {
    Expr::Function {
        name: "path_from".to_string(),
        distinct: false,
        args: vec![
            Expr::Variable(path_binding.to_string()),
            Expr::Literal(Literal::String(String::new())),
            Expr::Variable(source_binding.to_string()),
        ],
    }
}

fn recursive_relationship_expr(path_binding: &str, source_binding: &str) -> IrExpr {
    IrExpr::Call {
        name: "recursive_relationship_path".to_string(),
        args: vec![
            IrExpr::Binding(path_binding.to_string()),
            IrExpr::Lit(Lit::String(String::new())),
            IrExpr::Binding(source_binding.to_string()),
        ],
    }
}

fn rewrite_recursive_filter_vars(
    expr: &Expr,
    rel_variable: &str,
    node_variable: &str,
    pair_variable: &str,
) -> Expr {
    rewrite_recursive_filter_vars_with_bound(
        expr,
        rel_variable,
        node_variable,
        pair_variable,
        &mut BTreeSet::new(),
    )
}

fn rewrite_recursive_filter_vars_with_bound(
    expr: &Expr,
    rel_variable: &str,
    node_variable: &str,
    pair_variable: &str,
    bound: &mut BTreeSet<String>,
) -> Expr {
    let pair_property = |key: &str| Expr::Property {
        target: Box::new(Expr::Variable(pair_variable.to_string())),
        key: key.to_string(),
    };
    match expr {
        Expr::Variable(name) if !bound.contains(name) && name == rel_variable => pair_property("r"),
        Expr::Variable(name) if !bound.contains(name) && name == node_variable => {
            pair_property("n")
        }
        Expr::Property { target, key } => Expr::Property {
            target: Box::new(rewrite_recursive_filter_vars_with_bound(
                target,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            )),
            key: key.clone(),
        },
        Expr::LabelPredicate { target, labels } => Expr::LabelPredicate {
            target: Box::new(rewrite_recursive_filter_vars_with_bound(
                target,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            )),
            labels: labels.clone(),
        },
        Expr::List(items) => Expr::List(
            items
                .iter()
                .map(|item| {
                    rewrite_recursive_filter_vars_with_bound(
                        item,
                        rel_variable,
                        node_variable,
                        pair_variable,
                        bound,
                    )
                })
                .collect(),
        ),
        Expr::Map(items) => Expr::Map(
            items
                .iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        rewrite_recursive_filter_vars_with_bound(
                            value,
                            rel_variable,
                            node_variable,
                            pair_variable,
                            bound,
                        ),
                    )
                })
                .collect(),
        ),
        Expr::Unary { op, expr } => Expr::Unary {
            op: op.clone(),
            expr: Box::new(rewrite_recursive_filter_vars_with_bound(
                expr,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            )),
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op: *op,
            lhs: Box::new(rewrite_recursive_filter_vars_with_bound(
                lhs,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            )),
            rhs: Box::new(rewrite_recursive_filter_vars_with_bound(
                rhs,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            )),
        },
        Expr::IsNull(inner) => Expr::IsNull(Box::new(rewrite_recursive_filter_vars_with_bound(
            inner,
            rel_variable,
            node_variable,
            pair_variable,
            bound,
        ))),
        Expr::IsNotNull(inner) => {
            Expr::IsNotNull(Box::new(rewrite_recursive_filter_vars_with_bound(
                inner,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            )))
        }
        Expr::StringPredicate {
            op,
            target,
            pattern,
        } => Expr::StringPredicate {
            op: *op,
            target: Box::new(rewrite_recursive_filter_vars_with_bound(
                target,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            )),
            pattern: Box::new(rewrite_recursive_filter_vars_with_bound(
                pattern,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            )),
        },
        Expr::Function {
            name,
            distinct,
            args,
        } => Expr::Function {
            name: name.clone(),
            distinct: *distinct,
            args: args
                .iter()
                .map(|arg| {
                    rewrite_recursive_filter_vars_with_bound(
                        arg,
                        rel_variable,
                        node_variable,
                        pair_variable,
                        bound,
                    )
                })
                .collect(),
        },
        Expr::Case {
            case,
            arms,
            otherwise,
        } => Expr::Case {
            case: case.as_ref().map(|case| {
                Box::new(rewrite_recursive_filter_vars_with_bound(
                    case,
                    rel_variable,
                    node_variable,
                    pair_variable,
                    bound,
                ))
            }),
            arms: arms
                .iter()
                .map(|(when, then)| {
                    (
                        rewrite_recursive_filter_vars_with_bound(
                            when,
                            rel_variable,
                            node_variable,
                            pair_variable,
                            bound,
                        ),
                        rewrite_recursive_filter_vars_with_bound(
                            then,
                            rel_variable,
                            node_variable,
                            pair_variable,
                            bound,
                        ),
                    )
                })
                .collect(),
            otherwise: otherwise.as_ref().map(|otherwise| {
                Box::new(rewrite_recursive_filter_vars_with_bound(
                    otherwise,
                    rel_variable,
                    node_variable,
                    pair_variable,
                    bound,
                ))
            }),
        },
        Expr::ListComprehension {
            variable,
            collection,
            predicate,
            map,
        } => {
            let collection = rewrite_recursive_filter_vars_with_bound(
                collection,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.insert(variable.clone());
            let predicate = predicate.as_ref().map(|predicate| {
                Box::new(rewrite_recursive_filter_vars_with_bound(
                    predicate,
                    rel_variable,
                    node_variable,
                    pair_variable,
                    bound,
                ))
            });
            let map = rewrite_recursive_filter_vars_with_bound(
                map,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.remove(variable);
            Expr::ListComprehension {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate,
                map: Box::new(map),
            }
        }
        Expr::ListReduce {
            accumulator,
            variable,
            collection,
            map,
        } => {
            let collection = rewrite_recursive_filter_vars_with_bound(
                collection,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.insert(accumulator.clone());
            bound.insert(variable.clone());
            let map = rewrite_recursive_filter_vars_with_bound(
                map,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.remove(accumulator);
            bound.remove(variable);
            Expr::ListReduce {
                accumulator: accumulator.clone(),
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            }
        }
        Expr::ListTransform {
            variable,
            collection,
            map,
        } => {
            let collection = rewrite_recursive_filter_vars_with_bound(
                collection,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.insert(variable.clone());
            let map = rewrite_recursive_filter_vars_with_bound(
                map,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.remove(variable);
            Expr::ListTransform {
                variable: variable.clone(),
                collection: Box::new(collection),
                map: Box::new(map),
            }
        }
        Expr::ListFilter {
            variable,
            collection,
            predicate,
        } => {
            let collection = rewrite_recursive_filter_vars_with_bound(
                collection,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.insert(variable.clone());
            let predicate = rewrite_recursive_filter_vars_with_bound(
                predicate,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.remove(variable);
            Expr::ListFilter {
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            }
        }
        Expr::Quantifier {
            kind,
            variable,
            collection,
            predicate,
        } => {
            let collection = rewrite_recursive_filter_vars_with_bound(
                collection,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.insert(variable.clone());
            let predicate = rewrite_recursive_filter_vars_with_bound(
                predicate,
                rel_variable,
                node_variable,
                pair_variable,
                bound,
            );
            bound.remove(variable);
            Expr::Quantifier {
                kind: *kind,
                variable: variable.clone(),
                collection: Box::new(collection),
                predicate: Box::new(predicate),
            }
        }
        Expr::PatternComprehension { .. }
        | Expr::Exists(_)
        | Expr::PatternPredicate(_)
        | Expr::Star
        | Expr::Variable(_)
        | Expr::Parameter(_)
        | Expr::Literal(_)
        | Expr::CountStar => expr.clone(),
    }
}

fn is_variable_length(range: &RangeLiteral) -> bool {
    range.explicit || range.min != 1 || range.max != Some(1)
}

fn lower_property_expr_with_allowed(
    lowerer: &mut Lowerer,
    input: Node,
    expr: &Expr,
    allowed_bindings: &BTreeSet<String>,
) -> CypherPlanResult<(Node, IrExpr)> {
    lowerer.with_preserved_scope(|lowerer| {
        lowerer.replace_scope(allowed_bindings.iter().cloned().collect::<Vec<_>>());
        project::lower_expr_with_input(lowerer, input, expr)
    })
}

fn apply_property_filters(
    lowerer: &mut Lowerer,
    mut input: Node,
    binding: &str,
    expr: &Expr,
    allowed_bindings: &BTreeSet<String>,
) -> CypherPlanResult<Node> {
    match expr {
        Expr::Map(items) => {
            for (key, value) in items {
                project::validate_expression_refs(
                    value,
                    allowed_bindings,
                    "pattern property expression",
                )?;
                let (next, rhs) =
                    lower_property_expr_with_allowed(lowerer, input, value, allowed_bindings)?;
                input = Node::GraphFilter {
                    condition: IrExpr::Binary {
                        op: IrBinaryOp::Eq,
                        lhs: Box::new(IrExpr::Property {
                            binding: binding.to_string(),
                            name: key.clone(),
                            policy: crate::ir::policy::PropertyMissing::NullOnMissing,
                        }),
                        rhs: Box::new(rhs),
                    },
                    input: next.boxed(),
                };
            }
            Ok(input)
        }
        Expr::Parameter(name) => Ok(Node::GraphFilter {
            condition: IrExpr::Call {
                name: "cypher_properties_match".to_string(),
                args: vec![
                    IrExpr::Binding(binding.to_string()),
                    IrExpr::Call {
                        name: "parameter".to_string(),
                        args: vec![IrExpr::Lit(Lit::String(name.clone()))],
                    },
                ],
            },
            input: input.boxed(),
        }),
        other => {
            project::validate_expression_refs(
                other,
                allowed_bindings,
                "pattern property expression",
            )?;
            let (input, condition) =
                lower_property_expr_with_allowed(lowerer, input, other, allowed_bindings)?;
            Ok(Node::GraphFilter {
                condition,
                input: input.boxed(),
            })
        }
    }
}

fn node_binding(lowerer: &mut Lowerer, node: &NodePattern) -> String {
    node.variable
        .clone()
        .unwrap_or_else(|| lowerer.synthetic("node"))
}

fn pattern_mentions(part: &PatternPart, binding: &str) -> bool {
    part.element.start.variable.as_deref() == Some(binding)
        || part.element.chains.iter().any(|chain| {
            chain.node.variable.as_deref() == Some(binding)
                || chain.relationship.variable.as_deref() == Some(binding)
        })
}

#[allow(dead_code)]
fn reverse_direction(direction: Direction) -> Direction {
    match direction {
        Direction::Out => Direction::In,
        Direction::In => Direction::Out,
        Direction::Both => Direction::Both,
    }
}
