# Corpus / harness repair handoff (2026-07-26)

State of the Cypher Ladybug corpus-repair push (fixture loaders, broken-import
re-extraction, TCK expectation regeneration). All work is uncommitted in the
working tree on top of `main @ 2cb1999`. Nothing here touches `src/`.

## 1. Loader fixes landed (harness, `tests/cypher_case_runner/`)

- **Parquet loader** (`loader.rs::read_parquet_rows`): `parquet = "58.2.0"`
  added as a dev-dependency (matches `arrow 58.2.0`). Values are rendered to
  strings via `arrow::util::display::array_value_to_string` and re-parsed by
  the existing type-driven column builder. Kuzu-converted files carry generic
  `f0/f1/...` column names — those are treated as positional, otherwise
  columns map by name.
- **Typed CSV headers** (`loader.rs`): Neo4j-import-style headers
  (`id:ID(Person)`, `:START_ID(Person)`, `:END_ID(Person)`, `name:STRING`)
  are detected and name-mapped onto the schema; `|`-delimited CSVs
  (LDBC/LSQB) are auto-detected (`detect_delimiter`).
- **`CSV_TO_PARQUET(name)` datasets** (`dataset.rs::candidates_for`): resolve
  to the underlying CSV fixture directory (data is identical).
- **Results** (previously all these suites loaded EMPTY graphs / were
  skipped, 91 cases total):
  - `demo_db/demo_db_parquet` 33/33, `demo_db/demo_db_order_parquet` 3/3
  - `parquet/tinysnb_parquet` 11/15 (kuzu-ext tier; 4 remaining are engine
    gaps: error-expectation cases + path renders)
  - `ldbc` 18/22 (4 incorrect: IC3, IC11, IS1 x2 — real engine mismatches,
    now measurable)
  - `lsqb` 10/18 (8 are >240 s timeouts on the naive interpreter — q1, q3,
    q6, q9 in both csv and parquet variants; the ones that finish are all
    correct, e.g. q4 = 784511 in ~7 s)
- Caveat: `lsqb-sf01` / `ldbc-sf01` are multi-million-row fixtures. Run those
  suites one case per process (see "OOM" below).

## 2. Broken-import re-extraction (kuzu setups)

- Runner support added in `tests/cypher_ladybug_cases.rs`: a new optional
  `--- setup_statements` case section (raw Cypher, one statement per line,
  `#` comments) executed against the graph before the case query. A failing
  setup statement -> case is Skipped, never Incorrect. Cases with setup or
  initializer sections are no longer machine-tagged broken-import.
  Interpreter facts probed: node `CREATE`, `MATCH...SET`, `DELETE` work;
  relationship patterns in `CREATE` are NOT supported (edges must go through
  `--- graph_initializer`).
- Importer: `tests/import_kuzu_setups.py <kuzu-repo-root>` (kuzu shallow
  clone; matches cases via `metadata.source` -> `test/test_files/...` +
  `source_case`, extracts preceding write statements, emits
  `graph_initializer` DSL and/or `setup_statements`, drops Kuzu-only DDL).
- Sub-agent report: **164 conversions kept / 151 newly passing / 649 blocked**
  (blockers: COPY FROM / CALL / transaction control in setup, relationship
  CREATE ordering that cannot be front-loaded into the initializer, MERGE
  unsupported, no upstream match, engine-mismatch conversions reverted to
  keep the no-Incorrect invariant). Machine-tagged broken-import count in the
  interim sweep dropped from 529 to ~273.
- 62 case files currently carry `--- setup_statements` sections; provenance
  is recorded as a `#` comment line referencing the kuzu source test.

## 3. TCK work state

- `tests/import_tck_expectations.py <openCypher-root>`: regenerates
  `--- expected` from upstream TCK tables in neutral TCK syntax
  (`(:L {p: 1})`, quoted strings, `null`), fixes `ordered` from the upstream
  "in order"/"in any order" step, and stamps `"expected_provenance"` into the
  metadata JSON. **100 cases regenerated** (grep `expected_provenance`).
- Harness bridge (`tests/cypher_case_runner/format.rs::canonicalize_value`):
  engine Kuzu-style renders (`{_ID: 0:0, _LABEL: End, num: 42}`) and TCK
  neutral renders (`(:End {num: 42})`) canonicalize to the same text.
  `_ID` and `__`-prefixed synthetic keys are dropped; applied symmetrically
  to both sides, so previously-equal comparisons cannot regress.
- Property-less initializer nodes (`initializer.rs`): now materialize via a
  hidden `__row` Int64 column (previously zero-row tables), stripped from
  renders by the bridge.
- tck suite: **730/933 (78.2%)**, up from ~655/933. Remaining 187 incorrect
  are mostly genuine engine gaps (row counts on aggregation/OPTIONAL MATCH,
  boolean ternary logic, list slicing) plus ~25 un-regenerated `_ID`-render
  cases and a few label-less-node artifacts (`(:N ...)` placeholder label vs
  upstream `({...})`).

## 4. Best tier-summary numbers (scoped/partial — see caveats)

Aggregated from per-suite runs with the current tree (binary
`target/release/deps/cypher_ladybug_cases-fda31db3cfc52a07`,
`CYPHER_TIMEOUT_MS=15000`, heavy suites per-case). The dml_* / issue /
transaction numbers were taken mid-re-import and may shift slightly with the
final kuzu-agent reverts.

- **core (headline): ~3713/4276 runnable = ~86.8%** (baseline 3489/4136 =
  84.4%; the denominator grew because formerly-unmeasurable cases became
  runnable core signal, many of which fail honestly — e.g. dml_node 46/190)
- kuzu-ext (info): ~460/926 = ~49.7% (plus ~108 skipped)
- broken-import: ~273 excluded (down from 529)
- Newly honest-but-red suites worth knowing: dml_node 46/190, dml_rel 29/63,
  transaction (kuzu-ext) 137/387, copy (kuzu-ext) 55/131.

### Measurement hazards — read before trusting any number here

Two defects have understated or destabilised every sweep taken so far.

1. **`-- --nocapture` is mandatory.** `cargo test` captures stdout for a
   suite that fully passes, so the tier-summary block is only printed when
   the suite *fails*. Without the flag, every all-green suite contributes
   nothing and the totals come out low. The numbers in section 4 above
   predate this discovery and are understated by roughly a dozen suites.

2. **Timed-out cases leak live threads.** `CYPHER_TIMEOUT_MS` runs each
   case on a worker thread and gives up with `recv_timeout`; the thread
   cannot be cancelled and keeps allocating for the life of the process
   (each one also pins a deep clone of the dataset). Accumulated leaks have
   exhausted host memory and tripped the macOS kernel watchdog
   (`/Library/Logs/DiagnosticReports/panic-full-*.panic`, "no checkins from
   watchdogd in 90 seconds"). `MAX_ABANDONED_CASES = 2` in
   `tests/cypher_ladybug_cases.rs` now bounds this: past the budget the run
   names the offenders and hard-exits 101 rather than taking the host down.
   Note the runner default is `CYPHER_TIMEOUT_MS=0` — *no* per-case
   timeout — so an unguarded runaway case will otherwise hang forever.

The paired upstream guard is `BULK_ROW_LIMIT = 2000` in
`tests/import_kuzu_setups.py`, which refuses to import setups that generate
huge row counts (one imported `UNWIND range(1, 300000) AS i CREATE …` was
the case that wedged the machine).

Run per-suite (`CYPHER_SUITE="ladybug/<suite>/"` — keep the prefix and
trailing slash, bare names double-count via substring overlap), and for
`lsqb`/`ldbc`/`read_list` run per-case or per-subdir in separate processes.

## 5. Ranked remaining corpus-side work

1. **Finish the remaining blocked re-imports.** The two biggest historical
   buckets — setups needing relationship `CREATE` mid-sequence, and
   `MERGE`-based setups — are now unblocked: the interpreter supports edge
   insertion, relationship patterns/chains/self-loops in `CREATE`, `SET`
   map forms, relationship and `DETACH DELETE`, and `MERGE` with
   `ON CREATE` / `ON MATCH`. Combined with fixture dataset overrides this
   converted 127 cases and dropped broken-import from 368 to 78. Remaining
   importer blockers, by count: `no_setup_writes` 383, `substitution` 35,
   `call` 19, `directive` 9, `no_upstream_case` 8, `copy` 7,
   `no_upstream_file` 6, `no_query_match` 4, plus `bulk_generation`
   (deliberately refused, see `BULK_ROW_LIMIT`). Script:
   `tests/import_kuzu_setups.py`.
2. **Missing TCK initializers / unmatched scenarios** (~119 unmatched at last
   count): extend the matching in `tests/import_tck_initializers.py` /
   `tests/import_tck_expectations.py` (openCypher clone lives in the session
   scratchpad; re-clone if gone).
3. **Remaining `_ID`-render expectations** (~25 tck cases the regenerator
   could not unambiguously match) — same script, needs manual matching.
4. **Lexicographic-expected artifact** (ordered:true cases whose expected
   rows were sorted at import): category-B detection exists in
   `tests/import_tck_expectations.py`; sweep beyond tck (order_by 2/4 looks
   like this pattern).
5. **Error-prose expectation cases outside tck** (e.g. tinysnb_parquet
   Binder-exception cases now Incorrect because the engine returns rows):
   either engine work or per-case category tags.
6. **Dropped query parameters**: some imported cases reference `$param`
   values that the importer dropped; a `--- parameters` section + runner
   support would recover them.
7. **lsqb q1/q3/q6/q9 timeouts**: engine performance work (multi-way joins),
   not corpus work.

## Pointers

- Runner: `tests/cypher_ladybug_cases.rs` (setup_statements support,
  `extract_setup_statements`)
- Loader/dataset/bridge: `tests/cypher_case_runner/{loader,dataset,format,initializer}.rs`
- Tier manifest: `cases/cypher/ladybug/tiers.toml` (unchanged; loads clean,
  all 19 harness unit tests pass)
- Importers: `tests/import_kuzu_setups.py`, `tests/import_tck_expectations.py`,
  `tests/import_tck_initializers.py`
