-- Ordinary Cypher retrieval. Load Orchid before running this script.
-- Applications bind the three query inputs as named parameters (see README).
CREATE TABLE chunks(id BIGINT PRIMARY KEY, text VARCHAR, embedding FLOAT[2], token_vectors FLOAT[2][]);
CREATE TABLE content(id BIGINT PRIMARY KEY, title VARCHAR);
CREATE TABLE has_chunk(id BIGINT PRIMARY KEY, content_id BIGINT, chunk_id BIGINT);
INSERT INTO chunks VALUES
    (1, 'duckdb duckdb', [1,0], [[0.5,0.5]]),
    (2, 'duckdb', [0.8,0.2], [[1,0],[0,1]]),
    (3, 'gardening flowers', [0,1], [[5,5]]);
INSERT INTO content VALUES (10, 'Quick overview'), (20, 'Detailed guide'), (30, 'Gardening');
INSERT INTO has_chunk VALUES (1,10,1), (2,20,2), (3,30,3);
CREATE PROPERTY GRAPH knowledge
VERTEX TABLES (
    chunks KEY(id) LABEL Chunk PROPERTIES(id, text, embedding, token_vectors),
    content KEY(id) LABEL Content PROPERTIES(id, title)
)
EDGE TABLES (
    has_chunk KEY(id) SOURCE KEY(content_id) REFERENCES content(id)
    DESTINATION KEY(chunk_id) REFERENCES chunks(id) LABEL HAS_CHUNK
);

CYPHER knowledge
MATCH (c:Chunk)
WITH c,
     text.bm25('duckdb', c.text) AS lexical,
     vector.cosine_similarity([1.0,0.0], c.embedding) AS semantic
WITH c, lexical / (1 + lexical) + (semantic + 1) / 2 AS candidate_score
ORDER BY candidate_score DESC
LIMIT 2
WITH c, vector.maxsim([[1.0,0.0],[0.0,1.0]], c.token_vectors) AS score
ORDER BY score DESC
LIMIT 1
MATCH (document:Content)-[:HAS_CHUNK]->(c)
RETURN document.title AS title, c.text AS text, score
ORDER BY score DESC;
-- Detailed guide | duckdb | 2.0
