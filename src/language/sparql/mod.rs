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
    query_dataset: Option<QueryDataset>,
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
                let lowered = self.with_query_dataset(dataset.as_ref())?.lower(pattern)?;
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

    fn lower(&self, pattern: &GraphPattern) -> Result<Lowered, SparqlError> {
        let scope = match &self.query_dataset {
            Some(dataset) => RdfGraphScope::DatasetDefaultGraph(
                dataset
                    .default
                    .iter()
                    .map(|iri| iri.as_str().into())
                    .collect(),
            ),
            None => RdfGraphScope::DefaultGraph,
        };
        self.lower_in_scope(pattern, scope)
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
                    identity_variables: BTreeSet::new(),
                    projection: None,
                })
            }
            GraphPattern::Project { inner, variables } => {
                let mut lowered = self.lower_in_scope(inner, scope)?;
                let fields: Vec<String> = variables.iter().map(binding).collect();
                // Projection is an algebra operation, not just return metadata:
                // DISTINCT and subquery joins must not see hidden bindings.
                lowered.node = Node::GraphProject {
                    mode: ProjectMode::ReplaceScope,
                    items: fields
                        .iter()
                        .map(|name| ProjectionItem {
                            alias: name.clone(),
                            expr: if lowered.variables.contains(name) {
                                IrExpr::Binding(name.clone())
                            } else {
                                IrExpr::Lit(crate::ir::expr::Lit::Null)
                            },
                        })
                        .collect(),
                    error_policy: ProjectErrorPolicy::UnboundOnExpressionError,
                    input: Box::new(lowered.node),
                };
                lowered.variables = fields.iter().cloned().collect();
                lowered.identity_variables.clear();
                lowered.projection = Some(fields);
                Ok(lowered)
            }
            GraphPattern::Distinct { inner } => {
                let mut lowered = self.lower_in_scope(inner, scope)?;
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
                let mut lowered = self.lower_in_scope(inner, scope)?;
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
                if left.identity_variables != right.identity_variables
                    && (!left.identity_variables.is_empty() || !right.identity_variables.is_empty())
                {
                    return Err(SparqlError::Unsupported(
                        "SPARQL UNION branches do not preserve the same RDF term identity bindings".into(),
                    ));
                }
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
                    identity_variables: left.identity_variables.intersection(&right.identity_variables).cloned().collect(),
                    projection: left.projection.or(right.projection),
                })
            }
            GraphPattern::Graph { name, inner } => {
                let named_scope = match (name, &self.query_dataset) {
                    (NamedNodePattern::NamedNode(value), Some(dataset)) => {
                        RdfGraphScope::DatasetNamedGraph {
                            iri: value.as_str().into(),
                            allowed: dataset
                                .named
                                .as_ref()
                                .into_iter()
                                .flatten()
                                .map(|iri| iri.as_str().into())
                                .collect(),
                        }
                    }
                    (NamedNodePattern::Variable(value), Some(dataset)) => {
                        RdfGraphScope::DatasetNamedGraphVariable {
                            variable: binding(value),
                            allowed: dataset
                                .named
                                .as_ref()
                                .into_iter()
                                .flatten()
                                .map(|iri| iri.as_str().into())
                                .collect(),
                        }
                    }
                    (NamedNodePattern::NamedNode(value), None) => {
                        RdfGraphScope::NamedGraph(RdfTerm::Iri(value.as_str().into()))
                    }
                    (NamedNodePattern::Variable(value), None) => {
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
                let shared: Vec<_> = left
                    .variables
                    .intersection(&right.variables)
                    .cloned()
                    .collect();
                if shared.iter().any(|variable| {
                    left.identity_variables.contains(variable)
                        || right.identity_variables.contains(variable)
                }) {
                    return Err(SparqlError::Unsupported(
                        "SPARQL MINUS over RDF terms requires identity-aware compatibility".into(),
                    ));
                }
                Ok(Lowered {
                    node: Node::GraphSparqlMinus {
                        compatible: MinusCompatibility::SharedVariables,
                        shared,
                        left: Box::new(left.node),
                        right: Box::new(right.node),
                    },
                    variables: left.variables,
                    identity_variables: left.identity_variables,
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
                    identity_variables: BTreeSet::new(),
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
                    identity_variables: BTreeSet::new(),
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
                    identity_variables: BTreeSet::new(),
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
            if graph_scope != RdfGraphScope::DefaultGraph {
                return Err(SparqlError::Unsupported(
                    "ontology mappings do not define named or active RDF graphs".into(),
                ));
            }
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
            let mut correlation: Vec<_> = variables
                .intersection(&pattern_variables)
                .cloned()
                .collect();
            let mut outputs: Vec<_> = pattern_variables.difference(&variables).cloned().collect();
            let identity_correlation = correlation
                .iter()
                .flat_map(|variable| crate::ir::rel::rdf::binding_identity_columns(variable))
                .collect::<Vec<_>>();
            correlation.extend(identity_correlation);
            for variable in pattern_variables.difference(&variables) {
                outputs.extend(crate::ir::rel::rdf::binding_identity_columns(variable));
            }
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
            identity_variables: variables.clone(),
            variables,
            projection: None,
        })
    }
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Resolves a basic graph pattern through ontology metadata.
/// Returning `None` means the configured ontology does not cover the pattern.
fn lower_ontology_bgp(
    patterns: &[TriplePattern],
    ontology: &OntologyMapping,
) -> Result<Option<Lowered>, SparqlError> {
    if patterns.is_empty() {
        return Ok(Some(Lowered {
            node: Node::GraphOneRow,
            variables: BTreeSet::new(),
            identity_variables: BTreeSet::new(),
            projection: None,
        }));
    }
    let mut typed_bindings = BTreeMap::<String, ClassMapping>::new();
    let mut root = None;
    for pattern in patterns {
        if !matches!(
            &pattern.predicate,
            NamedNodePattern::NamedNode(predicate) if predicate.as_str() == RDF_TYPE
        ) {
            continue;
        }
        let (TermPattern::Variable(subject), TermPattern::NamedNode(class)) =
            (&pattern.subject, &pattern.object)
        else {
            return Ok(None);
        };
        let Some(mapped) = ontology.class_for_iri(class.as_str()) else {
            return Ok(None);
        };
        let subject = binding(subject);
        if let Some(previous) = typed_bindings.get(&subject) {
            if previous.iri != mapped.iri {
                return Err(SparqlError::Unsupported(format!(
                    "multiple mapped rdf:type classes for {subject} require class intersection"
                )));
            }
        }
        root.get_or_insert_with(|| (subject.clone(), mapped.clone()));
        typed_bindings.insert(subject, mapped.clone());
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
    let mut conditions = Vec::new();
    let mut scalar_bindings = BTreeMap::<String, IrExpr>::new();
    let mut materialized_nodes = BTreeSet::from([root_binding.clone()]);
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
    while !pending.is_empty()
        || typed_bindings
            .keys()
            .any(|binding| !materialized_nodes.contains(binding))
    {
        let mut progressed = false;
        let mut remaining = Vec::new();
        for pattern in pending {
            let (TermPattern::Variable(subject), NamedNodePattern::NamedNode(predicate)) =
                (&pattern.subject, &pattern.predicate)
            else {
                return Ok(None);
            };
            let subject = binding(subject);
            if !materialized_nodes.contains(&subject) {
                remaining.push(pattern);
                continue;
            }
            let Some(subject_label) = binding_labels.get(&subject).cloned() else {
                return Ok(None);
            };
            match ontology.predicate_for_iri(predicate.as_str()) {
                Some(PredicateMapping::Property {
                    domain_label,
                    property,
                    ..
                }) if domain_label == &subject_label => {
                    let property_expr = IrExpr::Property {
                        binding: subject,
                        name: property.clone(),
                        policy: PropertyMissing::Unbound,
                    };
                    match &pattern.object {
                        TermPattern::Variable(object) => {
                            let object = binding(object);
                            if binding_labels.contains_key(&object)
                                || materialized_nodes.contains(&object)
                            {
                                return Ok(None);
                            }
                            if let Some(previous) = scalar_bindings.get(&object) {
                                conditions.push(eq(property_expr, previous.clone()));
                            } else {
                                conditions.push(IrExpr::IsNotNull(Box::new(property_expr.clone())));
                                scalar_bindings.insert(object.clone(), property_expr);
                                variables.insert(object);
                            }
                        }
                        TermPattern::NamedNode(value) => conditions.push(eq(
                            property_expr,
                            IrExpr::Lit(crate::ir::expr::Lit::String(value.as_str().into())),
                        )),
                        TermPattern::Literal(value) => {
                            let RdfTerm::Literal(literal) = terms::literal(value) else {
                                return Ok(None);
                            };
                            conditions.push(eq(property_expr, IrExpr::Lit(literal)));
                        }
                        TermPattern::BlankNode(_) => return Ok(None),
                    }
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
                    let TermPattern::Variable(object) = &pattern.object else {
                        return Ok(None);
                    };
                    let object = binding(object);
                    if scalar_bindings.contains_key(&object) {
                        return Ok(None);
                    }
                    let target_label = binding_labels
                        .get(&object)
                        .cloned()
                        .or_else(|| range_label.clone());
                    let target_labels = target_label
                        .as_ref()
                        .map(LabelExpr::label)
                        .unwrap_or(LabelExpr::Any);
                    let target_mode = if materialized_nodes.contains(&object) {
                        TargetMode::Existing
                    } else {
                        TargetMode::BindNew
                    };
                    node = Node::GraphExpand {
                        graph: "mapped".into(),
                        source: subject,
                        target: object.clone(),
                        target_mode,
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
                    materialized_nodes.insert(object.clone());
                    variables.insert(object);
                    progressed = true;
                }
                _ => return Ok(None),
            }
        }
        if !progressed {
            // A typed root with no edge from the current component is an
            // independent BGP factor. Scanning it also makes its properties
            // and outgoing relationships available in the next pass.
            let Some((binding, class)) = typed_bindings
                .iter()
                .find(|(binding, _)| !materialized_nodes.contains(*binding))
            else {
                return Ok(None);
            };
            node = Node::GraphApply {
                kind: ApplyKind::Inner,
                correlation: vec![],
                outputs: vec![binding.clone()],
                optional_missing: OptionalMissing::Unbound,
                left: Box::new(node),
                right: Box::new(Node::GraphNodeScan {
                    graph: "mapped".into(),
                    binding: binding.clone(),
                    labels: LabelExpr::label(class.label.clone()),
                }),
            };
            materialized_nodes.insert(binding.clone());
            variables.insert(binding.clone());
        }
        pending = remaining;
    }
    for (alias, expr) in scalar_bindings {
        items.push(ProjectionItem { alias, expr });
    }
    // Expose configured resource identities for every materialized class,
    // never Crabgraph's internal row ids unless the mapping requests that.
    for (binding, label) in &binding_labels {
        let identity = typed_bindings
            .get(binding)
            .or_else(|| ontology.class_for_label(label))
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
    if let Some(condition) = conditions.into_iter().reduce(|lhs, rhs| IrExpr::Binary {
        op: BinaryOp::And,
        lhs: Box::new(lhs),
        rhs: Box::new(rhs),
    }) {
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
        identity_variables: BTreeSet::new(),
        projection: None,
    }))
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
        if left.identity_variables.contains(variable) != right.identity_variables.contains(variable) {
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
