# Graph IR Language Examples

Status: draft v0.2 for review  
Author: Codex  
Scope: examples that map Cypher, GQL, SPARQL, and Gremlin into initial Graph IR DAGs.

This document complements `docs/graph_ir_design.md`. It shows the initial logical plan emitted by language-specific builders. The format is intentionally close to database `EXPLAIN` output: one logical operator per line, with all operator properties shown in brackets.

This version tightens the semantic contract around language boundaries. Cypher, GQL, SPARQL, and Gremlin often share a structural graph operation, but they do not always share missing-value, path, multiplicity, graph-scope, or result-shape semantics. Where semantics differ, examples either split into separate plans or include an explicit policy.

## 0. Semantic Contract

Every initial Graph IR plan carries a policy block. The policy is not an optimization hint; it is part of the logical semantics that later rewrites must preserve.

```text
GraphPlanPolicy(
  language=[Cypher | GQL | SPARQL | Gremlin],
  resultForm=[RowSet | TraverserStream | RdfGraph | Boolean],
  multiplicity=[Bag | BulkAwareBag | Set],
  propertyMissing=[NullOnMissing | DropUnproductive | Unbound | Error | ProviderDefined],
  optionalMissing=[Null | Unbound],
  pathMode=[None | Walk | Trail | Simple | Acyclic],
  matchMode=[None | DifferentRelationships | RepeatableElements | ProviderDefined],
  graphScope=[PropertyGraph(default) | ActiveRdfGraph | DefaultRdfGraph | NamedRdfGraph | ProviderDefined],
  outputNaming=[SourceNames | AliasedNames | SyntheticNames]
)
```

### 0.1 Binding conventions

- `p`, `a`, `b`, `f`, `r`, and `path` are property-graph logical binding ids.
- `current` is the current Gremlin traverser object.
- `start`, `knows`, `created`, and similar names are internal builder bindings used to explain Gremlin traversal state.
- `?p`, `?name`, `?g`, and similar names are SPARQL solution variables. SPARQL variables are printed with the leading `?` in both plans and output schemas.
- `stmt`, `type_stmt`, and similar names are RDF statement bindings when a statement-level plan is needed.
- Hidden metadata, such as Gremlin traverser bulk, is printed as `_bulk` and is not visible unless a step explicitly observes it.

### 0.2 Result boundaries

Every complete query or traversal has an explicit output boundary:

```text
GraphReturn(fields=[...], resultForm=[RowSet | TraverserStream])
GraphAsk(field=[ask])
GraphConstructTriples(template=[...])
GraphDescribe(terms=[...])
```

A Gremlin traversal can use `GraphReturn(resultForm=[TraverserStream])` even when the source language has no textual `RETURN` keyword. This keeps terminal result shape explicit and avoids confusing a row-producing Cypher query with a current-object Gremlin traversal.

### 0.3 Missing, null, unbound, and unproductive values

Use explicit property-access policies:

```text
property(p, "name", policy=NullOnMissing)       -- Cypher/GQL property access
property(current, "name", policy=DropUnproductive) -- Gremlin values('name')-style traversal
?name with optionalMissing=Unbound              -- SPARQL OPTIONAL output
```

Important distinction:

```cypher
RETURN p.name AS name
```

returns a row containing `null` if `p.name` is missing.

```gremlin
g.V().values('name')
```

emits no traverser for an element whose `name` value is unproductive.

### 0.4 Multiplicity and Gremlin bulk

Cypher and SPARQL row streams are bag-valued unless a distinct or set-producing operation applies. Gremlin is bulk-aware: a single traverser can represent multiple traversers. Bulk is hidden metadata unless a bulk-sensitive step such as `count()`, `barrier()`, or `dedup()` requires it.

Recommended convention:

```text
countRows()       -- counts visible rows / solution mappings
countBulk()       -- sums hidden Gremlin traverser bulk
collectRows(x)    -- collects row values
collectTraversers(current) -- collects Gremlin current objects, respecting traversal semantics
```

### 0.5 Path semantics

Property-graph path examples should separate path mode from match mode:

```text
pathMode=[Walk | Trail | Simple | Acyclic]
matchMode=[DifferentRelationships | RepeatableElements]
```

For Cypher-like matching, the default is represented as:

```text
pathMode=[Walk]
matchMode=[DifferentRelationships]
```

because nodes may repeat, but relationships may not repeat within a given `MATCH` result unless `REPEATABLE ELEMENTS` is requested.

Gremlin path semantics are traverser-history semantics. A Gremlin path contains the objects that the traversal has visited; it does not contain relationship objects unless the traversal visits edges, for example with `outE(...).inV()`.

SPARQL property paths bind endpoints; they generally do not materialize a path value. Some SPARQL property path operators are set-producing, and property paths do not span multiple RDF graphs in a dataset.

### 0.6 RDF dataset and active graph semantics

SPARQL plans distinguish the query dataset from the graph scope used by a pattern:

```text
GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], ...)
GraphRdfQuadScan(dataset=[queryDataset], graphScope=[NamedGraph(iri(:g1))], ...)
GraphRdfQuadScan(dataset=[queryDataset], graphScope=[NamedGraphVariable(?g)], ...)
GraphRdfPropertyPath(dataset=[queryDataset], graphScope=[ActiveGraph], ...)
```

`GRAPH ?g { ... }` ranges over named graphs in the dataset, not the default graph.

### 0.7 When shared examples are only structural

Some examples show analogous Cypher and Gremlin inputs. If the plan is shared, it means the structural graph operation is shared. It does not imply all language semantics are identical. If missing values, result shape, or path history change the answer, this document provides separate plans.

## 1. Explain Format

Plans are printed top-down:

```text
Policy: GraphPlanPolicy(language=[Cypher], resultForm=[RowSet], multiplicity=[Bag], propertyMissing=[NullOnMissing])

GraphReturn(fields=[p], resultForm=[RowSet])
  GraphBind(bind=[p], kind=[Node])
    GraphNodeScan(graph=[default], labels=[Person])
```

Branching operators use named inputs:

```text
GraphApply(kind=[Optional], correlation=[p], outputs=[c])
  left:
    ...
  right:
    ...
```

Conventions:

- `GraphNodeScan` and `GraphRelScan` enumerate property-graph elements.
- `GraphRdfQuadScan` enumerates RDF triples/quads in a dataset and graph scope. Terms may be constants, variables, or correlated variables.
- `GraphBind` assigns an internal binding id to the current graph element or expression.
- `GraphFilter` constrains rows. Property predicates do not live inside scans unless a later physical optimization pushes them down.
- `GraphExpand` performs property-graph traversal. Relationship property predicates are represented by a separate `GraphFilter`.
- `GraphProject` computes row values and controls visible scope.
- `GraphCurrentProject` computes Gremlin current-object values.
- `GraphApply` is the correlated subquery operator. Optional, semi, anti, scalar, exists, and not-exists forms are represented by `kind`.
- `GraphSparqlMinus` preserves SPARQL `MINUS` semantics and must not be lowered to anti-join until compatibility analysis proves equivalence.

## 2. Property Graph Match And Traversal

### 2.1 Node Scan

Cypher:

```cypher
MATCH (p:Person)
RETURN p
```

GQL:

```gql
MATCH (p:Person)
RETURN p
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Cypher|GQL], resultForm=[RowSet], multiplicity=[Bag], propertyMissing=[NullOnMissing])

GraphReturn(fields=[p], resultForm=[RowSet])
  GraphBind(bind=[p], kind=[Node])
    GraphNodeScan(graph=[default], labels=[Person])
```

Gremlin:

```gremlin
g.V().hasLabel('Person')
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Gremlin], resultForm=[TraverserStream], multiplicity=[BulkAwareBag], propertyMissing=[DropUnproductive])

GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphBind(bind=[current], kind=[Node])
    GraphNodeScan(graph=[default], labels=[Person])
```

### 2.2 Multi-Label Node Scan

Cypher:

```cypher
MATCH (p:Person:Employee)
RETURN p
```

GQL:

```gql
MATCH (p:Person&Employee)
RETURN p
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Cypher|GQL], resultForm=[RowSet], multiplicity=[Bag], propertyMissing=[NullOnMissing])

GraphReturn(fields=[p], resultForm=[RowSet])
  GraphBind(bind=[p], kind=[Node])
    GraphNodeScan(graph=[default], labelsExpr=[allOf(Person, Employee)])
```

Portable Gremlin does not have native multi-label vertices. A provider may expose multi-label behavior, but that should be represented with an explicit provider policy:

```text
Policy: GraphPlanPolicy(language=[Gremlin], resultForm=[TraverserStream], graphScope=[ProviderDefined], providerFeature=[MultiLabelVertices])
```

### 2.3 Label Expression Variants

GQL / Cypher-style label expressions:

```cypher
MATCH (n:Person|Company)
RETURN n
```

```text
GraphReturn(fields=[n], resultForm=[RowSet])
  GraphBind(bind=[n], kind=[Node])
    GraphNodeScan(graph=[default], labelsExpr=[anyOf(Person, Company)])
```

```cypher
MATCH (n:!Deleted)
RETURN n
```

```text
GraphReturn(fields=[n], resultForm=[RowSet])
  GraphBind(bind=[n], kind=[Node])
    GraphNodeScan(graph=[default], labelsExpr=[not(Deleted)])
```

### 2.4 Property Filter

Cypher:

```cypher
MATCH (p:Person)
WHERE p.name = 'marko' AND p.age = 29
RETURN p
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Cypher], resultForm=[RowSet], propertyMissing=[NullOnMissing])

GraphReturn(fields=[p], resultForm=[RowSet])
  GraphFilter(condition=[and(=(property(p, "name", policy=NullOnMissing), "marko"), =(property(p, "age", policy=NullOnMissing), 29))])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

Gremlin:

```gremlin
g.V().hasLabel('Person').has('name', 'marko').has('age', 29)
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Gremlin], resultForm=[TraverserStream], propertyMissing=[DropUnproductive])

GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphFilter(condition=[and(=(property(current, "name", policy=DropUnproductive), "marko"), =(property(current, "age", policy=DropUnproductive), 29))])
    GraphBind(bind=[current], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

### 2.5 Property Map Pattern

Cypher:

```cypher
MATCH (p:Person {name: 'marko', age: 29})
RETURN p
```

GQL:

```gql
MATCH (p:Person {name: 'marko', age: 29})
RETURN p
```

Initial logical plan:

```text
GraphReturn(fields=[p], resultForm=[RowSet])
  GraphFilter(condition=[and(=(property(p, "name", policy=NullOnMissing), "marko"), =(property(p, "age", policy=NullOnMissing), 29))])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

### 2.6 One-Hop Directed Expansion

Cypher:

```cypher
MATCH (p:Person)-[:KNOWS]->(f)
RETURN f.name AS name
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Cypher], resultForm=[RowSet], propertyMissing=[NullOnMissing], matchMode=[DifferentRelationships])

GraphReturn(fields=[name], resultForm=[RowSet])
  GraphProject(mode=[PreserveVisible], exprs=[name=property(f, "name", policy=NullOnMissing)], fields=[name])
    GraphExpand(graph=[default], source=[p], target=[f], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], matchMode=[DifferentRelationships])
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
```

Gremlin:

```gremlin
g.V().hasLabel('Person').out('KNOWS').values('name')
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Gremlin], resultForm=[TraverserStream], propertyMissing=[DropUnproductive], multiplicity=[BulkAwareBag])

GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphCurrentProject(expr=[current=property(current, "name", policy=DropUnproductive)])
    GraphExpand(graph=[default], source=[start], target=[current], targetMode=[ReplaceCurrent], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], matchMode=[ProviderDefined])
      GraphBind(bind=[start], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
```

### 2.7 Relationship Binding

Cypher:

```cypher
MATCH (p)-[r:KNOWS]->(f)
RETURN r.since AS since
```

Initial logical plan:

```text
GraphReturn(fields=[since], resultForm=[RowSet])
  GraphProject(mode=[PreserveVisible], exprs=[since=property(r, "since", policy=NullOnMissing)], fields=[since])
    GraphExpand(graph=[default], source=[p], target=[f], targetMode=[BindNew], targetLabels=[any], relBinding=[r], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], matchMode=[DifferentRelationships])
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
```

Gremlin:

```gremlin
g.V().outE('KNOWS').as('r').inV().select('r').values('since')
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphCurrentProject(expr=[current=property(r, "since", policy=DropUnproductive)])
    GraphSelect(labels=[r], output=[current])
      GraphExpand(graph=[default], source=[start], target=[v], targetMode=[ReplaceCurrent], targetLabels=[any], relBinding=[r], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], pathMaterialization=[VisitedEdgesAndVertices])
        GraphBind(bind=[start], kind=[Node])
          GraphNodeScan(graph=[default], labels=[any])
```

### 2.8 Incoming Expansion

Cypher:

```cypher
MATCH (p:Person)<-[:KNOWS]-(f)
RETURN f
```

Gremlin:

```gremlin
g.V().hasLabel('Person').in('KNOWS')
```

Initial logical plan for the shared structural expansion:

```text
GraphReturn(fields=[f], resultForm=[RowSet|TraverserStream])
  GraphExpand(graph=[default], source=[p], target=[f], targetMode=[BindNewOrReplaceCurrent], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[In], length=[min=1, max=1])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

For Gremlin, the visible output field is `current`; `f` is an explanatory internal binding unless the traversal labels it and selects it.

### 2.9 Bidirectional Expansion

Cypher:

```cypher
MATCH (p)-[:KNOWS]-(f)
RETURN p, f
```

Gremlin, pair-producing form:

```gremlin
g.V().as('p').both('KNOWS').as('f').select('p','f')
```

Initial logical plan:

```text
GraphReturn(fields=[p, f], resultForm=[RowSet|TraverserStream])
  GraphExpand(graph=[default], source=[p], target=[f], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Both], length=[min=1, max=1])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[any])
```

Bare Gremlin `g.V().both('KNOWS')` is different. It returns only adjacent vertices:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphExpand(graph=[default], source=[start], target=[current], targetMode=[ReplaceCurrent], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Both], length=[min=1, max=1])
    GraphBind(bind=[start], kind=[Node])
      GraphNodeScan(graph=[default], labels=[any])
```

### 2.10 Existing Target / Expand Into

Cypher:

```cypher
MATCH (a:Person), (b:Person)
WHERE a.name = 'marko' AND b.name = 'vadas'
MATCH (a)-[:KNOWS]->(b)
RETURN a, b
```

Initial logical plan:

```text
GraphReturn(fields=[a, b], resultForm=[RowSet])
  GraphExpand(graph=[default], source=[a], target=[b], targetMode=[Existing], targetLabels=[Person], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], matchMode=[DifferentRelationships])
    GraphFilter(condition=[and(=(property(a, "name", policy=NullOnMissing), "marko"), =(property(b, "name", policy=NullOnMissing), "vadas"))])
      GraphJoin(kind=[Inner], condition=[true])
        GraphBind(bind=[a], kind=[Node])
          GraphNodeScan(graph=[default], labels=[Person])
        GraphBind(bind=[b], kind=[Node])
          GraphNodeScan(graph=[default], labels=[Person])
```

### 2.11 Repeated Node Variable Cycle

Cypher:

```cypher
MATCH (a)-[:KNOWS]->(b)-[:KNOWS]->(a)
RETURN a, b
```

Gremlin:

```gremlin
g.V().as('a').out('KNOWS').as('b').out('KNOWS').where(eq('a')).select('a','b')
```

Initial logical plan:

```text
GraphReturn(fields=[a, b], resultForm=[RowSet|TraverserStream])
  GraphExpand(graph=[default], source=[b], target=[a], targetMode=[Existing], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
    GraphExpand(graph=[default], source=[a], target=[b], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
      GraphBind(bind=[a], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
```

### 2.12 Edge Property Filter

Cypher:

```cypher
MATCH (a)-[r:KNOWS]->(b)
WHERE r.since >= 2020
RETURN a, b
```

Gremlin:

```gremlin
g.V().outE('KNOWS').has('since', gte(2020)).inV()
```

Initial logical plan:

```text
GraphReturn(fields=[a, b], resultForm=[RowSet|TraverserStream])
  GraphFilter(condition=[>=(property(r, "since", policy=LanguageDefault), 2020)])
    GraphExpand(graph=[default], source=[a], target=[b], targetMode=[BindNewOrReplaceCurrent], targetLabels=[any], relBinding=[r], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
      GraphBind(bind=[a], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
```

For a final builder, replace `LanguageDefault` with `NullOnMissing` for Cypher/GQL and `DropUnproductive` for Gremlin.

## 3. Horizons, Projection, And Scope

### 3.1 Cypher WITH Scope Replacement

Cypher:

```cypher
MATCH (p:Person)
WITH p.name AS name
RETURN name
```

Initial logical plan:

```text
GraphReturn(fields=[name], resultForm=[RowSet])
  GraphProject(mode=[ReplaceScope], exprs=[name=property(p, "name", policy=NullOnMissing)], fields=[name])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

### 3.2 WITH Preserves Selected Binding

Cypher:

```cypher
MATCH (p:Person)-[:KNOWS]->(f)
WITH p, count(f) AS friend_count
RETURN p.name AS name, friend_count
```

Initial logical plan:

```text
GraphReturn(fields=[name, friend_count], resultForm=[RowSet])
  GraphProject(mode=[ReplaceScope], exprs=[name=property(p, "name", policy=NullOnMissing), friend_count=friend_count], fields=[name, friend_count])
    GraphAggregate(group=[p], aggs=[friend_count=countRows(f)], fields=[p, friend_count])
      GraphExpand(graph=[default], source=[p], target=[f], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], matchMode=[DifferentRelationships])
        GraphBind(bind=[p], kind=[Node])
          GraphNodeScan(graph=[default], labels=[Person])
```

### 3.3 Gremlin Current Projection

Gremlin:

```gremlin
g.V().hasLabel('Person').values('name')
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphCurrentProject(expr=[current=property(current, "name", policy=DropUnproductive)])
    GraphBind(bind=[current], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

### 3.4 Gremlin Label Binding

Gremlin:

```gremlin
g.V().as('a').out('KNOWS').as('b').select('a','b')
```

Initial logical plan:

```text
GraphReturn(fields=[a, b], resultForm=[TraverserStream])
  GraphSelect(labels=[a, b], output=[a, b])
    GraphExpand(graph=[default], source=[a], target=[b], targetMode=[ReplaceCurrentAndBindLabel], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
      GraphBind(bind=[a], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
```

### 3.5 Map Projection

Cypher:

```cypher
MATCH (p:Person)
RETURN p { .name, .age, city: p.address.city } AS profile
```

Initial logical plan:

```text
GraphReturn(fields=[profile], resultForm=[RowSet])
  GraphProject(mode=[PreserveVisible], exprs=[profile=map{name: property(p, "name", policy=NullOnMissing), age: property(p, "age", policy=NullOnMissing), city: property(property(p, "address", policy=NullOnMissing), "city", policy=NullOnMissing)}], fields=[profile])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

## 4. Optional, Exists, Anti, And Correlation

### 4.1 Optional Match

Cypher:

```cypher
MATCH (p:Person)
OPTIONAL MATCH (p)-[:WORKS_AT]->(c)
RETURN p, c
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Cypher], optionalMissing=[Null])

GraphReturn(fields=[p, c], resultForm=[RowSet])
  GraphApply(kind=[Optional], correlation=[p], outputs=[c], optionalMissing=[Null])
    left:
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
    right:
      GraphExpand(graph=[default], source=[p], target=[c], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[WORKS_AT], dir=[Out], length=[min=1, max=1], matchMode=[DifferentRelationships])
        GraphCorrelate(bindings=[p])
```

### 4.2 Optional Match With Predicate

Cypher:

```cypher
MATCH (p:Person)
OPTIONAL MATCH (p)-[:WORKS_AT]->(c)
WHERE c.name STARTS WITH 'A'
RETURN p, c
```

Initial logical plan:

```text
GraphReturn(fields=[p, c], resultForm=[RowSet])
  GraphApply(kind=[Optional], correlation=[p], outputs=[c], optionalMissing=[Null])
    left:
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
    right:
      GraphFilter(condition=[starts_with(property(c, "name", policy=NullOnMissing), "A")])
        GraphExpand(graph=[default], source=[p], target=[c], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[WORKS_AT], dir=[Out], length=[min=1, max=1], matchMode=[DifferentRelationships])
          GraphCorrelate(bindings=[p])
```

The filter remains inside the optional branch. Moving it above `GraphApply` would filter null-extended rows after optional matching and would change Cypher semantics.

### 4.3 Exists Pattern

Cypher:

```cypher
MATCH (p:Person)
WHERE EXISTS { MATCH (p)-[:KNOWS]->(:Person {name: 'marko'}) }
RETURN p
```

Gremlin:

```gremlin
g.V().hasLabel('Person').where(out('KNOWS').hasLabel('Person').has('name', 'marko'))
```

Initial logical plan:

```text
GraphReturn(fields=[p], resultForm=[RowSet|TraverserStream])
  GraphApply(kind=[Semi], correlation=[p], outputs=[])
    left:
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
    right:
      GraphFilter(condition=[=(property(person, "name", policy=LanguageDefault), "marko")])
        GraphExpand(graph=[default], source=[p], target=[person], targetMode=[BindNew], targetLabels=[Person], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
          GraphCorrelate(bindings=[p])
```

### 4.4 Not Exists / Anti Pattern

Cypher:

```cypher
MATCH (p:Person)
WHERE NOT EXISTS { MATCH (p)-[:BLOCKED]->(:Person) }
RETURN p
```

Gremlin:

```gremlin
g.V().hasLabel('Person').not(out('BLOCKED').hasLabel('Person'))
```

Initial logical plan:

```text
GraphReturn(fields=[p], resultForm=[RowSet|TraverserStream])
  GraphApply(kind=[Anti], correlation=[p], outputs=[])
    left:
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
    right:
      GraphExpand(graph=[default], source=[p], target=[blocked], targetMode=[BindNew], targetLabels=[Person], relBinding=[anonymous], relTypes=[BLOCKED], dir=[Out], length=[min=1, max=1])
        GraphCorrelate(bindings=[p])
```

### 4.5 Scalar Correlated Subquery

Cypher:

```cypher
MATCH (p:Person)
RETURN p.name AS name, COUNT { (p)-[:KNOWS]->() } AS degree
```

Gremlin:

```gremlin
g.V().hasLabel('Person').project('name','degree').by('name').by(out('KNOWS').count())
```

Initial logical plan:

```text
GraphReturn(fields=[name, degree], resultForm=[RowSet|TraverserStream])
  GraphProject(mode=[PreserveVisible], exprs=[name=property(p, "name", policy=LanguageDefault), degree=degree], fields=[name, degree])
    GraphApply(kind=[Scalar], correlation=[p], outputs=[degree])
      left:
        GraphBind(bind=[p], kind=[Node])
          GraphNodeScan(graph=[default], labels=[Person])
      right:
        GraphAggregate(group=[], aggs=[degree=countRowsOrBulk(neighbor)], fields=[degree])
          GraphExpand(graph=[default], source=[p], target=[neighbor], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
            GraphCorrelate(bindings=[p])
```

A final builder should specialize `countRowsOrBulk` to `countRows` for Cypher/GQL and `countBulk` for Gremlin.

### 4.6 Gremlin Coalesce

Gremlin:

```gremlin
g.V().coalesce(out('KNOWS'), out('CREATED'), constant('none'))
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Gremlin], resultForm=[TraverserStream], multiplicity=[BulkAwareBag])

GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphCoalesce(success=[firstNonEmpty], output=[current], armOutputs=[knows->current, created->current, current->current])
    input:
      GraphBind(bind=[start], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
    arm0:
      GraphProject(mode=[ReplaceCurrent], exprs=[current=knows], fields=[current])
        GraphExpand(graph=[default], source=[start], target=[knows], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
          GraphCorrelate(bindings=[start])
    arm1:
      GraphProject(mode=[ReplaceCurrent], exprs=[current=created], fields=[current])
        GraphExpand(graph=[default], source=[start], target=[created], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[CREATED], dir=[Out], length=[min=1, max=1])
          GraphCorrelate(bindings=[start])
    arm2:
      GraphProject(mode=[ReplaceCurrent], exprs=[current="none"], fields=[current])
        GraphCorrelate(bindings=[start])
```

`GraphCoalesce` evaluates arms per input traverser. It chooses the first arm that emits at least one result. If the chosen arm emits multiple results, all emitted results from that arm are returned for that input traverser.

### 4.7 Gremlin Choose

Gremlin:

```gremlin
g.V().choose(hasLabel('Person'), values('name'), values('title'))
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphChoose(condition=[has_label(current, Person)], arms=[true, false], output=[current])
    input:
      GraphBind(bind=[current], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
    true:
      GraphCurrentProject(expr=[current=property(current, "name", policy=DropUnproductive)])
        GraphCorrelate(bindings=[current])
    false:
      GraphCurrentProject(expr=[current=property(current, "title", policy=DropUnproductive)])
        GraphCorrelate(bindings=[current])
```

The general operator should also support switch-style `choose()` with `option(...)`, default arms, and unmatched policies.

## 5. Path And Variable Length

### 5.1 Fixed Two-Hop Pattern

Cypher:

```cypher
MATCH (a)-[:KNOWS]->()-[:CREATED]->(m)
RETURN m
```

Initial logical plan:

```text
GraphReturn(fields=[m], resultForm=[RowSet])
  GraphExpand(graph=[default], source=[mid], target=[m], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[CREATED], dir=[Out], length=[min=1, max=1], matchMode=[DifferentRelationships])
    GraphExpand(graph=[default], source=[a], target=[mid], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], matchMode=[DifferentRelationships])
      GraphBind(bind=[a], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
```

### 5.2 Cypher Variable-Length Relationship

Cypher:

```cypher
MATCH p = (a)-[:KNOWS*1..3]->(b)
RETURN p
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Cypher], resultForm=[RowSet], pathMode=[Walk], matchMode=[DifferentRelationships])

GraphReturn(fields=[p], resultForm=[RowSet])
  GraphExpand(graph=[default], source=[a], target=[b], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=3], path=[p], pathMode=[Walk], matchMode=[DifferentRelationships], pathMaterialization=[NodesAndRelationships])
    GraphBind(bind=[a], kind=[Node])
      GraphNodeScan(graph=[default], labels=[any])
```

### 5.3 Gremlin Variable-Length Path

Gremlin, vertex-only path history:

```gremlin
g.V().repeat(out('KNOWS')).emit().times(3).path()
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Gremlin], resultForm=[TraverserStream], pathMode=[TraverserHistory], matchMode=[ProviderDefined])

GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphCurrentProject(expr=[current=path], fields=[current])
    GraphRepeat(emit=[AfterEachIteration], times=[max=3], output=[current], path=[path], pathObjects=[VerticesOnly])
      seed:
        GraphBind(bind=[current], kind=[Node])
          GraphNodeScan(graph=[default], labels=[any])
      body:
        GraphExpand(graph=[default], source=[current], target=[current], targetMode=[ReplaceCurrent], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], pathUpdate=[AppendTargetVertex])
```

Gremlin, vertices and edges in path history:

```gremlin
g.V().repeat(outE('KNOWS').inV()).emit().times(3).path()
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphCurrentProject(expr=[current=path], fields=[current])
    GraphRepeat(emit=[AfterEachIteration], times=[max=3], output=[current], path=[path], pathObjects=[VerticesAndEdges])
      seed:
        GraphBind(bind=[current], kind=[Node])
          GraphNodeScan(graph=[default], labels=[any])
      body:
        GraphExpand(graph=[default], source=[current], target=[current], targetMode=[ReplaceCurrent], targetLabels=[any], relBinding=[edge], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], pathUpdate=[AppendEdgeAndTargetVertex])
```

### 5.4 Exact-Length Path

Cypher:

```cypher
MATCH (a)-[:KNOWS*2]->(b)
RETURN b
```

Initial logical plan:

```text
GraphReturn(fields=[b], resultForm=[RowSet])
  GraphExpand(graph=[default], source=[a], target=[b], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=2, max=2], path=[none], pathMode=[Walk], matchMode=[DifferentRelationships])
    GraphBind(bind=[a], kind=[Node])
      GraphNodeScan(graph=[default], labels=[any])
```

### 5.5 Gremlin Simple Path Constraint

Gremlin:

```gremlin
g.V().repeat(out('KNOWS').simplePath()).times(3).path()
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphCurrentProject(expr=[current=path], fields=[current])
    GraphRepeat(emit=[AfterLoop], times=[exact=3], output=[current], path=[path], prefixPredicate=[simple_path(path)])
      seed:
        GraphBind(bind=[current], kind=[Node])
          GraphNodeScan(graph=[default], labels=[any])
      body:
        GraphPathFilter(condition=[simple_path(path)], scope=[currentPrefix])
          GraphExpand(graph=[default], source=[current], target=[current], targetMode=[ReplaceCurrent], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1], pathUpdate=[AppendTargetVertex])
```

`simplePath()` is modeled inside the repeat body because it prunes each prefix during expansion. A final-result-only filter can be too late when later loop iterations depend on the pruned traversers.

### 5.6 Shortest Path

Cypher:

```cypher
MATCH p = shortestPath((a)-[:KNOWS*]-(b))
RETURN p
```

GQL:

```gql
MATCH p = ANY SHORTEST (a)-[:KNOWS]->+(b)
RETURN p
```

Initial logical plan:

```text
GraphReturn(fields=[p], resultForm=[RowSet])
  GraphPathPattern(graph=[default], path=[p], selector=[Shortest(k=1, ties=Any)], pathMode=[Walk], matchMode=[DifferentRelationships], endpoints=[a, b], pathMaterialization=[NodesAndRelationships])
    parts=[
      Node(bind=a, labels=any),
      Rel(bind=anonymous, types=KNOWS, dir=BothOrOutByLanguage, min=1, max=unbounded),
      Node(bind=b, labels=any)
    ]
```

The direction should be specialized by the language builder. The Cypher example is undirected; the GQL example is directed.

### 5.7 GQL Path Mode

GQL:

```gql
MATCH p = TRAIL (a)-[:KNOWS]->+(b)
RETURN p
```

Initial logical plan:

```text
GraphReturn(fields=[p], resultForm=[RowSet])
  GraphPathPattern(graph=[default], path=[p], selector=[All], pathMode=[Trail], matchMode=[DifferentRelationships], endpoints=[a, b], pathMaterialization=[NodesAndRelationships])
    parts=[
      Node(bind=a, labels=any),
      Rel(bind=anonymous, types=KNOWS, dir=Out, min=1, max=unbounded),
      Node(bind=b, labels=any)
    ]
```

### 5.8 Explicit Match Modes

Cypher / GQL-style examples:

```cypher
MATCH DIFFERENT RELATIONSHIPS p = (a)--{1,5}(b)
RETURN p
```

```text
GraphPathPattern(graph=[default], path=[p], selector=[All], pathMode=[Walk], matchMode=[DifferentRelationships], endpoints=[a, b])
```

```cypher
MATCH REPEATABLE ELEMENTS p = (a)--{1,5}(b)
RETURN p
```

```text
GraphPathPattern(graph=[default], path=[p], selector=[All], pathMode=[Walk], matchMode=[RepeatableElements], endpoints=[a, b])
```

```cypher
MATCH ACYCLIC p = (a)--+(b)
RETURN p
```

```text
GraphPathPattern(graph=[default], path=[p], selector=[All], pathMode=[Acyclic], matchMode=[DifferentRelationships], endpoints=[a, b])
```

### 5.9 SPARQL Property Path

SPARQL:

```sparql
SELECT ?x ?y WHERE {
  ?x (:knows/:worksWith)+ ?y .
}
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[SPARQL], resultForm=[RowSet], multiplicity=[Bag], optionalMissing=[Unbound], graphScope=[ActiveRdfGraph])

GraphReturn(fields=[?x, ?y], resultForm=[RowSet])
  GraphRdfPropertyPath(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?x], object=[?y], path=[one_or_more(seq(iri(:knows), iri(:worksWith)))], pathMaterialization=[EndpointsOnly])
```

### 5.10 SPARQL Zero-Length Property Path

SPARQL:

```sparql
SELECT ?x WHERE {
  ?x :knows* ?x .
}
```

Initial logical plan:

```text
GraphReturn(fields=[?x], resultForm=[RowSet])
  GraphRdfPropertyPath(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?x], object=[?x], path=[zero_or_more(iri(:knows))], pathMaterialization=[EndpointsOnly], zeroLength=[Allowed])
```

## 6. Aggregation, Distinct, Order, And Barriers

### 6.1 Count All

Cypher:

```cypher
MATCH (p:Person)
RETURN count(*) AS n
```

Initial logical plan:

```text
GraphReturn(fields=[n], resultForm=[RowSet])
  GraphAggregate(group=[], aggs=[n=countRows()], fields=[n])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

Gremlin:

```gremlin
g.V().hasLabel('Person').count()
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphAggregate(group=[], aggs=[current=countBulk()], fields=[current])
    GraphBind(bind=[current], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

### 6.2 Group By Property

Cypher:

```cypher
MATCH (p:Person)
RETURN p.country AS country, count(*) AS n
```

Initial logical plan:

```text
GraphReturn(fields=[country, n], resultForm=[RowSet])
  GraphAggregate(group=[country=property(p, "country", policy=NullOnMissing)], aggs=[n=countRows()], fields=[country, n])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

Gremlin:

```gremlin
g.V().hasLabel('Person').groupCount().by('country')
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphGroupMap(key=[property(current, "country", policy=DropUnproductive)], value=[countBulk()], merge=[sum], output=[current])
    GraphBind(bind=[current], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

Gremlin rowification, if desired:

```gremlin
g.V().hasLabel('Person').groupCount().by('country').unfold()
```

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphUnwind(input=[map_entries(current)], bind=[current], outer=[false])
    GraphGroupMap(key=[property(current, "country", policy=DropUnproductive)], value=[countBulk()], merge=[sum], output=[current])
      GraphBind(bind=[current], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
```

### 6.3 Group With Local Traversal Value

Gremlin:

```gremlin
g.V().hasLabel('Person').group().by('country').by(out('KNOWS').count())
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphGroupMap(key=[property(p, "country", policy=DropUnproductive)], value=[local_knows], merge=[collectValues], output=[current])
    GraphApply(kind=[Scalar], correlation=[p], outputs=[local_knows])
      left:
        GraphBind(bind=[p], kind=[Node])
          GraphNodeScan(graph=[default], labels=[Person])
      right:
        GraphAggregate(group=[], aggs=[local_knows=countBulk()], fields=[local_knows])
          GraphExpand(graph=[default], source=[p], target=[f], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
            GraphCorrelate(bindings=[p])
```

Cypher row aggregation with similar intent, but not the same output shape:

```cypher
MATCH (p:Person)
OPTIONAL MATCH (p)-[:KNOWS]->(f)
RETURN p.country AS country, count(f) AS knows
```

```text
GraphReturn(fields=[country, knows], resultForm=[RowSet])
  GraphAggregate(group=[country=property(p, "country", policy=NullOnMissing)], aggs=[knows=countRows(f)], fields=[country, knows])
    GraphApply(kind=[Optional], correlation=[p], outputs=[f], optionalMissing=[Null])
      left:
        GraphBind(bind=[p], kind=[Node])
          GraphNodeScan(graph=[default], labels=[Person])
      right:
        GraphExpand(graph=[default], source=[p], target=[f], targetMode=[BindNew], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
          GraphCorrelate(bindings=[p])
```

### 6.4 Distinct

Cypher:

```cypher
MATCH (p:Person)
RETURN DISTINCT p.country AS country
```

Initial logical plan:

```text
GraphReturn(fields=[country], resultForm=[RowSet])
  GraphDistinct(keys=[country], mode=[Row])
    GraphProject(mode=[PreserveVisible], exprs=[country=property(p, "country", policy=NullOnMissing)], fields=[country])
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
```

Gremlin:

```gremlin
g.V().hasLabel('Person').values('country').dedup()
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphDistinct(keys=[current], mode=[Traverser], bulk=[ResetToOne])
    GraphCurrentProject(expr=[current=property(current, "country", policy=DropUnproductive)])
      GraphBind(bind=[current], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
```

### 6.5 Order And Limit

Cypher:

```cypher
MATCH (p:Person)
RETURN p
ORDER BY p.age DESC
LIMIT 10
```

Initial logical plan:

```text
GraphReturn(fields=[p], resultForm=[RowSet])
  GraphSlice(offset=[0], fetch=[10], tail=[none])
    GraphSort(keys=[property(p, "age", policy=NullOnMissing) DESC NULLS FIRST])
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
```

Gremlin:

```gremlin
g.V().hasLabel('Person').order().by('age', desc).limit(10)
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphSlice(offset=[0], fetch=[10], tail=[none])
    GraphSort(keys=[property(current, "age", policy=ProviderDefined) DESC], missingOrder=[ProviderDefined])
      GraphBind(bind=[current], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
```

### 6.6 Tail

Gremlin:

```gremlin
g.V().hasLabel('Person').tail(5)
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphBarrier(partition=[], order=[], slice=[tail=5], materialize=[true])
    GraphBind(bind=[current], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

### 6.7 Local Barrier

Gremlin:

```gremlin
g.V().local(out('KNOWS').order().by('name').limit(2))
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphApply(kind=[Inner], correlation=[p], outputs=[current])
    left:
      GraphBind(bind=[p], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
    right:
      GraphBarrier(partition=[p], order=[property(current, "name", policy=ProviderDefined) ASC], slice=[offset=0, fetch=2], materialize=[true])
        GraphExpand(graph=[default], source=[p], target=[current], targetMode=[ReplaceCurrent], targetLabels=[any], relBinding=[anonymous], relTypes=[KNOWS], dir=[Out], length=[min=1, max=1])
          GraphCorrelate(bindings=[p])
```

### 6.8 SPARQL Group And Having

SPARQL:

```sparql
SELECT ?country (COUNT(?p) AS ?n) WHERE {
  ?p :country ?country .
}
GROUP BY ?country
HAVING (COUNT(?p) > 1)
```

Initial logical plan:

```text
GraphReturn(fields=[?country, ?n], resultForm=[RowSet])
  GraphFilter(condition=[>(?n, 1)])
    GraphAggregate(group=[?country], aggs=[?n=countRows(?p)], fields=[?country, ?n])
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:country)], object=[?country], outputs=[?p, ?country])
```

### 6.9 Gremlin Bulk-Sensitive Golden Tests

Gremlin:

```gremlin
g.V(1,1).count()
```

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphAggregate(group=[], aggs=[current=countBulk()], fields=[current])
    GraphValues(bindings=[current], rows=[[v1, _bulk=2]], fields=[current], hidden=[_bulk])
```

Gremlin:

```gremlin
g.V(1,1).dedup().count()
```

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphAggregate(group=[], aggs=[current=countBulk()], fields=[current])
    GraphDistinct(keys=[current], mode=[Traverser], bulk=[ResetToOne])
      GraphValues(bindings=[current], rows=[[v1, _bulk=2]], fields=[current], hidden=[_bulk])
```

## 7. Collections, Values, And Unnesting

### 7.1 Cypher UNWIND

Cypher:

```cypher
UNWIND [1, 2, 3] AS x
RETURN x
```

Initial logical plan:

```text
GraphReturn(fields=[x], resultForm=[RowSet])
  GraphUnwind(input=[list(1, 2, 3)], bind=[x], outer=[false])
    GraphOneRow()
```

### 7.2 Gremlin Inject

Gremlin:

```gremlin
g.inject(1, 2, 3)
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphValues(bindings=[current], rows=[[1], [2], [3]], fields=[current])
```

### 7.3 SPARQL VALUES

SPARQL:

```sparql
SELECT ?x WHERE {
  VALUES ?x { 1 2 3 }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?x], resultForm=[RowSet])
  GraphValues(bindings=[?x], rows=[[1], [2], [3]], fields=[?x])
```

### 7.4 Cypher List Comprehension

Cypher:

```cypher
MATCH (p:Person)
RETURN [x IN p.scores WHERE x > 10 | x * 2] AS scores
```

Initial logical plan:

```text
GraphReturn(fields=[scores], resultForm=[RowSet])
  GraphProject(mode=[PreserveVisible], exprs=[scores=list_comprehension(input=property(p, "scores", policy=NullOnMissing), item=x, filter=>(x, 10), map=*(x, 2))], fields=[scores])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

### 7.5 Gremlin Fold

Gremlin:

```gremlin
g.V().hasLabel('Person').values('name').fold()
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphAggregate(group=[], aggs=[current=collectTraversers(current)], fields=[current])
    GraphCurrentProject(expr=[current=property(current, "name", policy=DropUnproductive)])
      GraphBind(bind=[current], kind=[Node])
        GraphNodeScan(graph=[default], labels=[Person])
```

### 7.6 Gremlin Dedup Then Fold

Gremlin:

```gremlin
g.V().hasLabel('Person').values('name').dedup().fold()
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphAggregate(group=[], aggs=[current=collectTraversers(current)], fields=[current])
    GraphDistinct(keys=[current], mode=[Traverser], bulk=[ResetToOne])
      GraphCurrentProject(expr=[current=property(current, "name", policy=DropUnproductive)])
        GraphBind(bind=[current], kind=[Node])
          GraphNodeScan(graph=[default], labels=[Person])
```

### 7.7 Gremlin Fold And Unfold

Gremlin:

```gremlin
g.V().hasLabel('Person').values('name').fold().unfold()
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphUnwind(input=[current], bind=[current], outer=[false])
    GraphAggregate(group=[], aggs=[current=collectTraversers(current)], fields=[current])
      GraphCurrentProject(expr=[current=property(current, "name", policy=DropUnproductive)])
        GraphBind(bind=[current], kind=[Node])
          GraphNodeScan(graph=[default], labels=[Person])
```

## 8. SPARQL RDF Examples

### 8.1 Basic Graph Pattern

SPARQL:

```sparql
SELECT ?p ?name WHERE {
  ?p a :Person .
  ?p :name ?name .
}
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[SPARQL], resultForm=[RowSet], multiplicity=[Bag], optionalMissing=[Unbound], graphScope=[DefaultRdfGraph])

GraphReturn(fields=[?p, ?name], resultForm=[RowSet])
  GraphApply(kind=[Inner], correlation=[?p], outputs=[?name])
    left:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(rdf:type)], object=[iri(:Person)], outputs=[?p])
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:name)], object=[?name], outputs=[?name])
```

### 8.2 Multi-Triple RDF Pattern

SPARQL:

```sparql
SELECT ?name WHERE {
  ?p a :Person .
  ?p :knows ?f .
  ?f :name ?name .
}
```

Initial logical plan:

```text
GraphReturn(fields=[?name], resultForm=[RowSet])
  GraphApply(kind=[Inner], correlation=[?f], outputs=[?name])
    left:
      GraphApply(kind=[Inner], correlation=[?p], outputs=[?f])
        left:
          GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(rdf:type)], object=[iri(:Person)], outputs=[?p])
        right:
          GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:knows)], object=[?f], outputs=[?f])
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?f], predicate=[iri(:name)], object=[?name], outputs=[?name])
```

### 8.3 OPTIONAL

SPARQL:

```sparql
SELECT ?p ?name WHERE {
  ?p a :Person .
  OPTIONAL { ?p :name ?name . }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?p, ?name], resultForm=[RowSet])
  GraphApply(kind=[Optional], correlation=[?p], outputs=[?name], optionalMissing=[Unbound])
    left:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(rdf:type)], object=[iri(:Person)], outputs=[?p])
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:name)], object=[?name], outputs=[?name])
```

### 8.4 MINUS

SPARQL:

```sparql
SELECT ?p WHERE {
  ?p a :Person .
  MINUS { ?p :blocked true . }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?p], resultForm=[RowSet])
  GraphSparqlMinus(compatible=[sharedVariables], shared=[?p])
    left:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(rdf:type)], object=[iri(:Person)], outputs=[?p])
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:blocked)], object=[literal(true)], outputs=[?p])
```

`GraphSparqlMinus` must survive initial planning. It may be lowered to anti-join only when compatibility analysis proves equivalence.

### 8.5 FILTER EXISTS

SPARQL:

```sparql
SELECT ?p WHERE {
  ?p a :Person .
  FILTER EXISTS { ?p :name ?name . }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?p], resultForm=[RowSet])
  GraphApply(kind=[Semi], correlation=[?p], outputs=[])
    left:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(rdf:type)], object=[iri(:Person)], outputs=[?p])
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:name)], object=[?name], outputs=[?name])
```

### 8.6 FILTER NOT EXISTS With No Shared Variables

SPARQL:

```sparql
SELECT ?s WHERE {
  ?s ?p ?o .
  FILTER NOT EXISTS { ?x ?y ?z . }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?s], resultForm=[RowSet])
  GraphApply(kind=[Anti], correlation=[], outputs=[])
    left:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?s], predicate=[?p], object=[?o], outputs=[?s, ?p, ?o])
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?x], predicate=[?y], object=[?z], outputs=[?x, ?y, ?z])
```

### 8.7 MINUS With No Shared Variables

SPARQL:

```sparql
SELECT ?s WHERE {
  ?s ?p ?o .
  MINUS { ?x ?y ?z . }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?s], resultForm=[RowSet])
  GraphSparqlMinus(compatible=[sharedVariables], shared=[])
    left:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?s], predicate=[?p], object=[?o], outputs=[?s, ?p, ?o])
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?x], predicate=[?y], object=[?z], outputs=[?x, ?y, ?z])
```

This differs from `FILTER NOT EXISTS`: with no shared variables, `MINUS` does not eliminate left-side bindings solely because the right pattern has matches.

### 8.8 Named Graph

SPARQL:

```sparql
SELECT ?s ?o WHERE {
  GRAPH :g1 { ?s :knows ?o . }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?s, ?o], resultForm=[RowSet])
  GraphRdfQuadScan(dataset=[queryDataset], graphScope=[NamedGraph(iri(:g1))], subject=[?s], predicate=[iri(:knows)], object=[?o], outputs=[?s, ?o])
```

### 8.9 GRAPH Variable

SPARQL:

```sparql
SELECT ?g ?s WHERE {
  GRAPH ?g { ?s :status "active" . }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?g, ?s], resultForm=[RowSet])
  GraphRdfQuadScan(dataset=[queryDataset], graphScope=[NamedGraphVariable(?g)], subject=[?s], predicate=[iri(:status)], object=[literal("active")], outputs=[?g, ?s])
```

`NamedGraphVariable(?g)` ranges over named graphs in the dataset.

### 8.10 BIND

SPARQL:

```sparql
SELECT ?lower WHERE {
  ?p :name ?name .
  BIND(LCASE(?name) AS ?lower)
}
```

Initial logical plan:

```text
GraphReturn(fields=[?lower], resultForm=[RowSet])
  GraphProject(mode=[PreserveVisible], exprs=[?lower=lcase(?name)], fields=[?lower], errorPolicy=[UnboundOnExpressionError])
    GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:name)], object=[?name], outputs=[?p, ?name])
```

### 8.11 ASK

SPARQL:

```sparql
ASK { ?p a :Person . }
```

Initial logical plan:

```text
GraphAsk(field=[ask])
  GraphAggregate(group=[], aggs=[ask=countRows() > 0], fields=[ask])
    GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(rdf:type)], object=[iri(:Person)], outputs=[?p])
```

### 8.12 CONSTRUCT

SPARQL:

```sparql
CONSTRUCT { ?p :label ?name . }
WHERE { ?p :name ?name . }
```

Initial logical plan:

```text
GraphConstructTriples(template=[(?p, iri(:label), ?name)])
  GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:name)], object=[?name], outputs=[?p, ?name])
```

### 8.13 SERVICE

SPARQL:

```sparql
SELECT ?p WHERE {
  SERVICE <https://example.org/sparql> { ?p a :Person . }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?p], resultForm=[RowSet])
  GraphService(endpoint=[iri(https://example.org/sparql)], silent=[false], outputs=[?p])
    GraphRdfQuadScan(dataset=[serviceDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(rdf:type)], object=[iri(:Person)], outputs=[?p])
```

### 8.14 UNION

SPARQL:

```sparql
SELECT ?s WHERE {
  { ?s :p ?o }
  UNION
  { ?s :q ?o }
}
```

Initial logical plan:

```text
GraphReturn(fields=[?s], resultForm=[RowSet])
  GraphUnion(all=[true], align=[ByVariableName])
    left:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?s], predicate=[iri(:p)], object=[?o], outputs=[?s, ?o])
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?s], predicate=[iri(:q)], object=[?o], outputs=[?s, ?o])
```

## 9. Procedure, Function, And Extension Examples

### 9.1 Cypher CALL

Cypher:

```cypher
CALL db.labels() YIELD label
RETURN label
```

Initial logical plan:

```text
GraphReturn(fields=[label], resultForm=[RowSet])
  GraphProcedureCall(name=[db.labels], args=[], yields=[label], mode=[Read])
```

### 9.2 Gremlin call

Gremlin:

```gremlin
g.call('pagerank').with('iterations', 20)
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphProcedureCall(name=[pagerank], args=[map{iterations: 20}], yields=[current], mode=[Read])
```

### 9.3 Custom Function In Predicate

Cypher:

```cypher
MATCH (p:Person)
WHERE distance(p.location, point({x: 0, y: 0})) < 10
RETURN p
```

Initial logical plan:

```text
GraphReturn(fields=[p], resultForm=[RowSet])
  GraphFilter(condition=[<(distance(property(p, "location", policy=NullOnMissing), point(0, 0)), 10)])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

## 10. Language-Specific Semantics That Become Policies

### 10.1 Cypher Missing Property

Cypher:

```cypher
MATCH (p:Person)
RETURN p.missing AS x
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Cypher], propertyMissing=[NullOnMissing])

GraphReturn(fields=[x], resultForm=[RowSet])
  GraphProject(mode=[PreserveVisible], exprs=[x=property(p, "missing", policy=NullOnMissing)], fields=[x])
    GraphBind(bind=[p], kind=[Node])
      GraphNodeScan(graph=[default], labels=[Person])
```

### 10.2 SPARQL OPTIONAL And Unbound

SPARQL:

```sparql
SELECT ?x WHERE {
  OPTIONAL { ?p :missing ?x . }
}
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[SPARQL], optionalMissing=[Unbound])

GraphReturn(fields=[?x], resultForm=[RowSet])
  GraphApply(kind=[Optional], correlation=[], outputs=[?x], optionalMissing=[Unbound])
    left:
      GraphOneRow()
    right:
      GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:missing)], object=[?x], outputs=[?p, ?x])
```

### 10.3 Gremlin Unproductive Property

Gremlin:

```gremlin
g.V().values('missing')
```

Initial logical plan:

```text
Policy: GraphPlanPolicy(language=[Gremlin], propertyMissing=[DropUnproductive])

GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphCurrentProject(expr=[current=property(current, "missing", policy=DropUnproductive)])
    GraphBind(bind=[current], kind=[Node])
      GraphNodeScan(graph=[default], labels=[any])
```

### 10.4 Bags, Sets, And Bulk

Gremlin:

```gremlin
g.V().barrier().count()
```

Initial logical plan:

```text
GraphReturn(fields=[current], resultForm=[TraverserStream])
  GraphAggregate(group=[], aggs=[current=countBulk()], fields=[current])
    GraphBarrier(partition=[], order=[], slice=[none], materialize=[true], bulkPolicy=[PreserveAndMerge])
      GraphBind(bind=[current], kind=[Node])
        GraphNodeScan(graph=[default], labels=[any])
```

Bulk is hidden metadata. Do not project `_bulk` into the visible row schema unless a traversal explicitly asks for bulk or a diagnostic plan view requests hidden state.

### 10.5 SPARQL Unbound Filter

SPARQL:

```sparql
SELECT ?p WHERE {
  ?p a :Person .
  OPTIONAL { ?p :name ?name . }
  FILTER(!BOUND(?name))
}
```

Initial logical plan:

```text
GraphReturn(fields=[?p], resultForm=[RowSet])
  GraphFilter(condition=[not(is_bound(?name))])
    GraphApply(kind=[Optional], correlation=[?p], outputs=[?name], optionalMissing=[Unbound])
      left:
        GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(rdf:type)], object=[iri(:Person)], outputs=[?p])
      right:
        GraphRdfQuadScan(dataset=[queryDataset], graphScope=[DefaultGraph], subject=[?p], predicate=[iri(:name)], object=[?name], outputs=[?name])
```

## 11. High-Level Graph IR Node Catalog

These are logical operator families for the initial Graph IR. Names here are descriptive; implementation names can differ, but the operator boundaries should remain atomic.

### Source Nodes

- `GraphNodeScan(graph, labels | labelsExpr)`: scans property-graph nodes for a graph and label expression.
- `GraphRelScan(graph, types, dir)`: scans property-graph relationships for a graph and type set.
- `GraphBind(bind, kind, expr)`: assigns a binding id to the current graph element or to a component expression.
- `GraphRdfQuadScan(dataset, graphScope, subject, predicate, object, outputs)`: scans RDF triples/quads in a query dataset and graph scope. Terms may be constants, newly bound variables, or variables provided by correlation.
- `GraphValues(bindings, rows, fields, hidden)`: inline relation of graph values.
- `GraphOneRow()`: single empty row source.
- `GraphEmpty()`: zero-row source.
- `GraphCorrelate(bindings)`: exposes outer bindings inside a correlated input when the subtree needs an explicit source row.

### Pattern Nodes

- `GraphExpand(graph, source, target, targetMode, targetLabels, relBinding, relTypes, dir, length, path, pathMode, matchMode, pathMaterialization)`: traverses from a source node to a target node.
- `GraphPathPattern(graph, path, selector, pathMode, matchMode, endpoints, parts, pathMaterialization)`: represents a full property-graph path expression.
- `GraphRepeat(seed, body, emit, times, until, path, pathObjects, prefixPredicate)`: represents Gremlin-style repeat loops, including prefix-pruning path predicates.
- `GraphRdfPropertyPath(dataset, graphScope, subject, object, path, pathMaterialization, zeroLength)`: represents SPARQL property path syntax and RDF term semantics.
- `GraphPathFilter(condition, scope)`: applies path-specific predicates such as simple, cyclic, or acyclic checks.

### Row Algebra Nodes

- `GraphFilter(condition)`: filters rows.
- `GraphProject(mode, exprs, fields, errorPolicy)`: computes output values and controls visible result fields.
- `GraphCurrentProject(expr, fields)`: computes Gremlin current-object values and may drop unproductive traversers.
- `GraphAggregate(group, aggs, fields)`: groups rows and computes aggregates.
- `GraphGroupMap(key, value, merge, output)`: constructs Gremlin map-valued group or groupCount results.
- `GraphDistinct(keys, mode, bulk)`: removes duplicate rows, solution mappings, or traversers. Gremlin `dedup()` resets bulk to one.
- `GraphSort(keys, missingOrder)`: orders rows or traversers.
- `GraphSlice(offset, fetch, tail)`: applies offset, limit, or tail semantics.
- `GraphJoin(kind, condition)`: combines uncorrelated row streams.
- `GraphApply(kind, correlation, outputs)`: correlated apply, including inner, optional, semi, anti, scalar, exists, and not-exists forms.
- `GraphUnion(all, align)`: combines compatible row streams.
- `GraphBarrier(partition, order, slice, materialize, bulkPolicy)`: expresses stream materialization and local partitioned order/slice behavior.

### Collection Nodes

- `GraphUnwind(input, bind, outer)`: expands a list or map-entry value into rows/traversers.
- `GraphListComprehension(input, item, filter, map)`: expression-level list mapping and filtering when represented as a node.
- `GraphQuantifier(kind, input, predicate)`: all, any, none, and single style collection predicates.
- `GraphCollect(value, distinct, order)`: collection construction when not represented as an aggregate function.

### Language-Shaped Nodes

- `GraphCoalesce(arms, success, output, armOutputs)`: Gremlin first-success branch.
- `GraphChoose(condition, arms, defaultArm, output, unmatchedPolicy)`: Gremlin conditional or switch branch.
- `GraphSelect(labels, output)`: Gremlin `select()` label materialization.
- `GraphSparqlMinus(compatible, shared)`: SPARQL `MINUS` with compatibility and disjoint-domain semantics.
- `GraphService(endpoint, silent, outputs)`: SPARQL federated pattern.
- `GraphProcedureCall(name, args, yields, mode)`: Cypher/GQL procedure calls and Gremlin `call`.
- `GraphExtension(name, inputs, metadata)`: explicit escape hatch for a language or backend-specific operation not represented by shared nodes.

### Output Nodes

- `GraphReturn(fields, resultForm)`: read-query or traversal output.
- `GraphConstructTriples(template)`: SPARQL `CONSTRUCT` output.
- `GraphDescribe(terms)`: SPARQL `DESCRIBE` output request.
- `GraphAsk(field)`: SPARQL boolean output.

## 12. Next Batch Golden Tests

These examples should receive full plans in the next pass.

### Gremlin

```gremlin
g.V().match(
  __.as('a').out('knows').as('b'),
  __.as('b').out('created').as('c')
).select('a','c')
```

```gremlin
g.V().repeat(out().simplePath()).emit().until(hasLabel('Target')).path()
```

```gremlin
g.V().branch(label()).option('person', values('name')).option(none, constant('unknown'))
```

```gremlin
g.V().properties('name').valueMap()
```

```gremlin
g.V().project('name','degree').by('name').by(out().count())
```

### Cypher / GQL

```cypher
CALL {
  WITH p
  MATCH (p)-[:KNOWS]->(f)
  RETURN count(f) AS degree
}
RETURN p, degree
```

```cypher
MATCH (p:Person)
OPTIONAL MATCH (p)-[:WORKS_AT]->(c)
MATCH (p)-[:KNOWS]->(f)
RETURN p, c, f
```

```gql
MATCH p = SIMPLE (a)-[:KNOWS]->+(b)
RETURN p
```

```gql
MATCH p = ANY 3 SHORTEST (a)-[:KNOWS]->+(b)
RETURN p
```

```gql
FROM GRAPH social MATCH (p:Person) RETURN p
```

### SPARQL

```sparql
SELECT ?s WHERE {
  ?s :p ?o .
  FILTER NOT EXISTS { ?x ?y ?z }
}
```

```sparql
SELECT ?s WHERE {
  ?s :p ?o .
  MINUS { ?x ?y ?z }
}
```

```sparql
SELECT ?x ?y WHERE {
  ?x (:p|:q)+ ?y .
}
```

```sparql
SELECT ?s WHERE {
  GRAPH ?g { ?s ?p ?o }
  FILTER(?g = :g1)
}
```

```sparql
CONSTRUCT { _:b :label ?name }
WHERE { ?p :name ?name }
```

### Cross-cutting features

- RDF-star / quoted triples and property-graph mappings for statement properties.
- Temporal, spatial, vector, full-text, and user-defined scalar functions across languages.
- Graph selection and multi-graph queries in GQL and provider-specific Cypher.
- Gremlin side effects, sacks, and traversal strategies that change visible execution while preserving logical results.

## 13. Reference Notes

This draft aligns the examples with the following language-level assumptions:

- Cypher/GQL-style path examples separate path mode (`WALK`, `TRAIL`, `ACYCLIC`) from match mode (`DIFFERENT RELATIONSHIPS`, `REPEATABLE ELEMENTS`).
- Cypher default graph pattern matching does not allow a relationship to be traversed more than once in a given `MATCH` result, while nodes may repeat unless a stricter path mode is used.
- Cypher null ordering is language-defined: nulls sort last in ascending order and first in descending order.
- TinkerPop Gremlin uses traversers and traverser bulk; `count()` observes represented traversers rather than merely enumerated traverser objects.
- SPARQL solution mappings can contain unbound variables; `MINUS` and `FILTER NOT EXISTS` are not interchangeable in all cases; RDF graph scope is determined by the active graph and `GRAPH` clauses.
