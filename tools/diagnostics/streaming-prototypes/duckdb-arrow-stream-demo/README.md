# DuckDB consuming a live Arrow stream from Rust

A standalone example, independent of OrchidDB and DataFusion. A simulated remote
search produces Arrow batches **when DuckDB requests them**. DuckDB joins those
matches to a local document table and filters/sorts them using SQL.

```text
Six source IDs (two per request)
    → simulated batched search, invoked by reader.next()
    → Arrow C Stream: source_id, document_id, score
    → DuckDB stream-backed view: remote_matches
    → SQL JOIN documents / WHERE score >= 0.8 / ORDER BY source_id
```

## Run

With Rust installed, download the pinned DuckDB 1.5.5 library and run:

```sh
DUCKDB_DOWNLOAD_LIB=1 cargo run --locked --no-default-features
DUCKDB_DOWNLOAD_LIB=1 cargo test --locked --no-default-features -- --nocapture
```

These are the commands used to validate this example on macOS ARM64. Both tests
pass. The first build needs network access to download dependencies and DuckDB.

Or build DuckDB from source (first build compiles C++):

```sh
cargo run --locked
cargo test --locked -- --nocapture
```

Alternatively, use an installed DuckDB 1.5.x shared library. On Apple Silicon
with Homebrew DuckDB installed:

```sh
DUCKDB_LIB_DIR=/opt/homebrew/lib cargo run --locked --no-default-features
DUCKDB_LIB_DIR=/opt/homebrew/lib cargo test --locked --no-default-features -- --nocapture
```

On other systems, set `DUCKDB_LIB_DIR` to the directory containing your compatible
shared library, and configure your platform's shared-library loader if needed.

Expected output (diagnostic messages go to stderr):

```text
DuckDB v1.5.5
Registered stream: 0 remote requests. Executing SQL now.
remote request 1: search sources [1, 2] (DuckDB requested a batch)
remote request 2: search sources [3, 4] (DuckDB requested a batch)
remote request 3: search sources [5, 6] (DuckDB requested a batch)
1 | Arrow streaming | 0.95
2 | Arrow streaming | 0.90
3 | Arrow streaming | 0.95
4 | Arrow streaming | 0.90
5 | Arrow streaming | 0.95
6 | Arrow streaming | 0.90
```

## What to look at

`src/main.rs` contains the entire example:

- `RemoteMatches::next()` performs one mock request for two sources and yields
  one `RecordBatch`. Replace its fixture with a remote API request and decoding.
- `FFI_ArrowArrayStream` exports the reader through the Arrow C Stream interface.
- `duckdb_arrow_scan` registers a view over that stream. It does **not** populate
  a temporary table. An assertion verifies zero requests during registration.
- The SQL query triggers three batched requests, then joins and filters the hits.
- Tests check exact SQL results and propagation of a failure in the second batch.

## Scope and tradeoffs

This is a synchronous, pull-based producer: no background worker, queue, network
credentials, Python, or host scalar UDF. It constructs only the requested batch
at a time. DuckDB can retain batches or operator state internally; this does not
promise a fixed memory bound for the entire query. A production asynchronous
producer could use a bounded channel with cancellation and backpressure.

The six source IDs stand in for source SQL output; this example demonstrates the
remote-to-DuckDB input boundary, not a full SQL-to-remote-to-SQL orchestrator.
Each mock source returns two hits, retaining its ID for correlation. Real API
responses still need decoding into Arrow; this is not a claim of zero-copy
network ingestion.

The stream is consumed once. It is kept alive until the DuckDB connection is
closed. Repeated scans need a fresh producer or explicit buffering; do not reuse
this view for a second query expecting replay. The final `ORDER BY` is blocking,
and the C API `duckdb_query` materializes the **final output**. Input delivery is
streamed, but this example does not demonstrate incremental output delivery.

The small demo deliberately uses the DuckDB 1.5 C API `duckdb_arrow_scan` bridge,
which is marked deprecated upstream. Dependencies are pinned. For the production
OrchidDB extension, implement the same pull contract through its DuckDB table
function/Arrow scan integration rather than building a new public dependency on
this deprecated entry point.

An initial test against the installed DuckDB 1.0.0 library exposed a legacy
bridge filter-pushdown issue: predicates were not applied to this reader. The
demo conservatively disables `filter_pushdown`
on its private connection so predicates remain in DuckDB. The result test caught
incorrect, unfiltered rows without this setting. A production table function
should declare its actual pushdown capabilities instead of requiring this
connection setting.

References:

- [DuckDB: SQL on Arrow](https://duckdb.org/docs/stable/guides/python/sql_on_arrow)
- [DuckDB Rust bindings](https://github.com/duckdb/duckdb-rs)
- [Arrow C Stream interface](https://arrow.apache.org/docs/format/CStreamInterface.html)

## Measured benchmark

The benchmark adds a generated Arrow source, a bounded concurrent producer, and
SQL scan/join/materialization variants. Run on an Apple M3 Pro (12 logical CPUs,
18 GiB RAM), DuckDB 1.5.5, optimized Rust build. Each case runs three times in a
fresh process; the tables below use medians. Cases are shuffled within each
sweep. Follow-up tuning and larger-scale sweeps were run separately.

```sh
DUCKDB_DOWNLOAD_LIB=1 cargo build --release --locked --no-default-features
python3 tools/bench.py
```

The runner uses macOS `/usr/bin/time -l` for whole-process peak RSS. Re-running
replaces the result files; `--resume` retains completed case/repetition pairs.
Only use resume with the same benchmark binary and workload definition.

| Workload | Input rows | Median seconds | Million input rows/s | Peak RSS MiB |
|---|---:|---:|---:|---:|
| Default join, 4-row batches, 1 SQL thread | 1 million | 0.391 | 2.56 | 48.8 |
| Default join, 8,192-row batches, 1 SQL thread | 1 million | 0.0159 | 63.07 | 49.1 |
| Default join, 8,192-row batches, 1 SQL thread | 10 million | 0.155 | 64.42 | 277.5 |
| Materialize then join, 1 SQL thread | 10 million | 0.182 | 55.01 | 331.6 |
| Fixed build side, 8,192-row batches, 1 SQL thread | 10 million | 0.113 | 88.41 | 32.7 |
| Fixed build side, 8,192-row batches, 8 SQL threads | 100 million | 0.210 | 476.24 | 40.4 |
| Fixed build side, 65,536-row batches, 8 SQL threads | 100 million | 0.188 | 532.54 | 64.1 |
| Fixed build side, 8,192-row batches, 8 SQL threads | 1 billion | 2.025 | 493.85 | 42.1 |

The biggest lessons:

- Tiny Arrow batches cost heavily: 8,192 rows/batch achieved about 25 times the
  throughput of four rows/batch on the same one-million-row workload.
- The legacy Arrow scan estimates one row. The default optimizer builds a hash
  table from the large remote input. Keeping the small local table as the build
  side reduced peak RSS about 8.5 times at ten million rows. `fixed` mode disables
  join reordering/build-side swapping for this experiment; a production adapter
  should provide useful cardinality estimates instead. Saved EXPLAIN plans show
  both arrangements.
- Larger batches are not universally better. At one thread, 65,536-row batches
  were slower on the short default-join case. At eight threads on the larger
  fixed-plan case, they increased throughput but used more memory.
- Concurrency adds little for this cheap producer without latency: one, four,
  and eight producer workers all measured around 62–64 million rows/s on the
  ten-million-row default join. Parallel SQL and join planning mattered more.

With a simulated **10 ms sleep per response batch**, 200,000 rows, 2,000 rows per
response, and one SQL thread:

| Producer | Median seconds | Million input rows/s | Peak RSS MiB |
|---|---:|---:|---:|
| Synchronous | 1.227 | 0.163 | 33.0 |
| 4 workers | 0.313 | 0.638 | 33.2 |
| 16 workers | 0.090 | 2.215 | 34.0 |

That is a 13.6-times improvement from overlapping latency. Each worker generates
one batch after sleeping; the queue capacity is twice the worker count. Dropping
its receiver unblocks producers, and dropping the reader joins them. This models
bounded prefetch, not real HTTP cancellation or a server's capacity constraints.

### What the numbers include

The timed region includes Arrow generation and consumption, the real DuckDB
join/filter/aggregation, and synthetic request latency when configured. The
materialized variant also includes `CREATE TEMP TABLE AS` before its join.
Database startup and creation of the 100,000-row local table are excluded from
query time but included in process wall time and peak RSS. Every run verifies
input row delivery, filtered count, and source/document ID sums. Half the hits
pass the score predicate. Final output is three aggregate values, not millions
of returned documents.

There are three fixed-width columns (24 logical bytes per input row). The
one-billion-row case generates 24 GB of logical input incrementally; it does not
read 24 GB from disk or a network. These are **not Weaviate benchmarks**: no
vector search, embeddings, HTTP, JSON decoding, document bodies, global sort, or
large result export is measured. Short-run rates are sensitive to machine load;
raw timings include the observed spread.

The legacy C bridge also mishandled projected column subsets in a pilot scan
case. The validated benchmark consumes all three columns and leaves filtering
inside DuckDB. This reinforces the need for a production scan that declares and
implements projection/filter capabilities correctly; this deprecated bridge
should not be treated as a general-purpose production adapter.

Files:

- `src/benchmark.rs`: generator, concurrent reader, SQL, and correctness checks.
- `tools/bench.py`: reproducible measurement runner.
- `results/summary.json`: all cases, median/min/max times, throughput, peak RSS.
- `results/raw.jsonl`: individual measurements including process wall time.
- `results/default-plan.txt` and `results/fixed-plan.txt`: physical plan evidence.
- `results/pilot.*`: retained initial exploratory run, excluded from final results.

To inspect one case (macOS):

```sh
DYLD_LIBRARY_PATH=target/release/deps target/release/duckdb-arrow-stream-demo \
  --bench 100000000 8192 0 0 8 fixed
```

Arguments: input rows, Arrow batch rows, simulated latency milliseconds,
producer workers (`0` means synchronous pull), SQL threads, and mode
(`scan`, `join`, `fixed`, or `materialize`). `BENCH_EXPLAIN=1` also prints the plan;
use it for inspection, since that extra work is inside the reported timer.

## HTTP Arrow control for the Trino comparison

`src/benchmark/http.rs` adds a benchmark-only reader for the Rust HTTP service in
`../trino-arrow-stream-demo`. It reads Arrow IPC incrementally, either directly
(one stream) or through a bounded queue (multiple partition streams). The simple
HTTP parser supports this fixture's close-delimited responses, not arbitrary
HTTP services or TLS.

With that service running:

```sh
DUCKDB_DOWNLOAD_LIB=1 cargo build --release --locked --no-default-features
ARROW_BENCH_HTTP=http://127.0.0.1:18881 DYLD_LIBRARY_PATH=target/release/deps \
  target/release/duckdb-arrow-stream-demo --bench 10000000 8192 0 4 8 fixed
```

In HTTP mode the worker argument determines partition count (zero/one means a
single synchronous stream). Registration still fetches no rows. The same
count/source-ID/document-ID checks validate the whole result. The comparison
runner in the Trino project benchmarks both this HTTP path and the original
in-process generator; its README and `results/comparison/` contain the paired
results. Original DuckDB-only measurements in this project's `results/` are
unchanged.
