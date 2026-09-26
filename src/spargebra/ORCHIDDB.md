# Spargebra 0.4.7 integration

Source: crates.io `spargebra` 0.4.7, Oxigraph revision
`0e81a29d27575ab69e7f85ea9c7f1058b277fcd0`, directory `lib/spargebra`.
The upstream MIT and Apache 2.0 licenses are included.

Local parser corrections:

- Keep the join around a nested group filter. Removing the empty left BGP
  before OPTIONAL translation incorrectly promotes an inner filter to the
  left-join condition, changing its variable scope.
- Apply the longest-token rule to `<`: a complete IRIREF cannot instead be
  parsed as a comparison followed by an expression.
- Accept case-insensitive boolean keywords, preserving strings and IRI names
  through the grammar's existing token rules.

These changes apply to production parsing. The conformance adapter does not
rewrite queries. Regressions live in `tests/sparql_typed_algebra.rs`.
