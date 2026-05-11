//! Cypher → Graph IR bridge.
//!
//! The bridge accepts a small `CypherQuery` AST that the existing cypher
//! parser (under `src/language/cypher/ast/`) can be adapted to produce.
//! The shape mirrors the doc's §2 / §3 / §4 / §6 examples for the subset
//! we support so far. SPARQL/GQL semantics are out of scope.
//!
//! The intended call site is `CypherToDataFusionPlanner::plan` in
//! `src/language/cypher/planner/mod.rs`: once that planner consumes the
//! existing AST, it should construct a `CypherQuery` value here and call
//! `lower_query`.

use crate::ir::expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit, StringOp};
use crate::ir::plan::{
    ApplyKind, Direction, DistinctBulk, GraphPlan, JoinKind, LabelExpr, Length, Node, NullsOrder,
    PathMaterialization, PathUpdate, ProjectErrorPolicy, ProjectMode, ProjectionItem, Slice,
    SortDir, SortKey, TargetMode,
};
use crate::ir::policy::{GraphPlanPolicy, OptionalMissing, PropertyMissing, ResultForm};

#[derive(Debug, Clone)]
pub struct CypherQuery {
    pub matches: Vec<MatchClause>,
    pub r#where: Option<Predicate>,
    pub r#return: ReturnClause,
}

#[derive(Debug, Clone)]
pub struct MatchClause {
    pub optional: bool,
    pub pattern: Pattern,
    pub r#where: Option<Predicate>,
}

#[derive(Debug, Clone)]
pub struct Pattern {
    pub start: NodePattern,
    pub chains: Vec<RelChain>,
}

#[derive(Debug, Clone)]
pub struct NodePattern {
    pub binding: String,
    pub label: Option<String>,
    pub property_filters: Vec<(String, Predicate)>,
}

#[derive(Debug, Clone)]
pub struct RelChain {
    pub rel: RelPattern,
    pub node: NodePattern,
}

#[derive(Debug, Clone)]
pub struct RelPattern {
    pub binding: Option<String>,
    pub rel_types: Vec<String>,
    pub direction: Direction,
    pub length: Length,
}

#[derive(Debug, Clone)]
pub struct ReturnClause {
    pub items: Vec<ReturnItem>,
    pub distinct: bool,
    pub order_by: Vec<OrderItem>,
    pub skip: Option<u64>,
    pub limit: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ReturnItem {
    pub alias: String,
    pub value: ReturnValue,
}

/// Cypher return values either project a value or compute an aggregate.
/// We keep the surface narrow so the bridge stays focused.
#[derive(Debug, Clone)]
pub enum ReturnValue {
    Expr(Predicate),
    /// `count(*)`.
    CountStar,
    /// `count(x)`, `sum(x)`, etc. — `arg` is the bound variable.
    Aggregate {
        kind: AggKind,
        arg: Predicate,
    },
}

#[derive(Debug, Clone)]
pub struct OrderItem {
    pub value: Predicate,
    pub dir: SortDir,
}

/// Surface predicate for filters and projections. Maps directly onto
/// `IrExpr` shapes.
#[derive(Debug, Clone)]
pub enum Predicate {
    Lit(Lit),
    Var(String),
    Property {
        binding: String,
        name: String,
    },
    And(Vec<Predicate>),
    Or(Vec<Predicate>),
    Not(Box<Predicate>),
    Compare {
        op: BinaryOp,
        lhs: Box<Predicate>,
        rhs: Box<Predicate>,
    },
    StringPred {
        op: StringOp,
        target: Box<Predicate>,
        pattern: Box<Predicate>,
    },
    IsNull(Box<Predicate>),
    IsNotNull(Box<Predicate>),
}

/// Lower a `CypherQuery` to a `GraphPlan` with the standard Cypher policy.
pub fn lower_query(query: &CypherQuery) -> GraphPlan {
    let policy = GraphPlanPolicy::cypher();
    let mut plan: Option<Node> = None;
    let mut all_optional = false;
    for clause in &query.matches {
        let mut sub = lower_pattern(&clause.pattern);
        if let Some(predicate) = &clause.r#where {
            sub = Node::GraphFilter {
                condition: lower_predicate(predicate),
                input: sub.boxed(),
            };
        }
        for filter in pattern_property_filters(&clause.pattern) {
            sub = Node::GraphFilter {
                condition: filter,
                input: sub.boxed(),
            };
        }
        plan = Some(match plan.take() {
            None if clause.optional => {
                all_optional = true;
                // OPTIONAL as the very first clause emits a single
                // null-extended row when the pattern is empty.
                let outputs = pattern_visible_bindings(&clause.pattern);
                Node::GraphApply {
                    kind: ApplyKind::Optional,
                    correlation: Vec::new(),
                    outputs,
                    optional_missing: OptionalMissing::Null,
                    left: Node::GraphOneRow.boxed(),
                    right: sub.boxed(),
                }
            }
            None => sub,
            Some(prev) if clause.optional => Node::GraphApply {
                kind: ApplyKind::Optional,
                correlation: pattern_visible_bindings(&clause.pattern)
                    .into_iter()
                    .filter(|b| binding_in(&prev, b))
                    .collect(),
                outputs: pattern_visible_bindings(&clause.pattern)
                    .into_iter()
                    .filter(|b| !binding_in(&prev, b))
                    .collect(),
                optional_missing: OptionalMissing::Null,
                left: prev.boxed(),
                right: rewrite_pattern_to_correlated(&clause.pattern, sub),
            },
            Some(prev) => Node::GraphJoin {
                kind: JoinKind::Inner,
                left: prev.boxed(),
                right: sub.boxed(),
                condition: None,
            },
        });
    }
    let mut node = plan.unwrap_or(Node::GraphOneRow);
    let _ = all_optional;
    if let Some(predicate) = &query.r#where {
        node = Node::GraphFilter {
            condition: lower_predicate(predicate),
            input: node.boxed(),
        };
    }
    node = lower_return(&query.r#return, node);
    GraphPlan::new(policy, node)
}

fn pattern_visible_bindings(pattern: &Pattern) -> Vec<String> {
    let mut out = vec![pattern.start.binding.clone()];
    for chain in &pattern.chains {
        if let Some(b) = &chain.rel.binding {
            out.push(b.clone());
        }
        out.push(chain.node.binding.clone());
    }
    out
}

fn pattern_property_filters(pattern: &Pattern) -> Vec<IrExpr> {
    let mut out = Vec::new();
    out.extend(node_property_filters(&pattern.start));
    for chain in &pattern.chains {
        out.extend(node_property_filters(&chain.node));
    }
    out
}

fn node_property_filters(pattern: &NodePattern) -> Vec<IrExpr> {
    pattern
        .property_filters
        .iter()
        .map(|(key, predicate)| {
            IrExpr::eq(
                IrExpr::property(
                    pattern.binding.clone(),
                    key.clone(),
                    PropertyMissing::NullOnMissing,
                ),
                lower_predicate(predicate),
            )
        })
        .collect()
}

fn lower_pattern(pattern: &Pattern) -> Node {
    let mut node = Node::GraphNodeScan {
        graph: "default".into(),
        binding: pattern.start.binding.clone(),
        labels: match &pattern.start.label {
            Some(label) => LabelExpr::label(label),
            None => LabelExpr::Any,
        },
    };
    let mut current_source = pattern.start.binding.clone();
    for chain in &pattern.chains {
        node = Node::GraphExpand {
            graph: "default".into(),
            source: current_source.clone(),
            target: chain.node.binding.clone(),
            target_mode: TargetMode::BindNew,
            target_labels: match &chain.node.label {
                Some(label) => LabelExpr::label(label),
                None => LabelExpr::Any,
            },
            rel_binding: chain.rel.binding.clone(),
            rel_types: if chain.rel.rel_types.is_empty() {
                LabelExpr::Any
            } else {
                LabelExpr::AnyOf(chain.rel.rel_types.clone())
            },
            dir: chain.rel.direction,
            length: chain.rel.length.clone(),
            history: None,
            path: None,
            path_mode: crate::ir::policy::PathMode::Walk,
            match_mode: crate::ir::policy::MatchMode::DifferentRelationships,
            path_materialization: PathMaterialization::None,
            path_update: PathUpdate::None,
            input: node.boxed(),
        };
        current_source = chain.node.binding.clone();
    }
    node
}

/// Replace the bottom-most `NodeScan` of `sub` with a `Correlate` so the
/// pattern can be applied per outer row in `OPTIONAL MATCH`.
fn rewrite_pattern_to_correlated(pattern: &Pattern, sub: Node) -> Box<Node> {
    let correlate = Node::GraphCorrelate {
        bindings: vec![pattern.start.binding.clone()],
    };
    Box::new(replace_leaf_scan(sub, &pattern.start.binding, correlate))
}

fn replace_leaf_scan(node: Node, binding: &str, replacement: Node) -> Node {
    match node {
        Node::GraphNodeScan { binding: b, .. } if b == binding => replacement,
        Node::GraphFilter { condition, input } => Node::GraphFilter {
            condition,
            input: Box::new(replace_leaf_scan(*input, binding, replacement)),
        },
        Node::GraphBind {
            bind,
            kind,
            expr,
            input,
        } => Node::GraphBind {
            bind,
            kind,
            expr,
            input: Box::new(replace_leaf_scan(*input, binding, replacement)),
        },
        Node::GraphExpand {
            graph,
            source,
            target,
            target_mode,
            target_labels,
            rel_binding,
            rel_types,
            dir,
            length,
            history,
            path,
            path_mode,
            match_mode,
            path_materialization,
            path_update,
            input,
        } => Node::GraphExpand {
            graph,
            source,
            target,
            target_mode,
            target_labels,
            rel_binding,
            rel_types,
            dir,
            length,
            history,
            path,
            path_mode,
            match_mode,
            path_materialization,
            path_update,
            input: Box::new(replace_leaf_scan(*input, binding, replacement)),
        },
        Node::GraphProject {
            mode,
            items,
            error_policy,
            input,
        } => Node::GraphProject {
            mode,
            items,
            error_policy,
            input: Box::new(replace_leaf_scan(*input, binding, replacement)),
        },
        other => other,
    }
}

fn binding_in(node: &Node, binding: &str) -> bool {
    match node {
        Node::GraphNodeScan { binding: b, .. } | Node::GraphRelScan { binding: b, .. } => {
            b == binding
        }
        Node::GraphBind { bind, input, .. } => bind == binding || binding_in(input, binding),
        Node::GraphExpand {
            target,
            rel_binding,
            history,
            path,
            input,
            ..
        } => {
            target == binding
                || rel_binding.as_deref() == Some(binding)
                || history.as_deref() == Some(binding)
                || path.as_deref() == Some(binding)
                || binding_in(input, binding)
        }
        Node::GraphFilter { input, .. }
        | Node::GraphSort { input, .. }
        | Node::GraphSlice { input, .. }
        | Node::GraphSliceExpr { input, .. }
        | Node::GraphDistinct { input, .. } => binding_in(input, binding),
        Node::GraphProject { items, input, .. } => {
            items.iter().any(|i| i.alias == binding) || binding_in(input, binding)
        }
        Node::GraphAggregate { group, aggs, .. } => {
            group.iter().any(|i| i.alias == binding) || aggs.iter().any(|a| a.alias == binding)
        }
        Node::GraphApply { outputs, left, .. } => {
            outputs.iter().any(|s| s == binding) || binding_in(left, binding)
        }
        Node::GraphJoin { left, right, .. } | Node::GraphUnion { left, right, .. } => {
            binding_in(left, binding) || binding_in(right, binding)
        }
        Node::GraphUnwind { bind, input, .. } => bind == binding || binding_in(input, binding),
        Node::GraphOneRow
        | Node::GraphEmpty
        | Node::GraphCorrelate { .. }
        | Node::GraphValues { .. } => false,
        _ => false,
    }
}

fn lower_return(clause: &ReturnClause, mut input: Node) -> Node {
    // Aggregates first.
    let has_agg = clause.items.iter().any(|item| {
        matches!(
            item.value,
            ReturnValue::CountStar | ReturnValue::Aggregate { .. }
        )
    });
    if has_agg {
        let group: Vec<ProjectionItem> = clause
            .items
            .iter()
            .filter_map(|item| match &item.value {
                ReturnValue::Expr(predicate) => Some(ProjectionItem {
                    alias: item.alias.clone(),
                    expr: lower_predicate(predicate),
                }),
                _ => None,
            })
            .collect();
        let aggs: Vec<AggCall> = clause
            .items
            .iter()
            .filter_map(|item| match &item.value {
                ReturnValue::CountStar => Some(AggCall {
                    kind: AggKind::CountRows,
                    alias: item.alias.clone(),
                    arg: None,
                    distinct: false,
                }),
                ReturnValue::Aggregate { kind, arg } => Some(AggCall {
                    kind: *kind,
                    alias: item.alias.clone(),
                    arg: Some(lower_predicate(arg)),
                    distinct: false,
                }),
                ReturnValue::Expr(_) => None,
            })
            .collect();
        let agg_fields: Vec<String> = group
            .iter()
            .map(|i| i.alias.clone())
            .chain(aggs.iter().map(|a| a.alias.clone()))
            .collect();
        input = Node::GraphAggregate {
            group,
            aggs,
            fields: agg_fields,
            input: input.boxed(),
        };
    } else {
        let items = clause
            .items
            .iter()
            .map(|item| match &item.value {
                ReturnValue::Expr(predicate) => ProjectionItem {
                    alias: item.alias.clone(),
                    expr: lower_predicate(predicate),
                },
                _ => unreachable!("aggregates handled above"),
            })
            .collect();
        input = Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items,
            error_policy: ProjectErrorPolicy::PropagateError,
            input: input.boxed(),
        };
    }
    if clause.distinct {
        input = Node::GraphDistinct {
            keys: clause.items.iter().map(|i| i.alias.clone()).collect(),
            mode: crate::ir::plan::DistinctMode::Row,
            bulk: DistinctBulk::NotApplicable,
            input: input.boxed(),
        };
    }
    if !clause.order_by.is_empty() {
        input = Node::GraphSort {
            keys: clause
                .order_by
                .iter()
                .map(|item| SortKey {
                    expr: lower_predicate(&item.value),
                    dir: item.dir.clone(),
                    nulls: match item.dir {
                        SortDir::Asc => NullsOrder::Last,
                        SortDir::Desc => NullsOrder::First,
                    },
                })
                .collect(),
            input: input.boxed(),
        };
    }
    if clause.skip.is_some() || clause.limit.is_some() {
        input = Node::GraphSlice {
            slice: Slice {
                offset: clause.skip.unwrap_or(0),
                fetch: clause.limit,
                tail: None,
            },
            input: input.boxed(),
        };
    }
    let fields: Vec<String> = clause.items.iter().map(|i| i.alias.clone()).collect();
    Node::GraphReturn {
        fields,
        result_form: ResultForm::RowSet,
        input: input.boxed(),
    }
}

fn lower_predicate(predicate: &Predicate) -> IrExpr {
    match predicate {
        Predicate::Lit(lit) => IrExpr::Lit(lit.clone()),
        Predicate::Var(name) => IrExpr::Binding(name.clone()),
        Predicate::Property { binding, name } => IrExpr::property(
            binding.clone(),
            name.clone(),
            PropertyMissing::NullOnMissing,
        ),
        Predicate::And(parts) => IrExpr::and(parts.iter().map(lower_predicate).collect()),
        Predicate::Or(parts) => {
            let mut iter = parts.iter().map(lower_predicate);
            let mut acc = match iter.next() {
                Some(first) => first,
                None => return IrExpr::lit_bool(false),
            };
            for next in iter {
                acc = IrExpr::Binary {
                    op: BinaryOp::Or,
                    lhs: Box::new(acc),
                    rhs: Box::new(next),
                };
            }
            acc
        }
        Predicate::Not(inner) => IrExpr::Not(Box::new(lower_predicate(inner))),
        Predicate::Compare { op, lhs, rhs } => IrExpr::Binary {
            op: *op,
            lhs: Box::new(lower_predicate(lhs)),
            rhs: Box::new(lower_predicate(rhs)),
        },
        Predicate::StringPred {
            op,
            target,
            pattern,
        } => IrExpr::StringPredicate {
            op: *op,
            target: Box::new(lower_predicate(target)),
            pattern: Box::new(lower_predicate(pattern)),
        },
        Predicate::IsNull(inner) => IrExpr::IsNull(Box::new(lower_predicate(inner))),
        Predicate::IsNotNull(inner) => IrExpr::IsNotNull(Box::new(lower_predicate(inner))),
    }
}
