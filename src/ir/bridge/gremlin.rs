//! Gremlin → Graph IR bridge.
//!
//! The bridge accepts a small `GremlinTraversal` AST that mirrors a useful
//! subset of TinkerPop steps. The flat step list reflects how `parser::lower`
//! presents traversals; the bridge folds the list left-to-right into IR
//! operators with `current` semantics.

use crate::ir::expr::{AggCall, AggKind, BinaryOp, IrExpr, Lit, StringOp};
use crate::ir::plan::{
    Direction, DistinctBulk, DistinctMode, EmitMode, GraphPlan, LabelExpr, Length, Node,
    NullsOrder, PathMaterialization, PathObjects, PathUpdate, ProjectErrorPolicy, ProjectMode,
    ProjectionItem, Slice, SortDir, SortKey, TargetMode,
};
use crate::ir::policy::{GraphPlanPolicy, MatchMode, PathMode, PropertyMissing, ResultForm};

#[derive(Debug, Clone)]
pub struct GremlinTraversal {
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone)]
pub enum Step {
    /// `g.V()` / `g.V().hasLabel(label)`. The optional label is folded in
    /// for the common case.
    V {
        label: Option<String>,
    },
    HasLabel(String),
    Has {
        key: String,
        value: Lit,
    },
    HasGt {
        key: String,
        value: Lit,
    },
    HasGte {
        key: String,
        value: Lit,
    },
    Out {
        rel_types: Vec<String>,
    },
    In {
        rel_types: Vec<String>,
    },
    Both {
        rel_types: Vec<String>,
    },
    /// `as('a')` — record a label on the current binding.
    As(String),
    /// `select('a','b')`.
    Select(Vec<String>),
    /// `values('name')`.
    Values(String),
    /// `count()`.
    Count,
    /// `dedup()`.
    Dedup,
    /// `order().by('age', desc)`.
    OrderBy {
        key: String,
        dir: SortDir,
    },
    /// `limit(n)`.
    Limit(u64),
    /// `tail(n)`.
    Tail(u64),
    /// `where(neq('a'))` form — currently we model only `where(<other>)`
    /// expressed as predicate node ids. The bridge accepts a raw predicate
    /// so the planner can compose more complex wheres.
    Where(Predicate),
    /// `groupCount().by(key)`.
    GroupCountBy(String),
    /// `path()`.
    Path,
    /// `repeat(out('KNOWS')).times(n).emit()` — a single-step body for now.
    RepeatOutTimesEmit {
        rel_types: Vec<String>,
        times: u32,
    },
}

#[derive(Debug, Clone)]
pub enum Predicate {
    Lit(Lit),
    Property {
        binding: String,
        name: String,
    },
    Var(String),
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
    HasLabel {
        binding: String,
        label: String,
    },
    And(Vec<Predicate>),
}

pub fn lower_traversal(traversal: &GremlinTraversal) -> GraphPlan {
    let policy = GraphPlanPolicy::gremlin();
    let mut node = Node::GraphOneRow;
    for (idx, step) in traversal.steps.iter().enumerate() {
        node = lower_step(step, node, idx);
    }
    let root = Node::GraphReturn {
        fields: vec!["current".to_string()],
        result_form: ResultForm::TraverserStream,
        input: node.boxed(),
    };
    GraphPlan::new(policy, root)
}

fn lower_step(step: &Step, input: Node, idx: usize) -> Node {
    match step {
        Step::V { label } => {
            let scan = Node::GraphNodeScan {
                graph: "default".into(),
                binding: "current".into(),
                labels: match label {
                    Some(label) => LabelExpr::label(label.clone()),
                    None => LabelExpr::Any,
                },
            };
            // Discard the OneRow upstream; `g.V()` is a fresh source.
            let _ = idx;
            let _ = input;
            Node::GraphBind {
                bind: "current".into(),
                kind: crate::ir::plan::BindKind::Node,
                expr: None,
                input: scan.boxed(),
            }
        }
        Step::HasLabel(label) => Node::GraphFilter {
            condition: IrExpr::HasLabel {
                binding: "current".into(),
                label: label.clone(),
            },
            input: input.boxed(),
        },
        Step::Has { key, value } => Node::GraphFilter {
            condition: IrExpr::eq(
                IrExpr::property(
                    "current".to_string(),
                    key.clone(),
                    PropertyMissing::DropUnproductive,
                ),
                IrExpr::Lit(value.clone()),
            ),
            input: input.boxed(),
        },
        Step::HasGt { key, value } => Node::GraphFilter {
            condition: IrExpr::Binary {
                op: BinaryOp::Gt,
                lhs: Box::new(IrExpr::property(
                    "current".to_string(),
                    key.clone(),
                    PropertyMissing::DropUnproductive,
                )),
                rhs: Box::new(IrExpr::Lit(value.clone())),
            },
            input: input.boxed(),
        },
        Step::HasGte { key, value } => Node::GraphFilter {
            condition: IrExpr::Binary {
                op: BinaryOp::Gte,
                lhs: Box::new(IrExpr::property(
                    "current".to_string(),
                    key.clone(),
                    PropertyMissing::DropUnproductive,
                )),
                rhs: Box::new(IrExpr::Lit(value.clone())),
            },
            input: input.boxed(),
        },
        Step::Out { rel_types } => expand("current", rel_types, Direction::Out, input),
        Step::In { rel_types } => expand("current", rel_types, Direction::In, input),
        Step::Both { rel_types } => expand("current", rel_types, Direction::Both, input),
        Step::As(label) => Node::GraphBind {
            bind: label.clone(),
            kind: crate::ir::plan::BindKind::Node,
            expr: Some(IrExpr::Binding("current".into())),
            input: input.boxed(),
        },
        Step::Select(labels) => Node::GraphSelect {
            labels: labels.clone(),
            outputs: labels.clone(),
            input: input.boxed(),
        },
        Step::Values(key) => Node::GraphCurrentProject {
            expr: IrExpr::property(
                "current".to_string(),
                key.clone(),
                PropertyMissing::DropUnproductive,
            ),
            fields: vec!["current".to_string()],
            input: input.boxed(),
        },
        Step::Count => Node::GraphAggregate {
            group: Vec::new(),
            aggs: vec![AggCall {
                kind: AggKind::CountBulk,
                alias: "current".into(),
                arg: None,
                distinct: false,
            }],
            fields: vec!["current".to_string()],
            input: input.boxed(),
        },
        Step::Dedup => Node::GraphDistinct {
            keys: vec!["current".into()],
            mode: DistinctMode::Traverser,
            bulk: DistinctBulk::ResetToOne,
            input: input.boxed(),
        },
        Step::OrderBy { key, dir } => Node::GraphSort {
            keys: vec![SortKey {
                expr: IrExpr::property(
                    "current".to_string(),
                    key.clone(),
                    PropertyMissing::DropUnproductive,
                ),
                dir: dir.clone(),
                nulls: NullsOrder::ProviderDefined,
            }],
            input: input.boxed(),
        },
        Step::Limit(n) => Node::GraphSlice {
            slice: Slice {
                offset: 0,
                fetch: Some(*n),
                tail: None,
            },
            input: input.boxed(),
        },
        Step::Tail(n) => Node::GraphSlice {
            slice: Slice {
                offset: 0,
                fetch: None,
                tail: Some(*n),
            },
            input: input.boxed(),
        },
        Step::Where(predicate) => Node::GraphFilter {
            condition: lower_predicate(predicate),
            input: input.boxed(),
        },
        Step::GroupCountBy(key) => {
            // Lower as Project to a key/count pair, then aggregate.
            let aggregated = Node::GraphAggregate {
                group: vec![ProjectionItem {
                    alias: "key".into(),
                    expr: IrExpr::property(
                        "current".to_string(),
                        key.clone(),
                        PropertyMissing::DropUnproductive,
                    ),
                }],
                aggs: vec![AggCall {
                    kind: AggKind::CountBulk,
                    alias: "value".into(),
                    arg: None,
                    distinct: false,
                }],
                fields: vec!["key".into(), "value".into()],
                input: input.boxed(),
            };
            Node::GraphProject {
                mode: ProjectMode::ReplaceCurrent,
                items: vec![ProjectionItem {
                    alias: "current".into(),
                    expr: IrExpr::List(vec![
                        IrExpr::Binding("key".into()),
                        IrExpr::Binding("value".into()),
                    ]),
                }],
                error_policy: ProjectErrorPolicy::PropagateError,
                input: aggregated.boxed(),
            }
        }
        Step::Path => Node::GraphCurrentProject {
            expr: IrExpr::Binding("path".into()),
            fields: vec!["current".to_string()],
            input: input.boxed(),
        },
        Step::RepeatOutTimesEmit { rel_types, times } => {
            let body = Node::GraphExpand {
                graph: "default".into(),
                source: "current".into(),
                target: "current".into(),
                target_mode: TargetMode::ReplaceCurrent,
                target_labels: LabelExpr::Any,
                rel_binding: None,
                rel_types: if rel_types.is_empty() {
                    LabelExpr::Any
                } else {
                    LabelExpr::AnyOf(rel_types.clone())
                },
                dir: Direction::Out,
                length: Length::ONE,
                history: None,
                path: None,
                path_mode: PathMode::None,
                match_mode: MatchMode::ProviderDefined,
                path_materialization: PathMaterialization::None,
                path_update: PathUpdate::None,
                input: Node::GraphCorrelate {
                    bindings: vec!["current".into()],
                }
                .boxed(),
            };
            Node::GraphRepeat {
                loop_name: None,
                times: Some(*times),
                emit: EmitMode::AfterEachIteration,
                until: None,
                until_traversal: None,
                path: None,
                path_objects: PathObjects::VerticesOnly,
                prefix_predicate: None,
                prefix_traversal: None,
                seed: input.boxed(),
                body: body.boxed(),
            }
        }
    }
}

fn expand(source: &str, rel_types: &[String], dir: Direction, input: Node) -> Node {
    Node::GraphExpand {
        graph: "default".into(),
        source: source.into(),
        target: "current".into(),
        target_mode: TargetMode::ReplaceCurrent,
        target_labels: LabelExpr::Any,
        rel_binding: None,
        rel_types: if rel_types.is_empty() {
            LabelExpr::Any
        } else {
            LabelExpr::AnyOf(rel_types.to_vec())
        },
        dir,
        length: Length::ONE,
        history: None,
        path: None,
        path_mode: PathMode::None,
        match_mode: MatchMode::ProviderDefined,
        path_materialization: PathMaterialization::None,
        path_update: PathUpdate::None,
        input: input.boxed(),
    }
}

fn lower_predicate(predicate: &Predicate) -> IrExpr {
    match predicate {
        Predicate::Lit(lit) => IrExpr::Lit(lit.clone()),
        Predicate::Property { binding, name } => IrExpr::property(
            binding.clone(),
            name.clone(),
            PropertyMissing::DropUnproductive,
        ),
        Predicate::Var(name) => IrExpr::Binding(name.clone()),
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
        Predicate::HasLabel { binding, label } => IrExpr::HasLabel {
            binding: binding.clone(),
            label: label.clone(),
        },
        Predicate::And(parts) => IrExpr::and(parts.iter().map(lower_predicate).collect()),
    }
}
