-- Self-contained RAG graph. Load Orchid first; no model or external data needed.
-- Token vectors are supplied explicitly to keep retrieval/reranking reproducible.
CREATE TABLE questions(id BIGINT PRIMARY KEY, tenant_id BIGINT, text VARCHAR, tokens FLOAT[2][]);
CREATE TABLE documents(id BIGINT PRIMARY KEY, tenant_id BIGINT, title VARCHAR, body VARCHAR, tokens FLOAT[2][]);
CREATE TABLE authors(id BIGINT PRIMARY KEY, name VARCHAR);
CREATE TABLE written_by(id BIGINT PRIMARY KEY, document_id BIGINT, author_id BIGINT);
INSERT INTO questions VALUES
    (1, 10, 'duckdb', [[1,0],[0,1]]),
    (2, 20, 'duckdb', [[1,0],[0,1]]);
INSERT INTO documents VALUES
    (101, 10, 'Quick overview', 'duckdb duckdb', [[0.5,0.5]]),
    (102, 10, 'Detailed guide', 'duckdb', [[1,0],[0,1]]),
    (103, 10, 'Unrelated', 'gardening flowers', [[5,5]]),
    (104, 20, 'Other tenant', 'duckdb', [[10,10]]);
INSERT INTO authors VALUES (1, 'Alice'), (2, 'Bob');
INSERT INTO written_by VALUES (1,101,1), (2,102,2), (3,103,1), (4,104,2);

CREATE PROPERTY GRAPH rag
VERTEX TABLES (
    questions KEY (id) LABEL Question PROPERTIES (id, tenant_id, text, tokens),
    documents KEY (id) LABEL Document PROPERTIES (id, tenant_id, title, body, tokens),
    authors KEY (id) LABEL Author PROPERTIES (name)
)
EDGE TABLES (
    written_by KEY (id)
        SOURCE KEY (document_id) REFERENCES documents (id)
        DESTINATION KEY (author_id) REFERENCES authors (id)
        LABEL WRITTEN_BY
)
COMPUTED EDGES (
    RELEVANT_TO SOURCE Question DESTINATION Document
        CANDIDATES (
            WHERE (source.tenant_id = target.tenant_id)
            PROPERTIES (text.bm25(source.text, target.body) AS lexical)
            ORDER BY (lexical DESC)
            LIMIT PER SOURCE 2
        )
        PROPERTIES (vector.maxsim(source.tokens, target.tokens) AS score)
        ORDER BY (score DESC)
        LIMIT PER SOURCE 1
);

-- BM25 chooses two eligible documents; MaxSim reranks those candidates.
-- The unrelated document has a higher MaxSim score but is not a candidate.
-- The other tenant's document is excluded before candidate selection.
CYPHER rag
MATCH (q:Question)-[r:RELEVANT_TO]->(d:Document)-[:WRITTEN_BY]->(a:Author)
WHERE q.id = 1
RETURN d.title AS title, a.name AS author, r.score AS score;
-- Detailed guide | Bob | 2.0

GREMLIN rag g.V().has('Question', 'id', 1).out('RELEVANT_TO').values('title');
-- Detailed guide

-- Candidate-stage properties are also available on the computed edge.
CYPHER rag
MATCH (q:Question)-[r:RELEVANT_TO]->(d:Document)
WHERE q.id = 1
RETURN d.title, r.lexical, r.score;

DESCRIBE PROPERTY GRAPH rag;
