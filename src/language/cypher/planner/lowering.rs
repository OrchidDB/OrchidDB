pub mod context;
pub mod dispatch;
pub mod pattern;
pub mod predicate;
pub mod project;
pub mod sources;

use std::collections::BTreeSet;

use crate::ir::expr::BindingId;
use crate::ir::plan::{GraphPlan, Node, UnionAlign};
use crate::ir::policy::GraphPlanPolicy;
use crate::language::cypher::ast::Query;
use crate::language::cypher::planner::error::{CypherPlanError, CypherPlanResult};

use context::{CypherTraversalContext, CypherTraversalKind, ScopeFrame};

pub fn lower_query(query: &Query) -> CypherPlanResult<GraphPlan> {
    let (root, _) = lower_query_node(query)?;
    Ok(GraphPlan::new(GraphPlanPolicy::cypher(), root))
}

fn lower_query_node(query: &Query) -> CypherPlanResult<(Node, Vec<BindingId>)> {
    let mut lowerer = Lowerer::new();
    lowerer.lower_query_with_unions(query)
}

pub struct Lowerer {
    next_scope_id: u32,
    scopes: Vec<ScopeFrame>,
    traversal_stack: Vec<CypherTraversalContext>,
    synthetic_counter: usize,
    result_fields: Option<Vec<BindingId>>,
}

impl Lowerer {
    pub fn new() -> Self {
        Self {
            next_scope_id: 0,
            scopes: vec![ScopeFrame::default()],
            traversal_stack: Vec::new(),
            synthetic_counter: 0,
            result_fields: None,
        }
    }

    pub fn lower_query_body(&mut self, query: &Query) -> CypherPlanResult<Node> {
        self.lower_query_with_unions(query).map(|(node, _)| node)
    }

    pub(crate) fn lower_query_with_unions(
        &mut self,
        query: &Query,
    ) -> CypherPlanResult<(Node, Vec<BindingId>)> {
        let initial_scopes = self.scopes.clone();
        let initial_traversals = self.traversal_stack.clone();
        let initial_result_fields = self.result_fields.clone();

        let base_result = self.lower_query_body_with_fields(query);
        if base_result.is_err() {
            self.scopes = initial_scopes.clone();
            self.traversal_stack = initial_traversals.clone();
            self.result_fields = initial_result_fields.clone();
        }
        let (mut root, root_fields) = base_result?;
        for branch in &query.unions {
            self.scopes = initial_scopes.clone();
            self.traversal_stack = initial_traversals.clone();
            self.result_fields = initial_result_fields.clone();

            let branch_result = self.lower_query_with_unions(branch.query.as_ref());
            self.scopes = initial_scopes.clone();
            self.traversal_stack = initial_traversals.clone();
            self.result_fields = initial_result_fields.clone();
            let (right, right_fields) = branch_result?;
            if right_fields != root_fields {
                return Err(CypherPlanError::Invalid(format!(
                    "UNION branches must project the same columns: left [{}], right [{}]",
                    root_fields.join(", "),
                    right_fields.join(", ")
                )));
            }
            root = Node::GraphUnion {
                all: branch.all,
                align: UnionAlign::ByPosition,
                left: root.boxed(),
                right: right.boxed(),
            };
        }

        self.scopes = initial_scopes;
        self.traversal_stack = initial_traversals;
        self.result_fields = initial_result_fields;
        self.replace_scope(root_fields.clone());
        Ok((root, root_fields))
    }

    pub(crate) fn lower_query_body_with_fields(
        &mut self,
        query: &Query,
    ) -> CypherPlanResult<(Node, Vec<BindingId>)> {
        let pushed_root = if self.current_traversal().is_none() {
            let root = self.root_traversal();
            self.push_traversal(root);
            true
        } else {
            false
        };
        let input = if pushed_root {
            Node::GraphOneRow
        } else {
            Node::GraphCorrelate {
                bindings: self.visible_fields(),
            }
        };
        let mut input = Some(input);
        let previous_result_fields = self.result_fields.take();
        let mut returned = false;
        let mut failed = None;
        for clause in &query.clauses {
            let current = input
                .take()
                .expect("query body input should be present before lowering a clause");
            match dispatch::lower_clause(self, current, clause) {
                Ok(next) => input = Some(next),
                Err(err) => {
                    failed = Some(err);
                    break;
                }
            }
            if matches!(clause, crate::language::cypher::ast::Clause::Return(_)) {
                returned = true;
            }
        }
        let result = if let Some(err) = failed {
            Err(err)
        } else {
            let fields = if returned {
                self.result_fields
                    .clone()
                    .unwrap_or_else(|| self.visible_fields())
            } else {
                self.visible_fields()
            };
            if returned {
                let input = input.expect("returned query should retain lowered input");
                Ok((input, fields))
            } else {
                let input = input.expect("implicit return query should retain lowered input");
                Ok((
                    Node::GraphReturn {
                        fields: fields.clone(),
                        result_form: crate::ir::policy::ResultForm::RowSet,
                        input: input.boxed(),
                    },
                    fields,
                ))
            }
        };
        if pushed_root {
            self.pop_traversal();
        }
        self.result_fields = previous_result_fields;
        result
    }

    pub(crate) fn root_traversal(&mut self) -> CypherTraversalContext {
        let id = self.fresh_scope_id();
        CypherTraversalContext::root(id)
    }

    pub(crate) fn child_traversal(
        &mut self,
        parent: &CypherTraversalContext,
        kind: CypherTraversalKind,
    ) -> CypherTraversalContext {
        let id = self.fresh_scope_id();
        CypherTraversalContext::child(id, parent, kind)
    }

    pub(crate) fn current_traversal(&self) -> Option<&CypherTraversalContext> {
        self.traversal_stack.last()
    }

    pub(crate) fn current_traversal_mut(&mut self) -> Option<&mut CypherTraversalContext> {
        self.traversal_stack.last_mut()
    }

    pub(crate) fn push_traversal(&mut self, traversal: CypherTraversalContext) {
        self.traversal_stack.push(traversal);
        let inherited = self.scopes.last().cloned().unwrap_or_default();
        self.scopes.push(inherited);
    }

    pub(crate) fn pop_traversal(&mut self) -> Option<CypherTraversalContext> {
        let traversal = self.traversal_stack.pop();
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
        traversal
    }

    pub(crate) fn is_visible(&self, binding: &str) -> bool {
        self.scopes
            .last()
            .map(|scope| scope.visible.contains(binding))
            .unwrap_or(false)
    }

    pub(crate) fn add_visible(&mut self, binding: impl Into<BindingId>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.visible.insert(binding.into());
        }
    }

    pub(crate) fn add_nullable(&mut self, binding: impl Into<BindingId>) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.nullable.insert(binding.into());
        }
    }

    pub(crate) fn record_current_imports<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        if let Some(traversal) = self.current_traversal_mut() {
            traversal.add_imports(bindings);
        }
    }

    pub(crate) fn record_current_correlation<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        if let Some(traversal) = self.current_traversal_mut() {
            traversal.add_correlation(bindings);
        }
    }

    pub(crate) fn record_current_outputs<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        if let Some(traversal) = self.current_traversal_mut() {
            traversal.add_produced(bindings);
        }
    }

    pub(crate) fn record_current_nullable<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        if let Some(traversal) = self.current_traversal_mut() {
            traversal.add_nullable(bindings);
        }
    }

    pub(crate) fn replace_scope<I>(&mut self, bindings: I)
    where
        I: IntoIterator<Item = BindingId>,
    {
        if let Some(scope) = self.scopes.last_mut() {
            scope.visible = bindings.into_iter().collect();
            scope.nullable.clear();
        }
    }

    pub(crate) fn set_result_fields(&mut self, bindings: Vec<BindingId>) {
        self.result_fields = Some(bindings);
    }

    pub(crate) fn visible_fields(&self) -> Vec<BindingId> {
        self.scopes
            .last()
            .map(|scope| scope.visible.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn visible_set(&self) -> BTreeSet<BindingId> {
        self.visible_fields().into_iter().collect()
    }

    pub(crate) fn with_preserved_scope<T, F>(&mut self, f: F) -> CypherPlanResult<T>
    where
        F: FnOnce(&mut Self) -> CypherPlanResult<T>,
    {
        let scopes = self.scopes.clone();
        let traversals = self.traversal_stack.clone();
        let result_fields = self.result_fields.clone();
        let result = f(self);
        self.scopes = scopes;
        self.traversal_stack = traversals;
        self.result_fields = result_fields;
        result
    }

    pub(crate) fn synthetic(&mut self, prefix: &str) -> BindingId {
        let id = self.synthetic_counter;
        self.synthetic_counter += 1;
        format!("__cypher_{prefix}_{id}")
    }

    fn fresh_scope_id(&mut self) -> u32 {
        let id = self.next_scope_id;
        self.next_scope_id += 1;
        id
    }
}
