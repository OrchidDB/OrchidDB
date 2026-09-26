# Verification and evidence

The source of public conformance numbers is the committed data under
[`conformance/upstream-results`](../conformance/upstream-results), with pinned
catalogs and source provenance under [`conformance/upstream`](../conformance/upstream).
The [runner guide](../conformance/README.md) documents reproduction and reporting.
The mdBook generates the public comparison from those artifacts.

Keep execution profiles separate. Managed-runtime language results, native JVM
provider tests, SQL-only compiler checks and supplemental regressions are not
interchangeable. Do not count skips, placeholders, unsupported operations or
planning-only checks as successful query execution. Historical per-profile
results must not be combined into a synthetic pass total.

## Local checks

```sh
cargo test --locked --test sql_compiler --test execution
cargo run --locked --example compile_sql
cargo test --locked --features duckdb --test mapped_engine
bash conformance/run-relational-gremlin.sh
```

The final command needs Java 21 and the production JVM classpath; see
[runtime verification](runtime.md#local-verification). Full conformance is run
locally; publishing committed reports does not rerun the suite.

`cases/` is executable corpus input and remains in the repository. `jvm/` and
`jvm-codecs/` are production modules, not copies of the standalone Java client.
`src/spargebra` embeds the modified SPARQL parser and its upstream license notices. Keep all of them.

## Performance

[`performance/`](performance/) retains before/after measurements, revisions,
binary identities and per-scenario timing. These are historical measurements,
not current performance promises. Compare the same build profile, unchanged
assertions and deadlines, one engine instance, and no concurrent compilation.
Report full-suite wall time separately from passed-scenario totals.

## Historical records

Superseded handoffs, plans and `conformance/legacy` probes are preserved in
[the pre-cleanup revision](https://github.com/OrchidDB/OrchidDB/tree/374977f985869f795ef628e2e37c566dafefe19a).
A local archive was also saved to
`~/orchiddb/archives/orchiddb-maintainer-history-374977f.tar.gz` with a SHA-256 sidecar.
Those probes are not part of the active conformance denominator.
