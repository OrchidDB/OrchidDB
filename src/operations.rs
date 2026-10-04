//! Typed prepared requests for engines whose execution protocol is not SQL.
//!
//! Adapters own payload semantics; the planner owns parameter binding, input
//! dependencies, and the declared Arrow result schema. No string interpolation
//! or execution-time fallback is involved.
use datafusion::{
    common::{DFSchemaRef, ScalarValue},
    logical_expr::LogicalPlan,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock, RwLock},
};

/// A parameter's payload value or its SQL validity bit. The latter preserves
/// SQL null independently of a non-null domain value containing JSON null.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequestEncoding {
    #[default]
    Value,
    SqlNull,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestBinding {
    /// RFC 6901 JSON pointer to an existing payload slot.
    pub pointer: String,
    /// Zero-based source-column position.
    pub parameter: usize,
    #[serde(default)]
    pub encoding: RequestEncoding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestTemplate {
    pub adapter: String,
    pub parameters: usize,
    pub request: Value,
    #[serde(default)]
    pub bindings: Vec<RequestBinding>,
}
impl RequestTemplate {
    pub fn bind(&self, values: &[ScalarValue]) -> Result<Value, String> {
        if values.len() != self.parameters {
            return Err("request parameter count mismatch".into());
        }
        self.validate()?;
        let nulls = values.iter().map(ScalarValue::is_null).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(parameter_json)
            .collect::<Result<Vec<_>, _>>()?;
        let mut request = self.request.clone();
        for binding in &self.bindings {
            let slot = request.pointer_mut(&binding.pointer).ok_or_else(|| {
                format!(
                    "request binding pointer does not exist: {}",
                    binding.pointer
                )
            })?;
            *slot = match binding.encoding {
                RequestEncoding::Value => values[binding.parameter].clone(),
                RequestEncoding::SqlNull => Value::Bool(nulls[binding.parameter]),
            };
        }
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.adapter.is_empty() {
            return Err("request adapter cannot be empty".into());
        }
        let mut pointers = Vec::<&str>::new();
        for binding in &self.bindings {
            if binding.parameter >= self.parameters {
                return Err("request binding parameter out of range".into());
            }
            if self.request.pointer(&binding.pointer).is_none() {
                return Err(format!(
                    "request binding pointer does not exist: {}",
                    binding.pointer
                ));
            }
            // Overlapping destinations make binding order observable and could
            // replace the enclosing structure of another parameter.
            if pointers.iter().any(|p| {
                *p == binding.pointer
                    || p.is_empty()
                    || binding.pointer.is_empty()
                    || p.strip_prefix(binding.pointer.as_str())
                        .is_some_and(|suffix| suffix.starts_with('/'))
                    || binding
                        .pointer
                        .strip_prefix(*p)
                        .is_some_and(|suffix| suffix.starts_with('/'))
            }) {
                return Err("request binding pointers must not overlap".into());
            }
            pointers.push(&binding.pointer);
        }
        Ok(())
    }
}

/// Encode typed values as payload values. JSON domains are JSON, not strings
/// containing JSON; numeric values retain their integer width and precision.
pub fn parameter_json(value: &ScalarValue) -> Result<Value, String> {
    if value.is_null() {
        return Ok(Value::Null);
    }
    if crate::ir::functions::domain::is_json(&value.data_type()) {
        return crate::ir::functions::domain::json_text(value)
            .map_err(|e| e.to_string())?
            .map(|text| serde_json::from_str(&text).map_err(|e| e.to_string()))
            .transpose()
            .map(|v| v.unwrap_or(Value::Null));
    }
    match value {
        ScalarValue::Float32(Some(v)) if !v.is_finite() => {
            Err("request parameters require finite numbers".into())
        }
        ScalarValue::Float64(Some(v)) if !v.is_finite() => {
            Err("request parameters require finite numbers".into())
        }
        ScalarValue::Struct(array) => array
            .fields()
            .iter()
            .zip(array.columns())
            .map(|(f, array)| {
                Ok((
                    f.name().clone(),
                    parameter_json(
                        &ScalarValue::try_from_array(array, 0).map_err(|e| e.to_string())?,
                    )?,
                ))
            })
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(Value::Object),
        ScalarValue::List(array) => array_json(array.value(0)),
        ScalarValue::LargeList(array) => array_json(array.value(0)),
        ScalarValue::FixedSizeList(array) => array_json(array.value(0)),
        _ => crate::federation::scalar_json(value).map_err(|e| e.to_string()),
    }
}
fn array_json(array: arrow::array::ArrayRef) -> Result<Value, String> {
    (0..array.len())
        .map(|row| {
            parameter_json(&ScalarValue::try_from_array(&array, row).map_err(|e| e.to_string())?)
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Value::Array)
}

#[derive(Debug, Clone)]
pub struct PreparedOperation {
    /// None is a closed operation executed once, without querying a SQL input.
    pub source: Option<Arc<LogicalPlan>>,
    pub template: RequestTemplate,
    pub schema: DFSchemaRef,
    /// Optional relational continuation containing exactly one RequestResult
    /// leaf. Its output schema must match the original operation. This permits
    /// an external access path to return keys for an authoritative SQL join.
    pub replacement: Option<LogicalPlan>,
}

/// Typed placeholder for a prepared request's output in its continuation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestResult {
    pub schema: DFSchemaRef,
}
impl PartialOrd for RequestResult {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(format!("{self:?}").cmp(&format!("{other:?}")))
    }
}
impl RequestResult {
    pub fn into_plan(self) -> LogicalPlan {
        LogicalPlan::Extension(datafusion::logical_expr::Extension {
            node: Arc::new(self),
        })
    }
}
impl datafusion::logical_expr::UserDefinedLogicalNodeCore for RequestResult {
    fn name(&self) -> &str {
        "RequestResult"
    }
    fn inputs(&self) -> Vec<&LogicalPlan> {
        vec![]
    }
    fn schema(&self) -> &DFSchemaRef {
        &self.schema
    }
    fn expressions(&self) -> Vec<datafusion::logical_expr::Expr> {
        vec![]
    }
    fn fmt_for_explain(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "RequestResult")
    }
    fn with_exprs_and_inputs(
        &self,
        expressions: Vec<datafusion::logical_expr::Expr>,
        inputs: Vec<LogicalPlan>,
    ) -> datafusion::common::Result<Self> {
        if !expressions.is_empty() || !inputs.is_empty() {
            return Err(datafusion::common::DataFusionError::Plan(
                "request result is a leaf".into(),
            ));
        }
        Ok(self.clone())
    }
}

pub trait RequestAdapter: Send + Sync {
    fn name(&self) -> &str;
    /// Optional access-path owner overriding the target table's owner.
    fn owner(&self, _plan: &LogicalPlan) -> Result<Option<String>, String> {
        Ok(None)
    }
    /// None means this subtree is not representable; the planner may descend
    /// into smaller valid islands. A recognized but invalid operation errors.
    fn lower(&self, plan: &LogicalPlan) -> Result<Option<PreparedOperation>, String>;
}
fn registry() -> &'static RwLock<BTreeMap<String, Arc<dyn RequestAdapter>>> {
    static REGISTRY: OnceLock<RwLock<BTreeMap<String, Arc<dyn RequestAdapter>>>> = OnceLock::new();
    REGISTRY.get_or_init(Default::default)
}
pub fn register(adapter: Arc<dyn RequestAdapter>) -> Result<(), String> {
    let name = adapter.name().to_owned();
    if name.is_empty() || crate::execution::SqlDialect::resolve(&name).is_ok() {
        return Err("request adapter name must be nonempty and distinct from SQL dialects".into());
    }
    let mut registry = registry()
        .write()
        .map_err(|_| "request adapter registry poisoned")?;
    if registry.contains_key(&name) || builtins().iter().any(|a| a.name() == name) {
        return Err(format!("request adapter already registered: {name}"));
    }
    registry.insert(name, adapter);
    Ok(())
}
fn builtins() -> Vec<Arc<dyn RequestAdapter>> {
    #[cfg(any(feature = "quickwit", feature = "elasticsearch"))]
    {
        crate::remote::adapters()
    }
    #[cfg(not(any(feature = "quickwit", feature = "elasticsearch")))]
    {
        vec![]
    }
}
pub fn adapters() -> Result<Vec<Arc<dyn RequestAdapter>>, String> {
    let mut adapters = builtins();
    adapters.extend(
        registry()
            .read()
            .map_err(|_| "request adapter registry poisoned")?
            .values()
            .cloned(),
    );
    Ok(adapters)
}
pub fn resolve(name: &str) -> Result<Option<Arc<dyn RequestAdapter>>, String> {
    Ok(adapters()?
        .into_iter()
        .find(|adapter| adapter.name() == name))
}
pub(crate) fn owner(plan: &LogicalPlan) -> Result<Option<String>, String> {
    let mut selected = None;
    for adapter in adapters()? {
        if let Some(owner) = adapter.owner(plan)? {
            if selected.as_ref().is_some_and(|current| current != &owner) {
                return Err("request adapters supplied conflicting engine ownership".into());
            }
            selected = Some(owner);
        }
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn binding_is_typed_and_does_not_interpret_user_text() {
        let template = RequestTemplate {
            adapter: "test".into(),
            parameters: 3,
            request: json!({"query":null,"count":null,"payload":null,"literal":"$1"}),
            bindings: vec![
                RequestBinding {
                    pointer: "/query".into(),
                    parameter: 0,
                    encoding: RequestEncoding::Value,
                },
                RequestBinding {
                    pointer: "/count".into(),
                    parameter: 1,
                    encoding: RequestEncoding::Value,
                },
                RequestBinding {
                    pointer: "/payload".into(),
                    parameter: 2,
                    encoding: RequestEncoding::Value,
                },
            ],
        };
        let domain = crate::ir::functions::domain::json_scalar("{\"a\":null}").unwrap();
        let bound = template
            .bind(&[
                ScalarValue::Utf8(Some("\"} OR *:*".into())),
                ScalarValue::UInt64(Some(u64::MAX)),
                domain,
            ])
            .unwrap();
        assert_eq!(
            bound,
            json!({"query":"\"} OR *:*","count":u64::MAX,"payload":{"a":null},"literal":"$1"})
        );
        assert!(template.bind(&[]).is_err());
    }
    #[test]
    fn malformed_and_overlapping_bindings_fail() {
        let mut template = RequestTemplate {
            adapter: "test".into(),
            parameters: 1,
            request: json!({"a":{"b":null}}),
            bindings: vec![RequestBinding {
                pointer: "/missing".into(),
                parameter: 0,
                encoding: RequestEncoding::Value,
            }],
        };
        assert!(template.bind(&[ScalarValue::Null]).is_err());
        template.bindings = vec![
            RequestBinding {
                pointer: "/a".into(),
                parameter: 0,
                encoding: RequestEncoding::Value,
            },
            RequestBinding {
                pointer: "/a/b".into(),
                parameter: 0,
                encoding: RequestEncoding::Value,
            },
        ];
        assert!(template.bind(&[ScalarValue::Null]).is_err());
        assert!(parameter_json(&ScalarValue::Float64(Some(f64::NAN))).is_err());
    }
    #[test]
    fn binding_preserves_domain_null_validity() {
        let template = RequestTemplate {
            adapter: "test".into(),
            parameters: 2,
            request: json!({"values":[null,null],"nulls":[null,null]}),
            bindings: (0..2)
                .flat_map(|i| {
                    [
                        RequestBinding {
                            pointer: format!("/values/{i}"),
                            parameter: i,
                            encoding: RequestEncoding::Value,
                        },
                        RequestBinding {
                            pointer: format!("/nulls/{i}"),
                            parameter: i,
                            encoding: RequestEncoding::SqlNull,
                        },
                    ]
                })
                .collect(),
        };
        let bound = template
            .bind(&[
                ScalarValue::try_from(&crate::ir::functions::domain::json_type()).unwrap(),
                crate::ir::functions::domain::json_scalar("null").unwrap(),
            ])
            .unwrap();
        assert_eq!(bound, json!({"values":[null,null],"nulls":[true,false]}));
    }

    struct Inventory;
    impl RequestAdapter for Inventory {
        fn name(&self) -> &str {
            "test_request_inventory"
        }
        fn lower(&self, plan: &LogicalPlan) -> Result<Option<PreparedOperation>, String> {
            let LogicalPlan::TableScan(scan) = plan else {
                return Ok(None);
            };
            Ok(Some(PreparedOperation {
                source: None,
                schema: plan.schema().clone(),
                replacement: Some(
                    RequestResult {
                        schema: plan.schema().clone(),
                    }
                    .into_plan(),
                ),
                template: RequestTemplate {
                    adapter: self.name().into(),
                    parameters: 0,
                    request: json!({"operation":"inventory","table":scan.table_name.to_string()}),
                    bindings: vec![],
                },
            }))
        }
    }
    fn inventory_adapter() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| register(Arc::new(Inventory)).unwrap());
    }
    #[tokio::test]
    async fn non_sql_adapter_compiles_without_sql_dialect_or_search_contract() {
        inventory_adapter();
        let request = json!({"version":1,"dialect":"duckdb","language":"cypher",
            "query":"MATCH (p:Product) WHERE p.price > 3 RETURN p.price AS price",
            "engines":{"local":{"dialect":"duckdb"},"inventory":{"dialect":"test_request_inventory"}},
            "execution_engine":"local",
            "tables":[{"name":"products","engine":"inventory","columns":[{"name":"id","data_type":"int64"},{"name":"price","data_type":"int64"}]}],
            "nodes":[{"label":"Product","table":"products","id":"id","properties":{"price":"price"}}]});
        let plan = crate::compiler::compile(serde_json::from_value(request).unwrap())
            .await
            .unwrap();
        assert_eq!(plan.transfers.len(), 1);
        assert!(plan.transfers[0].sql.is_empty());
        assert!(plan.transfers[0].operation.is_none());
        let operation = plan.transfers[0].request.as_ref().unwrap();
        assert_eq!(operation.engine, "inventory");
        assert_eq!(operation.template.request["operation"], "inventory");
        assert!(!plan.sql.contains("products"));
        assert!(plan.sql.contains(&plan.transfers[0].target_relation));
        assert!(crate::execution::SqlDialect::resolve("test_request_inventory").is_err());
        let serialized = serde_json::to_value(&plan).unwrap();
        let bound = crate::federation::bind_operation_command(
            json!({"plan":serialized,"relation":plan.transfers[0].target_relation,"rows":[[]]}),
        )
        .unwrap();
        assert_eq!(bound["requests"][0]["operation"], "inventory");
    }
}
