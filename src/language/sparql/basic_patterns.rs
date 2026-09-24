//! Basic graph pattern planning for typed RDF and ontology datasets.

use super::ontology_lowering::lower_ontology_bgp;
use super::{
    ApplyKind, BTreeSet, JoinKind, Lowered, Node, OptionalMissing, RdfGraphScope, SparqlError,
    SparqlPlanner, TriplePattern, named_term, pattern_term, term, term_variables,
};
impl SparqlPlanner {
    pub(super) fn lower_bgp_typed(
        &self,
        patterns: &[TriplePattern],
        graph_scope: RdfGraphScope,
    ) -> Lowered {
        let mut variables = BTreeSet::new();
        let mut node: Option<Node> = None;
        for pattern in patterns {
            let subject = pattern_term(&pattern.subject);
            let predicate = named_term(&pattern.predicate);
            let object = pattern_term(&pattern.object);
            let pattern_variables = term_variables([&subject, &predicate, &object]);
            let scan = Node::GraphSparqlTriplePattern {
                dataset: self.dataset.clone(),
                graph_scope: graph_scope.clone(),
                subject,
                predicate,
                object,
                outputs: pattern_variables.iter().cloned().collect(),
            };
            node = Some(match node {
                None => scan,
                Some(previous) => Node::GraphJoin {
                    kind: JoinKind::Inner,
                    left: Box::new(previous),
                    right: Box::new(scan),
                    condition: None,
                },
            });
            variables.extend(pattern_variables);
        }
        Lowered {
            node: node.unwrap_or(Node::GraphOneRow),
            variables,
            identity_variables: BTreeSet::new(),
            projection: None,
        }
    }

    pub(super) fn lower_bgp(
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
        if self.typed() {
            return Ok(self.lower_bgp_typed(patterns, graph_scope));
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
