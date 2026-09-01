//! SPARQL frontend backed by Oxigraph's standards parser.
//!
//! Query algebra is preserved in Graph IR. An ontology mapping resolves
//! SPARQL vocabulary to property-graph labels, relationships, and properties
//! before the existing relational schema mapping lowers it to SQL.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use spargebra::algebra::{GraphPattern, OrderExpression, QueryDataset};
use spargebra::term::{NamedNodePattern, TermPattern, TriplePattern};
use spargebra::{Query, SparqlParser};

use crate::ir::expr::{BinaryOp, IrExpr};
use crate::ir::plan::{
    ApplyKind, ConstructTriple, DistinctBulk, DistinctMode, GraphPlan, LabelExpr, Length,
    MinusCompatibility, Node, NullsOrder, PathMaterialization, PathUpdate, ProjectErrorPolicy,
    ProjectMode, ProjectionItem, RdfGraphScope, RdfTerm, Slice, SortDir, SortKey, TargetMode,
    UnionAlign,
};
use crate::ir::policy::{
    GraphPlanPolicy, MatchMode, OptionalMissing, PathMode, PropertyMissing, ResultForm,
};
use crate::ir::value::Value;

mod expression;
pub mod ontology;
mod path;
mod terms;
pub use ontology::{ClassMapping, OntologyMapping, PredicateMapping};
use terms::{binding, named_term, term};

#[derive(Debug, thiserror::Error)]
pub enum SparqlError {
    #[error("SPARQL parse error: {0}")]
    Parse(#[from] spargebra::SparqlSyntaxError),
    #[error("invalid SPARQL base IRI: {0}")]
    BaseIri(String),
    #[error("unsupported SPARQL: {0}")]
    Unsupported(String),
}

pub fn parse_query(source: &str) -> Result<Query, SparqlError> {
    Ok(SparqlParser::new().parse_query(source)?)
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
        match query {
            Query::Select {
                dataset, pattern, ..
            } => {
                let lowered = wrap_dataset(self.lower(pattern)?, dataset.as_ref());
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
                let lowered = wrap_dataset(self.lower(pattern)?, dataset.as_ref());
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
                let lowered = wrap_dataset(self.lower(pattern)?, dataset.as_ref());
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
                let lowered = wrap_dataset(self.lower(pattern)?, dataset.as_ref());
                let terms = lowered
                    .projection
                    .clone()
                    .unwrap_or_else(|| lowered.variables.iter().cloned().collect())
                    .into_iter()
                    .map(RdfTerm::Variable)
                    .collect();
                let mut policy = GraphPlanPolicy::sparql();
                policy.result_form = ResultForm::RdfGraph;
                Ok(GraphPlan {
                    policy,
                    root: Box::new(Node::GraphDescribe {
                        terms,
                        input: Box::new(lowered.node),
                    }),
                })
            }
        }
    }

    fn lower(&self, pattern: &GraphPattern) -> Result<Lowered, SparqlError> {
        self.lower_in_scope(pattern, RdfGraphScope::DefaultGraph)
    }

    fn lower_in_scope(
        &self,
        pattern: &GraphPattern,
        scope: RdfGraphScope,
    ) -> Result<Lowered, SparqlError> {
        match pattern {
            GraphPattern::Bgp { patterns } => self.lower_bgp(patterns, scope),
            GraphPattern::Path {
                subject,
                path: path_expr,
                object,
            } => {
                let subject = term(subject);
                let object = term(object);
                let variables = term_variables([&subject, &object]);
                Ok(Lowered {
                    node: Node::GraphRdfPropertyPath {
                        dataset: self.dataset.clone(),
                        graph_scope: scope,
                        subject,
                        object,
                        path: path::lower(path_expr),
                        path_materialization: PathMaterialization::EndpointsOnly,
                        zero_length: path::zero_length(path_expr),
                    },
                    variables,
                    projection: None,
                })
            }
            GraphPattern::Project { inner, variables } => {
                let mut lowered = self.lower(inner)?;
                lowered.projection = Some(variables.iter().map(binding).collect());
                Ok(lowered)
            }
            GraphPattern::Distinct { inner } => {
                let mut lowered = self.lower(inner)?;
                let keys = lowered
                    .projection
                    .clone()
                    .unwrap_or_else(|| lowered.variables.iter().cloned().collect());
                lowered.node = Node::GraphDistinct {
                    keys,
                    mode: DistinctMode::Solution,
                    bulk: DistinctBulk::NotApplicable,
                    input: Box::new(lowered.node),
                };
                Ok(lowered)
            }
            GraphPattern::Slice {
                inner,
                start,
                length,
            } => {
                let mut lowered = self.lower(inner)?;
                lowered.node = Node::GraphSlice {
                    slice: Slice {
                        offset: *start as u64,
                        fetch: length.map(|value| value as u64),
                        tail: None,
                    },
                    input: Box::new(lowered.node),
                };
                Ok(lowered)
            }
            GraphPattern::Join { left, right } => combine_apply(
                self.lower_in_scope(left, scope.clone())?,
                self.lower_in_scope(right, scope)?,
                ApplyKind::Inner,
            ),
            GraphPattern::LeftJoin {
                left,
                right,
                expression: optional_expr,
            } => {
                let left = self.lower_in_scope(left, scope.clone())?;
                let mut right = self.lower_in_scope(right, scope)?;
                if let Some(expr) = optional_expr {
                    right.node = Node::GraphFilter {
                        condition: expression::lower(expr),
                        input: Box::new(right.node),
                    };
                }
                combine_apply(left, right, ApplyKind::Optional)
            }
            GraphPattern::Filter { expr, inner } => {
                let mut lowered = self.lower_in_scope(inner, scope)?;
                lowered.node = Node::GraphFilter {
                    condition: expression::lower(expr),
                    input: Box::new(lowered.node),
                };
                Ok(lowered)
            }
            GraphPattern::Union { left, right } => {
                let left = self.lower_in_scope(left, scope.clone())?;
                let right = self.lower_in_scope(right, scope)?;
                let mut variables = left.variables.clone();
                variables.extend(right.variables.iter().cloned());
                Ok(Lowered {
                    node: Node::GraphUnion {
                        all: true,
                        align: UnionAlign::ByVariableName,
                        left: Box::new(left.node),
                        right: Box::new(right.node),
                    },
                    variables,
                    projection: left.projection.or(right.projection),
                })
            }
            GraphPattern::Graph { name, inner } => {
                let named_scope = match name {
                    NamedNodePattern::NamedNode(value) => {
                        RdfGraphScope::NamedGraph(RdfTerm::Iri(value.as_str().into()))
                    }
                    NamedNodePattern::Variable(value) => {
                        RdfGraphScope::NamedGraphVariable(binding(value))
                    }
                };
                let mut lowered = self.lower_in_scope(inner, named_scope)?;
                if let NamedNodePattern::Variable(value) = name {
                    lowered.variables.insert(binding(value));
                }
                Ok(lowered)
            }
            GraphPattern::Extend {
                inner,
                variable,
                expression: expr,
            } => {
                let mut lowered = self.lower_in_scope(inner, scope)?;
                let alias = binding(variable);
                lowered.variables.insert(alias.clone());
                lowered.node = Node::GraphProject {
                    mode: ProjectMode::PreserveVisible,
                    items: vec![ProjectionItem {
                        alias,
                        expr: expression::lower(expr),
                    }],
                    error_policy: ProjectErrorPolicy::UnboundOnExpressionError,
                    input: Box::new(lowered.node),
                };
                Ok(lowered)
            }
            GraphPattern::Minus { left, right } => {
                let left = self.lower_in_scope(left, scope.clone())?;
                let right = self.lower_in_scope(right, scope)?;
                let shared = left
                    .variables
                    .intersection(&right.variables)
                    .cloned()
                    .collect();
                Ok(Lowered {
                    node: Node::GraphSparqlMinus {
                        compatible: MinusCompatibility::SharedVariables,
                        shared,
                        left: Box::new(left.node),
                        right: Box::new(right.node),
                    },
                    variables: left.variables,
                    projection: left.projection,
                })
            }
            GraphPattern::Values {
                variables,
                bindings,
            } => {
                let variables: Vec<_> = variables.iter().map(binding).collect();
                let rows = bindings
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|value| {
                                value
                                    .as_ref()
                                    .map(terms::ground_value)
                                    .unwrap_or(Value::Null)
                            })
                            .collect()
                    })
                    .collect();
                Ok(Lowered {
                    node: Node::GraphValues {
                        bindings: variables.clone(),
                        rows,
                        bulk: None,
                    },
                    variables: variables.into_iter().collect(),
                    projection: None,
                })
            }
            GraphPattern::OrderBy {
                inner,
                expression: keys,
            } => {
                let mut lowered = self.lower_in_scope(inner, scope)?;
                lowered.node = Node::GraphSort {
                    keys: keys
                        .iter()
                        .map(|key| match key {
                            OrderExpression::Asc(expr) => SortKey {
                                expr: expression::lower(expr),
                                dir: SortDir::Asc,
                                nulls: NullsOrder::ProviderDefined,
                            },
                            OrderExpression::Desc(expr) => SortKey {
                                expr: expression::lower(expr),
                                dir: SortDir::Desc,
                                nulls: NullsOrder::ProviderDefined,
                            },
                        })
                        .collect(),
                    input: Box::new(lowered.node),
                };
                Ok(lowered)
            }
            GraphPattern::Reduced { inner } => {
                let mut lowered = self.lower_in_scope(inner, scope)?;
                lowered.node = Node::GraphExtension {
                    name: "SparqlReduced".into(),
                    metadata: vec![],
                    inputs: vec![lowered.node],
                };
                Ok(lowered)
            }
            GraphPattern::Group {
                inner,
                variables,
                aggregates,
            } => {
                let inner = self.lower_in_scope(inner, scope)?;
                let mut fields: BTreeSet<_> = variables.iter().map(binding).collect();
                fields.extend(aggregates.iter().map(|(variable, _)| binding(variable)));
                Ok(Lowered {
                    node: Node::GraphExtension {
                        name: "SparqlGroup".into(),
                        metadata: vec![("algebra".into(), Value::String(pattern.to_string()))],
                        inputs: vec![inner.node],
                    },
                    variables: fields,
                    projection: None,
                })
            }
            GraphPattern::Service {
                name,
                inner,
                silent,
            } => {
                let inner = self.lower_in_scope(inner, RdfGraphScope::ActiveGraph)?;
                let outputs = inner.variables.iter().cloned().collect();
                Ok(Lowered {
                    node: Node::GraphService {
                        endpoint: named_term(name),
                        silent: *silent,
                        outputs,
                        input: Box::new(inner.node),
                    },
                    variables: inner.variables,
                    projection: inner.projection,
                })
            }
        }
    }

    fn lower_bgp(
        &self,
        patterns: &[TriplePattern],
        graph_scope: RdfGraphScope,
    ) -> Result<Lowered, SparqlError> {
        if let Some(ontology) = &self.ontology {
            return lower_ontology_bgp(patterns, ontology)?.ok_or_else(|| {
                SparqlError::Unsupported(
                    "triple pattern is not covered by the configured ontology mapping".into(),
                )
            });
        }
        let mut variables = BTreeSet::new();
        let mut node = Node::GraphOneRow;
        for pattern in patterns {
            let subject = term(&pattern.subject);
            let predicate = named_term(&pattern.predicate);
            let object = term(&pattern.object);
            let pattern_variables = term_variables([&subject, &predicate, &object]);
            let correlation: Vec<_> = variables
                .intersection(&pattern_variables)
                .cloned()
                .collect();
            let outputs: Vec<_> = pattern_variables.difference(&variables).cloned().collect();
            let scan = Node::GraphSparqlTriplePattern {
                dataset: self.dataset.clone(),
                graph_scope: graph_scope.clone(),
                subject,
                predicate,
                object,
                outputs: outputs.clone(),
            };
            node = Node::GraphApply {
                kind: ApplyKind::Inner,
                correlation,
                outputs,
                optional_missing: OptionalMissing::Unbound,
                left: Box::new(node),
                right: Box::new(scan),
            };
            variables.extend(pattern_variables);
        }
        Ok(Lowered {
            node,
            variables,
            projection: None,
        })
    }
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Resolves a star-shaped basic graph pattern through ontology metadata.
/// Returning `None` means the configured ontology does not cover the pattern.
fn lower_ontology_bgp(
    patterns: &[TriplePattern],
    ontology: &OntologyMapping,
) -> Result<Option<Lowered>, SparqlError> {
    let mut typed_bindings = BTreeMap::<String, ClassMapping>::new();
    let mut root = None;
    for pattern in patterns {
        let (
            TermPattern::Variable(subject),
            NamedNodePattern::NamedNode(predicate),
            TermPattern::NamedNode(class),
        ) = (&pattern.subject, &pattern.predicate, &pattern.object)
        else {
            continue;
        };
        if predicate.as_str() == RDF_TYPE {
            let Some(mapped) = ontology.class_for_iri(class.as_str()) else {
                return Ok(None);
            };
            let subject = binding(subject);
            root.get_or_insert_with(|| (subject.clone(), mapped.clone()));
            typed_bindings.insert(subject, mapped.clone());
        }
    }
    let Some((root_binding, root_class)) = root else {
        return Ok(None);
    };
    let mut binding_labels = typed_bindings
        .iter()
        .map(|(binding, class)| (binding.clone(), class.label.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut node = Node::GraphNodeScan {
        graph: "mapped".into(),
        binding: root_binding.clone(),
        labels: LabelExpr::label(root_class.label.clone()),
    };
    let mut items = Vec::new();
    let mut required_properties = Vec::new();
    let mut variables = BTreeSet::from([root_binding.clone()]);
    let mut pending = patterns
        .iter()
        .filter(|pattern| {
            !matches!(
                &pattern.predicate,
                NamedNodePattern::NamedNode(predicate) if predicate.as_str() == RDF_TYPE
            )
        })
        .collect::<Vec<_>>();
    while !pending.is_empty() {
        let mut progressed = false;
        let mut remaining = Vec::new();
        for pattern in pending {
            let (
                TermPattern::Variable(subject),
                NamedNodePattern::NamedNode(predicate),
                TermPattern::Variable(object),
            ) = (&pattern.subject, &pattern.predicate, &pattern.object)
            else {
                return Ok(None);
            };
            let subject = binding(subject);
            let object = binding(object);
            let Some(subject_label) = binding_labels.get(&subject).cloned() else {
                remaining.push(pattern);
                continue;
            };
            match ontology.predicate_for_iri(predicate.as_str()) {
                Some(PredicateMapping::Property {
                    domain_label,
                    property,
                    ..
                }) if domain_label == &subject_label => {
                    variables.insert(object.clone());
                    let property_expr = IrExpr::Property {
                        binding: subject,
                        name: property.clone(),
                        policy: PropertyMissing::Unbound,
                    };
                    required_properties.push(IrExpr::IsNotNull(Box::new(property_expr.clone())));
                    items.push(ProjectionItem {
                        alias: object,
                        expr: property_expr,
                    });
                    progressed = true;
                }
                Some(PredicateMapping::Relationship {
                    rel_type,
                    direction,
                    domain_label,
                    range_label,
                    ..
                }) if domain_label
                    .as_ref()
                    .is_none_or(|label| label == &subject_label) =>
                {
                    let target_label = range_label
                        .clone()
                        .or_else(|| binding_labels.get(&object).cloned());
                    let target_labels = target_label
                        .as_ref()
                        .map(LabelExpr::label)
                        .unwrap_or(LabelExpr::Any);
                    node = Node::GraphExpand {
                        graph: "mapped".into(),
                        source: subject,
                        target: object.clone(),
                        target_mode: TargetMode::BindNew,
                        target_labels,
                        rel_binding: None,
                        rel_types: LabelExpr::label(rel_type.clone()),
                        dir: *direction,
                        length: Length::ONE,
                        history: None,
                        path: None,
                        path_mode: PathMode::Walk,
                        match_mode: MatchMode::DifferentRelationships,
                        path_materialization: PathMaterialization::EndpointsOnly,
                        path_update: PathUpdate::None,
                        input: Box::new(node),
                    };
                    if let Some(label) = target_label {
                        binding_labels.insert(object.clone(), label);
                    }
                    variables.insert(object);
                    progressed = true;
                }
                _ => return Ok(None),
            }
        }
        if !progressed {
            return Ok(None);
        }
        pending = remaining;
    }
    // Expose configured resource identities for every materialized class,
    // never Crabgraph's internal row ids unless the mapping requests that.
    for (binding, label) in &binding_labels {
        let identity = ontology
            .class_for_label(label)
            .and_then(|class| class.identity_property.clone());
        items.push(ProjectionItem {
            alias: binding.clone(),
            expr: match identity {
                Some(name) => IrExpr::Property {
                    binding: binding.clone(),
                    name,
                    policy: PropertyMissing::Unbound,
                },
                None => IrExpr::Id(binding.clone()),
            },
        });
    }
    if let Some(condition) = required_properties
        .into_iter()
        .reduce(|lhs, rhs| IrExpr::Binary {
            op: BinaryOp::And,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    {
        node = Node::GraphFilter {
            condition,
            input: Box::new(node),
        };
    }
    Ok(Some(Lowered {
        node: Node::GraphProject {
            mode: ProjectMode::PreserveVisible,
            items,
            error_policy: ProjectErrorPolicy::UnboundOnExpressionError,
            input: Box::new(node),
        },
        variables,
        projection: None,
    }))
}

struct Lowered {
    node: Node,
    variables: BTreeSet<String>,
    projection: Option<Vec<String>>,
}

fn combine_apply(left: Lowered, right: Lowered, kind: ApplyKind) -> Result<Lowered, SparqlError> {
    let correlation: Vec<_> = left
        .variables
        .intersection(&right.variables)
        .cloned()
        .collect();
    let outputs: Vec<_> = right
        .variables
        .difference(&left.variables)
        .cloned()
        .collect();
    let mut variables = left.variables.clone();
    variables.extend(right.variables.iter().cloned());
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
        projection: left.projection.or(right.projection),
    })
}

fn wrap_dataset(mut lowered: Lowered, dataset: Option<&QueryDataset>) -> Lowered {
    if let Some(dataset) = dataset {
        lowered.node = Node::GraphExtension {
            name: "SparqlDataset".into(),
            metadata: vec![("dataset".into(), Value::String(dataset.to_string()))],
            inputs: vec![lowered.node],
        };
    }
    lowered
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
