# Rust Arrow streams queried by Trino

A runnable Trino 484 example: a Rust service generates batched search matches,
streams them over HTTP using Arrow IPC, and a Java connector exposes them as
`remote.default.matches`. Trino performs the join, filter, and sort.

```text
Rust mock search service
  /matches/0 → Arrow stream for source IDs 1, 3, 5
  /matches/1 → Arrow stream for source IDs 2, 4, 6
                  ↓
Trino connector: two splits, one page per Arrow batch
                  ↓
JOIN memory.default.documents → WHERE score >= 0.8 → ORDER BY source_id
```

There is no temporary table for remote results, host scalar UDF, or DataFusion.
`memory.default.documents` is a small local lookup table populated for the join.

## Run the verified example

Prerequisites: Rust, Maven, Docker with Compose, and Python 3. The build uses the
Trino container's JDK 25 and exact SPI; no local JDK 25 installation is required.
Maven needs a working local Java installation to resolve Arrow dependencies.
First build requires network access. Ports 18081 and 18881 must be free.

```sh
./tools/build.sh
python3 tools/demo.py
```

The script starts a separate Trino container and the Rust service, runs the
queries, writes evidence to `results/`, and stops/removes its own containers and
network afterward. It does not change the existing OrchidDB Trino fixture.
Build artifacts and dependency caches are retained.

The actual query executed is:

```sql
SELECT m.source_id, d.title, m.score
FROM remote.default.matches m
JOIN memory.default.documents d ON d.id = m.document_id
WHERE m.score >= 0.8
ORDER BY m.source_id;
```

Expected result:

```text
source_id | title           | score
----------+-----------------+------
1         | Arrow streaming | 0.95
2         | Arrow streaming | 0.90
3         | Arrow streaming | 0.95
4         | Arrow streaming | 0.90
5         | Arrow streaming | 0.95
6         | Arrow streaming | 0.90
```

Verified against Trino 484 on Docker Desktop, macOS ARM64. Seven checks pass:
planning performs no remote fetches; join/filter/sort; zero-column `count(*)`;
projected/reordered columns; partition completeness; deterministic replay on a
new scan; and propagation of an HTTP 503 failure. See
`results/verification.json`, `results/plan.txt`, and `results/server.log`.

## Explore interactively

Start the Rust service in one terminal:

```sh
cargo run --release --locked --manifest-path server/Cargo.toml
```

In another terminal, start Trino and open its CLI after startup:

```sh
docker compose -p orchid-arrow-demo up -d
docker compose -p orchid-arrow-demo exec trino trino --user arrow-demo
```

Initialize the lookup table and run the query above:

```sql
CREATE SCHEMA IF NOT EXISTS memory.default;
CREATE TABLE memory.default.documents (id BIGINT, title VARCHAR);
INSERT INTO memory.default.documents VALUES
    (10, 'Arrow streaming'), (20, 'Other document');

SELECT count(*) FROM remote.default.matches; -- 12: two hits per source
SELECT score, source_id
FROM remote.default.matches
WHERE document_id = 20
ORDER BY source_id;
```

Stop with `docker compose -p orchid-arrow-demo down` and Ctrl-C in the Rust
terminal. Do not run the automated verification concurrently with interactive
mode: they use the same demo ports/project. The service listens on port 18881 on
all interfaces so the Docker worker can reach it; it is an unauthenticated local
fixture, not a public service.

## How it works

- `server/src/main.rs` flushes an Arrow IPC batch after each mock search request.
  Each partition has three sources, batched two at a time. Each source returns
  document 10 with a high score and document 20 with a low score. Replace this
  fixture generation with a real batched API request to integrate remote search.
- `connector/src/main/java/demo/ArrowPlugin.java` supplies metadata, two split
  handles, and a `ConnectorPageSource`. Network reads begin only during scan
  execution. The connector loads one Arrow batch, copies requested columns into
  a Trino page, and releases its stream/allocator when closed.
- Projection respects column identity and requested order, including an empty
  column list. Predicates, joins, and aggregations stay in Trino; the connector
  does not advertise pushdowns it cannot execute.
- Metadata supplies the actual fixture row count (12) to avoid the unknown
  cardinality problem observed in the DuckDB demo.
- Every GET reconstructs the same partition, allowing another scan to reproduce
  its results without a staging table. Production remote searches require a
  stable execution/snapshot identity or a replay buffer to make that promise.
  The demo tests repeat scans, not forced Trino task-retry recovery.

## Scope

This is a **real Trino integration with simulated search results**, not a
Weaviate benchmark. It uses Arrow IPC over HTTP, not Arrow Flight. Arrow-to-Trino
page conversion copies values; this is not zero-copy integration.

For simplicity, the demo runs one Trino node with two schedulable splits. On a
cluster, the plugin and its catalog configuration must be installed on workers
as well as the coordinator, and every worker must reach the service. No
multi-node throughput claim is made here.

Reads are synchronous and bounded by connect/read timeouts. Arrow allocation is
capped at 16 MiB per split; the fixture has tiny batches. A production version
should integrate asynchronous reads with Trino's blocked-source API, robust
cancellation, resource accounting, service concurrency limits, and replayable
execution IDs. This project demonstrates the ingestion boundary only: source
IDs are fixtures, rather than the output of an earlier Trino SQL island.

Versions are pinned to Trino 484, Java Arrow 18.3.0, and Rust Arrow 58.4.0. Arrow
IPC provides the cross-language format. The connector's Jackson dependencies
are aligned with Trino 484, including Blackbird for serialized connector handles.
The plugin must be rebuilt/revalidated when upgrading Trino's SPI.

## Benchmark comparison with DuckDB

The comparison runner uses the same generated numeric values and SQL
join/filter/aggregate workload as the DuckDB example, and adds an HTTP Arrow
client to DuckDB as a control. This distinguishes in-process input from streamed
IPC input rather than comparing only two different transports.

```sh
./tools/build.sh
cd ../duckdb-arrow-stream-demo
DUCKDB_DOWNLOAD_LIB=1 cargo build --release --locked --no-default-features
cd ../trino-arrow-stream-demo
python3 tools/compare.py
```

Results on this Apple M3 Pro: 81 validated measurements (nine cases, three input
paths, three repetitions), shuffled order. DuckDB 1.5.5 runs natively with eight
SQL threads. Trino 484 runs as one Docker node (Docker VM has ten CPUs) with a
1 GiB Java heap. Trino is warmed before measurement. The fixture is a separate
native Rust process, so Trino crosses Docker Desktop's VM network boundary;
DuckDB's HTTP control uses host loopback. These are measurements of these
implementations/deployments, not an engine-only comparison or a Trino ceiling.

Median **client wall times**, seconds:

| Input / workload | Streams | DuckDB in-process | DuckDB HTTP Arrow | Trino HTTP Arrow |
|---|---:|---:|---:|---:|
| 1 million rows, join | 1 | 0.045 | 0.054 | 0.151 |
| 10 million rows, join | 4 | 0.062 | 0.116 | 0.629 |
| 10 million rows, scan/filter/aggregate | 4 | 0.053 | 0.081 | 0.628 |
| 100 million rows, join | 4 | 0.235 | 0.383 | 5.981 |
| 200,000 rows, 10 ms delay per batch | 1 | 1.241 | 1.253 | 1.347 |
| 200,000 rows, 10 ms delay per batch | 4 | 0.347 | 0.342 | 0.413 |
| 200,000 rows, 10 ms delay per batch | 16 | 0.113 | 0.118 | 0.172 |

No-delay cases use 8,192-row Arrow batches. Latency cases use 2,000-row batches.
Increasing batch size to 65,536 rows did not help this Trino implementation:
the ten-million-row/four-stream join took a median 0.950 s instead of 0.629 s.
This result does not establish a universal optimal batch size.

On the hundred-million-row case, observed client throughput was 425 million
rows/s for in-process DuckDB, 261 million for DuckDB HTTP, and 16.7 million for
Trino HTTP. Thus this Trino path was about 15.6 times slower than DuckDB HTTP in
that case. Scan-only and joined Trino times were almost identical on ten million
rows, suggesting ingestion/conversion/scheduling costs dominate this small
lookup join; no isolated profiler attribution is claimed.

When synthetic remote delay dominates, the difference is much smaller. Trino
improved from 1.347 s with one stream to 0.172 s with sixteen (about 7.9 times).
These sleeps are not Weaviate search benchmarks: there is no server CPU
contention, rate limiting, HTTP JSON decoding, or real vector search.

### Timing, workload, and memory interpretation

- All paths generate the same three fixed-width columns: source ID, document ID,
  score (24 logical bytes/row). Half the rows pass the score filter. The lookup
  table has 100,000 unique document IDs. The final result contains count and two
  ID sums; all three values are checked for every run. No large result export or
  global sort is measured.
- DuckDB client time includes launching the benchmark process, database setup,
  input generation/reading, and the SQL query. Trino client time includes SQL
  submission, planning/scheduling, execution, and polling the final result on an
  already-running, warmed server. Service/container startup and Trino lookup
  table creation are excluded. DuckDB's inner query time is retained separately
  as `seconds` in raw output; it differs from the client times above and from
  the earlier DuckDB-only report's timing boundary.
- Each HTTP partition is a disjoint row range and a fresh deterministic stream.
  The native DuckDB generator uses a bounded worker pool when parallelism is
  requested. Its batch assignment differs from fixed HTTP partitions, including
  rounding of final batches. No-delay producer work is cheap integer generation.
- Trino supplies real cardinality estimates and chooses a replicated small-table
  join. DuckDB uses the previously tested fixed-build-side plan. These are the
  respective engines' actual plans for equivalent queries, not identical plans.
- DuckDB HTTP peak process RSS was about 47 MiB at 100 million rows. Trino's
  reported query peak was about 5.4 MiB, but that excludes the JVM baseline,
  portions of Arrow/native memory, and the Rust service. **Those memory numbers
  are not comparable.** This run does not measure total Trino deployment peak
  RSS. Raw Trino query statistics, including reported memory and spill bytes,
  are preserved without interpreting them as total memory.
- Three repetitions are a small sample; use `min_seconds`/`max_seconds` to inspect
  variability. These are local prototype results, not a production capacity SLA
  or a multi-node scaling benchmark.

Evidence: `results/comparison/summary.json`, `results/comparison/raw.jsonl`,
`results/comparison/trino-plan.txt`, and the corresponding server/Trino logs.
The earlier demo's verification files are preserved separately.

Benchmark-only tables encode input parameters, for example
`remote.default.bench_10000000_8192_4_0` means ten million rows, 8,192 rows per
Arrow batch, four partitions, and zero simulated delay. They are resolved on
demand rather than listed by `SHOW TABLES`. The service endpoint is
`/bench/{rows}/{batch}/{partitions}/{partition}/{latency_ms}`. Both sides validate
limits; the benchmark still performs all relational work in the SQL engine.
