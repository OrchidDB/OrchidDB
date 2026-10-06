# Glossary

| Term | Meaning |
| --- | --- |
| Graph definition | Persisted mapping from labels and endpoints to DuckDB sources |
| Managed graph | Graph records stored through Orchid's shared persistence codecs |
| Source | A DuckDB table or view, including an attached storage extension |
| Kernel | Existing graph operation hosted by a DuckDB physical operator |
| Graph IR | Shared language-independent graph compilation representation |
| RDF term | IRI, blank node, or literal with identity metadata |
| Native statement | Unquoted `CYPHER graph ...` or `GREMLIN graph ...` syntax |
| Mapping protocol | Versioned advanced request accepted by `orchid_query` |
