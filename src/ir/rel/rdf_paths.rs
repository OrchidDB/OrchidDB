//! Typed endpoint relations for SPARQL paths. Closure uses UNION recursion,
//! so cycles terminate on term pairs rather than enumerating walks.
use super::*;
use crate::ir::plan::RdfPathExpr;
use datafusion::datasource::{cte_worktable::CteWorkTable, provider_as_source};

const START: &str = "__rdf_path_start";
const END: &str = "__rdf_path_end";
const GRAPH: &str = "__rdf_path_graph";

pub(super) fn needs_substitution(node: &Node, outer: &BTreeMap<String, bool>) -> bool {
    if let Node::GraphRdfPropertyPath {
        subject, object, ..
    } = node
    {
        if [subject, object].iter().any(|term| {
            matches!(term,
            RdfTerm::Variable(var) if outer.contains_key(var))
        }) {
            return true;
        }
    }
    super::super::node_children(node)
        .into_iter()
        .any(|child| needs_substitution(child, outer))
}

impl Lowerer<'_, '_> {
    fn path_project(&self, mut plan: LogicalPlan, terms: Vec<(String, Term)>) -> RelResult<Sol> {
        // Every relation inside a correlated path is partitioned by the
        // outer row keys. Otherwise a bound row's absent endpoint could leak
        // into the zero-length domain of a different, unbound outer row.
        let keys = self
            .seeds
            .last()
            .map(|seed| seed.keys.clone())
            .unwrap_or_default();
        let present = keys
            .iter()
            .filter(|key| {
                plan.schema()
                    .fields()
                    .iter()
                    .any(|field| field.name() == *key)
            })
            .count();
        if present == 0 && !keys.is_empty() {
            let seed = self.seeds.last().expect("keys have a seed");
            let key_plan = self.project(seed.plan.clone(), keys.iter().map(col_exact).collect())?;
            plan = LogicalPlanBuilder::from(plan)
                .cross_join(key_plan)?
                .build()?;
        } else if present != keys.len() {
            return unsupported("partially preserved path correlation keys");
        }
        let vars = terms.iter().map(|(name, _)| (name.clone(), true)).collect();
        // Recursive UNION types come from its seed in DuckDB. Preserve SQL
        // casts even for NULL metadata; an untyped NULL otherwise becomes
        // INTEGER and fails when a later step reaches a typed literal.
        let mut columns: Vec<Expr> = terms
            .into_iter()
            .flat_map(|(name, term)| {
                Term {
                    value: cast(term.value, DataType::Utf8),
                    kind: cast(term.kind, DataType::Utf8),
                    dt: cast(term.dt, DataType::Utf8),
                    lang: cast(term.lang, DataType::Utf8),
                }
                .aliased(&name)
            })
            .collect();
        columns.extend(keys.iter().map(col_exact));
        Ok(Sol {
            plan: LogicalPlanBuilder::from(plan).project(columns)?.build()?,
            vars,
            keys,
            ord: None,
        })
    }

    fn path_rename(&self, sol: Sol, names: &[(&str, &str)]) -> RelResult<Sol> {
        let terms = sol
            .vars
            .keys()
            .map(|name| {
                let target = names
                    .iter()
                    .find(|(old, _)| *old == name)
                    .map_or(name.as_str(), |(_, new)| *new);
                (target.to_string(), Term::columns(name))
            })
            .collect();
        self.path_project(sol.plan, terms)
    }

    fn path_compose(&mut self, left: Sol, right: Sol) -> RelResult<Sol> {
        let middle = self.fresh("path_middle");
        let left = self.path_rename(left, &[(END, &middle)])?;
        let right = self.path_rename(right, &[(START, &middle)])?;
        let joined = self.join(left, right, false)?;
        let terms = joined
            .vars
            .keys()
            .filter(|name| *name != &middle)
            .map(|name| (name.clone(), Term::columns(name)))
            .collect();
        self.path_project(joined.plan, terms)
    }

    fn path_edges(&mut self, source: &QuadSource, predicate: Option<Expr>) -> RelResult<Sol> {
        let role = |i: usize| Term {
            value: col_exact(&source.names[i]),
            kind: col_exact(&source.identity[i][0]),
            dt: col_exact(&source.identity[i][1]),
            lang: col_exact(&source.identity[i][2]),
        };
        let mut conditions: Vec<Expr> = source.names[1..]
            .iter()
            .map(|name| col_exact(name).is_not_null())
            .collect();
        conditions.extend(predicate);
        let plan = self.filter_plan(source.plan.clone(), and_all(conditions))?;
        self.path_project(
            plan,
            vec![
                (START.into(), role(1)),
                (END.into(), role(3)),
                // NULL denotes the default graph. Use a non-null internal key so
                // ordinary solution compatibility also partitions default paths.
                (
                    GRAPH.into(),
                    Term::iri(duck_str(
                        "coalesce",
                        vec![col_exact(&source.names[0]), s("")],
                    )),
                ),
            ],
        )
    }

    fn path_identity(
        &mut self,
        source: &QuadSource,
        constants: &[Term],
        named: bool,
    ) -> RelResult<Sol> {
        let edges = self.path_edges(source, None)?;
        let mut branches = Vec::new();
        for endpoint in [START, END] {
            branches.push(self.path_project(
                edges.plan.clone(),
                vec![
                    (START.into(), Term::columns(endpoint)),
                    (END.into(), Term::columns(endpoint)),
                    (GRAPH.into(), Term::columns(GRAPH)),
                ],
            )?);
        }
        for term in constants {
            let (plan, graph) = if named {
                (edges.plan.clone(), Term::columns(GRAPH))
            } else {
                (self.one_row()?.plan, Term::iri(s("")))
            };
            branches.push(self.path_project(
                plan,
                vec![
                    (START.into(), term.clone()),
                    (END.into(), term.clone()),
                    (GRAPH.into(), graph),
                ],
            )?);
        }
        let mut out = branches.remove(0);
        for branch in branches {
            out = self.union(out, branch)?;
        }
        self.distinct(out)
    }

    fn path_relation(
        &mut self,
        source: &QuadSource,
        path: &RdfPathExpr,
        identity: &Sol,
    ) -> RelResult<Sol> {
        match path {
            RdfPathExpr::Iri(iri) => {
                self.path_edges(source, Some(col_exact(&source.names[2]).eq(s(iri))))
            }
            RdfPathExpr::Inverse(inner) => {
                let sol = self.path_relation(source, inner, identity)?;
                self.path_rename(sol, &[(START, END), (END, START)])
            }
            RdfPathExpr::Negated(inner) => {
                fn excluded(path: &RdfPathExpr, iris: &mut Vec<Expr>) -> RelResult<()> {
                    match path {
                        RdfPathExpr::Iri(iri) => iris.push(s(iri)),
                        RdfPathExpr::Alternative(paths) => {
                            for path in paths {
                                excluded(path, iris)?;
                            }
                        }
                        _ => return unsupported("invalid negated property set"),
                    }
                    Ok(())
                }
                let mut iris = Vec::new();
                excluded(inner, &mut iris)?;
                let edges = self.path_edges(
                    source,
                    Some(col_exact(&source.names[2]).in_list(iris, true)),
                )?;
                self.distinct(edges)
            }
            RdfPathExpr::Sequence(paths) | RdfPathExpr::Alternative(paths) => {
                let mut paths = paths.iter();
                let Some(first) = paths.next() else {
                    return unsupported("empty path expression");
                };
                let mut out = self.path_relation(source, first, identity)?;
                for next in paths {
                    let right = self.path_relation(source, next, identity)?;
                    out = if matches!(path, RdfPathExpr::Sequence(_)) {
                        self.path_compose(out, right)?
                    } else {
                        self.union(out, right)?
                    };
                }
                Ok(out)
            }
            RdfPathExpr::ZeroOrOne(inner) => {
                let one = self.path_relation(source, inner, identity)?;
                let out = self.union(identity.clone(), one)?;
                self.distinct(out)
            }
            RdfPathExpr::ZeroOrMore(inner) | RdfPathExpr::OneOrMore(inner) => {
                let edge = self.path_relation(source, inner, identity)?;
                let edge = Sol {
                    plan: self.cte(edge.plan.clone())?,
                    ..edge
                };
                let seed = if matches!(path, RdfPathExpr::ZeroOrMore(_)) {
                    self.union(identity.clone(), edge.clone())?
                } else {
                    edge.clone()
                };
                let seed = self.distinct(seed)?;
                // All terms use the same sorted column order in both arms.
                let seed = self.path_rename(seed, &[])?;
                let name = self.fresh("rdf_closure");
                let table = Arc::new(CteWorkTable::new(
                    &name,
                    Arc::new(seed.plan.schema().as_arrow().clone()),
                ));
                let scan =
                    LogicalPlanBuilder::scan(&name, provider_as_source(table), None)?.build()?;
                let work = Sol {
                    plan: scan,
                    ..seed.clone()
                };
                let recursive = self.path_compose(work, edge)?;
                let recursive = self.path_rename(recursive, &[])?;
                let plan = LogicalPlanBuilder::from(seed.plan)
                    .to_recursive_query(name, recursive.plan, true)?
                    .build()?;
                Ok(Sol { plan, ..seed })
            }
        }
    }

    pub(super) fn property_path(
        &mut self,
        dataset: &str,
        scope: &RdfGraphScope,
        subject: &RdfTerm,
        object: &RdfTerm,
        path: &RdfPathExpr,
    ) -> RelResult<Sol> {
        let mut source = quad_source(self.ctx, dataset, scope)?;
        source.plan = self.cte(source.plan)?;
        let constant = |term: &RdfTerm| -> RelResult<Option<Term>> {
            Ok(Some(match term {
                RdfTerm::Variable(_) => return Ok(None),
                RdfTerm::Iri(iri) => Term::iri(s(iri)),
                RdfTerm::BlankNode(label) => Term {
                    value: s(label),
                    kind: s(KIND_BLANK),
                    dt: null_str(),
                    lang: null_str(),
                },
                RdfTerm::Typed { lexical, datatype } => Term::literal(s(lexical), datatype),
                RdfTerm::LanguageTagged { value, lang } => Term {
                    value: s(value),
                    kind: s(KIND_LITERAL),
                    dt: s(RDF_LANG_STRING),
                    lang: s(&lang.to_ascii_lowercase()),
                },
                RdfTerm::Literal(value) => {
                    let (value, dt) = literal_identity(value)?;
                    Term::literal(s(&value), &dt)
                }
            }))
        };
        let constants = [constant(subject)?, constant(object)?]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        let graph_var = match scope {
            RdfGraphScope::NamedGraphVariable(var)
            | RdfGraphScope::DatasetNamedGraphVariable { variable: var, .. } => Some(var),
            _ => None,
        };
        if graph_var.is_none() {
            let columns = source
                .plan
                .schema()
                .fields()
                .iter()
                .map(|field| {
                    if field.name() == &source.names[0] {
                        null_str().alias(field.name())
                    } else {
                        col_exact(field.name())
                    }
                })
                .collect();
            source.plan = self.project(source.plan, columns)?;
        }
        let named = matches!(
            scope,
            RdfGraphScope::NamedGraph(_)
                | RdfGraphScope::NamedGraphVariable(_)
                | RdfGraphScope::DatasetNamedGraph { .. }
                | RdfGraphScope::DatasetNamedGraphVariable { .. }
        );
        let mut identity = self.path_identity(&source, &constants, named)?;
        // EXISTS substitutes outer bindings before evaluating a path. A bound
        // endpoint may therefore match zero steps even outside nodes(graph).
        if let Some(seed) = self.seeds.last().cloned() {
            for endpoint in [subject, object] {
                let RdfTerm::Variable(var) = endpoint else {
                    continue;
                };
                let Some(term) = seed.term(var) else { continue };
                let plan = self.filter_plan(seed.plan.clone(), term.bound())?;
                let mut terms = vec![(START.into(), term.clone()), (END.into(), term)];
                if !named {
                    terms.push((GRAPH.into(), Term::iri(s(""))));
                }
                let mut branch = self.path_project(plan, terms)?;
                if named {
                    let edges = self.path_edges(&source, None)?;
                    let graphs =
                        self.path_project(edges.plan, vec![(GRAPH.into(), Term::columns(GRAPH))])?;
                    let graphs = self.distinct(graphs)?;
                    branch = self.join(branch, graphs, false)?;
                }
                identity = self.union(identity, branch)?;
            }
            identity = self.distinct(identity)?;
        }
        let sol = self.path_relation(&source, path, &identity)?;
        let mut predicates = Vec::new();
        let mut bindings = BTreeMap::<String, Term>::new();
        for (term, column) in [(subject, START), (object, END)] {
            let actual = Term::columns(column);
            if let RdfTerm::Variable(var) = term {
                if let Some(previous) = bindings.get(var) {
                    predicates.push(actual.same_term(previous));
                } else {
                    bindings.insert(var.clone(), actual);
                }
            } else if let Some(term) = constant(term)? {
                predicates.push(actual.same_term(&term));
            }
        }
        if let Some(var) = graph_var {
            let actual = Term::columns(GRAPH);
            if let Some(previous) = bindings.get(var) {
                predicates.push(actual.same_term(previous));
            } else {
                bindings.insert(var.clone(), actual);
            }
        }
        let plan = self.filter_plan(sol.plan, and_all(predicates))?;
        let mut out = if bindings.is_empty() {
            let column = self.fresh("path_match");
            let mut columns = vec![lit(1_i64).alias(column)];
            columns.extend(sol.keys.iter().map(col_exact));
            Sol {
                plan: self.project(plan, columns)?,
                vars: BTreeMap::new(),
                keys: sol.keys,
                ord: None,
            }
        } else {
            self.path_project(plan, bindings.into_iter().collect())?
        };
        if let Some(seed) = self.seeds.last().cloned() {
            out = self.join(out, seed, false)?;
        }
        Ok(out)
    }
}
