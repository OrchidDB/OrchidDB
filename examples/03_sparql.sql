-- Uses the people table created by 01_social.sql.
-- Register the RDF schema once on this connection.
CALL orchid_register_schema('people_rdf', $$
{
  "tables": [{"name": "people"}],
  "rdf": [{
    "table": "people",
    "subject": {"kind": "template", "prefix": "urn:person:", "columns": ["id"]},
    "predicate": {"kind": "constant", "value": "urn:name"},
    "object": {"kind": "literal", "column": "name"}
  }]
}
$$);
SELECT * FROM orchid_query('people_rdf',
  'SELECT ?name WHERE { ?person <urn:name> ?name } ORDER BY ?name',
  language := 'sparql');
-- Alice, Bob, Cara, each with RDF kind/datatype/language metadata.
