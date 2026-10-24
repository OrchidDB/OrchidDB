//! Shared, query-scoped rule contracts. Domain implementations remain beside
//! their IR; this module owns scheduling, semantic identity and diagnostics.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, Weak};
pub mod aggregate;
pub mod effects;
pub mod graph;
#[doc(hidden)]
pub use crate::ir::rel::language_functions::FloatHelperCapability;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum StringToFloatPolicy {
    #[default]
    Orchid,
    DuckDbNative,
}
/// Optional portability tradeoff; unrelated to whether execution must be SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CypherAggregatePolicy {
    #[default]
    Conformant,
    /// Use database aggregate/equality semantics on SQL-representable values.
    SqlNative,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SemanticProfile {
    pub version: u16,
    pub string_to_float: StringToFloatPolicy,
    #[serde(default)]
    pub cypher_aggregates: CypherAggregatePolicy,
}
impl Default for SemanticProfile {
    fn default() -> Self {
        Self {
            version: 1,
            string_to_float: StringToFloatPolicy::Orchid,
            cypher_aggregates: CypherAggregatePolicy::Conformant,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EngineIdentity {
    pub name: String,
    pub version: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct TargetCapabilities {
    pub language_functions_enabled: bool,
    pub float_helper: Option<FloatHelperCapability>,
}
impl TargetCapabilities {
    pub fn supports_float_helper(&self) -> bool {
        self.language_functions_enabled && self.float_helper.is_some()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RewriteBudget {
    pub query_visits: usize,
    pub stage_visits: usize,
    pub iterations: usize,
}
impl Default for RewriteBudget {
    fn default() -> Self {
        Self {
            query_visits: 100_000,
            stage_visits: 25_000,
            iterations: 8,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RuleContext {
    pub lowering_policy: crate::ir::rel::runtime::policy::LoweringPolicy,
    pub executables: crate::ir::rel::runtime::policy::ExecutableSet,
    pub profile: SemanticProfile,
    pub capabilities: TargetCapabilities,
    pub owner: Option<EngineIdentity>,
    pub catalog_revision: u64,
    /// Semantic session settings (timezone/collation/etc.), never credentials.
    pub session_fingerprint: String,
    pub optimize: bool,
    pub trace_limit: usize,
    pub budget: RewriteBudget,
    state: Arc<Mutex<RuleState>>,
}
impl Default for RuleContext {
    fn default() -> Self {
        Self {
            lowering_policy: Default::default(),
            executables: Default::default(),
            profile: SemanticProfile::default(),
            capabilities: TargetCapabilities::default(),
            owner: None,
            catalog_revision: 0,
            session_fingerprint: String::new(),
            optimize: true,
            trace_limit: 0,
            budget: RewriteBudget::default(),
            state: Arc::default(),
        }
    }
}
/// Semantic portion of a cache key. Callers additionally retain their existing
/// query/parameter/schema keys; this does not replace parameter-sensitive keys.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleCacheIdentity {
    pub lowering_policy: crate::ir::rel::runtime::policy::LoweringPolicy,
    pub executable_revision: u64,
    pub profile: SemanticProfile,
    pub capabilities: TargetCapabilities,
    pub owner: Option<EngineIdentity>,
    pub catalog_revision: u64,
    pub session_fingerprint: String,
}
impl RuleContext {
    pub fn cache_identity(&self) -> RuleCacheIdentity {
        RuleCacheIdentity {
            lowering_policy: self.lowering_policy,
            executable_revision: self.executables.revision(),
            profile: self.profile.clone(),
            capabilities: self.capabilities.clone(),
            owner: self.owner.clone(),
            catalog_revision: self.catalog_revision,
            session_fingerprint: self.session_fingerprint.clone(),
        }
    }

    /// Start another query with the same semantic and capability snapshot.
    pub fn new_query(&self) -> Self {
        Self {
            state: Arc::default(),
            ..self.clone()
        }
    }
    pub fn trace(&self) -> Vec<RuleTrace> {
        self.state
            .lock()
            .expect("rule state poisoned")
            .traces
            .clone()
    }
    pub fn revision(&self) -> u64 {
        self.state.lock().expect("rule state poisoned").revision
    }
    pub fn visits(&self) -> usize {
        self.state.lock().expect("rule state poisoned").visits
    }
    pub fn record_selection(&self, id: &'static str, contract: &str, evidence: &str) {
        let mut session = RuleSession::new(self.clone());
        session.record(
            RuleDescriptor {
                id,
                version: 1,
                stage: Stage::SemanticResolve,
                kind: RuleKind::ImplementationLowering,
                traversal: Traversal::Once,
                required: true,
                required_facts: &[],
                invalidated_facts: &[],
            },
            "selected",
            || format!("{contract}: {evidence}"),
        );
    }

    pub fn supports_float_helper(&self) -> bool {
        self.capabilities.supports_float_helper()
    }
    pub fn validate(&self) -> Result<(), RuleError> {
        if self.profile.version != 1 {
            return Err(RuleError("unsupported semantic profile version".into()));
        }
        if self.profile.string_to_float == StringToFloatPolicy::DuckDbNative
            && !self
                .owner
                .as_ref()
                .is_some_and(|owner| owner.name.eq_ignore_ascii_case("duckdb"))
        {
            return Err(RuleError(
                "native string-to-float requires its owning DuckDB engine".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Stage {
    GraphNormalize,
    SemanticResolve,
    RelationalOptimize,
    Placement,
    LateTyped,
    AdapterPrepare,
    Validate,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuleKind {
    SemanticSelection,
    EquivalentRewrite,
    ImplementationLowering,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Traversal {
    Once,
    BottomUp,
    FixedPoint,
}
#[derive(Debug, Clone, Copy)]
pub struct RuleDescriptor {
    pub id: &'static str,
    pub version: u16,
    pub stage: Stage,
    pub kind: RuleKind,
    pub traversal: Traversal,
    pub required: bool,
    pub required_facts: &'static [&'static str],
    pub invalidated_facts: &'static [&'static str],
}
#[derive(Debug)]
pub enum RuleOutcome<T> {
    Unchanged(T),
    Blocked {
        value: T,
        reason: String,
        missing_facts: Vec<&'static str>,
    },
    Changed {
        value: T,
        evidence: String,
    },
    Error(String),
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("rule pipeline: {0}")]
pub struct RuleError(pub String);
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleTrace {
    pub stage: Stage,
    pub rule_id: String,
    pub rule_version: u16,
    pub outcome: String,
    pub plan_revision: u64,
    pub semantic_profile: SemanticProfile,
    pub owner: Option<EngineIdentity>,
    pub evidence: String,
}
/// Fact cache tokens are valid only for one plan revision and lexical scope.
#[derive(Debug, Clone)]
pub struct AnalysisKey {
    pub revision: u64,
    pub scope: Vec<usize>,
    query: Weak<Mutex<RuleState>>,
}
impl PartialEq for AnalysisKey {
    fn eq(&self, other: &Self) -> bool {
        self.revision == other.revision
            && self.scope == other.scope
            && self.query.ptr_eq(&other.query)
    }
}
impl Eq for AnalysisKey {}
impl Hash for AnalysisKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.revision.hash(state);
        self.scope.hash(state);
        self.query.as_ptr().hash(state);
    }
}
#[derive(Debug, Default)]
struct RuleState {
    revision: u64,
    traces: Vec<RuleTrace>,
    visits: usize,
    stage_visits: BTreeMap<Stage, usize>,
}
#[derive(Debug)]
pub struct RuleSession {
    pub context: RuleContext,
}
impl RuleSession {
    pub fn new(context: RuleContext) -> Self {
        Self { context }
    }
    pub fn invalidate(&mut self) {
        self.context
            .state
            .lock()
            .expect("rule state poisoned")
            .revision += 1;
    }
    pub fn analysis_key(&self, scope: Vec<usize>) -> AnalysisKey {
        AnalysisKey {
            revision: self.context.revision(),
            scope,
            query: Arc::downgrade(&self.context.state),
        }
    }
    pub fn analysis_is_current(&self, key: &AnalysisKey) -> bool {
        key.revision == self.context.revision()
            && key.query.ptr_eq(&Arc::downgrade(&self.context.state))
    }
    pub fn record(
        &mut self,
        descriptor: RuleDescriptor,
        outcome: &str,
        evidence: impl FnOnce() -> String,
    ) {
        let state = self.context.state.lock().expect("rule state poisoned");
        if state.traces.len() >= self.context.trace_limit {
            return;
        }
        let revision = state.revision;
        drop(state);
        // Evidence may inspect scoped analysis or query counters. Never invoke
        // caller code while holding the query-state mutex.
        let evidence = evidence();
        let mut state = self.context.state.lock().expect("rule state poisoned");
        if state.traces.len() >= self.context.trace_limit {
            return;
        }
        state.traces.push(RuleTrace {
            stage: descriptor.stage,
            rule_id: descriptor.id.into(),
            rule_version: descriptor.version,
            outcome: outcome.into(),
            plan_revision: revision,
            semantic_profile: self.context.profile.clone(),
            owner: self.context.owner.clone(),
            evidence,
        });
    }
    pub fn changed(&mut self, descriptor: RuleDescriptor, evidence: impl FnOnce() -> String) {
        self.context
            .state
            .lock()
            .expect("rule state poisoned")
            .revision += 1;
        self.record(descriptor, "changed", evidence);
    }
    /// Required passes fail rather than return an unresolved plan. Optional
    /// equivalent passes may stop at their valid input when budgets run out.
    pub fn visit(&mut self, descriptor: RuleDescriptor) -> Result<bool, RuleError> {
        if !descriptor.required && !self.context.optimize {
            return Ok(false);
        }
        let mut state = self.context.state.lock().expect("rule state poisoned");
        let count = state
            .stage_visits
            .get(&descriptor.stage)
            .copied()
            .unwrap_or(0);
        if state.visits >= self.context.budget.query_visits
            || count >= self.context.budget.stage_visits
        {
            drop(state);
            if descriptor.required {
                return Err(RuleError(format!(
                    "required rule {} exhausted its rewrite budget",
                    descriptor.id
                )));
            }
            self.record(descriptor, "budget_exhausted", || {
                "equivalent input retained".into()
            });
            return Ok(false);
        }
        state.visits += 1;
        *state.stage_visits.entry(descriptor.stage).or_default() += 1;
        Ok(true)
    }
    pub fn apply<T>(
        &mut self,
        descriptor: RuleDescriptor,
        value: T,
        rewrite: impl FnOnce(T, &RuleContext) -> RuleOutcome<T>,
    ) -> Result<(T, bool), RuleError> {
        if !self.visit(descriptor)? {
            return Ok((value, false));
        }
        match rewrite(value, &self.context) {
            RuleOutcome::Unchanged(value) => Ok((value, false)),
            RuleOutcome::Blocked {
                value,
                reason,
                missing_facts,
            } => {
                self.record(descriptor, "blocked", || {
                    format!("{reason}; missing facts: {missing_facts:?}")
                });
                if descriptor.required {
                    Err(RuleError(format!(
                        "required rule {} blocked: {reason}; missing facts: {missing_facts:?}",
                        descriptor.id
                    )))
                } else {
                    Ok((value, false))
                }
            }
            RuleOutcome::Changed { value, evidence } => {
                self.changed(descriptor, || evidence);
                Ok((value, true))
            }
            RuleOutcome::Error(message) => Err(RuleError(format!("{}: {message}", descriptor.id))),
        }
    }
    pub fn fixed_point<T>(
        &mut self,
        descriptor: RuleDescriptor,
        mut value: T,
        mut rewrite: impl FnMut(T, &RuleContext) -> RuleOutcome<T>,
    ) -> Result<T, RuleError> {
        for _ in 0..self.context.budget.iterations {
            let (next, changed) =
                self.apply(descriptor, value, |value, context| rewrite(value, context))?;
            value = next;
            if !changed {
                return Ok(value);
            }
        }
        if descriptor.required {
            Err(RuleError(format!(
                "required rule {} did not converge",
                descriptor.id
            )))
        } else {
            self.record(descriptor, "iteration_limit", || {
                "equivalent plan retained".into()
            });
            Ok(value)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn descriptor(required: bool) -> RuleDescriptor {
        RuleDescriptor {
            required,
            ..graph::REMOVE_TRUE_FILTER
        }
    }
    #[test]
    fn nested_sessions_share_budgets_and_invalidate_scoped_facts() {
        let mut context = RuleContext::default();
        context.trace_limit = 2;
        context.budget.query_visits = 2;
        let mut first = RuleSession::new(context.clone());
        let key = first.analysis_key(vec![1]);
        let (value, changed) = first
            .apply(descriptor(false), 1, |value, _| RuleOutcome::Changed {
                value: value + 1,
                evidence: "proven".into(),
            })
            .unwrap();
        assert_eq!(value, 2);
        assert!(changed);
        assert!(!first.analysis_is_current(&key));
        let mut nested = RuleSession::new(context.clone());
        assert!(nested.visit(descriptor(true)).unwrap());
        assert!(nested.visit(descriptor(true)).is_err());
        assert!(!first.visit(descriptor(false)).unwrap());
        assert_eq!(context.visits(), 2);
        assert_eq!(context.trace().len(), 2);
        assert_eq!(context.new_query().visits(), 0);
        let key = first.analysis_key(vec![1]);
        assert!(!RuleSession::new(context.new_query()).analysis_is_current(&key));
    }
    #[test]
    fn bounded_fixed_points_and_required_stages() {
        let mut context = RuleContext::default();
        context.budget.iterations = 3;
        let mut session = RuleSession::new(context.clone());
        assert_eq!(
            session
                .fixed_point(descriptor(false), 0, |n, _| RuleOutcome::Changed {
                    value: n + 1,
                    evidence: String::new()
                })
                .unwrap(),
            3
        );
        assert!(
            session
                .fixed_point(descriptor(true), 0, |n, _| RuleOutcome::Changed {
                    value: n + 1,
                    evidence: String::new()
                })
                .is_err()
        );
        context.optimize = false;
        let mut session = RuleSession::new(context.new_query());
        assert!(!session.visit(descriptor(false)).unwrap());
        assert!(session.visit(descriptor(true)).unwrap());
    }
    #[test]
    fn trace_evidence_can_read_query_state_without_reentrant_lock() {
        let mut context = RuleContext::default();
        context.trace_limit = 1;
        let mut session = RuleSession::new(context.clone());
        session.record(descriptor(false), "proof", || {
            format!("revision {}", context.revision())
        });
        assert_eq!(context.trace()[0].evidence, "revision 0");
    }
    #[test]
    fn traces_are_lazy_and_native_contract_needs_owner() {
        let mut session = RuleSession::new(RuleContext::default());
        session.record(descriptor(true), "changed", || {
            panic!("trace formatting must be disabled")
        });
        session.context.profile.string_to_float = StringToFloatPolicy::DuckDbNative;
        assert!(session.context.validate().is_err());
        session.context.owner = Some(EngineIdentity {
            name: "duckdb".into(),
            version: "test".into(),
        });
        assert!(session.context.validate().is_ok());
    }
}
