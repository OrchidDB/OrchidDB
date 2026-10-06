# Core concepts

A graph definition maps vertex and edge labels to DuckDB tables or views. Keys
identify elements; edge endpoint columns refer to vertex keys. Properties expose
selected columns. The definition does not copy source rows or enforce uniqueness.

DuckDB owns connections, transactions, schema discovery, and relational execution.
Orchid supplies graph language compilation and existing graph kernels. Managed
graphs use Orchid's existing persistence codecs inside the host database.

Cypher and Gremlin share property graph mappings. SPARQL uses explicit RDF mappings
that preserve term identity. See [mapping reference](mapping-reference.md).
