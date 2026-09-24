//! SPARQL graph algebra lowering within an active RDF graph scope.

use super::{
    ApplyKind, BTreeSet, DistinctBulk, DistinctMode, Expression, GraphPattern, IrExpr, JoinKind,
    Lowered, MinusCompatibility, NamedNodePattern, Node, NullsOrder, OptionalMissing,
    OrderExpression, PathMaterialization, ProjectErrorPolicy, ProjectMode, ProjectionItem,
    RdfGraphScope, RdfTerm, Slice, SortDir, SortKey, SparqlError, SparqlPlanner, UnionAlign, Value,
    binding, combine_apply, expression, join_typed, named_term, path, split_conjuncts, term,
    term_variables, terms,
};
impl SparqlPlanner {
    pub(super) fn lower(&self, pattern: &GraphPattern) -> Result<Lowered, SparqlError> {
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

    pub(super) fn lower_in_scope(
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
            GraphPattern::Join { left, right } if self.typed() => {
                // SPARQL joins are evaluated bottom-up: neither side sees the
                // other's bindings. Compatibility joins are an uncorrelated
                // join, not a correlated apply.
                let left = self.lower_in_scope(left, scope.clone())?;
                let right = self.lower_in_scope(right, scope)?;
                Ok(join_typed(left, right, JoinKind::Inner, None))
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
            } if self.typed() => {
                // The OPTIONAL filter belongs to the left join itself: it is
                // evaluated over each merged candidate solution and may read
                // bindings from both sides.
                let left = self.lower_in_scope(left, scope.clone())?;
                let right = self.lower_in_scope(right, scope)?;
                Ok(join_typed(
                    left,
                    right,
                    JoinKind::LeftOuter,
                    optional_expr.as_ref().map(expression::lower_typed),
                ))
            }
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
            GraphPattern::Filter { expr, inner } if self.typed() => {
                let mut lowered = self.lower_in_scope(inner, scope.clone())?;
                let mut conjuncts = Vec::new();
                split_conjuncts(expr, &mut conjuncts);
                let mut plain = Vec::new();
                let mut exists = Vec::new();
                for conjunct in conjuncts {
                    match conjunct {
                        Expression::Exists(pattern) => exists.push((ApplyKind::Semi, pattern)),
                        Expression::Not(inner) => match inner.as_ref() {
                            Expression::Exists(pattern) => exists.push((ApplyKind::Anti, pattern)),
                            _ => plain.push(conjunct),
                        },
                        _ => plain.push(conjunct),
                    }
                }
                let plain = plain
                    .into_iter()
                    .map(|conjunct| {
                        self.lift_exists(conjunct, &mut lowered, &scope)
                            .map(|lifted| expression::lower_typed(&lifted))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if !plain.is_empty() {
                    lowered.node = Node::GraphFilter {
                        condition: IrExpr::and(plain),
                        input: Box::new(lowered.node),
                    };
                }
                // Top-level (NOT) EXISTS conjuncts are correlated semi/anti
                // applies: the pattern is evaluated with the current
                // solution's bindings substituted.
                for (kind, pattern) in exists {
                    let right = self.lower_in_scope(pattern, scope.clone())?;
                    let correlation = lowered
                        .variables
                        .iter()
                        .filter(|variable| right.variables.contains(*variable))
                        .cloned()
                        .collect();
                    lowered.node = Node::GraphApply {
                        kind,
                        correlation,
                        outputs: Vec::new(),
                        optional_missing: OptionalMissing::Unbound,
                        left: Box::new(lowered.node),
                        right: Box::new(right.node),
                    };
                }
                Ok(lowered)
            }
            GraphPattern::Filter { expr, inner } => {
                let mut lowered = self.lower_in_scope(inner, scope)?;
                lowered.node = Node::GraphFilter {
                    condition: expression::lower(expr),
                    input: Box::new(lowered.node),
                };
                Ok(lowered)
            }
            GraphPattern::Union { left, right } if self.typed() => {
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
                    identity_variables: BTreeSet::new(),
                    projection: None,
                })
            }
            GraphPattern::Union { left, right } => {
                let left = self.lower_in_scope(left, scope.clone())?;
                let right = self.lower_in_scope(right, scope)?;
                if left.identity_variables != right.identity_variables
                    && (!left.identity_variables.is_empty() || !right.identity_variables.is_empty())
                {
                    return Err(SparqlError::Unsupported(
                        "SPARQL UNION branches do not preserve the same RDF term identity bindings"
                            .into(),
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
                    identity_variables: left
                        .identity_variables
                        .intersection(&right.identity_variables)
                        .cloned()
                        .collect(),
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
                let mut lowered = self.lower_in_scope(inner, scope.clone())?;
                let expr = if self.typed() {
                    self.lift_exists(expr, &mut lowered, &scope)?
                } else {
                    expr.clone()
                };
                let alias = binding(variable);
                lowered.variables.insert(alias.clone());
                lowered.node = Node::GraphProject {
                    mode: ProjectMode::PreserveVisible,
                    items: vec![ProjectionItem {
                        alias,
                        expr: self.lower_expression(&expr),
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
                if !self.typed()
                    && shared.iter().any(|variable| {
                        left.identity_variables.contains(variable)
                            || right.identity_variables.contains(variable)
                    })
                {
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
            } if self.typed() => {
                // Each variable carries its full RDF term: lexical value,
                // kind, datatype, and language columns. UNDEF is unbound.
                let variables: Vec<_> = variables.iter().map(binding).collect();
                let columns = variables
                    .iter()
                    .flat_map(|variable| {
                        std::iter::once(variable.clone())
                            .chain(crate::ir::rel::rdf::binding_identity_columns(variable))
                    })
                    .collect();
                let rows = bindings
                    .iter()
                    .map(|row| {
                        row.iter()
                            .flat_map(|value| match value {
                                Some(value) => terms::ground_components(value),
                                None => [Value::Null, Value::Null, Value::Null, Value::Null],
                            })
                            .collect()
                    })
                    .collect();
                Ok(Lowered {
                    node: Node::GraphValues {
                        bindings: columns,
                        rows,
                        bulk: None,
                    },
                    variables: variables.into_iter().collect(),
                    identity_variables: BTreeSet::new(),
                    projection: None,
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
                                expr: self.lower_expression(expr),
                                dir: SortDir::Asc,
                                nulls: NullsOrder::ProviderDefined,
                            },
                            OrderExpression::Desc(expr) => SortKey {
                                expr: self.lower_expression(expr),
                                dir: SortDir::Desc,
                                nulls: NullsOrder::ProviderDefined,
                            },
                        })
                        .collect(),
                    input: Box::new(lowered.node),
                };
                Ok(lowered)
            }
            // REDUCED permits, but does not require, eliminating duplicates;
            // returning the unreduced multiset is a conforming evaluation.
            GraphPattern::Reduced { inner } => self.lower_in_scope(inner, scope),
            GraphPattern::Group {
                inner,
                variables,
                aggregates,
            } => {
                let inner = self.lower_in_scope(inner, scope)?;
                let group: Vec<_> = variables
                    .iter()
                    .map(|variable| ProjectionItem {
                        alias: binding(variable),
                        expr: IrExpr::Binding(binding(variable)),
                    })
                    .collect();
                let aggs = aggregates
                    .iter()
                    .map(|(variable, aggregate)| self.aggregate(variable, aggregate))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut fields: Vec<_> = group.iter().map(|item| item.alias.clone()).collect();
                fields.extend(aggs.iter().map(|agg| agg.alias.clone()));
                let _ = pattern;
                Ok(Lowered {
                    node: Node::GraphAggregate {
                        group,
                        aggs,
                        fields: fields.clone(),
                        input: Box::new(inner.node),
                    },
                    variables: fields.into_iter().collect(),
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
}
