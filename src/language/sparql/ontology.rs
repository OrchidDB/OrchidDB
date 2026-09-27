//! Ontology vocabulary mapped onto the property-graph schema users already
//! expose to OrchidDB. This layer contains no storage or table assumptions.

use std::collections::BTreeMap;

use crate::ir::plan::Direction;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OntologyMapping {
    classes: BTreeMap<String, ClassMapping>,
    predicates: BTreeMap<String, PredicateMapping>,
    declarations: Vec<PredicateMapping>,
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
                iri: iri.clone(),
                domain_label: domain_label.into(),
                property: property.into(),
            },
        );
        self.declarations.push(self.predicates[&iri].clone());
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
                iri: iri.clone(),
                rel_type: rel_type.into(),
                direction,
                domain_label: None,
                range_label: None,
            },
        );
        self.declarations.push(self.predicates[&iri].clone());
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
                iri: iri.clone(),
                rel_type: rel_type.into(),
                direction,
                domain_label: Some(domain_label.into()),
                range_label: Some(range_label.into()),
            },
        );
        self.declarations.push(self.predicates[&iri].clone());
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

impl OntologyMapping {
    /// Translate vocabulary into ordinary relational RDF rules. All declarations
    /// are retained, including predicates shared by multiple classes.
    pub fn apply_to(
        &self,
        mapping: &mut crate::ir::rel::mapping::GraphMapping,
        dataset: &str,
    ) -> Result<(), String> {
        use crate::ir::rel::{
            mapping::{MappedSource, NodeMapping},
            rdf_mapping::{RDF_TYPE, RdfMapping, RdfTermMapping as T},
        };
        let quote = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
        let source = |s: &MappedSource| match s {
            MappedSource::Table(t) => format!("SELECT * FROM {t}"),
            MappedSource::Query(q) => q.clone(),
        };
        let identity = |node: &NodeMapping, alias: Option<&str>| -> Result<T, String> {
            let column = |c: &str| alias.map(|a| format!("{a}{c}")).unwrap_or_else(|| c.into());
            if let Some(property) = self
                .class_for_label(&node.label)
                .and_then(|c| c.identity_property.as_ref())
            {
                let physical = node.properties.get(property).ok_or_else(|| {
                    format!("Missing identity property {}.{property}", node.label)
                })?;
                Ok(T::iri(column(physical)))
            } else {
                Ok(T::template(
                    format!("urn:orchiddb:{}:", node.label),
                    node.id_column.columns().iter().map(|c| column(c)),
                ))
            }
        };
        let mut node_sources = BTreeMap::new();
        for label in mapping.labels() {
            let node = mapping.node(&label).unwrap().clone();
            let table = match &node.source {
                MappedSource::Table(t) => t.clone(),
                MappedSource::Query(q) => {
                    let name = format!("__rdf_node_{}", node_sources.len());
                    mapping.register_view(&name, q).map_err(|e| e.to_string())?;
                    name
                }
            };
            node_sources.insert(label, (node, table));
        }
        for class in self.classes.values() {
            let (node, table) = node_sources
                .get(&class.label)
                .ok_or_else(|| format!("Unknown ontology label {}", class.label))?;
            mapping.map_rdf(
                RdfMapping::table(
                    table,
                    identity(node, None)?,
                    RDF_TYPE,
                    T::constant(&class.iri),
                )
                .dataset(dataset),
            );
        }
        for (index, predicate) in self.declarations.iter().enumerate() {
            match predicate {
                PredicateMapping::Property {
                    iri,
                    domain_label,
                    property,
                } => {
                    let (node, table) = node_sources
                        .get(domain_label)
                        .ok_or_else(|| format!("Unknown ontology label {domain_label}"))?;
                    let column = node.properties.get(property).ok_or_else(|| {
                        format!("Unknown mapped property {domain_label}.{property}")
                    })?;
                    mapping.map_rdf(
                        RdfMapping::table(table, identity(node, None)?, iri, T::literal(column))
                            .dataset(dataset),
                    );
                }
                PredicateMapping::Relationship {
                    iri,
                    rel_type,
                    direction,
                    ..
                } => {
                    let edge = mapping
                        .edge(rel_type)
                        .ok_or_else(|| format!("Unknown relationship {rel_type}"))?
                        .clone();
                    let (src, _) = node_sources
                        .get(&edge.src_label)
                        .ok_or("Missing source node mapping")?;
                    let (dst, _) = node_sources
                        .get(&edge.dst_label)
                        .ok_or("Missing target node mapping")?;
                    if src.id_column.len() != edge.src_column.len()
                        || dst.id_column.len() != edge.dst_column.len()
                    {
                        return Err("Relationship key arity mismatch".into());
                    }
                    let mut projected = Vec::new();
                    for (alias, node) in [("s", src), ("d", dst)] {
                        for column in identity(node, None)?.columns() {
                            projected.push(format!(
                                "{alias}.{} AS {}",
                                quote(column),
                                quote(&format!("{alias}_{column}"))
                            ));
                        }
                    }
                    let join = |node: &NodeMapping,
                                columns: &crate::ir::rel::mapping::KeyColumns,
                                alias: &str| {
                        node.id_column
                            .columns()
                            .iter()
                            .zip(columns.columns())
                            .map(|(a, b)| format!("{alias}.{} = e.{}", quote(a), quote(b)))
                            .collect::<Vec<_>>()
                            .join(" AND ")
                    };
                    let sql = format!(
                        "SELECT {} FROM ({}) e JOIN ({}) s ON {} JOIN ({}) d ON {}",
                        projected.join(","),
                        source(&edge.source),
                        source(&src.source),
                        join(src, &edge.src_column, "s"),
                        source(&dst.source),
                        join(dst, &edge.dst_column, "d")
                    );
                    let table = format!("__rdf_relationship_{index}");
                    mapping
                        .register_view(&table, &sql)
                        .map_err(|e| e.to_string())?;
                    let (subject, object) = match direction {
                        Direction::In => (identity(dst, Some("d_"))?, identity(src, Some("s_"))?),
                        _ => (identity(src, Some("s_"))?, identity(dst, Some("d_"))?),
                    };
                    mapping.map_rdf(
                        RdfMapping::table(&table, subject.clone(), iri, object.clone())
                            .dataset(dataset),
                    );
                    if *direction == Direction::Both {
                        mapping.map_rdf(
                            RdfMapping::table(table, object, iri, subject).dataset(dataset),
                        );
                    }
                }
            }
        }
        Ok(())
    }
}
