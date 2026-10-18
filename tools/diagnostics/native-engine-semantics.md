# Native engine semantics in OrchidDB

Internal design study • 2026-10-18 • DuckDB examples verified on **1.5.6**

**Recommendation:** first use optional engine-side compatibility functions to produce compact SQL while preserving full language conformance, retaining conformant SQL expansion for engines where functions cannot be installed or are disabled. Separately, offer a versioned, opt-in policy for specific native operations, starting with string conversions and explicitly native functions. Keep compatibility defaults for empty aggregates, null handling, graph identity, and explicit ordering. The largest potential gains come from keeping work in the engine and removing unnecessary collection ordering or semantic key computation—not from removing every small SQL wrapper.

Function availability and semantic policy are independent. A function executing inside DuckDB can implement Orchid's exact language contract. The [rule-pipeline implementation handoff](rule-pipeline-rearchitecture.md#separate-conformance-from-function-availability) defines capability-based selection among conformant helpers, conformant SQL, and explicitly native behavior.

This document inventories opportunities found in the current Cypher frontend, relational lowering, shared runtime, and SQL adapters. It covers adjacent JSON and portable-function behavior where the same decision arises. It is not a claim that every function or every engine has been exhaustively audited.

Read by topic: [conversions](#1-conversions), [aggregation](#2-aggregation-native-computation-already-exists), [comparisons and arithmetic](#3-comparisons-arithmetic-and-common-types), [collections](#4-lists-predicates-and-structured-values), [text and ordering](#5-text-regex-ordering-and-collation), [temporal values](#6-temporal-semantics), [JSON](#7-native-json-semantics), [policy design](#9-proposed-policy-design), or [validation and measurements](#11-validation-and-measurements).

## How to read the examples

Three different things must remain distinct:

| Term | Meaning here |
| --- | --- |
| **Current OrchidDB semantics** | The behavior encoded in this checkout's lowering, runtime, and tests. Some rules reflect openCypher, others shared-runtime or historical compatibility decisions. |
| **Native engine semantics** | The selected SQL engine's conversion, type, aggregate, comparison, or function behavior. This can change with engine version and session settings. |
| **Equivalent native lowering** | Using native SQL while preserving the selected language contract. This needs no relaxed mode. |

`native_values` in the code refers to OrchidDB's typed runtime values and kernels. **It is not a switch for accepting DuckDB semantics.** Likewise, `RelBackend::new()` versus `with_language_functions(true)` selects different lowering capabilities; neither is the proposed policy.

SQL below is intentionally simplified to expose the relevant operation. It is not a byte-for-byte dump of generated SQL. `Person` maps to a table named `person`; properties map to identically named columns. Input types are stated when they matter. Standalone `VALUES` examples make the result differences reproducible. Proposed configuration and SQL paths are labeled as proposals.

The source inspection used working tree HEAD `22262a0c` **plus ongoing, uncommitted compiler changes**. Source links describe that working tree. The original DuckDB examples were executed independently; the newly proposed compatibility-helper call is illustrative and was not executed. The entire OrchidDB conformance suite was not rerun for this document.

“Strict” should name a versioned OrchidDB language contract, not mean “whatever the newest Neo4j does.” For example, this checkout expects zero from empty `stdev`/`stdevp`, while Neo4j's current documentation records a change to null for null-only input beginning in 2026.05. That is a contract-version decision separate from engine selection. [Neo4j aggregate reference](https://neo4j.com/docs/cypher-manual/current/functions/aggregating/)

## Opportunity map

Benefits below are hypotheses about plan shape and execution cost, except for the explicitly measured conversion experiment near the end.

| Area | Native shortcut or capability | What may change | Recommendation |
| --- | --- | --- | --- |
| String → float | `TRY_CAST(x AS DOUBLE)` | Accepted grammar, Unicode whitespace, underscores, NaN/infinity | Good first opt-in |
| Float/string → integer | `TRY_CAST(x AS BIGINT)` | Rounding, saturation, invalid-input behavior | Separate opt-in |
| String → boolean | `TRY_CAST(x AS BOOLEAN)` | Accepted tokens and whitespace | Separate opt-in |
| Value → string | `CAST(x AS VARCHAR)` | Formatting, supported input types | Useful for presentation; keep out of identity keys |
| Empty aggregate results | Bare `sum`, `stddev_*`, `list` | Zero/empty-list versus null | Small saving; retain cheap wrappers by default |
| Collection aggregation | `list(x)` without synthetic order | Element order, nulls, empty results | Potentially large; split order from null policy |
| Distinct collections/grouping | Native keys and `DISTINCT` | Numeric equivalence, representative selection, nested values | Use exact typed domains first |
| Numeric accumulation | Engine accumulators and result types | Width, overflow, rounding, low floating-point bits | Valuable, but type inference and codecs must agree |
| Native/approximate aggregates | `median`, `arg_max`, `approx_*` | Function-specific nulls, ties, accuracy | Explicit calls are preferable to implicit substitutions |
| Scalar comparisons | Engine equality/order | Coercion, precision, NaN behavior | High impact; exclude from initial preset |
| Arithmetic/math | Native operators/functions | Integer division, errors, overflow, NaN | Separate family; audit current paths first |
| `CASE`/`coalesce`/mixed lists | Engine common-type resolution | Values may be coerced or binding may fail | Require an explicit homogeneous SQL type contract |
| List predicates/equality | Native list operators | Three-valued logic, nested nulls | Dangerous broad relaxation; preserve cheaply when possible |
| List comprehensions | Native list lambdas | Evaluation, typing, null rules | High-value equivalent lowering candidate |
| String/regex functions | Engine text/regex implementation | Unicode, pattern syntax, whitespace, errors | Named engine functions or narrow policy |
| Sorting/collation | Engine ordering and collation | Null placement, text equivalence, tie order | Preserve explicit order; collation opt-in only |
| Temporal values | `DATE`, `TIMESTAMP`, `TIMESTAMPTZ`, `INTERVAL` | Precision, zones, calendar arithmetic, duration identity | Restrict to declared representable domains |
| Maps/structs | Engine structured types | Missing fields, dynamic keys, common field types | Domain-specific mapping, not a global switch |
| JSON | Engine parser, paths, equality | Invalid extensions, duplicate keys, numeric normalization | Strong opportunity for explicitly native JSON calls |
| Error/evaluation behavior | Engine optimizer and error handling | Whether/when expressions run, diagnostics | Independent contract; never relax effects implicitly |

## 1. Conversions

### String to float: the motivating case

```cypher
WITH 'foo' AS foo_string, '' AS empty_string
RETURN toFloat(foo_string) AS foo, toFloat(empty_string) AS empty
```

Both conversions are null under the current contract. Native DuckDB can express this as:

```sql
SELECT TRY_CAST('foo' AS DOUBLE) AS foo,
       TRY_CAST('' AS DOUBLE) AS empty;
-- NULL, NULL
```

For these constants, folding to two typed nulls is also an **equivalent optimization**, with no policy change. The policy question matters for unknown string values:

```cypher
MATCH (p:Person)
RETURN toFloat(p.raw_amount) AS amount
```

The present string lowering removes underscores, trims the Rust Unicode White_Space set, checks a decimal grammar, tries a double cast, and rejects nonfinite parsed results. The following shows the shape; ordinary `trim` abbreviates the Unicode trimming for this ASCII-only illustration, and `isfinite` abbreviates the emitted comparisons against infinities:

```sql
WITH tokens AS (
  SELECT replace(trim(raw_amount), '_', '') AS token FROM person
), parsed AS (
  SELECT token, TRY_CAST(token AS DOUBLE) AS value FROM tokens
)
SELECT CASE
         WHEN regexp_matches(token,
           '^[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?$')
          AND isfinite(value)
         THEN value
         ELSE NULL::DOUBLE
       END AS amount
FROM parsed;
```

Proposed conformant helper path, when a typed compatibility function backed by the shared conversion kernel is registered:

```sql
SELECT __orchiddb_cypher_to_float_v1(raw_amount) AS amount FROM person;
```

This is a proposed function name, not an existing installed function or an executed example. It must preserve the complete selected string-conversion contract. When helpers are unavailable or disabled, use the conformant SQL expansion above, with its full Unicode handling restored. Validate both against the same reference semantics. Compact helper SQL does not require a native semantics mode.

Separately, under the proposed native-string-conversion policy:

```sql
SELECT TRY_CAST(raw_amount AS DOUBLE) AS amount FROM person;
```

| String input | Current OrchidDB string contract | DuckDB 1.5.6 `TRY_CAST(... AS DOUBLE)` |
| --- | --- | --- |
| `'foo'`, `''` | null | null |
| `'2.9'`, `'-2e2'` | `2.9`, `-200.0` | Same |
| `'1_2'` | `12.0` | `12.0` |
| `'__1__2__'` | `12.0` after removing every underscore | null |
| `'NaN'` | null | NaN |
| `'Infinity'` | null | Positive infinity |
| `'1e999'` | null | Positive infinity |
| NBSP + `'1.5'` + IDEOGRAPHIC SPACE | `1.5` | null |

This is not simply “the engine accepts more.” Each side accepts strings the other rejects. Also, the finite-result restriction applies to **parsed strings**; the current numeric-input branch directly casts numeric values. Do not accidentally broaden the proposal into rejecting or accepting all nonfinite numeric inputs.

`regexp_matches` itself is a valid DuckDB boolean function. Its pattern here is anchored, so its usual substring-search behavior is not the problem. [DuckDB regex functions](https://duckdb.org/docs/current/sql/functions/regular_expressions)

**Benefit:** fewer string scans and expression nodes, and potentially more complete SQL execution for computed inputs. **Cost:** different values feed downstream filters, aggregates, comparisons, and serialization. NaN is especially consequential because DuckDB gives it native equality/order behavior discussed below.

Keep `TRY_CAST`, rather than `CAST`, for this policy: `CAST('foo' AS DOUBLE)` throws. Trying a cast also does not guarantee that arbitrary input expressions or unsupported type signatures cannot fail before conversion. [DuckDB casting](https://duckdb.org/docs/current/sql/expressions/cast)

Evidence: [conversion lowering](../../src/ir/rel/scalar_semantics.rs), [conversion regression cases](../../tests/cypher_scalar_semantics_sql.rs), [runtime conversion domains](../../src/ir/runtime/scalar/cypher_conversion.rs).

### Integer conversion: rounding and saturation are separate decisions

```cypher
UNWIND [2.9, -2.9, 1e30] AS x
RETURN toInteger(x) AS i
```

Current float-input behavior truncates toward zero, saturates finite out-of-range values to signed 64-bit limits, and returns null for nonfinite inputs. A representative compatibility expression is:

```sql
SELECT CASE
         WHEN NOT isfinite(x) THEN NULL::BIGINT
         WHEN x >= 9223372036854775808.0::DOUBLE
           THEN 9223372036854775807::BIGINT
         WHEN x <= -9223372036854775808.0::DOUBLE
           THEN '-9223372036854775808'::BIGINT
         ELSE TRY_CAST(trunc(x) AS BIGINT)
       END AS i
FROM (VALUES (2.9::DOUBLE), (-2.9::DOUBLE), (1e30::DOUBLE)) t(x);
-- 2; -2; 9223372036854775807
```

Proposed native conversion:

```sql
SELECT TRY_CAST(x AS BIGINT) AS i
FROM (VALUES (2.9::DOUBLE), (-2.9::DOUBLE), (1e30::DOUBLE)) t(x);
-- 3; -3; NULL
```

String conversion differs too: `'2.9'` becomes `2` in the current runtime and `3` in DuckDB. Both can parse `'0x2a'` as `42`; preserve exact integers such as `'9007199254740993'` without routing through a double. DuckDB documents fractional-to-integer casting as rounding. [DuckDB typecasting](https://duckdb.org/docs/current/sql/data_types/typecasting)

The current SQL path for general string `toInteger` is particularly conservative: it can specialize a proven immutable finite string domain by evaluating tokens with the shared runtime. Otherwise it retains the runtime boundary. Native conversion could avoid that specialization and handle an arbitrary mapped `VARCHAR` column directly.

An intermediate policy can preserve truncation with `TRY_CAST(trunc(x) AS BIGINT)` while accepting native overflow behavior. That changes less than direct casting, but still does not preserve saturation. Keep string parsing, numeric rounding, and overflow as identifiable contract choices.

Evidence: [integer specialization and saturation](../../src/ir/rel/scalar_semantics.rs), [shared Cypher conversion implementation](../../src/ir/runtime/scalar/cypher.rs), [integer-domain tests](../../tests/cypher_integer_domain_sql.rs).

### Boolean conversion and accepted input types

```cypher
UNWIND ['true', ' false ', 'yes', '1'] AS x
RETURN toBoolean(x)
```

Current string behavior recognizes case-insensitive `true` and `false` after Unicode trimming. A native policy would emit:

```sql
SELECT x, TRY_CAST(x AS BOOLEAN)
FROM (VALUES ('true'), (' false '), ('yes'), ('1')) t(x);
-- true; NULL; true; true
-- Current OrchidDB: true; false; NULL; NULL
```

Accepting engine parsing does not have to change the language's input-type domain. The runtime rejects `toFloat(true)` and `toBoolean(1.5)`; DuckDB can convert those to `1.0` and `true`. Start with **known string operands** and keep the domain checks. Broad “anything DuckDB can cast” behavior is a different feature with more surprising failures and successes.

### String formatting

```cypher
WITH 1.0 AS x RETURN toString(x)
```

The shared runtime uses Rust float formatting, producing `"1"` for this value. DuckDB uses:

```sql
SELECT CAST(1.0::DOUBLE AS VARCHAR);
-- '1.0'
```

The current primitive SQL `toString` path accepts booleans, signed integers, strings, and null; floats and richer values retain a runtime boundary. Native formatting could extend pushdown, particularly for numeric and date presentation. It also changes exponent notation, precision, temporal spelling, and potentially allowed inputs. Strings used as joins, persistent keys, exports, or checksums are sensitive to these changes.

Evidence: [primitive `toString` eligibility](../../src/ir/rel/expression.rs), [runtime formatting](../../src/ir/runtime/scalar/casts.rs). Treat this as an implementation-backed example, not a statement about every Cypher implementation's formatting.

## 2. Aggregation: native computation already exists

The current Cypher planner selects distinct aggregate kinds for language behavior: `sum` becomes `SumOrZero`, `avg` becomes `AvgOrNull`, and `collect` becomes `CollectNonNull`. Statistical aggregates and catalog-bound engine functions already have native implementations.

This means “enable native aggregation” should be decomposed into **implementation choice, empty-input identity, null filtering, ordering, equality keys, accumulator type, and result type**. These choices have very different costs.

Evidence: [Cypher aggregate selection](../../src/language/cypher/planner/lowering/project/aggregate.rs), [relational aggregate assembly](../../src/ir/rel/operators.rs), [native aggregate lowering](../../src/ir/rel/aggregates.rs), [native aggregate integration tests](../../tests/duckdb_native_aggregates.rs).

### Empty input and null-only groups

```cypher
MATCH (p:Person) WHERE p.amount > 1000000
RETURN count(*), sum(p.amount), avg(p.amount),
       stdev(p.amount), stdevp(p.amount), collect(p.amount)
```

Assuming no matching rows, a compatibility-preserving SQL shape is:

```sql
SELECT count(*) AS n,
       coalesce(sum(amount), 0) AS total,
       avg(amount) AS mean,
       coalesce(stddev_samp(amount), 0.0) AS sample_sd,
       coalesce(stddev_pop(amount), 0.0) AS population_sd,
       coalesce(list(amount) FILTER (WHERE amount IS NOT NULL),
                []::BIGINT[]) AS amounts
FROM person WHERE amount > 1000000;
```

Here `amount` is `BIGINT`; this abbreviated example has no observable collection order. Bare native calls drop the `coalesce` wrappers and collection filter:

```sql
SELECT count(*), sum(amount), avg(amount),
       stddev_samp(amount), stddev_pop(amount), list(amount)
FROM person WHERE amount > 1000000;
```

| Case | Current OrchidDB | Bare DuckDB 1.5.6 |
| --- | --- | --- |
| `count(*)`, zero input rows | `0` | `0` |
| `count(x)`, null-only input | `0` | `0` |
| `sum(x)`, empty or null-only | `0` | null |
| `avg/min/max(x)`, empty or null-only | null | null |
| `stdev(x)`, empty or singleton numeric sample | `0.0` | null |
| `stdevp(x)`, empty input | `0.0` | null |
| `stdevp(x)`, singleton numeric sample | `0.0` | `0.0` |
| `collect(x)`, empty input | `[]` | `list(x)` is null |
| `collect(x)`, one null input | `[]` | `list(x)` is `[NULL]` |

These DuckDB results were independently probed. Its aggregate documentation also distinguishes list null retention and empty-input identities. [DuckDB aggregates](https://duckdb.org/docs/current/sql/functions/aggregates)

**Recommendation:** retain the cheap identities and filters even when computation is native. They usually do not prevent hash aggregation or parallel execution. Dropping them creates downstream null branches for little expected gain.

Preserve cardinality too: an ungrouped aggregate over an empty input produces one identity row; grouping by a real input key produces no groups. A nullable `OPTIONAL MATCH` can supply a null-extended row, which is different from zero rows: `count(*)` and `count(p)` must not be conflated. These are not conversion-policy choices.

### Collection order: a much larger opportunity

```cypher
MATCH (p:Person)
WITH p ORDER BY p.score DESC, p.id
RETURN collect(p.name) AS names
```

The current path can assign an input ordinal and use an ordered aggregate. If the ordering expressions remain available, the essential behavior can be written directly:

```sql
SELECT coalesce(
         list(name ORDER BY score DESC NULLS FIRST, id ASC NULLS LAST)
           FILTER (WHERE name IS NOT NULL),
         []::VARCHAR[]
       ) AS names
FROM person;
```

Rewriting to `list(name)` would discard an explicitly requested order. **Do not do that under a general native policy.** Propagating the existing sort keys into the aggregate and avoiding a synthetic row-number column is instead a promising equivalent optimization.

For a query with no ordering requirement:

```cypher
MATCH (p:Person) RETURN collect(p.name) AS names
```

a policy allowing unspecified collection order could use:

```sql
SELECT coalesce(list(name) FILTER (WHERE name IS NOT NULL),
                []::VARCHAR[]) AS names
FROM person;
```

This retains the null/empty rules while potentially removing ordinal generation and aggregate sorting. The present implementation preserves incoming order more broadly; determine which guarantees are required by the selected language contract and which are incidental before removing them. Without an ordering guarantee, native list order may change with the execution plan or parallelism. Do not promise scan order from one observed run.

For a stored input sequence `[2, NULL, 1, 2]`, the current collection content is `[2, 1, 2]`. A fully raw native `list` retains the null as well. **Order relaxation and null retention must not share one flag.**

### `collect(DISTINCT ...)`: equality, order, and representatives

```cypher
MATCH (p:Person)
RETURN collect(DISTINCT p.name) AS names, count(*) AS n
```

The current implementation can compute a semantic partition key and a per-key first-occurrence rank, then filter inside the collection aggregate. For one distinct expression this conceptually involves:

```text
input ordering/ordinal
  → row_number over (partition by group keys, semantic value key
                     order by input ordinal)
  → list(original value order by ordinal)
      filter (value is not null and occurrence rank = 1)
```

A native equality/order policy for a homogeneous string column could reduce this to:

```sql
SELECT coalesce(list(DISTINCT name) FILTER (WHERE name IS NOT NULL),
                []::VARCHAR[]) AS names,
       count(*) AS n
FROM person;
```

The native aggregate still does deduplication work, but it can avoid external first-occurrence windows and semantic-key construction. It need not keep the original first-appearance order or representative type. Collation and heterogeneous numeric coercion can also alter which inputs are equivalent.

Do not deduplicate the whole input relation to optimize one aggregate: that changes the sibling `count(*)`, can change other aggregates, and can lose null-only groups. The existing code deliberately filters **inside** each collection aggregate.

Nested distinct collections currently retain a boundary until recursive identity is supported. DuckDB can compare lists natively, but accepting that equality is a semantic choice; it does not automatically implement Cypher predicate equality.

Evidence: [collect implementation](../../src/ir/rel/aggregates.rs), [rank construction and sibling aggregate preservation](../../src/ir/rel/operators.rs).

### Grouping and `COUNT(DISTINCT ...)`

```cypher
MATCH (p:Person)
RETURN p.amount AS amount, count(*) AS n
```

For a plain, compatible `BIGINT` property, ordinary grouping is the desired shape:

```sql
SELECT amount, count(*) AS n FROM person GROUP BY amount;
```

Some current paths group on an encoded semantic key and retain an original-value representative. Those keys unify numeric representations while preserving exact numeric values, recursively encode collections, and handle graph identity. Bypassing them can enable simpler engine hash keys and larger SQL regions.

However, grouping is an **equivalence relation**, not the same operation as a nullable predicate `x = y`. Current keys deliberately group nulls and normalize NaNs. Reusing the scalar NaN-equality rule as a grouping rule would be wrong.

An especially dangerous case is a mixed integer/float domain around `2^53`. Coercing `9007199254740993` to a double produces `9007199254740992`, potentially merging previously distinct values. This affects counts, groups, joins, and collection membership, not just display.

Prefer proving native grouping equivalent for a declared homogeneous domain. Graph nodes must still group by their complete identity: graph/source/table identity plus identifier as required by the mapping. Equal payloads or equal IDs in different sources do not make two nodes identical.

Evidence: [Cypher equivalence keys](../../src/ir/runtime/ops/distinct.rs), [graph grouping tests](../../tests/cypher_graph_group_sql.rs), [graph identity tests](../../tests/cypher_graph_identity_sql.rs).

### Accumulator width, overflow, and floating-point reduction

```cypher
UNWIND [9223372036854775807, 1] AS x RETURN sum(x)
```

The shared integer aggregation kernel uses checked signed 64-bit accumulation for ordinary integer values and reports overflow. DuckDB can return a wider result:

```sql
SELECT sum(x), typeof(sum(x))
FROM (VALUES (9223372036854775807::BIGINT), (1::BIGINT)) t(x);
-- 9223372036854775808, HUGEINT
```

This is a potential inconsistency to audit in current native SQL paths, which already emit `sum`. Do not assume the whole existing implementation enforces one accumulator policy everywhere. A new native profile must deliberately select the result type **before** planning and transport; emitting wide SQL and decoding into an old 64-bit schema is not a complete implementation.

For floating-point aggregation, algorithm and accumulation order matter independently of null handling:

```sql
SELECT sum(x ORDER BY ord), sum(x ORDER BY ord DESC)
FROM (VALUES (1, 1e16::DOUBLE),
             (2, -1e16::DOUBLE),
             (3, 1.0::DOUBLE)) t(ord, x);
-- 1.0, 0.0
```

The engine's parallel reductions, compensated aggregates such as `fsum`, or alternative statistical implementations may produce different low bits. They may improve accuracy and speed, but they cannot promise bit-identical results to a particular row-by-row fold. Comparisons against thresholds can amplify small changes. Document algorithm/accuracy expectations rather than treating tolerance as permission to return arbitrary results.

Evidence: [runtime aggregation and checked addition](../../src/ir/runtime/ops/aggregate.rs), [aggregate SQL selection](../../src/ir/rel/operators.rs). The examples above are local engine probes, not claims about an end-to-end OrchidDB result.

### Mixed-type min/max and nonnumeric inputs

Ordinary homogeneous numeric `min`/`max` already fit native aggregation well. Mixed language values introduce a different ordering:

```cypher
UNWIND [1, '99'] AS x RETURN min(x), max(x)
-- Shared runtime: string '99', integer 1
```

The runtime ranks strings below numbers when they are not otherwise comparable. Native SQL first needs a common column type:

```sql
SELECT min(x), max(x) FROM (VALUES (1), ('99')) t(x);
-- INTEGER 1, INTEGER 99
```

This changes both the selected values and their types. Coercing everything to text would impose yet another order. Native aggregation cannot preserve a heterogeneous language domain merely by renaming `min`/`max`.

Nonnumeric inputs to numeric aggregates need explicit validation too. Some shared runtime aggregation paths ignore nonnumeric values, while SQL binding may reject a string column outright. That implementation behavior should be audited against the selected language contract, not promoted into an implicit rule that native mode may cast or discard any input. Temporal/duration aggregation likewise requires the domain mapping discussed below.

### Statistical, native, and approximate aggregates

The repository already tests this useful Cypher extension:

```cypher
MATCH (p:Person)
RETURN median(p.amount), quantile_cont(p.amount, [0.25, 0.5]),
       arg_max(p.name, p.score)
```

```sql
SELECT median(amount), quantile_cont(amount, [0.25, 0.5]),
       arg_max(name, score)
FROM person;
```

The function catalog needs to own signatures, aggregate classification, arity, output types, and availability. Preserve nested-aggregate rejection and proper grouping; an engine function is not an opaque scalar string substitution.

Three nuances deserve explicit treatment:

1. **Percentile validation.** `percentileCont(x, p)` has an OrchidDB range check for `p` between zero and one. DuckDB's `quantile_cont` also accepts negative quantiles: on `[1, 2, 10]`, `quantile_cont(x, -0.25)` returned `6.0`, like positive `0.75`. Keep validation when mapping the language function, even if a directly named engine function exposes the broader domain.
2. **Nulls and ties in argument selection.** On `('a', 1), (NULL, 2)`, DuckDB `arg_max(name, score)` returned `'a'`; `arg_max_null(name, score)` returned null. Equal maximum scores need a tie contract before replacing an ordered top-one query with `arg_max`.
3. **Approximation.** `approx_count_distinct` and approximate quantiles can avoid exact state, but approximation should be explicitly requested by function or an independent accuracy policy. Do not silently turn `count(DISTINCT x)` into an estimate because “native mode” is enabled.

For example, where catalog support is declared:

```cypher
MATCH (p:Person) RETURN approx_count_distinct(p.name) AS estimate
```

```sql
SELECT approx_count_distinct(name) AS estimate FROM person;
```

No broad compatibility relaxation is necessary to expose this as an explicitly native function. Current support is demonstrated for median, quantiles, product, lists, and argument selection in [native aggregate tests](../../tests/duckdb_native_aggregates.rs); each additional engine function must still bind against the target catalog.

The `count_if` extension is another special case: current lowering implements truthiness and distinctness on original values, whereas DuckDB 1.5.6 rejects a direct integer argument to `count_if`. Casting to boolean before `DISTINCT` collapses different nonzero integers. Keep this mapping explicit rather than dropping its filter-based implementation.

## 3. Comparisons, arithmetic, and common types

### Numeric comparison and NaN

```cypher
WITH 9007199254740993 AS a, 9007199254740992.0 AS b
RETURN a = b
```

The current comparison logic preserves the distinct exact operands and fences unsafe SQL promotion. Native DuckDB comparison can lose that distinction:

```sql
SELECT 9007199254740993::BIGINT = 9007199254740992::DOUBLE;
-- true
```

DuckDB also accepts combinations such as `1 = '1'` and `true = 1` as true. Current Cypher lowering rejects unrelated SQL operand types at the primitive boundary and retains language-aware evaluation. Native coercion can help query heterogeneous warehouse inputs, but it can also create new join matches. [DuckDB comparison casting](https://duckdb.org/docs/current/sql/expressions/comparison_operators)

For a mapped double column containing NaN:

```cypher
MATCH (p:Person) RETURN p.score = p.score, p.score > 1.0
```

The current numeric predicate lowering checks NaNs and makes equality and ordered comparisons false when a numeric operand is NaN; inequality is true. Native SQL removes those guards:

```sql
SELECT 'NaN'::DOUBLE = 'NaN'::DOUBLE,
       'NaN'::DOUBLE > 1.0,
       'NaN'::DOUBLE <> 'NaN'::DOUBLE;
-- true, true, false
```

**Recommendation:** separate native numeric comparison from native parsing. A conversion policy that allows NaN must document how the remaining selected comparison policy treats it. Do not automatically switch joins or grouping along with casts.

Evidence: [comparison eligibility and NaN guards](../../src/ir/rel/expression.rs), [NaN runtime regression](../../tests/cypher_upstream_parity.rs).

### Arithmetic, division, and math errors

```cypher
RETURN 5 / 2
```

The ordinary integer runtime path uses integer division. Choosing the SQL spelling matters:

```sql
SELECT 5::BIGINT / 2::BIGINT, 5::BIGINT // 2::BIGINT;
-- 2.5, 2
```

Zero is a separate difference:

```sql
SELECT 1::BIGINT / 0::BIGINT, 1::BIGINT // 0::BIGINT;
-- positive infinity, NULL
```

The ordinary integer runtime division reports a divide-by-zero error. Adopting native division can therefore change both type and failure behavior. Using `//` is a useful compatibility building block but does not alone preserve the zero-divisor error. The repository also has typed and wide-number paths with their own behavior, so audit the actual operand type rather than imposing one rule on all numerics.

The same analysis applies to integer overflow, decimal scale, power/modulo, rounding ties, and math-domain errors. DuckDB `sqrt(-1.0)` throws; a host floating-point implementation may instead produce NaN. Some math calls already lower directly, so first distinguish existing mismatches from performance opportunities. No new mode is required merely to fix a wrong operator mapping.

Evidence: [shared arithmetic](../../src/ir/runtime/expr/arithmetic.rs), [math lowering](../../src/ir/rel/scalar_functions.rs), [SQL function adaptation](../../src/ir/rel/sql/functions.rs).

### `CASE`, `coalesce`, and heterogeneous collections

```cypher
UNWIND [true, false] AS flag
RETURN CASE WHEN flag THEN 1 ELSE '2' END AS x
```

Language-aware values can retain the integer `1` and string `'2'` as distinct types. The current primitive `CASE` path fences heterogeneous branches. Native DuckDB may choose one common type:

```sql
SELECT flag, CASE WHEN flag THEN 1 ELSE '2' END AS x
FROM (VALUES (true), (false)) t(flag);
-- INTEGER values 1 and 2
```

`coalesce('2', 1)` similarly returned integer `2`, while `coalesce('2'::VARCHAR, 1)` failed binding. Literal inference and an actual string column are not interchangeable examples.

**Benefit:** homogeneous engine values may avoid typed-union transport and enable native filters, lists, and grouping. **Cost:** per-row types, formatting, equality, or even query validity change. This should be an explicit schema/type policy, not an accidental consequence of relaxing one function. Do not stringify arbitrary mixed values to force SQL execution.

Evidence: [`CASE` and `coalesce` guards](../../src/ir/rel/expression.rs), [mixed-list guards](../../src/ir/rel/scalar_semantics.rs).

## 4. Lists, predicates, and structured values

### List membership: native SQL has two different answers

```cypher
RETURN 2 IN [1, null]
-- Current contract: null
```

```sql
SELECT 2 IN [1, NULL], 2 IN (1, NULL), list_contains([1, NULL], 2);
-- false, NULL, false
```

DuckDB list membership ignores a null member when deciding that a value is absent; tuple membership retains SQL's unknown result. This is documented separately for the two syntaxes. [DuckDB `IN`](https://duckdb.org/docs/current/sql/expressions/in)

The current dynamic-list lowering checks empty input, null list/value, a matching member, and a null member. A native list policy can collapse that expression, but `NOT (2 IN [1, null])` then becomes true instead of unknown and changes `WHERE` results.

For static homogeneous lists, tuple `IN` or an appropriate quantified subquery can be an equivalent alternative. It still needs correct empty-list behavior, exact operand comparisons, and evaluation count. Do not replace every `IN` with `list_contains` solely because it is shorter.

Evidence: [membership lowering](../../src/ir/rel/expression.rs), [null and evaluation regressions](../../tests/cypher_membership_sql.rs).

### Nested null equality

```cypher
RETURN [1, null] = [1, null], [1, null] = [2, null]
-- Current contract: null, false
```

```sql
SELECT [1, NULL] = [1, NULL], [1, NULL] = [2, NULL];
-- true, false
```

Current compatible list equality can require element-level comparison and counts of definite mismatches and unknown comparisons. A native list comparison can avoid that expansion, with a substantial potential reduction in plan complexity. But the result difference propagates into list membership, predicates, and nested structures. Keep predicate equality separate from grouping equivalence, where grouping repeated null-bearing values is legitimate.

Evidence: [list comparison lowering](../../src/ir/rel/list_comparison.rs), [list comparison tests](../../tests/cypher_list_comparison_range_sql.rs).

### Concatenation, indexing, and range boundaries

```cypher
WITH [1] AS xs RETURN xs + null
-- Current contract: null
```

```sql
SELECT list_concat([1], NULL::BIGINT[]), [1] || NULL::BIGINT[];
-- [1], NULL
```

Choosing `||` can preserve the null result for compatible list types; this is a native optimization without relaxation. Keep separate handling for scalar append/prepend, heterogeneous elements, and evaluation count.

Index and range translations are similarly mechanical:

| Cypher | Compatible DuckDB expression | What raw name reuse would change |
| --- | --- | --- |
| `[10,20][0]` | `list_extract([10,20], 1)` | SQL list indexing is one-based |
| `substring('abc',0,2)` | `substring('abc',1,2)` | SQL substring starts at one |
| `range(1,3)` | `generate_series(1,3)` | DuckDB `range(1,3)` excludes the stop and returns `[1,2]` |

These positive constant examples were executed. Dynamic negative indices, slices, zero steps, overflow, and string lengths need their existing validation. Do not sell off-by-one behavior as a useful “native semantics” optimization; the adaptation is cheap.

### Quantifiers and list comprehensions

```cypher
RETURN all(x IN [true, null] WHERE x)
-- Current three-valued contract: null
```

```sql
SELECT bool_and(x) FROM (VALUES (true), (NULL)) t(x);
-- true
```

Naively using `bool_and` or `bool_or` loses unknown values. Both return null on empty SQL input; Cypher `all([])` is true and `any([])` is false. `none` and `single` have their own unknown-count logic. The current lowering encodes these rules rather than treating quantifiers as ordinary nullable boolean aggregates.

A more attractive target is homogeneous list comprehension:

```cypher
WITH [1,2,null,3] AS xs
RETURN [x IN xs WHERE x > 1 | x + 1]
```

```sql
SELECT list_transform(
         list_filter([1,2,NULL,3], lambda x: x > 1),
         lambda x: x + 1);
-- [3,4]
```

This can avoid an unnest/reaggregate plan. For this bounded, deterministic example it is an equivalent optimization. To extend it, prove element types, null propagation, lambda evaluation behavior, bounds/overflow, and correlation. A blanket native policy should not authorize evaluating a volatile callback twice or changing mutation count.

Evidence: [quantifier lowering](../../src/ir/rel/quantifier_scalar.rs), [primitive comprehension lowering](../../src/ir/rel/primitive_list_comprehension.rs), [collection evaluation tests](../../tests/cypher_collection_evaluation_sql.rs).

### Maps versus engine structs

```cypher
WITH {x: 1} AS m RETURN m.missing
-- Current contract: null
```

```sql
SELECT {'x': 1}.missing;
-- Binder error: missing struct field
```

DuckDB `map_extract_value(MAP {'x':1}, 'missing')` returns null, but SQL maps have a common value type and are not a universal replacement for heterogeneous language maps. Structs can be efficient for fixed fields and enable projection pruning; maps can be appropriate for homogeneous dynamic keys. Use schema evidence to choose these representations. Preserve missing-property semantics and graph-property access separately.

Evidence: [map lowering](../../src/ir/rel/cypher_maps.rs), [null/missing property tests](../../tests/cypher_null_sql.rs).

## 5. Text, regex, ordering, and collation

### Unicode and text functions

```cypher
RETURN trim('\tabc\n'), toLower('İ'), toUpper('ß')
```

The shared text runtime uses Rust Unicode trimming and case conversion. DuckDB 1.5.6 produced these independently checked results:

| Operation | Shared runtime implementation | Native DuckDB |
| --- | --- | --- |
| Trim tab + `abc` + newline | `abc` | Tabs/newline retained by default `trim` |
| Lower `İ` | `i` + COMBINING DOT ABOVE | `i` |
| Upper `ß` | `SS` | `ẞ` |

Some current text functions already have direct SQL lowering; these differences need an audit before declaring the default path fully conformant. Native behavior may improve collation integration and remove emulation. It also changes normalized keys and matching behavior. Character count, code-point slicing, grapheme slicing, and byte count are different operations; choose deliberately, especially for emoji and combining characters.

Evidence: [shared text functions](../../src/ir/runtime/scalar/cypher.rs), [primitive text lowering](../../src/ir/rel/scalar_functions.rs). Rust case-conversion examples were checked separately during document validation.

### Regex engine selection versus match mode

For the intended whole-string Cypher regex contract:

```cypher
RETURN 'abc' =~ 'b'
-- Whole-string contract: false
```

```sql
SELECT regexp_full_match('abc', 'b'), regexp_matches('abc', 'b');
-- false, true
```

Selecting RE2 as the regex implementation need not change full-match into substring-match. That would be a separate operator change with no demonstrated benefit. Native regex differences worth documenting include Unicode character classes, supported pattern constructs, replacement syntax, flags, and invalid-pattern errors. DuckDB's `regexp_full_match('١', '\d')` returned false; a Rust Unicode-aware `\d` recognizes that digit. Backreferences also fail in the probed DuckDB expression.

The language regex contract and Java-style pattern syntax are described in the [Cypher string-operator reference](https://neo4j.com/docs/cypher-manual/25/expressions/predicates/string-operators/); the Rust and DuckDB implementations must be assessed against the selected contract rather than assumed interchangeable.

**Current-code audit finding:** Cypher `=~` lowers to `regex_match`, and the shared scalar helper reached by that name uses search semantics with comments describing Gremlin TextP. This needs verification/correction independently of any native policy. Do not describe a present mismatch as a deliberate compatibility option. The portable regex layer separately restricts eligibility to a common Rust/RE2/POSIX subset.

Evidence: [frontend regex routing](../../src/language/cypher/planner/lowering/project/expression.rs), [shared search helper](../../src/ir/runtime/scalar/strings.rs), [portable regex mappings](../../src/ir/functions/portable/regex.rs).

### Sort order and collation

```cypher
UNWIND [1, null, 2] AS x RETURN x ORDER BY x DESC
```

Preserving the language's descending null placement requires an explicit clause:

```sql
SELECT x FROM (VALUES (1), (NULL), (2)) t(x)
ORDER BY x DESC NULLS FIRST;
-- NULL, 2, 1
```

Bare `ORDER BY x DESC` returned `2, 1, NULL` under the probed DuckDB defaults. Settings can change these defaults. Keeping the explicit null modifier is inexpensive; omitting it is not an attractive optimization. [DuckDB ordering](https://duckdb.org/docs/current/sql/query_syntax/orderby)

Collation is a more substantive feature. Native `NOCASE` comparison makes `'A'` and `'a'` equal, unlike binary comparison. Accepting a column/session collation can integrate graph queries with the application's database semantics, but the choice must apply consistently to predicates, joins, grouping, distinctness, ordering, and min/max. Do not assume an engine applies a collation identically to every nested or aggregate operation; verify the chosen operations. [DuckDB collations](https://duckdb.org/docs/current/sql/expressions/collations)

Ties without a specified tie-breaker need not have a stable order. Explicit tie-breakers, upstream ordering consumed by `collect`, and ordering needed by `LIMIT` must remain observable.

## 6. Temporal semantics

### Dates: start with a constrained native domain

```cypher
MATCH (p:Person) RETURN date(p.day) AS day
```

A proposed native date policy for `VARCHAR` input is simply:

```sql
SELECT CAST(day AS DATE) AS day FROM person;
```

The current code already uses this SQL for proven canonical valid dates in a constrained domain, while retaining the temporal type identity. Unknown inputs, alternative grammars, and wide calendar values require more care. Native parsing can remove finite-domain proof requirements and enable date filters/grouping on mapped data, but accepted grammar, year range, invalid-date errors, and return representation may change.

Do not silently use `TRY_CAST` if the selected date-constructor contract throws on invalid input. Null-on-error is an additional policy decision. Also, a transported temporal string is an encoding, not an ordinary lexicographic date key.

Evidence: [canonical native date lowering](../../src/ir/rel/temporal_semantics/runtime_date.rs), [temporal operand guards](../../src/ir/rel/temporal_semantics.rs), [temporal transport](../../src/ir/temporal/transport.rs).

### Timestamps, zones, and precision

```cypher
RETURN localdatetime('2024-01-01T00:00:00.123456789')
```

Possible native representations have different precision:

```sql
SELECT CAST(TIMESTAMP '2024-01-01 00:00:00.123456789' AS VARCHAR) AS micros,
       CAST(TIMESTAMP_NS '2024-01-01 00:00:00.123456789' AS VARCHAR) AS nanos;
-- Fractional parts: .123456 and .123456789
```

The display casts make precision visible without relying on a client timestamp type that might itself discard nanoseconds. Use a declared precision mapping instead of assuming “timestamp” is lossless. A zoned language datetime also carries more than an instant. DuckDB `TIMESTAMPTZ` stores an instant at microsecond precision; it does not preserve the original named zone or offset. Converting two representations of the same instant can discard information later requested through formatting or zone fields. [DuckDB timestamp types](https://duckdb.org/docs/current/sql/data_types/timestamp)

**Benefit:** native predicates, arithmetic, sort/group keys, storage statistics, and possibly range pruning. **Cost:** lost precision or zone identity, different ambiguous/nonexistent local-time handling, and session-timezone-dependent operations. Prefer date-only and explicitly zone-free domains first. Native temporal results need declared Arrow/driver codecs, not just a new SQL expression.

### Durations and clocks

```cypher
RETURN duration('P1M'), duration('P30D')
```

Mapping these to native intervals exposes a subtle equality rule:

```sql
SELECT INTERVAL '1 month' = INTERVAL '30 days',
       DATE '2024-02-01' + INTERVAL '1 month',
       DATE '2024-02-01' + INTERVAL '30 days';
-- true, 2024-03-01, 2024-03-02
```

Equal engine intervals here have different effects when added to a date. A duration policy must cover component preservation, comparison, normalization, scaling, and calendar arithmetic together. Do not infer a safe native mapping from matching printed values or one arithmetic example.

Clock functions have a different issue. Language transaction, statement, and realtime clocks cannot all be mapped to one engine `current_timestamp`. Capture and transport the required clock value/scope, or explicitly select an engine clock contract. Never constant-fold a realtime call at compile time. Existing [clock classification](../../src/ir/temporal/clock_policy.rs) distinguishes calls with explicit constructor arguments from clock reads.

## 7. Native JSON semantics

This is an adjacent shared-function opportunity, beyond Cypher's core map syntax. OrchidDB has a typed `json.*` function family with portable parsing, paths, duplicate-key handling, and equality. The DuckDB SQL implementation sometimes needs much more than a function rename.

For example, the portable Cypher-callable family exposes:

```cypher
RETURN json.valid('{"x":NaN}'), json.valid('[1,]')
-- Portable contract: false, false
```

Under an explicitly native JSON policy, direct SQL is cheap:

```sql
SELECT json_valid('{"x":NaN}'), json_valid('[1,]');
-- true, true
```

The shared strict-validity adapter rejects those extensions. Native JSON can therefore remove regex validation, but changes which documents are accepted.

Duplicate members are a bigger structural opportunity:

```cypher
RETURN json.query('{"x":1,"x":2}', '$.x')
-- Portable contract: JSON value 2
```

```sql
SELECT json_extract('{"x":1,"x":2}', '$.x');
-- JSON value 1
```

The portable implementation resolves the final occurrence, yielding `2`, and can walk `json_each`/`json_tree` with ordering and deduplication to do so. Native extraction avoids that work but changes the value. JSON equality/containment also depends on numeric normalization, object member order, duplicate members, and array multiplicity.

Dynamic paths could benefit from native engine path evaluation where the portable lowering currently requires a literal singular path. The selected path language, absent path versus JSON null, scalar extraction errors, and returned SQL type all need a contract. Prefer explicitly engine-bound JSON functions or a JSON-specific profile; do not make language map semantics depend on whichever JSON parser happens to be installed.

Evidence: [typed JSON function family](../../src/ir/functions/json.rs), [SQL validation, path lookup, and canonicalization](../../src/ir/rel/sql/json.rs). The examples above were locally executed; native JSON is not proposed as an interchangeable serialization for graph identities.

## 8. Error behavior and optimizer freedom

An engine can inline projections, fold constants, reorder predicates, and eliminate unused work. Those transformations are only interchangeable when evaluation effects and errors are compatible.

```cypher
UNWIND [0] AS x
WITH 1 / x AS y
RETURN count(*)
```

A superficially equivalent SQL shape is:

```sql
SELECT count(*)
FROM (SELECT 1::BIGINT / x AS y FROM (VALUES (0)) t(x)) q;
```

The engine can avoid computing unused `y`; native division also has its own zero behavior. The runtime expression would fail if evaluated under the ordinary integer contract. The current aggregate-scan simplifier deliberately removes trivial naming projections, while keeping computed projections out of that rewrite. **Leaving a projection in emitted SQL is not by itself a guarantee that the engine evaluates it.**

Native error behavior could allow more aggressive SQL fusion, but it must be an independent, explicit choice. Even then, preserve writes, mutation multiplicity, rollback boundaries, authorization, graph identity, volatile-function declarations, and external-call effects. A conversion preference never authorizes suppressing a `DELETE`, duplicating a callback, or changing which rows an `OPTIONAL MATCH` preserves.

Error message text and error timing may be engine-specific for explicitly native functions. Keep OrchidDB's diagnostic categories where feasible, while distinguishing parse/bind failure, per-value conversion failure, execution failure, and unsupported routing. A catch-all “return null on any error” would hide programmer errors and is not equivalent to `TRY_CAST`.

Evidence: [aggregate scan rewrite and computed-projection test](../../src/ir/rel/sql/aggregate_scan.rs), [membership evaluation guards](../../tests/cypher_membership_sql.rs), [effect preservation tests](../../tests/cypher_effect_regions_sql.rs).

## 9. Proposed policy design

### Keep one named preset, backed by explicit semantic choices

Avoid a boolean `strict = false` that silently accumulates unrelated deviations. A connection or query can select a named, versioned preset, while the compiler records the actual operations granted native semantics. The following is **design notation, not a current API**:

```text
profile = orchid-v1                         # default

profile = duckdb-analytics-v1                # optional named preset
  string_to_float = duckdb_try_cast
  string_to_integer = orchid                 # rounding not relaxed implicitly
  string_to_boolean = orchid
  numeric_comparison = orchid
  aggregate_empty_results = orchid
  collection_nulls = orchid
  collection_order = preserve_explicit_order
  approximation = explicit_functions_only
  temporal_representation = declared_types_only
```

This preset should be small initially. Other policies can be added after their input domains, result types, failure behavior, and cross-expression interactions are specified. A profile needs an engine/version capability fingerprint; “native” without an owning engine is incomplete.

**The policy is semantic, not just a late SQL printing switch.** The compiler must apply it to constant folding, type inference, relational lowering, host execution, fallback, result decoding, and plan caches. Otherwise a literal `toFloat('NaN')` can fold to null while the same value from a column returns NaN.

### Resolve semantics before choosing execution location

An internal operation should carry enough information to distinguish, for example:

```text
OrchidStringToFloat(operand)
EngineTryCast(engine=duckdb, operand, result=DOUBLE)
```

These are proposed semantic identities, not public compile-only APIs. The shared IR owns the selected contract; adapters own SQL spelling, catalogs, implementations, and transport.

For the motivating float case, **the decision belongs before the semantic call is expanded into regex/cast/finite-check expressions**. Changing the existing `regexp_like → regexp_matches` spelling does not address it, and stripping guards from emitted SQL would lose type/domain context.

| Implementation boundary | Existing location | Proposed responsibility |
| --- | --- | --- |
| Language function identity | [Cypher expression lowering](../../src/language/cypher/planner/lowering/project/expression.rs) | Preserve the meaning of `cypher_convert.tofloat` until policy resolution |
| Shared semantic selection | [Relational options](../../src/ir/rel/mod.rs), [scalar semantics](../../src/ir/rel/scalar_semantics.rs) | Carry the effective policy; select a conformant helper or SQL expansion by capability, or a separately requested engine-owned operation |
| Folding and runtime execution | [Relational constants](../../src/ir/rel/constants.rs), [runtime conversion](../../src/ir/runtime/scalar/cypher.rs) | Honor the same selected semantics; do not precompute a native call with the old evaluator |
| SQL dialect and functions | [Dialect adapter](../../src/ir/rel/sql/dialect.rs), [AST function adaptation](../../src/ir/rel/sql/functions.rs) | Bind supported signatures, types, and SQL forms for the selected engine |
| Aggregates | [Aggregate lowering](../../src/ir/rel/aggregates.rs), [operator assembly](../../src/ir/rel/operators.rs) | Keep identities, filters, order, keys, and accumulator/result contracts explicit |
| Placement and exchange | [Island planner](../../src/ir/rel/island_planner.rs), [SQL rows](../../src/ir/rel/sql/rows.rs) | Preserve engine ownership and declared types across boundaries |

For an engine-owned operation, moving execution to a host kernel or another engine must either preserve that exact behavior or fail explicitly. Do not fall back to the old OrchidDB implementation merely because pushdown was unavailable. This matters for mixed SQL/runtime plans, federation, prepared parameters, constants, and empty-input execution.

Cache keys should include the effective profile, engine capabilities/version, relevant types, and settings that affect behavior. Caller-owned sessions can change timezone, collation, and other settings; bind those deliberately or invalidate/revalidate affected plans. Do not silently mutate global session defaults to implement a per-query preference.

DuckDB SQL in this document is not a portability promise for PostgreSQL or StarRocks. Each adapter needs its own capability declaration and behavioral evidence, including null-on-conversion-failure, accumulator width, regex, collation, and structured types. A missing native capability should not silently select an unrelated operation on another engine.

Keep policies language-scoped as well: the current shared runtime distinguishes Gremlin empty-reduction cardinality from Cypher's identity row, and RDF-term grouping is fenced until it has an identity-aware contract. A Cypher analytics preset must not automatically redefine those frontends.

### Explain the decision internally

Plan diagnostics should identify which rules changed and why an expression remains local, for example:

```text
toFloat(person.raw_amount): engine-owned string conversion, DuckDB DOUBLE
  accepts nonfinite strings; native whitespace/underscore grammar
collect(person.name): native list aggregate
  null filter and empty-list identity retained; explicit order preserved
group(person.id): exact native BIGINT key
  no relaxation needed
```

This stays within OrchidDB's internal SQL-lowering architecture. It does not require exposing a new public compile-only endpoint or putting implementation details into ordinary result rows.

## 10. What to implement first

| Priority | Work | Reason |
| --- | --- | --- |
| 0 | Fold known conversions; remove naming-only projections; keep `size(collect(x)) → count(x)` where valid | Equivalent simplification, no new contract |
| 0 | Audit existing regex routing, text Unicode behavior, integer division, and aggregate output widths | Existing discrepancies must not become accidental defaults for a new mode |
| 1 | Typed, optional compatibility functions backed by shared kernels, starting with string-to-float; retain conformant SQL alternatives | Compact SQL with full conformance, including portability to engines without installable functions |
| 1 | Opt-in native **string-to-float** conversion, with explicit result type and engine ownership | Narrow, measurable, directly addresses the motivating SQL |
| 1 | Preserve native aggregate execution with cheap identity/null wrappers | Captures most engine execution benefits while retaining expected results |
| 1 | Carry explicit collection sort keys directly; avoid synthetic order when unobservable | Potentially larger savings than scalar wrapper removal |
| 2 | Native keys for proven homogeneous scalar grouping and distinctness | Avoids encoded keys without changing graph identity |
| 2 | Native list lambdas for proven domains; declared date mappings | Removes larger plan structures while preserving semantics |
| 2 | Explicit native/approximate aggregate and JSON function access | Clear local semantics and useful engine capabilities |
| 3 | Independent integer/boolean parsing and formatting policies | Useful, but observable changes need separate adoption |
| Defer | Broad coercion, native nested equality, general temporal erasure, arbitrary error suppression | High semantic surface and harder fallback guarantees |

`size(collect(x)) → count(x)` is already recognized in the current Cypher aggregate rewrite; it is listed as an approach to extend and preserve, not new work. Likewise, simple scan aggregate cleanup and constrained native dates already exist.

There is little reason to relax null placement, zero-based language indexing, missing-property nulls, or explicit collection ordering solely to shorten SQL. The important decision is whether a rewrite removes real per-row work, a sort/window, an encoded key, data transfer, or an execution boundary.

## 11. Validation and measurements

### Local evidence gathered for this study

Native behavior was probed using `extension/vendor/test-env/bin/python` with DuckDB **1.5.6**, using an in-memory database and UTC for temporal probes. The default Homebrew CLI was an older **1.0.0**, so it was not used as evidence for the reported results.

The probes covered conversion grammar/domains, empty and null-only aggregates, singleton deviation, wide sums, native statistics, negative quantiles, argument-selection nulls, NaNs, mixed numeric comparisons, nested list equality, membership, list concatenation, boolean reduction, common-type inference, division, regex/text, timestamp precision, intervals, JSON duplicates/validity, collation, and missing struct fields. Expected engine errors were also checked for invalid casts, invalid quantiles, unsupported regex constructs, and negative square roots.

Repository tests were read as evidence for intended current behavior; that is different from claiming they were freshly executed. The original **37 SQL code blocks** were executed against a typed `person` fixture where needed, including the intentional missing-field binder error. The subsequently added compatibility-helper call is a design example and has not been implemented or executed. Rust casing, trimming, float formatting, and Unicode regex examples were separately verified. Local source links were checked. No compiler implementation or product configuration was changed by this study.

### Conversion cost experiment

A small synthetic experiment used one million `VARCHAR` rows: every sixteenth row was `'foo'`; other rows were decimal strings ending in `.25`. DuckDB ran with one thread, one warmup, and repeated timed executions. The guarded expression used the actual Unicode whitespace character set, underscore removal, decimal regex, `TRY_CAST`, and finite-result comparisons.

Initial seven-run medians were **845.70 ms** for the guarded shape and **22.71 ms** for a bare native cast followed by `sum`. Both returned `4687734375.0` on this deliberately overlapping input domain. This demonstrates that these guards can matter; it does **not** establish a general OrchidDB speedup, nor correctness on the inputs where semantics differ.

A follow-up decomposition used the same input and five timed executions after warmup on macOS ARM64:

| Expression work before the final `sum` | Median time |
| --- | ---: |
| Bare `TRY_CAST(raw AS DOUBLE)` | 22.38 ms |
| Default `trim`, underscore removal, and cast | 77.13 ms |
| Full Unicode trim set, underscore removal, and cast | 801.85 ms |
| Decimal regex, cast, and finite check, without normalization | 56.87 ms |
| Full normalization, regex, cast, and finite check | 825.84 ms |

Unicode normalization dominates this particular experiment. Investigate an equivalent fast path for already-normalized ASCII values before deciding that accepting different parsing is the only way to improve it. These are whole-query measurements, not additive per-function costs; the optimizer may share or duplicate expression work.

The benchmark is a hand-written SQL expression experiment, not generated-query performance. It includes normalization as well as regex work, and should not be described as “regex alone costs 37×.” It did not measure a conformant compatibility UDF. Before implementing the policy, compare the conformant helper, conformant SQL expansion, and native conversion on real generated plans and actual workload data. Conversion followed by a filter, grouping, joins, or serialization may have different costs and consequences.

Reproduce the decomposition with the bundled Python environment, or another environment pinned to DuckDB 1.5.6:

```python
import statistics
import time
import duckdb

db = duckdb.connect()
db.execute("SET threads=1")
db.execute("""
  CREATE TEMP TABLE cast_input AS
  SELECT CASE WHEN i % 16 = 0 THEN 'foo'
         ELSE CAST(i % 10000 AS VARCHAR) || '.25' END AS raw
  FROM range(1000000) t(i)
""")
codepoints = [9, 10, 11, 12, 13, 32, 133, 160, 5760,
              *range(8192, 8203), 8232, 8233, 8239, 8287, 12288]
whitespace = "'" + "".join(map(chr, codepoints)) + "'"
normalized = f"replace(trim(raw, {whitespace}), '_', '')"
pattern = r"^[+-]?([0-9]+(\.[0-9]*)?|\.[0-9]+)([eE][+-]?[0-9]+)?$"

def guarded(token):
    return f"""
      WITH t AS (SELECT {token} AS token FROM cast_input),
      p AS (SELECT token, TRY_CAST(token AS DOUBLE) AS v FROM t)
      SELECT sum(CASE WHEN regexp_matches(token, '{pattern}')
                       AND v > '-Infinity'::DOUBLE
                       AND v < 'Infinity'::DOUBLE THEN v END)
      FROM p
    """

queries = {
    "native": "SELECT sum(TRY_CAST(raw AS DOUBLE)) FROM cast_input",
    "ascii_normalization": "SELECT sum(TRY_CAST(replace(trim(raw), '_', '') AS DOUBLE)) FROM cast_input",
    "unicode_normalization": f"SELECT sum(TRY_CAST({normalized} AS DOUBLE)) FROM cast_input",
    "regex_and_finite_only": guarded("raw"),
    "full_guard": guarded(normalized),
}
print("DuckDB", duckdb.__version__)
for name, query in queries.items():
    assert db.execute(query).fetchone()[0] == 4687734375.0
    samples = []
    for _ in range(5):
        start = time.perf_counter()
        db.execute(query).fetchall()
        samples.append(1000 * (time.perf_counter() - start))
    print(name, statistics.median(samples), "ms")
```

### Acceptance criteria for an implementation

Use three kinds of tests: equivalent optimizations must match the current contract; deliberate relaxations must have exact native-profile expectations; unsupported combinations must fail explicitly instead of changing behavior through fallback.

| Family | Required cases |
| --- | --- |
| Conversions | Null, empty, invalid, signs, exponents, underscore placement, Unicode whitespace, NaN/infinity, overflow, exact large integers, invalid input types |
| Aggregates | Zero rows, null-only groups, singleton samples, grouped versus ungrouped, mixed sibling aggregates, overflow, result width, ordered collection, distinct collection |
| Equality/grouping | NaN, signed zero, mixed numerics near `2^53`, nested nulls, graph IDs from different sources, collation |
| Lists/quantifiers | Null versus empty lists, unknown predicates, duplicates, negative/out-of-range bounds, throwing and volatile inputs |
| Temporal/JSON | Precision, zone preservation, DST boundaries, calendar ranges, interval components, duplicate keys, absent versus JSON null, invalid documents and paths |
| Execution placement | Literals, parameters, mapped columns, constants propagated through `WITH`, all-SQL, mixed execution, fallback, federation, engine-version changes |
| Effects/errors | Bind versus execution errors, short-circuiting, unused expressions, mutation count, rollback, callbacks |

Measure SQL size and preparation time, number of runtime boundaries and transfers, sorts/windows, peak memory, rows processed, and wall time. Record result types as well as values. Test one- and multi-thread execution where order or floating-point accumulation matters. Sampling data is not a proof that an arbitrary source column stays within a safe semantic domain.

The first native-string-conversion release should demonstrate that the same input gets the same selected semantics whether it appears as a literal, a parameter, or a table value. That is the minimum guarantee needed to make the feature understandable.
