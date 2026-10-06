-- Run 07_rag_setup.sql first. Execute this file through DuckDB with the
-- named parameters in the README. The placeholders are real bound parameters.
CYPHER knowledge
MATCH (c:Chunk)
WITH c,
     text.bm25($query_text, c.text) AS lexical,
     vector.cosine_similarity($query_embedding, c.embedding) AS semantic
WITH c, lexical / (1 + lexical) + (semantic + 1) / 2 AS candidate_score
ORDER BY candidate_score DESC
LIMIT 2
WITH c, vector.maxsim($query_tokens, c.token_vectors) AS score
ORDER BY score DESC
LIMIT 1
MATCH (document:Content)-[:HAS_CHUNK]->(c)
RETURN document.title AS title, c.text AS text, score
ORDER BY score DESC;
-- Detailed guide | duckdb | 2.0
