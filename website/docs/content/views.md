# Iceberg, Lance, and SQL sources

Load and configure source extensions with DuckDB's ordinary interfaces:

```sql
INSTALL iceberg;
LOAD iceberg;
INSTALL lance;
LOAD lance;
ATTACH '/data/lance' AS vectors (TYPE lance);
CREATE VIEW authorship AS
SELECT * FROM iceberg_scan('/data/authorship/metadata/v1.metadata.json');

CREATE PROPERTY GRAPH knowledge
VERTEX TABLES (
  people KEY (id) LABEL Person,
  vectors.main.documents AS docs KEY (id) LABEL Document PROPERTIES (title)
)
EDGE TABLES (
  authorship KEY (id)
    SOURCE KEY (person_id) REFERENCES people (id)
    DESTINATION KEY (document_id) REFERENCES docs (id)
    LABEL AUTHORED
);
CYPHER knowledge
MATCH (p:Person)-[:AUTHORED]->(d:Document) RETURN p.name, d.title;
```

This assumes a `people` table, a Lance `documents` dataset, and Iceberg metadata
at the example path. Attached Iceberg catalog tables can be used directly too.

Schema discovery binds `SELECT * ... LIMIT 0` without executing source scans;
external binding may fetch metadata. DuckDB and its source extensions own file
access, credentials, indexes, and storage behavior. Use `EXPLAIN CYPHER` to inspect
source scans. Atomicity across catalogs depends on their storage systems.
