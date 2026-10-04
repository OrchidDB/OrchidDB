# Conformance comparison

Compare Cypher with Neo4j Community, PuppyGraph and ArcadeDB, Gremlin with SQLg, PuppyGraph, JanusGraph and ArcadeDB, and SPARQL with Apache Jena.
The recorded percentages apply to the exact interfaces, revisions, and fixtures listed in the evidence. They are not a blanket capability guarantee for every API or SQL dialect. SQL emission rejects writes and operations that cannot lower to SQL; remote SERVICE calls are unsupported throughout OrchidDB. See [SQL API boundaries](sql-compiler.md#boundaries).

OrchidDB results include the complete optimized release run: 6,382 passes, no failures, and unchanged exclusions. Peer results retain their recorded source revisions. Current compiler and managed runtime
APIs reject SPARQL `SERVICE`, including `SERVICE SILENT`; OrchidDB does not issue
remote SPARQL HTTP requests.

## Upstream compatibility evidence

This comparison uses independently maintained test scenarios and expected results. It records the original upstream identifiers, fixtures, source revisions, outcomes and available diagnostics. Engines are grouped by supported language, using free editions and marking paid capabilities separately. The SPARQL peer, [Apache Jena](https://jena.apache.org/), uses the permissive [Apache 2.0 license](https://github.com/apache/jena/blob/jena-6.2.0/LICENSE).

All upstream scenarios appear in the report, including cases that were skipped or could not be executed. Passes, failures, unsupported interfaces, adapter limitations and timeouts are distinct outcomes. Timing records local scenario wall time and available query or step measurements. Everything runs locally; GitHub Actions only builds and publishes the static documentation and committed evidence. Download the full catalog and result files for reproducibility and investigation.

## Comparison report

[Open the full conformance report](conformance-report.html) for suite totals,
feature matrices, per-scenario results, query costs, timings, and downloadable JSON and CSV
evidence. The report is generated from committed results; building the docs
does not run the test suites.

The interactive report is separate from this book so the guides remain small
and easy to search. Its tables and downloads also work without JavaScript.

## Cypher

<span id="language-opencypher"></span>

[Inspect the openCypher results](conformance-report.html#language-opencypher).

**ArcadeDB comparison.** We reproduced its published 97.8% result on 26.9.1;
our harness records 91.9%. The difference comes from error checks, identity-based
side-effect checks, path representations, fixture formatting and exclusions.
Its harness accepts any exception for expected errors and any list as a path;
our error-category mapping and path normalization also need qualification.
Nineteen failures occur before the tested query because its embedded API rejects
a fixture's trailing semicolon; all 19 pass with only that terminator removed.
These are fixture/API compatibility failures, not 19 demonstrated query defects.
[Read the scenario-level reconciliation](downloads/conformance/arcadedb-reconciliation.json).

## Gremlin

<span id="language-tinkerpop"></span>

[Inspect the TinkerPop results](conformance-report.html#language-tinkerpop).

## SPARQL

<span id="language-rdf"></span>

[Inspect the W3C SPARQL results](conformance-report.html#language-rdf).

## Reproduce a run

See the [conformance runner documentation](https://github.com/OrchidDB/OrchidDB/tree/main/conformance)
for the pinned sources, adapters, and local commands. Read the scope and method
alongside each result; passing an imported corpus is not standards certification.

## Historical evidence

Project identifiers and local paths in the downloadable records were normalized
on 2026-09-25. Outcomes and timings remain those of the recorded runs; the name
change is not a new conformance run. Original source and binary hashes remain
unchanged. Where local paths changed a case definition, a separate
`normalized_case_sha256` identifies the displayed definition while
`case_sha256` retains its original value. The original records are available
in [the source snapshot](https://github.com/OrchidDB/OrchidDB/tree/b4c6909283114bfd77008de92929c428fc700062/conformance).

## Query cost

[Query cost in the report](conformance-report.html#query-cost) ranks OrchidDB
scenarios by measured work and shows measurement coverage. Each scenario includes
its cost; compact JSON evidence includes aggregate costs and coverage, and the CSV
includes metric version, coverage, work units and query request time.

Version 1 adds SQL output rows, native input/output rows, source rows, SQL output
memory in rounded-up KiB, and 100 units per SQL execution or source request.
It measures work at execution boundaries, not internal database scans or all
DataFusion operators. Partial costs are lower bounds; unmeasured work is shown
explicitly. Fixture and observation queries are excluded from rankings.
Costs do not change pass/fail outcomes or compare performance between products.
Gremlin attribution version 2 excludes graph initializers, parameter lookups and
assertion-helper requests. The published costs come from the new complete release run; comparisons use the
prior recorded requests with corrected helper attribution.

The first measured run provides a baseline. Use `--cost-baseline` with the local
conformance runner to compare later runs with matching case definitions, query
text, metric version and complete coverage.
