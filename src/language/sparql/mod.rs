//! SPARQL frontend backed by Oxigraph's standards parser.
//!
//! Query algebra is preserved in Graph IR. An ontology mapping resolves
//! SPARQL vocabulary to property-graph labels, relationships, and properties
//! before the existing relational schema mapping lowers it to SQL.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::spargebra::algebra::{
    AggregateExpression, AggregateFunction, Expression, GraphPattern, OrderExpression, QueryDataset,
};
use crate::spargebra::term::{NamedNodePattern, TermPattern, TriplePattern};
use crate::spargebra::{Query, SparqlParser};

use crate::ir::expr::{AggCall, AggKind, BinaryOp, IrExpr};
use crate::ir::plan::{
    ApplyKind, ConstructTriple, DistinctBulk, DistinctMode, GraphPlan, JoinKind, LabelExpr, Length,
    MinusCompatibility, Node, NullsOrder, PathMaterialization, PathUpdate, ProjectErrorPolicy,
    ProjectMode, ProjectionItem, RdfGraphScope, RdfTerm, Slice, SortDir, SortKey, TargetMode,
    UnionAlign,
};
use crate::ir::policy::{
    GraphPlanPolicy, MatchMode, OptionalMissing, PathMode, PropertyMissing, ResultForm,
};
use crate::ir::value::Value;

mod algebra;
mod basic_patterns;
mod ontology_lowering;
mod planner_expression;

mod expression;
pub mod ontology;
mod path;
mod terms;
pub(crate) use SPARQL_GROUP_CONCAT as SPARQL_GROUP_CONCAT_CALL;
pub(crate) use expression::{
    SPARQL_IRI as SPARQL_IRI_CALL, SPARQL_LANG_LITERAL as SPARQL_LANG_LITERAL_CALL,
    SPARQL_LITERAL as SPARQL_LITERAL_CALL, SPARQL_NESTED_EXISTS as SPARQL_NESTED_EXISTS_CALL,
};
pub use ontology::{ClassMapping, OntologyMapping, PredicateMapping};
use terms::{binding, exact_term, named_term, term};

#[derive(Debug, thiserror::Error)]
pub enum SparqlError {
    #[error("SPARQL parse error: {0}")]
    Parse(#[from] crate::spargebra::SparqlSyntaxError),
    #[error("invalid SPARQL base IRI: {0}")]
    BaseIri(String),
    #[error("unsupported SPARQL: {0}")]
    Unsupported(String),
}

pub fn parse_query(source: &str) -> Result<Query, SparqlError> {
    Ok(SparqlParser::new().parse_query(source)?)
}

pub fn parse_update(source: &str, base_iri: Option<&str>) -> Result<crate::spargebra::Update, SparqlError> {
    let parser = SparqlParser::new();
    let parser = if let Some(base) = base_iri {
        parser.with_base_iri(base).map_err(|error| SparqlError::BaseIri(error.to_string()))?
    } else { parser };
    Ok(parser.parse_update(source)?)
}

pub fn parse_query_with_base(source: &str, base_iri: &str) -> Result<Query, SparqlError> {
    let parser = SparqlParser::new()
        .with_base_iri(base_iri)
        .map_err(|error| SparqlError::BaseIri(error.to_string()))?;
    Ok(parser.parse_query(source)?)
}

#[derive(Debug, Clone)]
pub struct SparqlPlanner {
    dataset: String,
    ontology: Option<Arc<OntologyMapping>>,
    query_dataset: Option<QueryDataset>,
    base_iri: Option<String>,
    /// Fresh-name source for EXISTS marks nested inside expressions.
    exists_marks: Arc<std::sync::atomic::AtomicUsize>,
}

impl Default for SparqlPlanner {
    fn default() -> Self {
        Self::new("default")
    }
}

impl SparqlPlanner {
    pub fn new(dataset: impl Into<String>) -> Self {
        Self {
            dataset: dataset.into(),
            ontology: None,
            query_dataset: None,
            base_iri: None,
            exists_marks: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        }
    }

    pub fn with_ontology(mut self, ontology: OntologyMapping) -> Self {
        self.ontology = Some(Arc::new(ontology));
        self
    }

    pub fn plan_str(&self, source: &str) -> Result<GraphPlan, SparqlError> {
        self.plan(&parse_query(source)?)
    }

    pub fn plan(&self, query: &Query) -> Result<GraphPlan, SparqlError> {
        let mut planner = self.clone();
        planner.base_iri = query.base_iri().map(ToString::to_string);
        planner.plan_query(query)
    }

    fn plan_query(&self, query: &Query) -> Result<GraphPlan, SparqlError> {
        match query {
            Query::Select {
                dataset, pattern, ..
            } => {
                let lowered = self.with_query_dataset(dataset.as_ref())?.lower(pattern)?;
                let fields = lowered
                    .projection
                    .unwrap_or_else(|| lowered.variables.iter().cloned().collect());
                Ok(GraphPlan {
                    policy: GraphPlanPolicy::sparql(),
                    root: Box::new(Node::GraphReturn {
                        fields,
                        result_form: ResultForm::RowSet,
                        input: Box::new(lowered.node),
                    }),
                })
            }
            Query::Ask {
                dataset, pattern, ..
            } => {
                let lowered = self.with_query_dataset(dataset.as_ref())?.lower(pattern)?;
                let mut policy = GraphPlanPolicy::sparql();
                policy.result_form = ResultForm::Boolean;
                Ok(GraphPlan {
                    policy,
                    root: Box::new(Node::GraphAsk {
                        field: "ask".into(),
                        input: Box::new(lowered.node),
                    }),
                })
            }
            Query::Construct {
                template,
                dataset,
                pattern,
                ..
            } => {
                let lowered = self.with_query_dataset(dataset.as_ref())?.lower(pattern)?;
                let mut policy = GraphPlanPolicy::sparql();
                policy.result_form = ResultForm::RdfGraph;
                Ok(GraphPlan {
                    policy,
                    root: Box::new(Node::GraphConstructTriples {
                        template: template
                            .iter()
                            .map(|triple| ConstructTriple {
                                subject: term(&triple.subject),
                                predicate: named_term(&triple.predicate),
                                object: term(&triple.object),
                            })
                            .collect(),
                        input: Box::new(lowered.node),
                    }),
                })
            }
            Query::Describe {
                dataset, pattern, ..
            } => {
                let planner = self.with_query_dataset(dataset.as_ref())?;
                let lowered = planner.lower(pattern)?;
                let terms = lowered.projection.clone()
                    .unwrap_or_else(|| lowered.variables.iter().cloned().collect());
                // DESCRIBE policy: outgoing triples of selected resources in
                // the query's default graph, without recursive blank expansion.
                // Use fresh variables that cannot collide with query bindings.
                let mut names = Vec::new();
                for role in ["subject", "predicate", "object"] {
                    let mut name = format!("?__describe_{role}");
                    while lowered.variables.contains(&name) { name.push('_'); }
                    names.push(name);
                }
                let scope = match &planner.query_dataset {
                    Some(dataset) => RdfGraphScope::DatasetDefaultGraph(dataset.default.iter()
                        .map(|iri| iri.as_str().to_string()).collect()),
                    None => RdfGraphScope::DefaultGraph,
                };
                let mut branches = Vec::new();
                for term in terms {
                    let scan = Node::GraphSparqlTriplePattern {
                        dataset: planner.dataset.clone(), graph_scope: scope.clone(),
                        subject: RdfTerm::Variable(names[0].clone()),
                        predicate: RdfTerm::Variable(names[1].clone()),
                        object: RdfTerm::Variable(names[2].clone()), outputs: names.clone(),
                    };
                    // sameTerm is a filter: an unbound described variable must
                    // not act as a wildcard in a compatibility join.
                    branches.push(Node::GraphJoin {
                        kind: JoinKind::Inner, left: Box::new(lowered.node.clone()), right: Box::new(scan),
                        condition: Some(IrExpr::Call { name: "sparql_same_term".into(),
                            args: vec![IrExpr::Binding(term), IrExpr::Binding(names[0].clone())] }),
                    });
                }
                let mut input = Node::GraphEmpty;
                for branch in branches {
                    input = if matches!(input, Node::GraphEmpty) { branch } else {
                        Node::GraphUnion { left: Box::new(input), right: Box::new(branch),
                            all: true, align: UnionAlign::ByVariableName }
                    };
                }
                let mut policy = GraphPlanPolicy::sparql();
                policy.result_form = ResultForm::RdfGraph;
                Ok(GraphPlan { policy, root: Box::new(Node::GraphConstructTriples {
                    template: vec![ConstructTriple {
                        subject: RdfTerm::Variable(names[0].clone()),
                        predicate: RdfTerm::Variable(names[1].clone()),
                        object: RdfTerm::Variable(names[2].clone()),
                    }], input: Box::new(input),
                }) })
            }
        }
    }

    fn with_query_dataset(&self, dataset: Option<&QueryDataset>) -> Result<Self, SparqlError> {
        if dataset.is_some() && self.ontology.is_some() {
            return Err(SparqlError::Unsupported(
                "ontology mappings do not define SPARQL FROM/FROM NAMED graphs".into(),
            ));
        }
        let mut planner = self.clone();
        planner.query_dataset = dataset.cloned();
        Ok(planner)
    }

    /// Dataset-backed plans use typed RDF algebra; ontology plans keep the
    /// property-graph lowering.
    fn typed(&self) -> bool {
        self.ontology.is_none()
    }
}

fn eq(lhs: IrExpr, rhs: IrExpr) -> IrExpr {
    IrExpr::Binary {
        op: BinaryOp::Eq,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }
}

struct Lowered {
    node: Node,
    variables: BTreeSet<String>,
    identity_variables: BTreeSet<String>,
    projection: Option<Vec<String>>,
}

fn combine_apply(left: Lowered, right: Lowered, kind: ApplyKind) -> Result<Lowered, SparqlError> {
    let mut correlation: Vec<_> = left
        .variables
        .intersection(&right.variables)
        .cloned()
        .collect();
    let mut outputs: Vec<_> = right
        .variables
        .difference(&left.variables)
        .cloned()
        .collect();
    for variable in &correlation {
        if left.identity_variables.contains(variable) != right.identity_variables.contains(variable)
        {
            return Err(SparqlError::Unsupported(format!(
                "RDF variable `{variable}` crosses an operator boundary without term identity metadata"
            )));
        }
    }
    let identity_correlation = correlation
        .iter()
        .filter(|variable| left.identity_variables.contains(*variable))
        .flat_map(|variable| crate::ir::rel::rdf::binding_identity_columns(variable))
        .collect::<Vec<_>>();
    correlation.extend(identity_correlation);
    let identity_outputs = outputs
        .iter()
        .filter(|variable| right.identity_variables.contains(*variable))
        .flat_map(|variable| crate::ir::rel::rdf::binding_identity_columns(variable))
        .collect::<Vec<_>>();
    outputs.extend(identity_outputs);
    let mut variables = left.variables.clone();
    variables.extend(right.variables.iter().cloned());
    let mut identity_variables = left.identity_variables.clone();
    identity_variables.extend(right.identity_variables.iter().cloned());
    Ok(Lowered {
        node: Node::GraphApply {
            kind,
            correlation,
            outputs,
            optional_missing: OptionalMissing::Unbound,
            left: Box::new(left.node),
            right: Box::new(right.node),
        },
        variables,
        identity_variables,
        projection: left.projection.or(right.projection),
    })
}

fn term_variables<'a>(terms: impl IntoIterator<Item = &'a RdfTerm>) -> BTreeSet<String> {
    terms
        .into_iter()
        .filter_map(|term| match term {
            RdfTerm::Variable(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

/// `GROUP_CONCAT(expr; SEPARATOR=sep)` as a `CollectRows` aggregate argument.
pub(crate) const SPARQL_GROUP_CONCAT: &str = "sparql_group_concat";

/// Blank nodes in a query pattern are non-distinguished variables.
fn pattern_term(term: &TermPattern) -> RdfTerm {
    match term {
        TermPattern::BlankNode(value) => RdfTerm::Variable(format!("_:{}", value.as_str())),
        other => exact_term(other),
    }
}

fn split_conjuncts<'a>(expression: &'a Expression, out: &mut Vec<&'a Expression>) {
    match expression {
        Expression::And(left, right) => {
            split_conjuncts(left, out);
            split_conjuncts(right, out);
        }
        other => out.push(other),
    }
}

fn join_typed(left: Lowered, right: Lowered, kind: JoinKind, condition: Option<IrExpr>) -> Lowered {
    let mut variables = left.variables.clone();
    variables.extend(right.variables.iter().cloned());
    Lowered {
        node: Node::GraphJoin {
            kind,
            left: Box::new(left.node),
            right: Box::new(right.node),
            condition,
        },
        variables,
        identity_variables: BTreeSet::new(),
        projection: left.projection.or(right.projection),
    }
}
