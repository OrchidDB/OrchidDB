//! Ontology vocabulary mapped onto the property-graph schema users already
//! expose to Crabgraph. This layer contains no storage or table assumptions.

use std::collections::BTreeMap;

use crate::ir::plan::Direction;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OntologyMapping {
    classes: BTreeMap<String, ClassMapping>,
    predicates: BTreeMap<String, PredicateMapping>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassMapping {
    pub iri: String,
    pub label: String,
    /// Graph property used as the externally visible SPARQL subject.
    /// The schema mapping resolves it to a user-owned column.
    pub identity_property: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PredicateMapping {
    Property {
        iri: String,
        domain_label: String,
        property: String,
    },
    Relationship {
        iri: String,
        rel_type: String,
        direction: Direction,
        domain_label: Option<String>,
        range_label: Option<String>,
    },
}

impl OntologyMapping {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn class(mut self, iri: impl Into<String>, label: impl Into<String>) -> Self {
        let iri = iri.into();
        self.classes.insert(
            iri.clone(),
            ClassMapping {
                iri,
                label: label.into(),
                identity_property: None,
            },
        );
        self
    }

    pub fn class_with_identity(
        mut self,
        iri: impl Into<String>,
        label: impl Into<String>,
        identity_property: impl Into<String>,
    ) -> Self {
        let iri = iri.into();
        self.classes.insert(
            iri.clone(),
            ClassMapping {
                iri,
                label: label.into(),
                identity_property: Some(identity_property.into()),
            },
        );
        self
    }

    pub fn property(
        mut self,
        iri: impl Into<String>,
        domain_label: impl Into<String>,
        property: impl Into<String>,
    ) -> Self {
        let iri = iri.into();
        self.predicates.insert(
            iri.clone(),
            PredicateMapping::Property {
                iri,
                domain_label: domain_label.into(),
                property: property.into(),
            },
        );
        self
    }

    pub fn relationship(
        mut self,
        iri: impl Into<String>,
        rel_type: impl Into<String>,
        direction: Direction,
    ) -> Self {
        let iri = iri.into();
        self.predicates.insert(
            iri.clone(),
            PredicateMapping::Relationship {
                iri,
                rel_type: rel_type.into(),
                direction,
                domain_label: None,
                range_label: None,
            },
        );
        self
    }

    /// Map a predicate to a directed property-graph relationship and make
    /// both endpoint labels explicit. This resolves vocabulary ambiguity by
    /// configuration and lets the relational layer select the exact user
    /// views on both sides of the expansion.
    pub fn relationship_between(
        mut self,
        iri: impl Into<String>,
        rel_type: impl Into<String>,
        direction: Direction,
        domain_label: impl Into<String>,
        range_label: impl Into<String>,
    ) -> Self {
        let iri = iri.into();
        self.predicates.insert(
            iri.clone(),
            PredicateMapping::Relationship {
                iri,
                rel_type: rel_type.into(),
                direction,
                domain_label: Some(domain_label.into()),
                range_label: Some(range_label.into()),
            },
        );
        self
    }

    pub fn class_for_iri(&self, iri: &str) -> Option<&ClassMapping> {
        self.classes.get(iri)
    }

    pub fn class_for_label(&self, label: &str) -> Option<&ClassMapping> {
        self.classes.values().find(|class| class.label == label)
    }

    pub fn predicate_for_iri(&self, iri: &str) -> Option<&PredicateMapping> {
        self.predicates.get(iri)
    }
}
