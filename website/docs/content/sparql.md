# SPARQL

The existing SPARQL query implementation runs through the shared mapping protocol.
Using the `people` table from the quickstart:

```python
import json

request = {
    "version": 1,
    "language": "sparql",
    "query": "SELECT ?name WHERE { ?person <urn:name> ?name }",
    "tables": [{"name": "people"}],
    "rdf": [{
        "table": "people",
        "subject": {"kind": "template", "prefix": "urn:person:", "columns": ["id"]},
        "predicate": {"kind": "constant", "value": "urn:name"},
        "object": {"kind": "literal", "column": "name"},
    }],
}
rows = connection.execute("SELECT * FROM orchid_query(?)", [json.dumps(request)]).fetchall()
```

The existing baseline passes 974 applicable assertions; 77 cases remain skipped and
74 are not applicable. This migration does not add SERVICE execution, reasoning,
or a remote SPARQL protocol. See [RDF storage](rdf.md) for typed term mappings.
