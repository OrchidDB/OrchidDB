# Conformance comparison

Compare Cypher with Neo4j Community and PuppyGraph, Gremlin with SQLg, PuppyGraph and JanusGraph, and SPARQL with Apache Jena.
The recorded percentages describe the optional managed runtime and the tested interfaces. They do not establish full language conformance for the newer SQL compiler clients, which reject writes, remote SERVICE calls, and operations that cannot lower to SQL. See [compiler boundaries](sql-compiler.md#boundaries).

The recorded results are historical evidence for their recorded source revisions.
They are not a fresh run of the current code. Current compiler and managed runtime
APIs reject SPARQL `SERVICE`, including `SERVICE SILENT`; OrchidDB does not issue
remote SPARQL HTTP requests.

## Upstream compatibility evidence

This comparison uses independently maintained test scenarios and expected results. It records the original upstream identifiers, fixtures, source revisions, expected output, actual output and diagnostics. Engines are grouped by supported language, using free editions and marking paid capabilities separately. The SPARQL peer, [Apache Jena](https://jena.apache.org/), uses the permissive [Apache 2.0 license](https://github.com/apache/jena/blob/jena-6.2.0/LICENSE).

All upstream scenarios appear in the report, including cases that were skipped or could not be executed. Passes, failures, unsupported interfaces, adapter limitations and timeouts are distinct outcomes. Timing records local scenario wall time and available query or step measurements. Everything runs locally; GitHub Actions only builds and publishes the static documentation and committed evidence. Download the full catalog and result files for reproducibility and investigation.

## Comparison report

[Open the full conformance report](conformance-report.html) for suite totals,
feature matrices, per-scenario results, timings, and downloadable JSON and CSV
evidence. The report is generated from committed results; building the docs
does not run the test suites.

The interactive report is separate from this book so the guides remain small
and easy to search. Its tables and downloads also work without JavaScript.

## Cypher

<span id="language-opencypher"></span>

[Inspect the openCypher results](conformance-report.html#language-opencypher).

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
