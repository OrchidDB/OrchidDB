# Search

Use vector similarity, BM25 full-text retrieval, and ColBERT-style MaxSim scoring
as graph relationships. Traverse search results with Cypher or Gremlin and
compose them with the rest of your graph.

This guide covers relationship declarations, native scoring, indexed retrieval
with PostgreSQL/pgvector and duckdb-lance, and execution across SQL islands.
See the [mapping reference](mapping-reference.md) for ordinary nodes and stored
edges, and [SQL generation and execution](sql-compiler.md) for compiler requests.

{{#include ../../../docs/computed-relationships-and-search.md:3:}}
