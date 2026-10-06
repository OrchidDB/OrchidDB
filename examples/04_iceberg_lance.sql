-- Uses people from 01_social.sql and existing external datasets.
-- Replace /data paths with your own storage locations before running.
-- Lance documents columns: id BIGINT, title VARCHAR.
-- Iceberg authorship columns: id BIGINT, person_id BIGINT, document_id BIGINT.
INSTALL iceberg;
LOAD iceberg;
INSTALL lance;
LOAD lance;
ATTACH '/data/lance' AS vectors (TYPE lance);
CREATE VIEW authorship AS
SELECT * FROM iceberg_scan('/data/authorship/metadata/v1.metadata.json');

CREATE PROPERTY GRAPH knowledge
VERTEX TABLES (
    people KEY (id) LABEL Person PROPERTIES (name),
    vectors.main.documents AS docs KEY (id) LABEL Document PROPERTIES (title)
)
EDGE TABLES (
    authorship KEY (id)
    SOURCE KEY (person_id) REFERENCES people (id)
    DESTINATION KEY (document_id) REFERENCES docs (id)
    LABEL AUTHORED
);
CYPHER knowledge
MATCH (p:Person)-[:AUTHORED]->(d:Document)
WHERE p.name = 'Alice'
RETURN d.title AS title;
EXPLAIN CYPHER knowledge MATCH (p:Person)-[:AUTHORED]->(d:Document) RETURN p.name, d.title;
