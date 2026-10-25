//! Ordered scalar-family program over bound relational inputs. The old local
//! dispatch lives here once, with shared budgets, outcomes and invalidation;
//! individual implementation proofs remain in their domain modules.
use super::expression::exact_float_literal;
use super::*;
use crate::ir::rules::{RuleDescriptor, RuleKind, RuleSession, Stage, Traversal};

const fn descriptor(id: &'static str, facts: &'static [&'static str]) -> RuleDescriptor {
    RuleDescriptor {
        id,
        version: 1,
        stage: Stage::SemanticResolve,
        kind: RuleKind::ImplementationLowering,
        traversal: Traversal::Once,
        required: true,
        required_facts: facts,
        invalidated_facts: &["expression types", "expression effects", "placement"],
    }
}
const TEMPORAL_DATE: RuleDescriptor =
    descriptor("scalar.temporal.native_date", &["declared DATE domain"]);
const TEMPORAL_TYPED: RuleDescriptor = descriptor(
    "scalar.temporal.typed",
    &["bound temporal domain", "operand effects"],
);
const TEMPORAL_FINITE: RuleDescriptor = descriptor(
    "scalar.temporal.finite_domain",
    &["complete immutable value domain"],
);
const COMPREHENSION: RuleDescriptor = descriptor(
    "scalar.collection.comprehension",
    &["bound element type", "lexical scope", "totality"],
);
const QUANTIFIER: RuleDescriptor = descriptor(
    "scalar.collection.quantifier",
    &["bound element type", "three-valued predicate"],
);
const COLLECTION: RuleDescriptor = descriptor(
    "scalar.collection.evaluation",
    &["complete constant domain", "evaluation policy"],
);
const MEMBERSHIP: RuleDescriptor = descriptor(
    "scalar.collection.membership",
    &["bound element types", "repeatable operands", "three-valued equality"],
);
const COMPARISON: RuleDescriptor = descriptor(
    "scalar.comparison.cypher",
    &[
        "bound operand types",
        "lossless promotion",
        "null and NaN contract",
    ],
);

/// The finite interval excludes NaN under both IEEE unordered comparisons and
/// the SQL total ordering used by our adapters. Match bound expression identity,
/// not rendered SQL. A guard only constrains a later evaluation of a repeatable
/// expression; subqueries and unknown expression forms remain unproven.
fn repeatable_domain_expression(expr: &Expr, remaining: usize) -> bool {
    if remaining == 0 { return false; }
    match expr {
        Expr::Column(_) | Expr::Literal(..) => true,
        Expr::Alias(a) => repeatable_domain_expression(&a.expr, remaining - 1),
        Expr::Cast(c) => repeatable_domain_expression(&c.expr, remaining - 1),
        Expr::TryCast(c) => repeatable_domain_expression(&c.expr, remaining - 1),
        Expr::BinaryExpr(b) => repeatable_domain_expression(&b.left, remaining - 1)
            && repeatable_domain_expression(&b.right, remaining - 1),
        Expr::ScalarFunction(call) => call.func.signature().volatility
            == datafusion::logical_expr::Volatility::Immutable
            && call.args.iter().all(|arg| repeatable_domain_expression(arg, remaining - 1)),
        _ => false,
    }
}

fn guards_finite_result(condition: &Expr, result: &Expr, remaining: usize) -> bool {
    if remaining == 0 { return false; }
    let bounds = result.clone().gt(lit(f64::NEG_INFINITY))
        .and(result.clone().lt(lit(f64::INFINITY)));
    if condition == &bounds { return true; }
    match condition {
        Expr::BinaryExpr(b) if b.op == datafusion::logical_expr::Operator::And =>
            guards_finite_result(&b.left, result, remaining - 1)
                || guards_finite_result(&b.right, result, remaining - 1),
        _ => false,
    }
}

/// A result-domain proof, not a purity proof: the comparison still evaluates
/// its operand. Never infer this from a column name or an arbitrary UDF name.
/// Limit transport to transparent operators and bound, qualified columns.
fn excludes_nan(plan: &LogicalPlan, expr: &Expr, remaining: usize) -> bool {
    if remaining == 0 { return false; }
    if super::language_functions::returns_finite_float(expr) { return true; }
    // Integral/decimal result types cannot represent NaN, even when computing
    // the result can throw. The retained comparison still evaluates the value.
    if expr.get_type(plan.schema()).is_ok_and(|ty| ty.is_numeric()
        && !matches!(ty, DataType::Float16 | DataType::Float32 | DataType::Float64)) {
        return true;
    }
    match expr {
        Expr::Alias(a) => excludes_nan(plan, &a.expr, remaining - 1),
        Expr::Literal(value, _) if value.is_null() => true,
        Expr::Literal(ScalarValue::Float32(v), _) => v.is_none_or(|v| !v.is_nan()),
        Expr::Literal(ScalarValue::Float64(v), _) => v.is_none_or(|v| !v.is_nan()),
        Expr::Case(case) => case.when_then_expr.iter()
            .all(|(condition, result)| excludes_nan(plan, result, remaining - 1)
                || (case.expr.is_none()
                    && result.get_type(plan.schema()).is_ok_and(|ty| ty == DataType::Float64)
                    && repeatable_domain_expression(result, remaining - 1)
                    && guards_finite_result(condition, result, remaining - 1)))
            && case.else_expr.as_ref().is_none_or(|result| excludes_nan(plan, result, remaining - 1)),
        Expr::Cast(cast) if matches!(cast.data_type, DataType::Float32 | DataType::Float64)
            && cast.expr.get_type(plan.schema()).is_ok_and(|ty| ty.is_numeric()) =>
                excludes_nan(plan, &cast.expr, remaining - 1),
        Expr::TryCast(cast) if matches!(cast.data_type, DataType::Float32 | DataType::Float64)
            && cast.expr.get_type(plan.schema()).is_ok_and(|ty| ty.is_numeric()) =>
                excludes_nan(plan, &cast.expr, remaining - 1),
        Expr::Column(column) => {
            let Ok(index) = plan.schema().index_of_column(column) else { return false; };
            match plan {
                LogicalPlan::Projection(p) => p.expr.get(index)
                    .is_some_and(|expr| excludes_nan(&p.input, expr, remaining - 1)),
                LogicalPlan::SubqueryAlias(a) => {
                    let (qualifier, field) = a.input.schema().qualified_field(index);
                    excludes_nan(&a.input, &Expr::Column(Column::new(qualifier.cloned(), field.name())), remaining - 1)
                }
                LogicalPlan::Filter(p) => excludes_nan(&p.input, expr, remaining - 1),
                LogicalPlan::Sort(p) => excludes_nan(&p.input, expr, remaining - 1),
                LogicalPlan::Limit(p) => excludes_nan(&p.input, expr, remaining - 1),
                // Joins, aggregates and arithmetic require their own
                // domain/transport proofs; unknown means retain the guard.
                _ => false,
            }
        }
        _ => false,
    }
}

impl LoweringContext<'_> {
    pub(super) fn scalar_rule<T>(
        &self,
        rule: RuleDescriptor,
        apply: impl FnOnce() -> RelResult<Option<T>>,
    ) -> RelResult<Option<T>> {
        let mut session = RuleSession::new(self.options.rule_context.clone());
        session
            .visit(rule)
            .map_err(RelError::RulePipeline)?;
        match apply() {
            Ok(Some(value)) => {
                session.changed(rule, || {
                    format!(
                        "typed implementation selected; evidence: {:?}",
                        rule.required_facts
                    )
                });
                Ok(Some(value))
            }
            Ok(None) => Ok(None),
            Err(error) => {
                session.record(rule, "blocked", || error.to_string());
                Err(error)
            }
        }
    }
    pub(super) fn run_temporal_rules(
        &self,
        plan: &LogicalPlan,
        expr: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        if self.language != Language::Cypher || matches!(expr, IrExpr::Binding(_) | IrExpr::Lit(_))
        {
            return Ok(None);
        }
        // Finite temporal specialization calls the shared reference evaluator.
        // It cannot consume a subtree owned by another semantic engine.
        if self.native_float_dependency(expr) {
            return Ok(None);
        }
        // Preserve the previous precedence: native DATE, typed runtime domain,
        // then immutable finite-domain specialization. No second local call site.
        type Rule = for<'a> fn(&LoweringContext<'a>, &LogicalPlan, &IrExpr) -> Option<Expr>;
        let rules: [(RuleDescriptor, Rule); 3] = [
            (TEMPORAL_DATE, temporal_semantics::lower_runtime_date),
            (TEMPORAL_TYPED, temporal_semantics::runtime_typed::lower),
            (TEMPORAL_FINITE, temporal_semantics::lower_finite),
        ];
        for (rule, apply) in rules {
            if let Some(value) = self.scalar_rule(rule, || Ok(apply(self, plan, expr)))? {
                return Ok(Some(value));
            }
        }
        Ok(None)
    }
    pub(super) fn run_collection_comparison_rules(
        &self,
        plan: &LogicalPlan,
        expr: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        match expr {
            IrExpr::ListFilter { .. } | IrExpr::ListTransform { .. }
                if self.language == Language::Cypher =>
            {
                self.scalar_rule(COMPREHENSION, || {
                    self.lower_primitive_list_comprehension(plan, expr)
                })
            }
            IrExpr::Quantifier {
                kind,
                list,
                item,
                predicate,
            } => {
                let list = self.lower_expr(plan, list)?;
                self.run_quantifier_rule(plan, *kind, item, list, predicate)?
                    .map(Some)
                    .ok_or_else(|| {
                        RelError::Unsupported("Quantifier requires typed runtime evaluation".into())
                    })
            }
            IrExpr::Call { name, args } if name == "cypher_in" && args.len() == 2 =>
                self.scalar_rule(MEMBERSHIP, || self.lower_membership_rule(plan, args).map(Some)),
            IrExpr::Call { name, args } if name == "list_slice" && args.len() == 3 => self
                .scalar_rule(COLLECTION, || {
                    self.lower_list_slice_rule(plan, args).map(Some)
                }),
            IrExpr::Call { name, args } if is_constant_collection_function(name) => self
                .scalar_rule(COLLECTION, || {
                    self.lower_constant_collection_function(plan, name, args)
                        .map(Some)
                }),
            IrExpr::Call { name, args }
                if args.len() == 2
                    && matches!(
                        name.as_str(),
                        "cypher_eq"
                            | "cypher_neq"
                            | "cypher_lt"
                            | "cypher_lte"
                            | "cypher_gt"
                            | "cypher_gte"
                    ) =>
            {
                self.scalar_rule(COMPARISON, || {
                    self.lower_cypher_comparison_rule(plan, name, args)
                        .map(Some)
                })
            }
            _ => Ok(None),
        }
    }
    /// Both GraphQuantifier operators and nested IrExpr quantifiers use the
    /// same rule; a miss keeps the operator's existing reduction fallback.
    pub(super) fn run_quantifier_rule(
        &self,
        plan: &LogicalPlan,
        kind: QuantifierKind,
        item: &str,
        list: Expr,
        predicate: &IrExpr,
    ) -> RelResult<Option<Expr>> {
        self.scalar_rule(QUANTIFIER, || {
            self.lower_scalar_quantifier(plan, kind, item, list, predicate)
        })
    }

    fn lower_membership_rule(&self, plan: &LogicalPlan, args: &[IrExpr]) -> RelResult<Expr> {
        if self.language == Language::Cypher
            && let IrExpr::Lit(Lit::String(key)) = &args[0]
            && let IrExpr::Call { name, args: target } = &args[1]
            && name == "keys"
            && let [IrExpr::Binding(binding)] = target.as_slice()
            && has_binding_shape(plan, binding).is_some()
        {
            // Entity keys contain precisely the present nonnull properties.
            // Maps are deliberately excluded: a map can retain a null entry.
            let present = self.lower_expr(plan, &IrExpr::IsNotNull(Box::new(IrExpr::property(
                binding.clone(), key.clone(), crate::ir::policy::PropertyMissing::NullOnMissing,
            ))))?;
            let id = id_col(binding);
            return if plan.schema().field_with_unqualified_name(&id)?.is_nullable() {
                Ok(datafusion::logical_expr::when(col_exact(id).is_null(), lit(ScalarValue::Boolean(None)))
                    .otherwise(present)?)
            } else { Ok(present) };
        }
        match &args[1] {
            IrExpr::Lit(Lit::Null) => {
                if !scalar_semantics::repeatable_membership_input(&args[0]) {
                    return Err(RelError::Unsupported("Cypher IN null requires operand evaluation".into()));
                }
                self.lower_expr(plan, &args[0])?;
                Ok(lit(ScalarValue::Boolean(None)))
            }
            IrExpr::List(items) => {
                if !scalar_semantics::repeatable_membership_input(&args[0])
                    || items.iter().any(|item| !scalar_semantics::repeatable_membership_input(item)) {
                    return Err(RelError::Unsupported("Cypher IN with computed operands requires single-evaluation runtime semantics".into()));
                }
                // Validate even an empty RHS: its constant false result
                // must not hide an unsupported/invalid left expression.
                self.lower_expr(plan, &args[0])?;
                // Only repeat stable scalar reads, never a UDF/volatile
                // call. Existing equality preserves type/NaN checks,
                // three-valued OR and empty=false.
                let mut result = lit(false);
                for item in items {
                    let equality = IrExpr::Call {
                        name: "cypher_eq".into(),
                        args: vec![args[0].clone(), item.clone()],
                    };
                    result = result.or(self.lower_expr(plan, &equality)?);
                }
                Ok(result)
            }
            _ if scalar_semantics::repeatable_membership_input(&args[0])
                && scalar_semantics::repeatable_membership_input(&args[1]) => {
                let list = self.lower_native_list(plan, &args[1])?;
                let value = self.lower_expr(plan, &args[0])?;
                let DataType::List(element) = list.get_type(plan.schema())? else {
                    return Err(RelError::Unsupported("Cypher IN requires a native SQL list".into()));
                };
                if !matches!(element.data_type(), DataType::Null | DataType::Boolean | DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64 | DataType::UInt8 | DataType::UInt16 | DataType::UInt32 | DataType::UInt64 | DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View) {
                    return Err(RelError::Unsupported("Cypher IN element equality requires typed runtime semantics".into()));
                }
                let value_type = value.get_type(plan.schema())?;
                if value_type != DataType::Null && value_type != *element.data_type() {
                    return Err(RelError::Unsupported("Heterogeneous Cypher IN requires native comparison semantics".into()));
                }
                // The nested-function signature requires an element-typed
                // NULL; an untyped Null cannot coerce to List(Boolean).
                // All-null lists can use an arbitrary concrete carrier.
                let element_type = if *element.data_type() == DataType::Null {
                    DataType::Int64
                } else { element.data_type().clone() };
                let list = if *element.data_type() == DataType::Null {
                    list.cast_to(&DataType::List(Arc::new(element.as_ref().clone()
                        .with_data_type(element_type.clone()))), plan.schema())?
                } else { list };
                let value = value.cast_to(&element_type, plan.schema())?;
                let null = lit(ScalarValue::try_from(&element_type)?);
                let position = datafusion::functions_nested::expr_fn::array_position(list.clone(), value.clone(), lit(1_i64));
                let null_position = datafusion::functions_nested::expr_fn::array_position(list.clone(), null, lit(1_i64));
                let empty = datafusion::functions_nested::expr_fn::array_length(list.clone()).eq(lit(0_u64));
                Ok(datafusion::logical_expr::when(empty, lit(false))
                    .when(list.is_null().or(value.clone().is_null()), lit(ScalarValue::Boolean(None)))
                    .when(position.is_not_null(), lit(true))
                    .when(null_position.is_not_null(), lit(ScalarValue::Boolean(None)))
                    .otherwise(lit(false))?)
            }
            _ => Err(RelError::Unsupported("Cypher IN with computed operands requires single-evaluation runtime semantics".into())),
        }
    }

    fn lower_list_slice_rule(&self, plan: &LogicalPlan, args: &[IrExpr]) -> RelResult<Expr> {
        let array = self.lower_list_operand(plan, &args[0])?;
        let start = self.lower_expr(plan, &args[1])?;
        let end = self.lower_expr(plan, &args[2])?;
        if matches!(
            array.get_type(plan.schema())?,
            DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
        ) && !collections::is_encoded_property(plan, &args[0])
        {
            return Ok(collections::string_slice_expr(array, start, end));
        }
        Ok(datafusion::functions_nested::expr_fn::array_slice(
            array, start, end, None,
        ))
    }
    fn lower_cypher_comparison_rule(
        &self,
        plan: &LogicalPlan,
        name: &str,
        args: &[IrExpr],
    ) -> RelResult<Expr> {
        let op = match name {
            "cypher_eq" => BinaryOp::Eq,
            "cypher_neq" => BinaryOp::Neq,
            "cypher_lt" => BinaryOp::Lt,
            "cypher_lte" => BinaryOp::Lte,
            "cypher_gt" => BinaryOp::Gt,
            "cypher_gte" => BinaryOp::Gte,
            _ => {
                return Err(RelError::Unsupported(format!(
                    "function `{name}` is not relationally lowered yet"
                )));
            }
        };
        let lhs = self.lower_expr(plan, &args[0])?;
        let rhs = self.lower_expr(plan, &args[1])?;
        if let Some(value) = self.scalar_rule(TEMPORAL_TYPED, || Ok(
            temporal_semantics::runtime_typed::native_timestamp_comparison_bound(
                plan, op, lhs.clone(), rhs.clone(),
            )
        ))? {
            return Ok(value);
        }

        if let Some(value) =
            scalar_union::comparison(plan, &args[0], &args[1], lhs.clone(), rhs.clone(), op)?
        {
            return Ok(value);
        }
        let lt = lhs.get_type(plan.schema())?;
        let rt = rhs.get_type(plan.schema())?;
        // List helpers compare native elements without coercing either list's
        // payload or requiring identical Arrow field names/nullability.
        if let Some(value) = self.lower_primitive_list_equality(plan, lhs.clone(), rhs.clone(), op)? {
            return Ok(value);
        }
        if lt != rt && lt.is_numeric() && rt.is_numeric()
                    && !exact_float_literal(&lhs, &rt)
                    && !exact_float_literal(&rhs, &lt)
                    // Widening within the signed integer family is lossless.
                    // In particular, mapped INTEGER columns must compare with
                    // Cypher's BIGINT literals without requiring a runtime kernel.
                    && !(matches!(lt, DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64)
                        && matches!(rt, DataType::Int8 | DataType::Int16 | DataType::Int32 | DataType::Int64))
                    && !(matches!(lt, DataType::Float32 | DataType::Float64)
                        && matches!(rt, DataType::Float32 | DataType::Float64))
        {
            // SQL numeric promotion can round an integer before it is
            // compared with a float (notably beyond 2^53). Native
            // Cypher comparison preserves the exact operand values.
            return Err(RelError::Unsupported(
                "Mixed Cypher numeric comparison requires lossless runtime values".into(),
            ));
        }
        // Arrow string encodings represent the same Cypher value type.
        let text =
            |t: &DataType| matches!(t, DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View);
        if lt != rt
            && !(lt.is_numeric() && rt.is_numeric())
            && !(text(&lt) && text(&rt))
            && lt != DataType::Null
            && rt != DataType::Null
        {
            return Err(RelError::Unsupported(
                "Cypher comparisons must not coerce unrelated operand types".into(),
            ));
        }
        if matches!(
            lt,
            DataType::List(_) | DataType::LargeList(_) | DataType::Struct(_)
        ) || matches!(
            rt,
            DataType::List(_) | DataType::LargeList(_) | DataType::Struct(_)
        ) {
            return Err(RelError::Unsupported(
                "Cypher compound comparison requires three-valued element semantics".into(),
            ));
        }
        let comparison = self.lower_bound_binary(plan, &args[0], op, &args[1], lhs.clone(), rhs.clone())?;
        if lt.is_numeric() && rt.is_numeric() {
            let mut nan = None;
            if matches!(lt, DataType::Float32 | DataType::Float64) && !excludes_nan(plan, &lhs, 64) {
                nan = Some(df_math::isnan(lhs));
            }
            if matches!(rt, DataType::Float32 | DataType::Float64) && !excludes_nan(plan, &rhs, 64) {
                let rhs_nan = df_math::isnan(rhs);
                nan = Some(match nan { Some(lhs_nan) => lhs_nan.or(rhs_nan), None => rhs_nan });
            }
            let Some(nan) = nan else {
                self.options.rule_context.record_selection(
                    "scalar.comparison.non_nan", "cypher.comparison/v1",
                    "bound operand domains exclude NaN; ordinary comparison retains NULL and operand evaluation",
                );
                return Ok(comparison);
            };
            return Ok(Expr::Case(Case::new(
                None,
                vec![(Box::new(nan), Box::new(lit(matches!(op, BinaryOp::Neq))))],
                Some(Box::new(comparison)),
            )));
        }
        Ok(comparison)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_nan_facts_require_implementation_identity_and_preserve_unknown_domains() {
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        let helper = super::super::language_functions::cypher_to_float(lit("NaN"));
        assert!(excludes_nan(&plan, &helper, 64));
        assert!(excludes_nan(&plan, &lit(f64::INFINITY), 64));
        assert!(!excludes_nan(&plan, &lit(f64::NAN), 64));
        // Finite operands can produce NaN: never propagate blindly through arithmetic.
        assert!(!excludes_nan(&plan, &(helper.clone() / helper), 64));
        let spoof = datafusion::logical_expr::create_udf(
            super::super::language_functions::CYPHER_TO_FLOAT,
            vec![DataType::Utf8], DataType::Float64,
            datafusion::logical_expr::Volatility::Immutable,
            Arc::new(|_| panic!("domain analysis must not execute a UDF")),
        ).call(vec![lit("NaN")]);
        assert!(!excludes_nan(&plan, &spoof, 64));
    }
    #[test]
    fn conditional_domains_require_every_result_and_numeric_cast_inputs() {
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        let helper = super::super::language_functions::cypher_to_float(lit("12"));
        let conditional = datafusion::logical_expr::when(lit(true), helper.clone())
            .otherwise(lit(ScalarValue::Float64(None))).unwrap();
        assert!(excludes_nan(&plan, &conditional, 64));
        let possible_nan = datafusion::logical_expr::when(lit(true), helper)
            .otherwise(lit(f64::NAN)).unwrap();
        assert!(!excludes_nan(&plan, &possible_nan, 64));
        assert!(excludes_nan(&plan, &Expr::Cast(Cast::new(Box::new(lit(12_i64)), DataType::Float64)), 64));
        assert!(!excludes_nan(&plan, &Expr::Cast(Cast::new(Box::new(lit("NaN")), DataType::Float64)), 64));
    }
    #[test]
    fn finite_branch_guards_require_repeatability_and_matching_searched_case() {
        let plan = datafusion::logical_expr::LogicalPlanBuilder::empty(true).build().unwrap();
        let parsed = Expr::TryCast(TryCast::new(Box::new(lit("NaN")), DataType::Float64));
        let bounds = parsed.clone().gt(lit(f64::NEG_INFINITY))
            .and(parsed.clone().lt(lit(f64::INFINITY)));
        let null = lit(ScalarValue::Float64(None));
        let guarded = datafusion::logical_expr::when(bounds.clone(), parsed.clone())
            .otherwise(null.clone()).unwrap();
        assert!(excludes_nan(&plan, &guarded, 64));
        let expansion = super::super::scalar_semantics::conformant_string_to_float(lit("NaN")).unwrap();
        assert!(excludes_nan(&plan, &expansion, 64));
        for unsafe_case in [
            datafusion::logical_expr::when(bounds.clone().or(lit(true)), parsed.clone()).otherwise(null.clone()).unwrap(),
            datafusion::logical_expr::when(bounds.clone(), lit(f64::NAN)).otherwise(null.clone()).unwrap(),
            Expr::Case(Case::new(Some(Box::new(lit(false))), vec![(Box::new(bounds), Box::new(parsed))], Some(Box::new(null.clone())))),
        ] {
            assert!(!excludes_nan(&plan, &unsafe_case, 64));
        }
        let volatile = datafusion::logical_expr::create_udf("changing_float", vec![], DataType::Float64,
            datafusion::logical_expr::Volatility::Volatile,
            Arc::new(|_| panic!("proof must not execute functions"))).call(vec![]);
        let guarded = datafusion::logical_expr::when(
            volatile.clone().gt(lit(f64::NEG_INFINITY)).and(volatile.clone().lt(lit(f64::INFINITY))), volatile)
            .otherwise(null).unwrap();
        assert!(!excludes_nan(&plan, &guarded, 64));
    }
    #[tokio::test]
    async fn comparison_resolves_each_conversion_operand_once() {
        crate::ir::diagnostics::without_constant_folding(async {
            let graph = crate::language::cypher::preparation::prepare(
                "RETURN toFloat($left) > toFloat($right) AS result",
                &BTreeMap::from([("left".into(), Value::String("12".into())),
                    ("right".into(), Value::String("5".into()))]), None,
            ).unwrap();
            let mut options = RelBackendOptions::default();
            options.rule_context.trace_limit = 128;
            let lowered = RelBackend::with_options(options).lower(&graph, &PropertyGraph::new()).unwrap();
            let selections = lowered.rule_context.trace().into_iter()
                .filter(|t| t.rule_id == "semantic.cypher.string_to_float" && t.outcome == "changed").count();
            assert_eq!(selections, 2, "each operand must select its implementation once");
        }).await;
    }
    #[tokio::test]
    async fn required_scalar_rules_run_without_optional_optimization() {
        crate::ir::diagnostics::without_constant_folding(async {
            for (query, rule) in [
                ("UNWIND [1,2] AS x RETURN x = 2", "scalar.comparison.cypher"),
                ("UNWIND [1,2] AS x RETURN x IN [2, null]", "scalar.collection.membership"),
                (
                    "RETURN date({year:1816,month:1,day:1}) AS d",
                    "scalar.temporal.typed",
                ),
            ] {
                let graph =
                    crate::language::cypher::preparation::prepare(query, &Default::default(), None)
                        .unwrap();
                let mut options = RelBackendOptions::default();
                options.rule_context.optimize = false;
                options.rule_context.trace_limit = 128;
                let lowered = RelBackend::with_options(options)
                    .lower(&graph, &PropertyGraph::new())
                    .unwrap();
                assert!(
                    lowered
                        .rule_context
                        .trace()
                        .iter()
                        .any(|t| t.rule_id == rule && t.outcome == "changed"),
                    "{query}: {:?}",
                    lowered.rule_context.trace()
                );
            }
        })
        .await;
    }
}
