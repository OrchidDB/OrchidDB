# SPARQL coverage benchmark

This benchmark measures two independent boundaries over W3C SPARQL query
files:

1. Parser acceptance through the Oxigraph `spargebra` parser.
2. Crabgraph Graph IR planner acceptance among parsed queries.

It does not count a query as planned merely because the parser accepts it.
It also does not claim W3C conformance because this benchmark does not load
each manifest dataset and compare its expected result.

## Reproduce the August 2026 snapshot

The source corpus is `w3c/rdf-tests` commit
`369a90d1a60c021b746df2e411da0ff36258a758`, dual-licensed under the W3C Test
Suite License and W3C 3-clause BSD License. No corpus files are copied into
this repository.

```sh
git clone https://github.com/w3c/rdf-tests.git
git -C rdf-tests checkout 369a90d1a60c021b746df2e411da0ff36258a758
```

Run the benchmark over the SPARQL 1.0 query-evaluation collections, excluding
the five syntax-only directories, and selected SPARQL 1.1 query collections:

```sh
cargo run --example sparql_coverage -- \
  rdf-tests/sparql/sparql10/algebra \
  rdf-tests/sparql/sparql10/ask \
  rdf-tests/sparql/sparql10/basic \
  rdf-tests/sparql/sparql10/bnode-coreference \
  rdf-tests/sparql/sparql10/boolean-effective-value \
  rdf-tests/sparql/sparql10/bound \
  rdf-tests/sparql/sparql10/cast \
  rdf-tests/sparql/sparql10/construct \
  rdf-tests/sparql/sparql10/dataset \
  rdf-tests/sparql/sparql10/distinct \
  rdf-tests/sparql/sparql10/expr-builtin \
  rdf-tests/sparql/sparql10/expr-equals \
  rdf-tests/sparql/sparql10/expr-ops \
  rdf-tests/sparql/sparql10/graph \
  rdf-tests/sparql/sparql10/i18n \
  rdf-tests/sparql/sparql10/open-world \
  rdf-tests/sparql/sparql10/optional \
  rdf-tests/sparql/sparql10/optional-filter \
  rdf-tests/sparql/sparql10/reduced \
  rdf-tests/sparql/sparql10/regex \
  rdf-tests/sparql/sparql10/solution-seq \
  rdf-tests/sparql/sparql10/sort \
  rdf-tests/sparql/sparql10/triple-match \
  rdf-tests/sparql/sparql10/type-promotion \
  rdf-tests/sparql/sparql11/aggregates \
  rdf-tests/sparql/sparql11/bind \
  rdf-tests/sparql/sparql11/exists \
  rdf-tests/sparql/sparql11/grouping \
  rdf-tests/sparql/sparql11/negation \
  rdf-tests/sparql/sparql11/project-expression \
  rdf-tests/sparql/sparql11/property-path \
  rdf-tests/sparql/sparql11/service \
  rdf-tests/sparql/sparql11/subquery
```

## August 2026 result

| Boundary | Result |
| --- | ---: |
| Parser acceptance | 408 / 425 (96.0%) |
| Graph IR planner coverage among parsed queries | 408 / 408 (100.0%) |
| Graph IR planner coverage overall | 408 / 425 (96.0%) |
| Plans without extension nodes | 346 / 408 parsed (84.8%) |

The planner covers filters, optional patterns, named graphs, values, property
paths, datasets, services, subqueries, and all four query forms. Aggregate,
`REDUCED`, and dataset boundaries may use explicit Graph IR extension nodes.
The extension count is reported separately so planner acceptance does not
masquerade as native execution coverage.

SPARQL execution over relational data uses two mappings. Ontology metadata
resolves vocabulary IRIs to property-graph labels, relationship types, and
properties. The existing graph schema mapping then resolves those concepts to
user-owned tables, queries, and views. Crabgraph does not require or provide an
RDF triples or quads storage adapter.
