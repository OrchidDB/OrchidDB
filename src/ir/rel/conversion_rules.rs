//! Typed conversion implementation selection. Policy is chosen before expansion;
//! capabilities choose only between implementations of that selected contract.
use super::*;
use crate::ir::rules::{
    RuleDescriptor, RuleKind, RuleSession, Stage, StringToFloatPolicy, Traversal,
};
use datafusion::logical_expr::{
    ColumnarValue, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature, Volatility,
};
use std::any::Any;

const STRING_TO_FLOAT: RuleDescriptor = RuleDescriptor {
    id: "semantic.cypher.string_to_float",
    version: 1,
    stage: Stage::SemanticResolve,
    kind: RuleKind::ImplementationLowering,
    traversal: Traversal::Once,
    required: true,
    required_facts: &[
        "bound string operand",
        "semantic profile",
        "session function capability",
    ],
    invalidated_facts: &["expression effects", "placement"],
};

/// The supported operation descriptor is deliberately narrow: nullable text in,
/// nullable double out, invalid syntax yields NULL, evaluation of the operand
/// remains observable. Native policy changes only parsing, never input effects.
const CONTRACT: &str =
    "cypher.string_to_float/v1; nullable string -> nullable double; invalid syntax -> null";

pub(crate) const NATIVE_FLOAT: &str = "__orchiddb_duckdb_string_to_float_v1";

/// Conservative recursive policy dependency used before *any* scalar folding or
/// native-value fallback, including folds of an enclosing CASE/list/comparison.
pub(crate) fn contains_float_conversion(expr: &IrExpr) -> bool {
    let mut found = false;
    crate::ir::rules::graph::visit_expr(expr, &Default::default(), &mut |expr, _scope| {
        found |= matches!(expr, IrExpr::Call { name, .. } if name == "cypher_convert.tofloat");
    });
    found
}

/// Engine ownership is metadata on the logical implementation, not volatility.
/// A scalar simplifier without that evaluator must leave the operation intact.
pub(crate) fn can_fold_plan(plan: &LogicalPlan) -> bool {
    use datafusion::common::tree_node::{TreeNode, TreeNodeRecursion};
    let mut allowed = true;
    let _ = plan.apply_with_subqueries(|node| {
        for expr in node.expressions() {
            expr.apply(|expr| {
                if matches!(expr, Expr::ScalarFunction(call) if call.func.inner().as_any().is::<NativeFloat>()) {
                    allowed = false;
                    return Ok(TreeNodeRecursion::Stop);
                }
                Ok(TreeNodeRecursion::Continue)
            })?;
            if !allowed { return Ok(TreeNodeRecursion::Stop); }
        }
        Ok(TreeNodeRecursion::Continue)
    });
    allowed
}

impl LoweringContext<'_> {
    pub(super) fn native_float_dependency(&self, expr: &IrExpr) -> bool {
        self.language == Language::Cypher
            && self.options.rule_context.profile.string_to_float
                == StringToFloatPolicy::DuckDbNative
            && contains_float_conversion(expr)
    }

    pub(super) fn select_string_to_float(
        &self,
        value: Expr,
        arg: &IrExpr,
        input_type: &DataType,
    ) -> RelResult<Expr> {
        let context = &self.options.rule_context;
        let typed_text = |value: Expr| {
            if input_type == &DataType::Utf8 {
                value
            } else {
                Expr::Cast(Cast::new(Box::new(value), DataType::Utf8))
            }
        };
        let mut session = RuleSession::new(context.clone());
        session
            .visit(STRING_TO_FLOAT)
            .map_err(RelError::RulePipeline)?;
        let implementation = match context.profile.string_to_float {
            StringToFloatPolicy::DuckDbNative => {
                "explicit DuckDB-native parser; owning engine required"
            }
            StringToFloatPolicy::Orchid if context.supports_float_helper() => {
                "conformant registered typed helper"
            }
            StringToFloatPolicy::Orchid => "helper unavailable or disabled; conformant guarded SQL",
        };
        let result = (|| match context.profile.string_to_float {
            StringToFloatPolicy::DuckDbNative => {
                if !context
                    .owner
                    .as_ref()
                    .is_some_and(|owner| owner.name.eq_ignore_ascii_case("duckdb"))
                {
                    return Err(RelError::Unsupported(
                        "native string-to-float requires its owning DuckDB engine".into(),
                    ));
                }
                Ok(ScalarUDF::from(NativeFloat {
                    // Determinism and evaluation ownership are independent.
                    // Optimizer fold eligibility inspects this typed operation;
                    // only DuckDB may evaluate it, including constant inputs.
                    signature: Signature::exact(vec![DataType::Utf8], Volatility::Immutable),
                })
                .call(vec![typed_text(value)]))
            }
            StringToFloatPolicy::Orchid if context.supports_float_helper() => Ok(
                super::language_functions::cypher_to_float(typed_text(value)),
            ),
            StringToFloatPolicy::Orchid => {
                if !super::scalar_semantics::stable_scalar_input(arg) {
                    return Err(RelError::Unsupported("conformant string-to-float SQL requires a repeatable operand when a typed helper is unavailable".into()));
                }
                super::scalar_semantics::conformant_string_to_float(value)
            }
        })();
        match &result {
            Ok(_) => session.changed(STRING_TO_FLOAT, || format!("{CONTRACT}; {implementation}")),
            Err(error) => session.record(STRING_TO_FLOAT, "blocked", || error.to_string()),
        }
        result
    }
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct NativeFloat {
    signature: Signature,
}
impl ScalarUDFImpl for NativeFloat {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        NATIVE_FLOAT
    }
    fn signature(&self) -> &Signature {
        &self.signature
    }
    fn return_type(&self, _: &[DataType]) -> datafusion::common::Result<DataType> {
        Ok(DataType::Float64)
    }
    fn invoke_with_args(&self, _: ScalarFunctionArgs) -> datafusion::common::Result<ColumnarValue> {
        Err(DataFusionError::Execution("native string-to-float requires execution by its owning DuckDB engine; compatible host fallback is forbidden".into()))
    }
}

#[cfg(all(test, feature = "duckdb"))]
mod tests {
    use super::*;
    use crate::ir::rel::sql::{self, SqlDialect};
    use crate::ir::rules::{EngineIdentity, RuleContext};
    use std::collections::BTreeMap;

    fn native_options() -> RelBackendOptions {
        let mut context = RuleContext::default();
        context.profile.string_to_float = StringToFloatPolicy::DuckDbNative;
        context.owner = Some(EngineIdentity {
            name: "duckdb".into(),
            version: "test-session".into(),
        });
        RelBackendOptions {
            rule_context: context,
            ..Default::default()
        }
    }
    async fn query_sql(
        query: &str,
        parameters: BTreeMap<String, Value>,
        options: RelBackendOptions,
    ) -> String {
        let graph =
            crate::language::cypher::preparation::prepare(query, &parameters, None).unwrap();
        let lowered = RelBackend::with_options(options)
            .lower(&graph, &PropertyGraph::new())
            .unwrap();
        sql::prepare(&lowered, SqlDialect::DuckDb)
            .await
            .unwrap()
            .query
    }
    #[tokio::test]
    async fn native_literals_parameters_and_nested_projections_keep_engine_semantics() {
        let connection = duckdb::Connection::open_in_memory().unwrap();
        for (op, expected) in [("=", false), ("<>", true), (">", false)] {
            let text = query_sql(
                &format!("RETURN toFloat('NaN') {op} 5.0 AS x"),
                BTreeMap::new(), native_options(),
            ).await;
            assert!(text.to_lowercase().contains("isnan"), "{text}");
            let actual: bool = connection.query_row(&text, [], |row| row.get(0)).unwrap();
            assert_eq!(actual, expected, "{text}");
        }
        for query in [
            "RETURN toFloat('NaN') AS x",
            "WITH 'NaN' AS x RETURN toFloat(x) AS x",
            "WITH toFloat('NaN') AS x RETURN x AS x",
            "RETURN toFloat($value) AS x",
        ] {
            let text = query_sql(
                query,
                BTreeMap::from([("value".into(), Value::String("NaN".into()))]),
                native_options(),
            )
            .await;
            assert!(text.contains("TRY_CAST"), "{text}");
            assert!(!text.contains("regexp"), "{text}");
            let value: f64 = connection.query_row(&text, [], |row| row.get(0)).unwrap();
            assert!(value.is_nan(), "{query}: {text}");
        }
        for input in [
            "foo",
            "",
            "2.9",
            "__1__2__",
            "NaN",
            "Infinity",
            "1e999",
            "\u{a0}1.5\u{3000}",
        ] {
            let text = query_sql(
                "RETURN toFloat($value) AS x",
                BTreeMap::from([("value".into(), Value::String(input.into()))]),
                native_options(),
            )
            .await;
            let actual: Option<f64> = connection.query_row(&text, [], |row| row.get(0)).unwrap();
            let expected: Option<f64> = connection
                .query_row("SELECT TRY_CAST(? AS DOUBLE)", [input], |row| row.get(0))
                .unwrap();
            assert!(
                actual == expected
                    || actual.is_some_and(f64::is_nan) && expected.is_some_and(f64::is_nan),
                "{input:?}: {actual:?} vs {expected:?}"
            );
        }
    }
    #[tokio::test]
    async fn native_no_folding_and_nonowner_rejection() {
        crate::ir::diagnostics::without_constant_folding(async {
            let sql = query_sql(
                "RETURN toFloat('Infinity') AS x",
                BTreeMap::new(),
                native_options(),
            )
            .await;
            assert!(sql.contains("TRY_CAST"), "{sql}");
        })
        .await;
        let graph = crate::language::cypher::preparation::prepare(
            "RETURN toFloat('NaN')",
            &BTreeMap::new(),
            None,
        )
        .unwrap();
        let mut options = native_options();
        options.rule_context.owner = None;
        let error = RelBackend::with_options(options)
            .lower(&graph, &PropertyGraph::new())
            .unwrap_err();
        assert!(error.to_string().contains("owning DuckDB"), "{error}");
    }
    #[tokio::test]
    async fn unavailable_helpers_preserve_conformant_expansion_and_contract() {
        crate::ir::diagnostics::without_constant_folding(async {
            let connection = duckdb::Connection::open_in_memory().unwrap();
            for input in [
                "foo",
                "",
                "2.9",
                "__1__2__",
                "NaN",
                "Infinity",
                "1e999",
                "1e-999",
                "\u{a0}1.5\u{3000}",
            ] {
                let mut options = RelBackendOptions::default();
                options.language_functions = true;
                options.rule_context.capabilities.language_functions_enabled = true;
                // A no-install session advertises no verified registration.
                // An enabled global flag alone must not select the helper.
                let text = query_sql(
                    "RETURN toFloat($value) AS x",
                    BTreeMap::from([("value".into(), Value::String(input.into()))]),
                    options,
                )
                .await;
                assert!(!text.contains("__orchiddb_cypher_to_float"), "{text}");
                let actual: Option<f64> =
                    connection.query_row(&text, [], |row| row.get(0)).unwrap();
                assert_eq!(
                    actual,
                    crate::ir::functions::cypher_float::parse_string(input).ok(),
                    "{input:?}"
                );
            }
        })
        .await;
    }
    #[tokio::test]
    async fn native_mapped_columns_keep_owner_with_optimization_disabled() {
        use super::super::mapping::{GraphMapping, NodeMapping};
        use arrow::datatypes::{Field, Schema};
        let connection = duckdb::Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE conversion_values (id BIGINT, raw VARCHAR); INSERT INTO conversion_values VALUES (1,'NaN'),(2,'Infinity'),(3,'foo'),(4,NULL)").unwrap();
        let mut mapping = GraphMapping::new();
        mapping.register_table_schema(
            "conversion_values",
            Arc::new(Schema::new(vec![
                Field::new("id", DataType::Int64, false),
                Field::new("raw", DataType::Utf8, true),
            ])),
        );
        mapping.map_node(
            NodeMapping::table("Conversion", "conversion_values", "id")
                .property("raw", "raw")
                .property("id", "id"),
        );
        let mut options = native_options();
        options.rule_context.optimize = false;
        options.rule_context.trace_limit = 64;
        options.mapping = Some(Arc::new(mapping.clone()));
        let plan = crate::language::cypher::preparation::prepare(
            "MATCH (n:Conversion) RETURN toFloat(n.raw) AS x ORDER BY n.id",
            &BTreeMap::new(),
            None,
        )
        .unwrap();
        let lowered = RelBackend::with_options(options)
            .lower(&plan, &PropertyGraph::new())
            .unwrap();
        assert!(
            lowered
                .rule_context
                .trace()
                .iter()
                .any(|t| t.rule_id == "semantic.cypher.string_to_float"
                    && t.evidence.contains("DuckDB-native"))
        );
        let prepared = sql::prepare_with_external(
            &lowered,
            SqlDialect::DuckDb,
            &mapping.physical_table_names(),
        )
        .await
        .unwrap();
        assert!(prepared.query.contains("TRY_CAST"), "{}", prepared.query);
        let mut statement = connection.prepare(&prepared.query).unwrap();
        let values = statement
            .query_map([], |r| r.get::<_, Option<f64>>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(values[0].unwrap().is_nan());
        assert_eq!(values[1], Some(f64::INFINITY));
        assert_eq!(&values[2..], &[None, None]);
        let error = sql::unparse(&lowered, SqlDialect::Postgres).unwrap_err();
        assert!(error.to_string().contains("owning DuckDB"), "{error}");
    }
    #[tokio::test]
    async fn native_conversion_uses_default_duckdb_even_for_empty_input() {
        for (query, expected_rows) in [
            ("RETURN toFloat('NaN')", 1),
            ("UNWIND [] AS x RETURN toFloat('NaN')", 0),
        ] {
            let graph =
                crate::language::cypher::preparation::prepare(query, &BTreeMap::new(), None)
                    .unwrap();
            let result = RelBackend::with_options(native_options())
                .execute(&graph, &PropertyGraph::new())
                .await
                .unwrap();
            assert_eq!(result.batch.num_rows(), expected_rows, "{query}");
            if expected_rows == 1 {
                let value = result.batch.column(0).as_any()
                    .downcast_ref::<arrow::array::Float64Array>().unwrap().value(0);
                assert!(value.is_nan(), "{query}");
            }
        }
    }
}
