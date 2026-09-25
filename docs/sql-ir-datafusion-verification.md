# SQL IR / DataFusion migration verification

Verified locally on 2026-09-24 (macOS arm64, Java 21).

## Tested implementation

- Commit: `8fc43b8beb4d46cec1e0dfd9f19eae5ba737695b`.
- Working tree was clean when the upstream run captured provenance.
- Native runner SHA-256: `48c82a857c9d6ce4bdbb83e1b554aa33aeb47f2eac7097cafe59c5bf83960865`.
- Architecture: [relational DAG execution](sql-ir-datafusion-execution.md).

Managed execution lowers frontend Graph IR to relational IR. DuckDB executes
eligible SQL regions; DataFusion executes residual relational operators and
native JVM kernels. Managed queries do not fall back to the Graph IR interpreter.

## Local results

| Suite | Passed | Failed | Skipped / unsupported |
| --- | ---: | ---: | --- |
| Focused Rust regression and integration tests | 99 | 0 | 0 |
| Production Java provider tests | 63 | 0 | 0 |
| Python upstream adapter tests | 19 | 0 | 0 |
| Pinned Apache TinkerPop native Gremlin scenarios | 1,432 | 0 | 78 skipped; 1 unsupported |

The upstream run recorded all 1,511 unique catalog scenarios, using unmodified
Apache assertion code from TinkerPop 3.7.4, revision
`fa698ba2aba8967dcd17eb61cb13648b934fab5b`. Every scenario retained its previous
status. This is migration verification, not an increase in conformance passes.
The remaining unsupported native scenario uses a remote inline lambda.

The Rust suites cover engine transactions, storage integrity, functions,
incremental updates, shared side effects, group finalization, upstream
regressions, relational scheduling and execution, and JVM integration (including
normally ignored JVM tests). These are focused suites, not a claim that every
historical repository test passes.

## Reproduction and evidence

```sh
JAVA_HOME=/path/to/jdk-21 bash conformance/run-relational-gremlin.sh
python3 -m pip install -r conformance/requirements.txt
python3 -m unittest discover -s conformance/upstream -p 'test_*.py'
python3 conformance/validate.py
```

The complete native result record, including per-step timings, actual results,
case fingerprints, exclusions, and binary provenance, is committed at
[`conformance/upstream-results/orchiddb-tinkerpop.json`](../conformance/upstream-results/orchiddb-tinkerpop.json).

Existing JVM and GraphComputer evidence remains separately recorded and feeds
the existing combined OrchidDB comparison. The production page layout and
leaderboard calculation are unchanged. GitHub Actions only builds and publishes
static documentation and committed evidence; all tests run locally.
