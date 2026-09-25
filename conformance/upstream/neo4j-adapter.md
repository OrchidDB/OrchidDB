# Neo4j adapter contracts

The peer is Neo4j Community 2026.09.0, running `CYPHER 5`. The pinned upstream
openCypher scenarios and their assertions are unchanged.

## Error evidence

`neo4j_errors.py` takes only the server's status code, message, GQL cause chain,
and observed execution phase. It never receives a case ID, query, or expected
assertion. Specific GQL conditions are mapped to TCK detail names. Where the
condition is broader than the TCK detail, the observed diagnostic distinguishes
the detail. Unknown messages remain adapter errors.

The original Neo4j status title is retained. A current Neo4j `SyntaxError`
where the pinned TCK expects `TypeError`, for example, is an assertion failure.
The adapter does not substitute the expected error class or phase.

After an engine error, the adapter submits `EXPLAIN` with the same parameters.
If compilation fails the phase is compile time; otherwise it is runtime.
`EXPLAIN` does not execute graph mutations. This follows Neo4j's own TCK
adapter's phase observation, rather than inferring phase from an expected
assertion or from whether Bolt happened to raise during RUN or PULL.

Source references used to establish the diagnostic contracts:

- [Neo4j GQL conditions, revision 54a7dcf7c2501b31866199143364c5332da8936f](https://github.com/neo4j/neo4j/blob/54a7dcf7c2501b31866199143364c5332da8936f/community/neo4j-gql-status/src/main/java/org/neo4j/gqlstatus/GqlStatusInfoCodes.java).
- [Neo4j TCK phase observation, revision 51de0d4034999c839a934208ebc16dae80b41c6f](https://github.com/neo4j/neo4j/blob/51de0d4034999c839a934208ebc16dae80b41c6f/community/cypher/spec-suite-tools/src/test/scala/cypher/features/Neo4jAdapter.scala).
- [Neo4j's historical TCK diagnostic meanings at the same revision](https://github.com/neo4j/neo4j/blob/51de0d4034999c839a934208ebc16dae80b41c6f/community/cypher/spec-suite-tools/src/test/scala/cypher/features/Neo4jExceptionToExecutionFailed.scala).

## Temporal transport

The Python driver hydrates Bolt time offsets through `pytz.FixedOffset`, which
discards seconds, and historical named zones through `pytz`, which rounds
historical offsets to minutes. Its date objects also restrict year ranges.

The peer adapter installs temporal hydration hooks before opening connections.
They decode Bolt epoch days/seconds/nanoseconds directly; named zones use
`zoneinfo`. This preserves offset seconds, nanosecond precision, and wide years
for dates and local/fixed-offset datetimes. Values are then expressed in the
same TCK notation used for other engines. Queries and expected values are not
rewritten. Named zones outside Python's date range remain transport limitations.

These are private driver hooks, so the report records the driver version and
hashes all adapter modules. `test_neo4j_adapter.py` covers transport and phase
observation. The four upstream offset/historical-zone scenarios are also
exercised through the real Bolt connection in the full suite.

## Procedure fixtures

The fifty upstream `GIVEN there exists a procedure` cases still require a
server-side Neo4j fixture provider and remain explicitly skipped. The adapter
does not simulate their `CALL` queries. Ordinary unknown-procedure assertions
are executed and classified using the server's diagnostic.
