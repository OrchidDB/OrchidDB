use super::*;
use crate::ir::rel::sql;
/// Unknown or volatile UDFs and extension nodes execute only in DataFusion.
/// DuckDB eligibility is a planning decision; execution errors are never retried
/// through another engine, avoiding repeated effects or changed error semantics.
/// Analyze each logical node once. A rejected ancestor must not repeatedly
/// rescan its descendants while partitioning into smaller regions.
#[derive(Default)]
pub(crate) struct SqlEligibility {
    pub(crate) reasons: std::collections::HashMap<usize, Option<&'static str>>,
    pub(crate) dialect: Option<sql::SqlDialect>,
    pub(crate) language_functions: bool,
}
impl SqlEligibility {
    pub(crate) fn visit(&mut self, plan: &LogicalPlan) -> Option<&'static str> {
        let key = plan as *const LogicalPlan as usize;
        if let Some(reason) = self.reasons.get(&key) {
            return *reason;
        }
        // A complete SQL lowering consumes the node's expressions and inputs.
        // Requiring their scalar mappings again would reject index-provided
        // scores or engine operations that have no standalone scalar form.
        if matches!(plan, LogicalPlan::Extension(_))
            && matches!(
                sql::lowering::lower_relation(
                    plan,
                    &sql::lowering::LoweringContext {
                        dialect: self.dialect.unwrap_or(sql::SqlDialect::DuckDb),
                        mode: sql::lowering::LoweringMode::InIsland,
                    }
                ),
                Ok(Some(sql::lowering::RelationLowering::Sql(_)))
            )
        {
            self.reasons.insert(key, None);
            return None;
        }
        let mut reason = match plan {
            LogicalPlan::Extension(_)
                if matches!(
                    sql::lowering::lower_relation(
                        plan,
                        &sql::lowering::LoweringContext {
                            dialect: self.dialect.unwrap_or(sql::SqlDialect::DuckDb),
                            mode: sql::lowering::LoweringMode::InIsland,
                        }
                    ),
                    Ok(Some(
                        sql::lowering::RelationLowering::Sql(_)
                            | sql::lowering::RelationLowering::Rewrite(_)
                    ))
                ) =>
            {
                None
            }
            LogicalPlan::Extension(_) => Some("residual extension"),
            LogicalPlan::EmptyRelation(empty) if empty.produce_one_row => None,
            LogicalPlan::EmptyRelation(_) => Some("empty relation"),
            _ if plan.schema().fields().is_empty() => Some("zero-column relation"),
            _ => None,
        };
        if reason.is_some() {
            self.reasons.insert(key, reason);
            return reason;
        }
        let mut portable_schema = datafusion::common::DFSchema::empty();
        for input in plan.inputs() {
            portable_schema.merge(input.schema());
        }
        portable_schema.merge(plan.schema());
        for expr in plan.expressions() {
            let _ = expr.apply(|expr| {
                if let Some(requirements) =
                    super::capabilities::SubqueryRequirements::from_expr(expr)
                    && let Err(why) = self
                        .dialect
                        .unwrap_or(sql::SqlDialect::DuckDb)
                        .check_subquery(&requirements)
                {
                    reason.get_or_insert(why);
                    return Ok(TreeNodeRecursion::Stop);
                }
                if let Expr::Literal(value, _) = expr {
                    if let Some(why) = crate::ir::functions::portable::literal_issue(
                        value,
                        self.dialect.unwrap_or(sql::SqlDialect::DuckDb).name(),
                    ) {
                        reason.get_or_insert(why);
                        return Ok(TreeNodeRecursion::Stop);
                    }
                }
                if let Expr::ScalarFunction(function) = expr {
                    if function.func.name() == crate::ir::rel::conversion_rules::NATIVE_FLOAT {
                        if self.dialect != Some(sql::SqlDialect::DuckDb) {
                            reason.get_or_insert(
                                "native conversion requires its owning DuckDB engine",
                            );
                        }
                        return Ok(TreeNodeRecursion::Continue);
                    }
                    if crate::ir::rel::language_functions::supported(function.func.name())
                        || matches!(function.func.name(), super::super::native_values::VALUE | super::super::native_values::PREDICATE | super::super::native_values::ITEMS | super::super::native_values::KEY | super::super::native_values::PROJECT | super::super::native_values::SORT | super::super::native_values::AGGREGATE) {
                        if self.language_functions && self.dialect == Some(sql::SqlDialect::DuckDb)
                        {
                            return Ok(TreeNodeRecursion::Continue);
                        }
                        reason.get_or_insert("language functions disabled on SQL session");
                        return Ok(TreeNodeRecursion::Stop);
                    }
                    if self.dialect == Some(sql::SqlDialect::Postgres)
                        && matches!(
                            function.func.name(),
                            "__orchiddb_sparql_scalar" | "sha1" | "sha256" | "sha384" | "sha512"
                        )
                    {
                        reason.get_or_insert("native RDF scalar kernel");
                        return Ok(TreeNodeRecursion::Stop);
                    }
                    if let Some(logical) = crate::ir::functions::logical::definition(&function.func)
                    {
                        let dialect = self.dialect.unwrap_or(sql::SqlDialect::DuckDb);
                        let adapter_mapping = match dialect {
                            sql::SqlDialect::Custom(adapter) => {
                                adapter.function_mapping(logical.logical_name()).is_some()
                            }
                            _ => false,
                        };
                        if !adapter_mapping
                            && !logical.has_sql_mapping(dialect.name(), function.args.len())
                        {
                            reason.get_or_insert("logical function requires native execution");
                            return Ok(TreeNodeRecursion::Stop);
                        }
                        if !adapter_mapping && logical.portable_builtin {
                            if let Err(why) = crate::ir::functions::portable::validate_call(
                                logical.logical_name(),
                                &function.args,
                                &portable_schema,
                                dialect.name(),
                            ) {
                                reason.get_or_insert(why);
                                return Ok(TreeNodeRecursion::Stop);
                            }
                        }
                        return Ok(TreeNodeRecursion::Continue);
                    }
                    let native = self
                        .dialect
                        .unwrap_or(sql::SqlDialect::DuckDb)
                        .supports_scalar_function(function.func.name())
                        || {
                            #[cfg(feature = "duckdb")]
                            {
                                crate::ir::rel::sparql::is_duck_function(&function.func)
                            }
                            #[cfg(not(feature = "duckdb"))]
                            {
                                false
                            }
                        }
                        || function
                            .func
                            .name()
                            .starts_with(crate::ir::functions::ENGINE_FUNCTION_PREFIX)
                        || function.func.name() == crate::ir::functions::ENGINE_CAST_FUNCTION;
                    if !native
                        && (function.func.signature().volatility
                            == datafusion::logical_expr::Volatility::Volatile
                            || function.func.documentation().is_none())
                    {
                        reason.get_or_insert("function capability/effect boundary");
                        return Ok(TreeNodeRecursion::Stop);
                    }
                }
                Ok(TreeNodeRecursion::Continue)
            });
        }
        if reason.is_none() {
            for input in plan.inputs() {
                reason = self.visit(input);
                if reason.is_some() {
                    break;
                }
            }
        }
        self.reasons.insert(key, reason);
        reason
    }
}
