//! Active stage composition over the existing IRs. Domain passes retain their
//! proofs and implementation; this module owns their order and query lifetime.
use super::*;
use crate::ir::rules::{RuleContext, RuleDescriptor, RuleKind, RuleOutcome, RuleSession, Stage, Traversal};

pub(crate) const fn descriptor(id: &'static str, stage: Stage, kind: RuleKind, required: bool) -> RuleDescriptor {
    RuleDescriptor { id, version: 1, stage, kind, traversal: Traversal::Once, required,
        required_facts: &["bound schema"], invalidated_facts: &["relational properties", "island eligibility"] }
}
pub(crate) fn normalize_graph(plan: &GraphPlan, context: &RuleContext) -> RelResult<GraphPlan> {
    context.validate().map_err(RelError::RulePipeline)?;
    crate::ir::rules::graph::normalize(plan, &mut RuleSession::new(context.clone()))
        .map_err(RelError::RulePipeline)
}

/// Catalog-backed procedure binding precedes capability validation. Path
/// representation normalization follows it because bound procedure outputs can
/// expose the graph shape. Both were mandatory with optimization disabled;
/// retaining that contract keeps otherwise supported SQL routes available.
pub(crate) fn prepare_graph_implementations(
    plan: GraphPlan, graph: &PropertyGraph, context: &RuleContext,
) -> RelResult<GraphPlan> {
    let mut session = RuleSession::new(context.clone());
    let binding = graph_implementation("graph.procedure.catalog_binding", RuleKind::ImplementationLowering,
        &["registered immutable procedure snapshot", "constant primitive arguments", "total input"]);
    let (plan, _) = session.apply(binding, plan, |plan, _| {
        let selected = table_procedures::specialize_plan(&plan, graph);
        if selected == plan { RuleOutcome::Unchanged(plan) }
        else { RuleOutcome::Changed { value: selected, evidence: "immutable registered procedure bound before capability validation".into() } }
    }).map_err(RelError::RulePipeline)?;
    let demand = graph_implementation("graph.path.required_representation", RuleKind::EquivalentRewrite,
        &["scoped path producers", "observable payload demand", "nonnull count argument proof"]);
    session.apply(demand, plan, |plan, _| {
        let selected = path_demand::specialize_plan(&plan);
        if selected == plan { RuleOutcome::Unchanged(plan) }
        else { RuleOutcome::Changed { value: selected, evidence: "path payload demand normalized; traversal, trail history and row multiplicity retained".into() } }
    }).map(|(plan, _)| plan).map_err(RelError::RulePipeline)
}

const fn graph_implementation(id: &'static str, kind: RuleKind, facts: &'static [&'static str]) -> RuleDescriptor {
    RuleDescriptor { id, version: 1, stage: Stage::GraphNormalize, kind,
        traversal: Traversal::Once, required: true, required_facts: facts,
        invalidated_facts: &["graph bindings", "path demand", "island eligibility"] }
}

/// Island construction and rewritten subtrees can encounter a procedure call
/// without passing the whole-plan binding boundary. Revisit only actual calls;
/// calls already bound to values cannot select the implementation twice.
pub(crate) fn bind_procedure_call(
    node: &Node, graph: &PropertyGraph, context: &RuleContext,
) -> RelResult<Option<Node>> {
    if !matches!(node, Node::GraphProcedureCall { .. }) { return Ok(None); }
    let rule = graph_implementation("graph.procedure.catalog_binding", RuleKind::ImplementationLowering,
        &["registered immutable procedure snapshot", "constant primitive arguments", "total input"]);
    let mut session = RuleSession::new(context.clone());
    session.visit(rule).map_err(RelError::RulePipeline)?;
    let selected = table_procedures::specialize_call(node, graph);
    if selected.is_some() {
        session.changed(rule, || "catalog-backed procedure binding revisited for an island or constructed subtree".into());
    }
    Ok(selected)
}

impl LoweringContext<'_> {
    /// Scalar carriers for proven map projections are implementation choices:
    /// preserve their original availability even when optional optimization is off.
    /// Recursive projection lowering re-enters the shared bounded rule budget.
    pub(super) fn run_projection_rules(
        &self, mode: ProjectMode, items: &[ProjectionItem], input: &Node,
    ) -> RelResult<Option<(Vec<ProjectionItem>, Node)>> {
        if self.language != Language::Cypher { return Ok(None); }
        if !self.options.native_values {
            let rule = graph_implementation("graph.map.collected_projection", RuleKind::ImplementationLowering,
                &["closed map consumers", "aggregate cardinality", "lexical scope"]);
            if let Some(selected) = self.scalar_rule(rule, || Ok(
                cypher_maps::scalarize_collected_map_projection(mode, items, input)
            ))? { return Ok(Some(selected)); }
        }
        let rule = graph_implementation("graph.map.scalar_projection", RuleKind::ImplementationLowering,
            &["closed map consumers", "field identity", "lexical scope"]);
        self.scalar_rule(rule, || Ok(cypher_maps::scalarize_projection(mode, items, input)))
    }
}

/// Revisits after binding are intentional: new source constraints can become
/// available. All proof-backed rewrites keep the existing proof vocabulary.
pub(crate) fn relational(plan: LogicalPlan, context: &RuleContext, late: bool)
    -> RelResult<(LogicalPlan, Vec<constraints::RewriteProof>)> {
    let rule = descriptor(if late {"rel.constraints.after_binding"} else {"rel.constraints"},
        if late {Stage::LateTyped} else {Stage::RelationalOptimize}, RuleKind::EquivalentRewrite, false);
    let mut proofs = Vec::new();
    let mut session = RuleSession::new(context.clone());
    let (result, _) = session.apply(rule, plan, |plan, _| match constraints::optimize(plan) {
        Ok((plan, evidence)) => {
            let changed = !evidence.is_empty();
            proofs = evidence;
            if changed { RuleOutcome::Changed { value: plan, evidence: "catalog-backed relational proofs".into() } }
            else { RuleOutcome::Unchanged(plan) }
        }
        Err(error) => RuleOutcome::Error(error.to_string()),
    }).map_err(RelError::RulePipeline)?;
    for proof in &proofs {
        session.record(rule, "proof", || format!("{}: {}; {}", proof.rule, proof.reason, proof.evidence.join("; ")));
    }
    let empty_rule = descriptor(if late {"rel.empty_inputs.after_binding"} else {"rel.empty_inputs"},
        if late {Stage::LateTyped} else {Stage::RelationalOptimize}, RuleKind::EquivalentRewrite, false);
    let (result, _) = session.apply(empty_rule, result, |plan, _| {
        let original = plan.clone();
        match empty_relations::simplify(plan) {
            Ok(plan) if plan == original => RuleOutcome::Unchanged(plan),
            Ok(plan) => RuleOutcome::Changed { value: plan, evidence: "structurally empty inputs; preserve outer rows and null-extension schema".into() },
            Err(error) => RuleOutcome::Error(error.to_string()),
        }
    }).map_err(RelError::RulePipeline)?;
    Ok((result, proofs))
}

/// Typed adapter rules use structural equality, never formatted plans, to
/// report changes. Input and result schemas remain part of the rule contract.
pub(crate) fn typed_sql(
    plan: LogicalPlan, context: &RuleContext, id: &'static str, required: bool,
    rewrite: impl FnOnce(LogicalPlan) -> sql::SqlResult<LogicalPlan>,
) -> sql::SqlResult<LogicalPlan> {
    let rule = descriptor(id, Stage::AdapterPrepare, RuleKind::EquivalentRewrite, required);
    let mut session = RuleSession::new(context.clone());
    let (plan, _) = session.apply(rule, plan, |plan, _| {
        let original = plan.clone();
        match rewrite(plan) {
            Ok(plan) if plan == original => RuleOutcome::Unchanged(plan),
            Ok(plan) => RuleOutcome::Changed { value: plan, evidence: "typed relational rewrite; original eligibility guards retained".into() },
            Err(error) => RuleOutcome::Error(error.to_string()),
        }
    }).map_err(sql::SqlError::RulePipeline)?;
    Ok(plan)
}

/// All terminal emitters, including compact alternatives, enter here. No
/// policy is inferred from whichever SQL region happened to win placement.
pub(crate) fn validate_sql_contract(plan: &LogicalPlan, dialect: sql::SqlDialect, context: &RuleContext) -> sql::SqlResult<()> {
    use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
    context.validate().map_err(sql::SqlError::RulePipeline)?;
    let rule = descriptor("adapter.validate_contract", Stage::Validate, RuleKind::ImplementationLowering, true);
    let mut session = RuleSession::new(context.clone());
    session.visit(rule).map_err(sql::SqlError::RulePipeline)?;
    plan.apply_with_subqueries(|node| {
        for expr in node.expressions() {
            expr.apply(|expr| {
                if let Expr::ScalarFunction(call) = expr {
                    if call.func.name() == conversion_rules::NATIVE_FLOAT && dialect != sql::SqlDialect::DuckDb {
                        return Err(DataFusionError::Plan("native string-to-float requires its owning DuckDB engine".into()));
                    }
                    if call.func.name() == "__orchiddb_cypher_to_float_v1" && (!context.supports_float_helper() || dialect != sql::SqlDialect::DuckDb) {
                        return Err(DataFusionError::Plan("conformant float helper requires a verified capability for this SQL session".into()));
                    }
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    session.record(rule, "validated", || "semantic ownership and typed helper capability checked before terminal emission".into());
    Ok(())
}

/// Engine-owned operations cannot migrate to a residual evaluator, including
/// empty inputs where a throwing placeholder would otherwise never be invoked.
pub(crate) fn validate_host_contract(plan: &LogicalPlan) -> RelResult<()> {
    use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
    plan.apply_with_subqueries(|node| {
        for expression in node.expressions() {
            expression.apply(|expr| {
                if matches!(expr, Expr::ScalarFunction(call) if call.func.name() == conversion_rules::NATIVE_FLOAT) {
                    return Err(DataFusionError::Plan("native string-to-float requires its owning DuckDB engine; residual placement is forbidden".into()));
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
        }
        Ok(TreeNodeRecursion::Continue)
    })?;
    Ok(())
}

pub(crate) fn placement(plan: LogicalPlan, context: &RuleContext) -> RelResult<representation::SelectedPlan> {
    let rule = descriptor("placement.representation", Stage::Placement, RuleKind::ImplementationLowering, true);
    let mut session = RuleSession::new(context.clone());
    session.visit(rule).map_err(RelError::RulePipeline)?;
    let original = plan.clone();
    let selected = representation::select(plan)?;
    if selected.plan != original { session.invalidate(); }
    session.record(rule, "selected", || "representation selected only after semantic resolution; source facts retained".into());
    Ok(selected)
}


#[cfg(test)]
mod graph_implementation_tests {
    use super::*;

    #[test]
    fn required_graph_implementation_stages_cannot_be_skipped() {
        let graph = crate::language::cypher::preparation::prepare(
            "RETURN 1 AS n", &BTreeMap::new(), None,
        ).unwrap();
        for (budget, expected) in [
            (0, "graph.procedure.catalog_binding"),
            (1, "graph.path.required_representation"),
        ] {
            let mut context = RuleContext::default();
            context.optimize = false;
            context.budget.stage_visits = budget;
            let result = prepare_graph_implementations(graph.clone(), &PropertyGraph::new(), &context);
            assert!(matches!(result, Err(RelError::RulePipeline(ref error))
                if error.to_string().contains(expected)), "required stage {expected} bypassed: {result:?}");
        }
        let mut context = RuleContext::default();
        context.optimize = false;
        let prepared = prepare_graph_implementations(graph.clone(), &PropertyGraph::new(), &context).unwrap();
        assert_eq!(prepared, graph);
        assert_eq!(context.visits(), 2);
    }
}
