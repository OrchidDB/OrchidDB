-- Ordinary Cypher retrieval. Load Orchid before running this script.
-- Then execute 07_parameterized_rag.sql with named parameters (see README).
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

