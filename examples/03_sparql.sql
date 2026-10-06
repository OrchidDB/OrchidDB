-- Uses the people table created by 01_social.sql.
-- The existing RDF mapping protocol is available through a DuckDB table function.
SELECT * FROM orchid_query($$
{
  "version": 1,
  "language": "sparql",
  "query": "SELECT ?name WHERE { ?person <urn:name> ?name } ORDER BY ?name",
  "tables": [{"name": "people"}],
  "rdf": [{
    "table": "people",
    "subject": {"kind": "template", "prefix": "urn:person:", "columns": ["id"]},
    "predicate": {"kind": "constant", "value": "urn:name"},
    "object": {"kind": "literal", "column": "name"}
  }]
}
$$);
-- Alice, Bob, Cara, each with RDF kind/datatype/language metadata.
