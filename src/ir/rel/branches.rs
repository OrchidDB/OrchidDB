//! Branches.

use super::*;

impl<'a> LoweringContext<'a> {
    pub(super) fn lower_union(
        &mut self,
        all: bool,
        align: UnionAlign,
        left: &Node,
        right: &Node,
    ) -> RelResult<LoweredNode> {
        let left = self.lower_node(left)?;
        let right = self.lower_node(right)?;
        let left_rdf_identity = left
            .plan
            .schema()
            .fields()
            .iter()
            .filter(|field| field.name().starts_with("__rdf:term:"))
            .map(|field| field.name().to_string())
            .collect::<BTreeSet<_>>();
        let right_rdf_identity = right
            .plan
            .schema()
            .fields()
            .iter()
            .filter(|field| field.name().starts_with("__rdf:term:"))
            .map(|field| field.name().to_string())
            .collect::<BTreeSet<_>>();
        if left_rdf_identity != right_rdf_identity
            && (!left_rdf_identity.is_empty() || !right_rdf_identity.is_empty())
        {
            return Err(RelError::Unsupported(
                "SPARQL UNION cannot align branches with different RDF term identity columns"
                    .into(),
            ));
        }
        let builder = LogicalPlanBuilder::from(left.plan.clone());
        let plan = match (all, align) {
            (true, UnionAlign::ByPosition) if self.language == Language::Gremlin => {
                builder.union_by_name(right.plan.clone())?.build()?
            }
            (false, UnionAlign::ByPosition) if self.language == Language::Gremlin => builder
                .union_by_name_distinct(right.plan.clone())?
                .build()?,
            (true, UnionAlign::ByPosition) => builder.union(right.plan.clone())?.build()?,
            (false, UnionAlign::ByPosition) => {
                builder.union_distinct(right.plan.clone())?.build()?
            }
            (true, UnionAlign::ByVariableName) => {
                builder.union_by_name(right.plan.clone())?.build()?
            }
            (false, UnionAlign::ByVariableName) => builder
                .union_by_name_distinct(right.plan.clone())?
                .build()?,
        };
        let mut islands = left.islands;
        islands.merge(right.islands);
        Ok(LoweredNode {
            plan,
            islands,
            fields: left.fields,
            result_form: left.result_form,
        })
    }

    pub(super) fn lower_coalesce(
        &mut self,
        success: CoalesceSuccess,
        output: &str,
        correlation: &[String],
        input: &Node,
        arms: &[Node],
    ) -> RelResult<LoweredNode> {
        apply::lower_coalesce(self, success, output, correlation, input, arms)
    }

    pub(super) fn lower_choose(
        &mut self,
        selector: &ChooseSelector,
        arms: &[ChooseArm],
        default: Option<&Node>,
        unmatched: ChooseUnmatched,
        input: &Node,
    ) -> RelResult<LoweredNode> {
        let input = self.lower_node(input)?;
        let arm_conditions = self.choose_arm_conditions(&input.plan, selector, arms)?;
        let mut branches = Vec::<LoweredNode>::new();
        let mut unmatched_condition: Option<Expr> = None;

        for (arm, condition) in arms.iter().zip(arm_conditions.iter()) {
            let filtered = LogicalPlanBuilder::from(input.plan.clone())
                .filter(condition.clone())?
                .build()?;
            if !matches!(arm.body, Node::GraphEmpty) {
                let mut branch = self.lower_with_correlate(filtered.clone(), &arm.body)?;
                if matches!(selector, ChooseSelector::Predicates(_)) {
                    // A reducer must not manufacture a row for an empty routed
                    // arm. Gate against the saved stream, preserving any outer
                    // Apply occurrence keys rather than testing globally.
                    let keys = apply_correlation_key_columns(&filtered);
                    let mut projections = Vec::new();
                    let mut conditions = Vec::new();
                    for (index, key) in keys.iter().enumerate() {
                        let gate_key = format!("__branch_gate_{index}");
                        projections.push(col_exact(key).alias(&gate_key));
                        conditions.push(binary(col_exact(key), BinaryOp::Eq, col_exact(&gate_key)));
                    }
                    if projections.is_empty() {
                        projections.push(lit(true).alias("__branch_gate"));
                        conditions.push(lit(true));
                    }
                    let gate = LogicalPlanBuilder::from(filtered)
                        .project(projections)?
                        .build()?;
                    branch.plan = LogicalPlanBuilder::from(branch.plan)
                        .join_on(gate, JoinType::LeftSemi, conditions)?
                        .build()?;
                }
                branches.push(branch);
            }
            unmatched_condition = Some(match unmatched_condition {
                Some(acc) => Expr::or(acc, condition.clone()),
                None => condition.clone(),
            });
        }

        let unmatched_filter =
            unmatched_condition.map(|condition| Expr::IsNotTrue(Box::new(condition)));
        if let Some(default) = default {
            let default_input = match unmatched_filter {
                Some(condition) => LogicalPlanBuilder::from(input.plan.clone())
                    .filter(condition)?
                    .build()?,
                None => input.plan.clone(),
            };
            if !matches!(default, Node::GraphEmpty) {
                branches.push(self.lower_with_correlate(default_input, default)?);
            }
        } else if unmatched == ChooseUnmatched::PassThrough {
            let pass_input = match unmatched_filter {
                Some(condition) => LogicalPlanBuilder::from(input.plan.clone())
                    .filter(condition)?
                    .build()?,
                None => input.plan.clone(),
            };
            branches.push(LoweredNode {
                plan: pass_input,
                islands: IslandReport::default(),
                fields: input.fields.clone(),
                result_form: input.result_form,
            });
        } else if unmatched == ChooseUnmatched::Error {
            return Err(RelError::Unsupported(
                "GraphChoose unmatched=Error is not relationally lowered yet".into(),
            ));
        }

        let Some(first) = branches.first().cloned() else {
            return Ok(LoweredNode {
                plan: LogicalPlanBuilder::from(input.plan)
                    .filter(lit(false))?
                    .build()?,
                islands: input.islands,
                fields: input.fields,
                result_form: input.result_form,
            });
        };
        if self.language == Language::Gremlin {
            // SQL UNION coerces a mixed scalar column (for example name/age)
            // to one physical type. Preserve each traverser's value type by
            // using the native runtime for such choices.
            let mut current_type = None;
            for branch in &branches {
                if let Some(ty) = plan_column_type(&branch.plan, "current") {
                    if ty != DataType::Null {
                        if current_type
                            .as_ref()
                            .is_some_and(|previous| previous != &ty)
                        {
                            return Err(RelError::Unsupported(
                                "Gremlin heterogeneous choice values require native runtime types"
                                    .into(),
                            ));
                        }
                        current_type = Some(ty);
                    }
                }
            }
        }
        let mut plan = first.plan;
        let mut islands = input.islands;
        islands.merge(first.islands);
        for branch in branches.into_iter().skip(1) {
            plan = LogicalPlanBuilder::from(plan)
                .union_by_name(branch.plan)?
                .build()?;
            islands.merge(branch.islands);
        }
        Ok(LoweredNode {
            plan,
            islands,
            fields: input.fields,
            result_form: input.result_form,
        })
    }

    pub(super) fn choose_arm_conditions(
        &self,
        plan: &LogicalPlan,
        selector: &ChooseSelector,
        arms: &[ChooseArm],
    ) -> RelResult<Vec<Expr>> {
        match selector {
            ChooseSelector::Predicates(conditions) => conditions
                .iter()
                .map(|condition| self.lower_expr(plan, condition))
                .collect(),
            ChooseSelector::Boolean(condition) => {
                let condition = self.lower_expr(plan, condition)?;
                let mut out = Vec::with_capacity(arms.len());
                if !arms.is_empty() {
                    out.push(condition.clone());
                }
                if arms.len() >= 2 {
                    out.push(Expr::IsNotTrue(Box::new(condition)));
                }
                for _ in 2..arms.len() {
                    out.push(lit(false));
                }
                Ok(out)
            }
            ChooseSelector::Value(expr) => {
                let selector = self.lower_expr(plan, expr)?;
                arms.iter()
                    .map(|arm| {
                        let Some(key) = &arm.key else {
                            return Ok(lit(false));
                        };
                        let key = value_literal_expr(key)?;
                        Ok(binary(selector.clone(), BinaryOp::Eq, key))
                    })
                    .collect()
            }
        }
    }

    pub(super) fn lower_with_correlate(
        &mut self,
        plan: LogicalPlan,
        node: &Node,
    ) -> RelResult<LoweredNode> {
        let previous = self.correlate_plan.replace(plan);
        let lowered = self.lower_node(node);
        self.correlate_plan = previous;
        lowered
    }
}
