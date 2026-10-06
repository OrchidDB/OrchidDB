# RDF storage and updates

`rdf_sources` accepts the existing typed quad source mapping. Preserve a lexical
value, kind, datatype, and language column for each term. For example:

```python
import json
connection.execute('CREATE TABLE terms(g VARCHAR, s VARCHAR, sk VARCHAR, sd VARCHAR, sl VARCHAR, p VARCHAR, pk VARCHAR, pd VARCHAR, pl VARCHAR, o VARCHAR, ok VARCHAR, od VARCHAR, ol VARCHAR)')
request = dict(
    version=1, language='sparql', tables=[{'name': 'terms'}],
    rdf_sources=[dict(
        table='terms', subject_column='s', predicate_column='p',
        object_column='o', graph_column='g', writable=True,
        typed_terms=[dict(value=t, kind=t+'k', datatype=t+'d', language=t+'l')
                     for t in ('s', 'p', 'o')],
    )],
    query='INSERT DATA { <urn:s> <urn:p> "hello" }',
)
connection.execute('CALL orchid_sparql_update(?)', [json.dumps(request)])
request['query'] = 'SELECT ?o WHERE { <urn:s> <urn:p> ?o }'
rows = connection.execute('SELECT * FROM orchid_query(?)', [json.dumps(request)]).fetchall()
```

Writes require `writable=True` and use the caller's DuckDB transaction.
`rdf_graph_names` additionally maps a named graph registry when required.
These are the existing RDF mappings and update kernels, reused by the extension.
