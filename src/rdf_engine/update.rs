//! SPARQL update orchestration. WHERE regions use the normal Graph IR -> SQL
//! IR planner. Instantiated effects target only explicitly writable mappings.
use super::{RdfGraphEngine, RdfTermValue, SparqlResults};
use crate::ir::rel::rdf::IriQuadSource;
use crate::ir::rel::sql::mutation::{MappedMutation, RowCondition};
use crate::spargebra::algebra::GraphTarget;
use crate::spargebra::term::{
    BlankNode, GraphName, GraphNamePattern, GroundTermPattern, NamedNodePattern, QuadPattern,
    TermPattern,
};
use crate::spargebra::{GraphUpdateOperation, Query};
use std::collections::BTreeMap;

type Result<T> = std::result::Result<T, String>;
type Bindings = BTreeMap<String, RdfTermValue>;

fn bound(
    term: &TermPattern,
    row: &Bindings,
    blanks: &mut BTreeMap<String, String>,
) -> Option<RdfTermValue> {
    Some(match term {
        TermPattern::NamedNode(v) => RdfTermValue::iri(v.as_str()),
        TermPattern::BlankNode(v) => RdfTermValue::BlankNode(
            blanks
                .entry(v.as_str().into())
                .or_insert_with(|| BlankNode::default().as_str().to_owned())
                .clone(),
        ),
        TermPattern::Literal(v) => RdfTermValue::Literal {
            lexical: v.value().into(),
            datatype: v.datatype().as_str().into(),
            language: v.language().map(str::to_ascii_lowercase),
        },
        TermPattern::Variable(v) => row.get(v.as_str())?.clone(),
    })
}

fn instantiate(
    pattern: &QuadPattern,
    row: &Bindings,
    blanks: &mut BTreeMap<String, String>,
) -> Option<(Option<String>, [RdfTermValue; 3])> {
    let subject = bound(&pattern.subject, row, blanks)?;
    if matches!(subject, RdfTermValue::Literal { .. }) {
        return None;
    }
    let predicate = match &pattern.predicate {
        NamedNodePattern::NamedNode(v) => RdfTermValue::iri(v.as_str()),
        NamedNodePattern::Variable(v) => row.get(v.as_str())?.clone(),
    };
    if !matches!(predicate, RdfTermValue::Iri(_)) {
        return None;
    }
    let object = bound(&pattern.object, row, blanks)?;
    let graph = match &pattern.graph_name {
        GraphNamePattern::NamedNode(v) => Some(v.as_str().into()),
        GraphNamePattern::DefaultGraph => None,
        GraphNamePattern::Variable(v) => match row.get(v.as_str())? {
            RdfTermValue::Iri(v) => Some(v.clone()),
            _ => return None,
        },
    };
    Some((graph, [subject, predicate, object]))
}

fn graph_pattern(graph: GraphName) -> GraphNamePattern {
    match graph {
        GraphName::NamedNode(v) => GraphNamePattern::NamedNode(v),
        GraphName::DefaultGraph => GraphNamePattern::DefaultGraph,
    }
}

fn mapped_values(
    source: &IriQuadSource,
    graph: &Option<String>,
    triple: &[RdfTermValue; 3],
) -> Result<Vec<(String, Option<String>)>> {
    let mut values = Vec::new();
    if let Some(column) = &source.graph_column {
        values.push((column.clone(), graph.clone()));
    } else if graph.is_some() {
        return Err("Named graph has no mapped graph column".into());
    }
    let columns = [
        &source.subject_column,
        &source.predicate_column,
        &source.object_column,
    ];
    for (index, term) in triple.iter().enumerate() {
        let (value, kind, dt, lang) = match term {
            RdfTermValue::Iri(v) => (v, "IRI", None, None),
            RdfTermValue::BlankNode(v) => (v, "BLANK", None, None),
            RdfTermValue::Literal {
                lexical,
                datatype,
                language,
            } => (lexical, "LITERAL", Some(datatype.clone()), language.clone()),
        };
        if let Some(terms) = &source.typed_terms {
            let columns = &terms[index];
            values.push((columns.value.clone(), Some(value.clone())));
            values.push((columns.kind.clone(), Some(kind.into())));
            if let Some(column) = &columns.datatype {
                values.push((column.clone(), dt));
            } else if dt.is_some() {
                return Err("Literal datatype has no mapped column".into());
            }
            if let Some(column) = &columns.language {
                values.push((column.clone(), lang));
            } else if lang.is_some() {
                return Err("Literal language has no mapped column".into());
            }
        } else {
            if kind != "IRI" {
                return Err("IRI-only mapping cannot store literal or blank terms".into());
            }
            values.push((columns[index].clone(), Some(value.clone())));
        }
    }
    Ok(values)
}

impl RdfGraphEngine {
    /// Execute an atomic update request against existing writable mappings.
    /// No table or column is created implicitly.
    pub async fn update(&mut self, source: &str, base: Option<&str>) -> Result<()> {
        let update =
            crate::language::sparql::parse_update(source, base).map_err(|e| e.to_string())?;
        self.executor()?
            .connection()
            .map_err(|e| e.to_string())?
            .execute_batch("BEGIN TRANSACTION")
            .map_err(|e| e.to_string())?;
        let outcome = self.apply_operations(update).await;
        let mut executor = self.executor()?;
        let connection = executor.connection().map_err(|e| e.to_string())?;
        match outcome {
            Ok(()) => connection
                .execute_batch("COMMIT")
                .map_err(|e| e.to_string()),
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    fn graph_effect(&self, graph: String, insert: bool) -> Result<MappedMutation> {
        let (table, column) = self
            .mapping
            .writable_graph_table(&self.dataset)
            .ok_or("Named graph lifecycle requires an explicitly writable graph registry")?;
        Ok(if insert {
            MappedMutation::InsertAbsent {
                table: table.clone(),
                values: vec![(column.clone(), Some(graph))],
            }
        } else {
            MappedMutation::Delete {
                table: table.clone(),
                conditions: vec![RowCondition::Equal(column.clone(), Some(graph))],
            }
        })
    }

    fn quad_effects(
        &self,
        graph: Option<String>,
        triple: [RdfTermValue; 3],
        insert: bool,
    ) -> Result<Vec<MappedMutation>> {
        let RdfTermValue::Iri(predicate) = &triple[1] else {
            return Err("Predicate must be an IRI".into());
        };
        let targets = self
            .mapping
            .dataset_sources(&self.dataset)
            .iter()
            .filter(|source| {
                (graph.is_none() || source.graph_column.is_some())
                    && source.predicate_iri.as_ref().is_none_or(|p| p == predicate)
            })
            .collect::<Vec<_>>();
        if targets.len() != 1 {
            return Err(format!(
                "Update requires exactly one mapped target for predicate {predicate}; found {}",
                targets.len()
            ));
        }
        let target = targets[0];
        if !target.writable {
            return Err(format!("Mapped table {} is read-only", target.table));
        }
        let values = mapped_values(target, &graph, &triple)?;
        let mut effects = Vec::new();
        if insert {
            if let Some(graph) = graph {
                effects.push(self.graph_effect(graph, true)?);
            }
            effects.push(MappedMutation::InsertAbsent {
                table: target.table.clone(),
                values,
            });
        } else {
            effects.push(MappedMutation::Delete {
                table: target.table.clone(),
                conditions: values
                    .into_iter()
                    .map(|(k, v)| RowCondition::Equal(k, v))
                    .collect(),
            });
        }
        Ok(effects)
    }

    async fn graph_names(&mut self) -> Result<Vec<String>> {
        match self.query("SELECT ?g WHERE { GRAPH ?g {} }").await? {
            SparqlResults::Solutions { rows, .. } => Ok(rows
                .into_iter()
                .filter_map(|row| match row.into_iter().next().flatten() {
                    Some(RdfTermValue::Iri(v)) => Some(v),
                    _ => None,
                })
                .collect()),
            _ => Err("Graph enumeration did not return solutions".into()),
        }
    }

    async fn apply_operations(&mut self, update: crate::spargebra::Update) -> Result<()> {
        for operation in update.operations {
            let mut effects = Vec::new();
            match operation {
                GraphUpdateOperation::InsertData { data } => {
                    let mut blanks = BTreeMap::new();
                    for quad in data {
                        let pattern = QuadPattern {
                            subject: quad.subject.into(),
                            predicate: quad.predicate.into(),
                            object: quad.object.into(),
                            graph_name: graph_pattern(quad.graph_name),
                        };
                        if let Some((graph, triple)) =
                            instantiate(&pattern, &Bindings::new(), &mut blanks)
                        {
                            effects.extend(self.quad_effects(graph, triple, true)?);
                        }
                    }
                }
                GraphUpdateOperation::DeleteData { data } => {
                    for quad in data {
                        let pattern = QuadPattern {
                            subject: quad.subject.into(),
                            predicate: quad.predicate.into(),
                            object: TermPattern::from(GroundTermPattern::from(quad.object)),
                            graph_name: graph_pattern(quad.graph_name),
                        };
                        if let Some((graph, triple)) =
                            instantiate(&pattern, &Bindings::new(), &mut BTreeMap::new())
                        {
                            effects.extend(self.quad_effects(graph, triple, false)?);
                        }
                    }
                }
                GraphUpdateOperation::DeleteInsert {
                    delete,
                    insert,
                    using,
                    pattern,
                } => {
                    // Update WHERE has no outer SELECT projection. Export all
                    // in-scope bindings, including those outside a subselect.
                    let mut variables = std::collections::BTreeSet::new();
                    pattern.on_in_scope_variable(|variable| { variables.insert(variable.clone()); });
                    let query = Query::Select {
                        dataset: using,
                        pattern: crate::spargebra::algebra::GraphPattern::Project {
                            inner: pattern,
                            variables: variables.into_iter().collect(),
                        },
                        base_iri: update.base_iri.clone(),
                    };
                    let SparqlResults::Solutions { variables, rows } =
                        super::decode_results(&self.sparql_parsed(&query).await?)?
                    else {
                        return Err("Update WHERE must return solutions".into());
                    };
                    let mut inserts = Vec::new();
                    for row in rows {
                        let row = variables
                            .iter()
                            .zip(row)
                            .filter_map(|(key, value)| {
                                value.map(|value| (key.trim_start_matches('?').to_string(), value))
                            })
                            .collect::<Bindings>();
                        let mut blanks = BTreeMap::new();
                        for pattern in &delete {
                            let pattern = QuadPattern {
                                subject: pattern.subject.clone().into(),
                                predicate: pattern.predicate.clone(),
                                object: pattern.object.clone().into(),
                                graph_name: pattern.graph_name.clone(),
                            };
                            if let Some((graph, triple)) = instantiate(&pattern, &row, &mut blanks)
                            {
                                effects.extend(self.quad_effects(graph, triple, false)?);
                            }
                        }
                        for pattern in &insert {
                            if let Some((graph, triple)) = instantiate(pattern, &row, &mut blanks) {
                                inserts.extend(self.quad_effects(graph, triple, true)?);
                            }
                        }
                    }
                    effects.extend(inserts);
                }
                GraphUpdateOperation::Create { silent, graph } => {
                    if self
                        .graph_names()
                        .await?
                        .iter()
                        .any(|name| name == graph.as_str())
                    {
                        if silent {
                            continue;
                        } else {
                            return Err(format!("Graph {graph} already exists"));
                        }
                    }
                    effects.push(self.graph_effect(graph.as_str().into(), true)?);
                }
                GraphUpdateOperation::Clear { silent, graph } => {
                    effects.extend(self.clear_graph(&graph, silent, false).await?)
                }
                GraphUpdateOperation::Drop { silent, graph } => {
                    effects.extend(self.clear_graph(&graph, silent, true).await?)
                }
                GraphUpdateOperation::Load { silent, .. } => {
                    if !silent {
                        return Err("LOAD requires an RDF source resolver".into());
                    }
                }
            }
            for effect in effects {
                effect.execute(&mut *self.executor()?)?;
            }
        }
        Ok(())
    }

    async fn clear_graph(
        &mut self,
        graph: &GraphTarget,
        silent: bool,
        drop: bool,
    ) -> Result<Vec<MappedMutation>> {
        let names = self.graph_names().await?;
        if let GraphTarget::NamedNode(name) = graph {
            if !names.iter().any(|v| v == name.as_str()) {
                return if silent {
                    Ok(Vec::new())
                } else {
                    Err(format!("Graph {name} does not exist"))
                };
            }
        }
        let mut effects = Vec::new();
        for source in self.mapping.dataset_sources(&self.dataset) {
            let mut conditions = Vec::new();
            match (&source.graph_column, graph) {
                (Some(column), GraphTarget::NamedNode(name)) => conditions.push(
                    RowCondition::Equal(column.clone(), Some(name.as_str().into())),
                ),
                (Some(column), GraphTarget::DefaultGraph) => {
                    conditions.push(RowCondition::Equal(column.clone(), None))
                }
                (Some(column), GraphTarget::NamedGraphs) => {
                    conditions.push(RowCondition::NotNull(column.clone()))
                }
                (None, GraphTarget::NamedNode(_) | GraphTarget::NamedGraphs) => continue,
                _ => (),
            }
            if !source.writable {
                return Err(format!("Mapped table {} is read-only", source.table));
            }
            if let Some(predicate) = &source.predicate_iri {
                let column = source
                    .typed_terms
                    .as_ref()
                    .map(|terms| &terms[1].value)
                    .unwrap_or(&source.predicate_column);
                conditions.push(RowCondition::Equal(column.clone(), Some(predicate.clone())));
            }
            effects.push(MappedMutation::Delete {
                table: source.table.clone(),
                conditions,
            });
        }
        if drop {
            for name in names {
                if matches!(graph, GraphTarget::AllGraphs | GraphTarget::NamedGraphs)
                    || matches!(graph,GraphTarget::NamedNode(v) if v.as_str() == name)
                {
                    effects.push(self.graph_effect(name, false)?);
                }
            }
        }
        Ok(effects)
    }
}
