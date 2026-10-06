# Optional SpiceDB row authorization

## Scope and contract

Implement inside the DuckDB extension; no MCP server, federation service, new
executor, or authorization language. Preserve unrelated dependent-execution work.
Use the existing graph mappings, SQL compiler, DuckDB secret manager, and SpiceDB
HTTP API. Product documentation stays in README.md.

- Trusted host sets connection-local subject and optional caveat context/freshness.
- Persist graph policy in CREATE PROPERTY GRAPH / ALTER PROPERTY GRAPH.
- Map each vertex's resource key to a SpiceDB object. A Slack message can use
  channel_id, and chunks can use that same channel_id. Deduplicate checks.
- Default deny; explicit PUBLIC vertices permitted. Stored edges require both
  endpoints. Computed edges rank only authorized vertices (including BM25 corpus).
- Missing identity/provider, RPC errors, conditional decisions, and unsupported
  protected execution paths must never grant access. Protected writes rejected.
- Resolve permission decisions at execution, never freeze them in prepared plans.
- Graph access is the enforcement scope; unrestricted host SQL remains trusted.
- No persistent decision cache. Keep one authorization revision per statement.

## Implementation sequence

1. Add policy data structures/parser/validation and native authorization syntax.
2. Add DuckDB secret type and connection-local context with explicit reset.
3. Implement bounded, batched SpiceDB checks through its HTTP API; TLS by default,
   explicit localhost HTTP for tests; strict response checking and timeouts.
4. Apply source filters to nodes and stored-edge endpoints using existing mapping
   and compiler machinery. Reject unsupported protected mutation/mapping bypasses.
5. Validate all query entry points, prepared statements, and functions/macros.
6. Run real SpiceDB with memory datastore locally. Test user/team channel grants,
   denial, revocation, reset/pooling, conditional decisions, endpoint filtering,
   Cypher/Gremlin, computed RAG, and actual Iceberg/Lance sources.
7. Run relevant existing extension tests once after focused checks pass. Update
   README and add a runnable SQL example. Record exact validation evidence here.

## Completion evidence

Completed all seven implementation steps.

- DuckDB 1.5.6 extension builds and loads locally; existing compiler/mapped sources
  and DuckDB execution reused. Core compiler change adds filtered edge sources.
- Real SpiceDB 1.56.2, memory datastore and loopback HTTP gateway. Release archive
  SHA-256: 1421ff9226202862d423ad18279cc21cc7621420f5984e9f2cf87d1702e8879b.
- Complete extension acceptance: 86 tests passed, including the initial 14 real
  authorization tests and actual Iceberg 890b78a9c / Lance 2913169 sources.
  Log: /tmp/orchid-auth-acceptance.log.
- Additional real-server coverage passes: >2,048 distinct resources across bulk
  batches at one revision; server metrics verify one bulk RPC for 5,000 rows
  sharing existing channels; prepared policy changes; residual Gremlin node
  properties. The authorization module now contains 15 integration tests.
  Final authorization suite: all 15 passed, including these boundary checks.
  Log: /tmp/orchid-auth-final.log. Together with the unchanged 72 existing
  integration cases above, 87 integration cases are verified.
- Existing extension compiler unit tests: 5 passed.
  Log: /tmp/orchid-auth-compiler-tests.log.
- Optional build: cargo check --no-default-features passed.
  Log: /tmp/orchid-auth-optional.log.
- README updated; examples/06_authorization.sql executed against real SpiceDB.
- No worktrees or subagents. Existing dependent-execution edits preserved.

## Deliberate boundaries

Protected mapped Cypher/Gremlin graphs are read-only. Managed graphs and advanced
caller-supplied RDF/property mappings are not authorization entry points and are
rejected in authorized sessions. The host retains trusted SQL/administration and
responsibility for installed UDFs/extensions; no claim of DuckDB-wide SQL RLS.
Scalar macros with subqueries are rejected using DuckDB's macro ASTs, including
transitive calls/defaults. Credentials use DuckDB secrets; HTTP transport uses
reqwest with TLS verification, no redirects, bounded response size and timeouts.
No persistent decision cache or shared cross-system transaction is introduced.
