use std::collections::HashSet;

use crate::ir::expr::{BindingId, IrExpr};
use crate::ir::plan::{
    ApplyKind, CreateEdge, CreateNode, Direction, Node, ProcedureArg, ProcedureMode,
    ProjectErrorPolicy, ProjectMode, ProjectionItem, SetMode, SetPropertyItem,
};
use crate::ir::policy::OptionalMissing;
use crate::language::cypher::ast::{
    Clause, CreateClause, DeleteClause, Expr, MatchClause, MergeClause, NodePattern,
    ProcedureCallClause, SetClause, SetItem, UnwindClause,
};
use crate::language::cypher::planner::error::{CypherPlanError, CypherPlanResult};
use crate::language::cypher::planner::lowering::{
    Lowerer,
    context::{BindingKind, CypherTraversalKind},
    pattern, predicate, project,
};

pub fn lower_clause(lowerer: &mut Lowerer, input: Node, clause: &Clause) -> CypherPlanResult<Node> {
    match clause {
        Clause::Match(clause) => lower_match(lowerer, input, clause),
        Clause::Unwind(clause) => lower_unwind(lowerer, input, clause),
        Clause::Call(clause) => lower_call(lowerer, input, clause),
        Clause::Create(clause) => lower_create(lowerer, input, clause),
        Clause::Merge(clause) => lower_merge(lowerer, input, clause),
        Clause::Set(clause) => lower_set(lowerer, input, clause),
        Clause::Delete(clause) => lower_delete(lowerer, input, clause),
        Clause::With(clause) => project::lower_with(lowerer, input, clause),
        Clause::Return(clause) => project::lower_return(lowerer, input, clause),
    }
}

fn lower_match(
    lowerer: &mut Lowerer,
    mut input: Node,
    clause: &MatchClause,
) -> CypherPlanResult<Node> {
    if clause.optional {
        return lower_optional_match(lowerer, input, clause);
    }

    let history = pattern_history_binding(lowerer, &clause.patterns);
    for part in &clause.patterns {
        input =
            pattern::lower_pattern_part(lowerer, input, part, false, history.as_deref(), false)?;
    }
    if let Some(predicate) = &clause.predicate {
        input = lowerer.with_child_traversal(CypherTraversalKind::WherePredicate, |lowerer| {
            let input = predicate::lower_where_predicate(lowerer, input, predicate)?;
            lowerer.record_current_imports(lowerer.visible_fields());
            lowerer.record_current_correlation(lowerer.visible_fields());
            Ok(input)
        })?;
    }
    Ok(input)
}

fn lower_optional_match(
    lowerer: &mut Lowerer,
    input: Node,
    clause: &MatchClause,
) -> CypherPlanResult<Node> {
    let outer_fields = lowerer.visible_fields();
    let outer_visible = lowerer.visible_set();
    let (right, outputs, output_kinds) = lowerer.with_preserved_scope(|lowerer| {
        lowerer.with_child_traversal(CypherTraversalKind::OptionalMatchPattern, |lowerer| {
            let mut right = Node::GraphCorrelate {
                bindings: outer_fields.clone(),
            };
            let history = pattern_history_binding(lowerer, &clause.patterns);
            for part in &clause.patterns {
                right = pattern::lower_pattern_part(
                    lowerer,
                    right,
                    part,
                    false,
                    history.as_deref(),
                    false,
                )?;
            }
            if let Some(predicate) = &clause.predicate {
                right = lowerer.with_child_traversal(
                    CypherTraversalKind::WherePredicate,
                    |lowerer| {
                        let right = predicate::lower_where_predicate(lowerer, right, predicate)?;
                        lowerer.record_current_imports(lowerer.visible_fields());
                        lowerer.record_current_correlation(lowerer.visible_fields());
                        Ok(right)
                    },
                )?;
            }

            let outputs = lowerer
                .visible_fields()
                .into_iter()
                .filter(|binding| !outer_visible.contains(binding))
                .collect::<Vec<_>>();
            let output_kinds = outputs
                .iter()
                .map(|binding| {
                    (
                        binding.clone(),
                        lowerer
                            .binding_kind(binding)
                            .unwrap_or(BindingKind::Unknown),
                    )
                })
                .collect::<Vec<_>>();
            Ok((right, outputs, output_kinds))
        })
    })?;

    let node = Node::GraphApply {
        kind: ApplyKind::Optional,
        correlation: outer_fields.clone(),
        outputs: outputs.clone(),
        optional_missing: OptionalMissing::Null,
        left: input.boxed(),
        right: right.boxed(),
    };
    lowerer.record_current_imports(outer_fields.clone());
    lowerer.record_current_correlation(outer_fields);
    lowerer.record_current_outputs(outputs.clone());
    lowerer.record_current_nullable(outputs.clone());
    for output in outputs {
        lowerer.add_nullable(output.clone());
        let kind = output_kinds
            .iter()
            .find_map(|(binding, kind)| (binding == &output).then_some(*kind))
            .unwrap_or(BindingKind::Unknown);
        lowerer.add_visible_kind(output, kind);
    }
    Ok(node)
}

fn lower_unwind(
    lowerer: &mut Lowerer,
    input: Node,
    clause: &UnwindClause,
) -> CypherPlanResult<Node> {
    let node = lowerer.with_child_traversal(CypherTraversalKind::Unwind, |lowerer| {
        if lowerer.is_visible(&clause.alias) {
            return Err(CypherPlanError::Invalid(format!(
                "Binder exception: Variable {} already exists.",
                clause.alias
            )));
        }
        project::validate_expression_scope(lowerer, &clause.expr, "UNWIND expression")?;
        let (input, input_expr) = project::lower_expr_with_input(lowerer, input, &clause.expr)?;
        let node = Node::GraphUnwind {
            input_expr,
            bind: clause.alias.clone(),
            outer: false,
            input: input.boxed(),
        };
        lowerer.record_current_imports(lowerer.visible_fields());
        lowerer.record_current_outputs(vec![clause.alias.clone()]);
        Ok(node)
    })?;
    lowerer.add_visible(clause.alias.clone());
    Ok(node)
}

fn lower_create(
    lowerer: &mut Lowerer,
    input: Node,
    clause: &CreateClause,
) -> CypherPlanResult<Node> {
    let (node, node_outputs, edge_outputs) =
        lowerer.with_child_traversal(CypherTraversalKind::Create, |lowerer| {
            let mut state = CreateState::default();
            let mut input = input;
            for part in &clause.patterns {
                if part.variable.is_some() {
                    return Err(CypherPlanError::Unsupported(
                        "CREATE path-variable binding is not implemented yet".into(),
                    ));
                }
                let element = &part.element;
                let mut left = create_endpoint(lowerer, &mut state, &mut input, &element.start)?;
                for chain in &element.chains {
                    let right = create_endpoint(lowerer, &mut state, &mut input, &chain.node)?;
                    let rel = &chain.relationship;
                    if rel.types.len() != 1 {
                        return Err(CypherPlanError::Invalid(
                            "Binder exception: Create relationship requires exactly one \
                             relationship label."
                                .into(),
                        ));
                    }
                    if rel.range.explicit {
                        return Err(CypherPlanError::Invalid(
                            "Binder exception: Create relationship must have a single hop."
                                .into(),
                        ));
                    }
                    if rel.recursive.is_some() {
                        return Err(CypherPlanError::Unsupported(
                            "CREATE recursive relationship patterns are not implemented yet".into(),
                        ));
                    }
                    let (src, dst) = match rel.direction {
                        Direction::Out => (left.clone(), right.clone()),
                        Direction::In => (right.clone(), left.clone()),
                        Direction::Both => {
                            return Err(CypherPlanError::Invalid(
                                "Binder exception: Create undirected relationship is not \
                                 supported."
                                    .into(),
                            ));
                        }
                    };
                    if let Some(bind) = &rel.variable {
                        if lowerer.is_visible(bind) || state.bound.contains(bind) {
                            return Err(CypherPlanError::Invalid(format!(
                                "Binder exception: Variable {bind} already exists."
                            )));
                        }
                        state.bound.insert(bind.clone());
                        state.edge_outputs.push(bind.clone());
                    }
                    let properties = lower_create_properties(
                        lowerer,
                        &mut input,
                        rel.properties.as_ref(),
                    )?;
                    state.edges.push(CreateEdge {
                        bind: rel.variable.clone(),
                        rel_type: rel.types[0].clone(),
                        src,
                        dst,
                        properties,
                    });
                    left = right;
                }
            }
            let CreateState {
                nodes,
                edges,
                node_outputs,
                edge_outputs,
                ..
            } = state;
            let node = Node::GraphCreate {
                graph: "default".to_string(),
                nodes,
                edges,
                input: input.boxed(),
            };
            lowerer.record_current_imports(lowerer.visible_fields());
            let mut outputs = node_outputs.clone();
            outputs.extend(edge_outputs.iter().cloned());
            lowerer.record_current_outputs(outputs);
            Ok((node, node_outputs, edge_outputs))
        })?;
    for output in node_outputs {
        lowerer.add_visible_kind(output, BindingKind::Node);
    }
    for output in edge_outputs {
        lowerer.add_visible_kind(output, BindingKind::Relationship);
    }
    Ok(node)
}

/// Accumulator threaded through one CREATE clause. `bound` holds every
/// variable the clause has introduced so far, so a variable repeated across
/// pattern parts (`CREATE (a:A)-[:R]->(b:B), (a)-[:R2]->(c:C)`) and
/// self-loops `(a)-[:R]->(a)` reuse the first binding instead of erroring.
#[derive(Default)]
struct CreateState {
    nodes: Vec<CreateNode>,
    edges: Vec<CreateEdge>,
    node_outputs: Vec<BindingId>,
    edge_outputs: Vec<BindingId>,
    bound: HashSet<BindingId>,
}

fn lower_create_properties(
    lowerer: &mut Lowerer,
    input: &mut Node,
    properties: Option<&Expr>,
) -> CypherPlanResult<Option<IrExpr>> {
    let Some(properties) = properties else {
        return Ok(None);
    };
    project::validate_expression_scope(lowerer, properties, "CREATE properties")?;
    let owned = std::mem::replace(input, Node::GraphEmpty);
    let (next, lowered) = project::lower_expr_with_input(lowerer, owned, properties)?;
    *input = next;
    Ok(Some(lowered))
}

/// Resolve one node pattern inside CREATE to the binding its edges use,
/// registering a new `CreateNode` unless the variable already resolves.
fn create_endpoint(
    lowerer: &mut Lowerer,
    state: &mut CreateState,
    input: &mut Node,
    pattern: &NodePattern,
) -> CypherPlanResult<BindingId> {
    // A variable already in scope (from MATCH, or from an earlier part of
    // this same clause) refers to the existing node; only its bare form is
    // legal, since re-stating labels or properties would be a redefinition.
    if let Some(bind) = &pattern.variable {
        if lowerer.is_visible(bind) || state.bound.contains(bind) {
            if !pattern.labels.is_empty() || pattern.properties.is_some() {
                return Err(CypherPlanError::Invalid(format!(
                    "Binder exception: Variable {bind} already exists."
                )));
            }
            return Ok(bind.clone());
        }
    }
    if pattern.labels.len() != 1 {
        return Err(CypherPlanError::Invalid(
            "Binder exception: Create node requires exactly one node label.".into(),
        ));
    }
    let properties = lower_create_properties(lowerer, input, pattern.properties.as_ref())?;
    let bind = match &pattern.variable {
        Some(bind) => {
            state.bound.insert(bind.clone());
            state.node_outputs.push(bind.clone());
            bind.clone()
        }
        // Anonymous nodes still need a name so edges can reach them; the
        // synthetic binding never becomes visible to the user's scope.
        None => lowerer.synthetic("create_node"),
    };
    state.nodes.push(CreateNode {
        bind: Some(bind.clone()),
        label: pattern.labels[0].clone(),
        properties,
    });
    Ok(bind)
}

fn lower_set(lowerer: &mut Lowerer, input: Node, clause: &SetClause) -> CypherPlanResult<Node> {
    lower_set_items(lowerer, input, &clause.items)
}

/// `MERGE` lowers to two correlated arms over the same pattern: a MATCH-shaped
/// arm carrying `ON MATCH SET`, and a CREATE-shaped arm carrying
/// `ON CREATE SET`. `GraphMerge` runs the second only when the first is empty.
fn lower_merge(
    lowerer: &mut Lowerer,
    input: Node,
    clause: &MergeClause,
) -> CypherPlanResult<Node> {
    let outer_fields = lowerer.visible_fields();
    let outer_visible = lowerer.visible_set();

    let (match_arm, outputs, output_kinds) = lowerer.with_preserved_scope(|lowerer| {
        lowerer.with_child_traversal(CypherTraversalKind::MatchPattern, |lowerer| {
            let mut arm = Node::GraphCorrelate {
                bindings: outer_fields.clone(),
            };
            arm = pattern::lower_pattern_part(lowerer, arm, &clause.pattern, false, None, false)?;
            if !clause.on_match.is_empty() {
                arm = lower_set_items(lowerer, arm, &clause.on_match)?;
            }
            let outputs = lowerer
                .visible_fields()
                .into_iter()
                .filter(|binding| !outer_visible.contains(binding))
                .collect::<Vec<_>>();
            let output_kinds = outputs
                .iter()
                .map(|binding| {
                    (
                        binding.clone(),
                        lowerer
                            .binding_kind(binding)
                            .unwrap_or(BindingKind::Unknown),
                    )
                })
                .collect::<Vec<_>>();
            Ok((arm, outputs, output_kinds))
        })
    })?;

    let create_clause = CreateClause {
        patterns: vec![clause.pattern.clone()],
    };
    let create_arm = lowerer.with_preserved_scope(|lowerer| {
        let arm = Node::GraphCorrelate {
            bindings: outer_fields.clone(),
        };
        let mut arm = lower_create(lowerer, arm, &create_clause)?;
        if !clause.on_create.is_empty() {
            arm = lower_set_items(lowerer, arm, &clause.on_create)?;
        }
        Ok(arm)
    })?;

    let node = Node::GraphMerge {
        correlation: outer_fields.clone(),
        outputs: outputs.clone(),
        input: input.boxed(),
        match_arm: match_arm.boxed(),
        create_arm: create_arm.boxed(),
    };
    lowerer.record_current_imports(outer_fields.clone());
    lowerer.record_current_correlation(outer_fields);
    lowerer.record_current_outputs(outputs.clone());
    for output in outputs {
        let kind = output_kinds
            .iter()
            .find_map(|(binding, kind)| (binding == &output).then_some(*kind))
            .unwrap_or(BindingKind::Unknown);
        lowerer.add_visible_kind(output, kind);
    }
    Ok(node)
}

fn lower_set_items(
    lowerer: &mut Lowerer,
    input: Node,
    clause_items: &[SetItem],
) -> CypherPlanResult<Node> {
    lowerer.with_child_traversal(CypherTraversalKind::Set, |lowerer| {
        let mut input = input;
        let mut items = Vec::new();
        for item in clause_items {
            match item {
                SetItem::Property { target, key, value } => {
                    project::validate_expression_scope(lowerer, target, "SET property target")?;
                    project::validate_expression_scope(lowerer, value, "SET property value")?;
                    let (next, target) = project::lower_expr_with_input(lowerer, input, target)?;
                    input = next;
                    let (next, value) = project::lower_expr_with_input(lowerer, input, value)?;
                    input = next;
                    items.push(SetPropertyItem {
                        target,
                        key: key.clone(),
                        mode: SetMode::Property,
                        value,
                    });
                }
                SetItem::Replace { variable, value } | SetItem::Merge { variable, value } => {
                    let mode = match item {
                        SetItem::Replace { .. } => SetMode::Replace,
                        _ => SetMode::Merge,
                    };
                    if !lowerer.is_visible(variable) {
                        return Err(CypherPlanError::Invalid(format!(
                            "Binder exception: Variable {variable} is not in scope."
                        )));
                    }
                    project::validate_expression_scope(lowerer, value, "SET property value")?;
                    let target_expr = Expr::Variable(variable.clone());
                    let (next, target) =
                        project::lower_expr_with_input(lowerer, input, &target_expr)?;
                    input = next;
                    let (next, value) = project::lower_expr_with_input(lowerer, input, value)?;
                    input = next;
                    items.push(SetPropertyItem {
                        target,
                        key: String::new(),
                        mode,
                        value,
                    });
                }
                SetItem::Labels { variable, .. } => {
                    return Err(CypherPlanError::Unsupported(format!(
                        "SET {variable}:Label is not implemented yet"
                    )));
                }
            }
        }
        let node = Node::GraphSetProperty {
            items,
            input: input.boxed(),
        };
        lowerer.record_current_imports(lowerer.visible_fields());
        lowerer.record_current_outputs(Vec::new());
        Ok(node)
    })
}

fn lower_delete(
    lowerer: &mut Lowerer,
    input: Node,
    clause: &DeleteClause,
) -> CypherPlanResult<Node> {
    lowerer.with_child_traversal(CypherTraversalKind::Delete, |lowerer| {
        let mut input = input;
        let mut targets = Vec::new();
        for expr in &clause.expressions {
            project::validate_expression_scope(lowerer, expr, "DELETE expression")?;
            let (next, target) = project::lower_expr_with_input(lowerer, input, expr)?;
            input = next;
            targets.push(target);
        }
        let node = Node::GraphDelete {
            targets,
            detach: clause.detach,
            input: input.boxed(),
        };
        lowerer.record_current_imports(lowerer.visible_fields());
        lowerer.record_current_outputs(Vec::new());
        Ok(node)
    })
}

fn lower_call(
    lowerer: &mut Lowerer,
    input: Node,
    clause: &ProcedureCallClause,
) -> CypherPlanResult<Node> {
    let (source_yields, alias_yields) = procedure_yields(clause);
    validate_unique_yields(&alias_yields)?;
    if !clause.standalone
        && source_yields.is_empty()
        && matches!(procedure_mode(&clause.name), ProcedureMode::Read)
    {
        return Err(CypherPlanError::Invalid(format!(
            "procedure `{}` declares result fields and requires explicit YIELD inside a larger query",
            clause.name
        )));
    }
    let visible = lowerer.visible_set();
    let rebound = alias_yields
        .iter()
        .filter(|yield_name| visible.contains(*yield_name))
        .cloned()
        .collect::<Vec<_>>();
    if !rebound.is_empty() {
        return Err(CypherPlanError::Invalid(format!(
            "procedure `{}` tries to rebind variables already in scope: {}",
            clause.name,
            rebound.join(", ")
        )));
    }

    let (node, imports) =
        lowerer.with_child_traversal(CypherTraversalKind::ProcedureCall, |lowerer| {
            let mut call_input = input;
            let mut args = Vec::with_capacity(clause.args.len());
            for arg in &clause.args {
                project::validate_expression_scope(lowerer, arg, "procedure argument")?;
                let (next, value) = project::lower_expr_with_input(lowerer, call_input, arg)?;
                call_input = next;
                args.push(ProcedureArg { name: None, value });
            }
            let call = Node::GraphProcedureCall {
                name: clause.name.clone(),
                args,
                yields: source_yields.clone(),
                mode: procedure_mode(&clause.name),
                input: if clause.standalone && clause.args.is_empty() {
                    None
                } else {
                    Some(call_input.boxed())
                },
            };

            let mut node = if source_yields != alias_yields {
                let mut items = lowerer
                    .visible_fields()
                    .into_iter()
                    .map(|field| ProjectionItem {
                        alias: field.clone(),
                        expr: crate::ir::expr::IrExpr::Binding(field),
                    })
                    .collect::<Vec<_>>();
                items.extend(source_yields.iter().zip(alias_yields.iter()).map(
                    |(field, alias)| ProjectionItem {
                        alias: alias.clone(),
                        expr: crate::ir::expr::IrExpr::Binding(field.clone()),
                    },
                ));
                Node::GraphProject {
                    mode: ProjectMode::ReplaceScope,
                    items,
                    error_policy: ProjectErrorPolicy::PropagateError,
                    input: call.boxed(),
                }
            } else {
                call
            };

            let imports = lowerer.visible_fields();
            for output in &alias_yields {
                lowerer.add_visible(output.clone());
            }
            if let Some(predicate) = &clause.predicate {
                node = lowerer.with_child_traversal(
                    CypherTraversalKind::WherePredicate,
                    |lowerer| {
                        let node = predicate::lower_where_predicate(lowerer, node, predicate)?;
                        lowerer.record_current_imports(lowerer.visible_fields());
                        lowerer.record_current_correlation(lowerer.visible_fields());
                        Ok(node)
                    },
                )?;
            }

            lowerer.record_current_imports(imports.clone());
            lowerer.record_current_outputs(alias_yields.clone());
            Ok((node, imports))
        })?;
    for output in &alias_yields {
        lowerer.add_visible(output.clone());
    }
    lowerer.record_current_imports(imports);
    lowerer.record_current_outputs(alias_yields.clone());
    Ok(node)
}

fn validate_unique_yields(yields: &[String]) -> CypherPlanResult<()> {
    let mut seen = std::collections::BTreeSet::new();
    let duplicates = yields
        .iter()
        .filter(|yield_name| !seen.insert((*yield_name).clone()))
        .cloned()
        .collect::<Vec<_>>();
    if duplicates.is_empty() {
        Ok(())
    } else {
        Err(CypherPlanError::Invalid(format!(
            "procedure YIELD contains duplicate output variables: {}",
            duplicates.join(", ")
        )))
    }
}

fn procedure_yields(clause: &ProcedureCallClause) -> (Vec<String>, Vec<String>) {
    if !clause.yields.is_empty() {
        return (
            clause
                .yields
                .iter()
                .map(|item| item.field.clone())
                .collect(),
            clause
                .yields
                .iter()
                .map(|item| item.alias.clone())
                .collect(),
        );
    }
    if clause.yield_all || clause.standalone {
        let yields = default_procedure_yields(&clause.name);
        return (yields.clone(), yields);
    }
    (Vec::new(), Vec::new())
}

fn default_procedure_yields(name: &str) -> Vec<String> {
    match name.to_ascii_lowercase().as_str() {
        "db.labels" => vec!["label".to_string()],
        "db.relationshiptypes" => vec!["relationshipType".to_string()],
        "db.propertykeys" => vec!["propertyKey".to_string()],
        _ => vec!["value".to_string()],
    }
}

fn procedure_mode(name: &str) -> ProcedureMode {
    match name.to_ascii_lowercase().as_str() {
        "db.labels" | "db.relationshiptypes" | "db.propertykeys" => ProcedureMode::Read,
        _ => ProcedureMode::Write,
    }
}

fn pattern_history_binding(
    lowerer: &mut Lowerer,
    patterns: &[crate::language::cypher::ast::PatternPart],
) -> Option<String> {
    patterns
        .iter()
        .any(|part| !part.element.chains.is_empty())
        .then(|| lowerer.synthetic("match_history"))
}
