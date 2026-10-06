# Mapping reference

```sql
CREATE PROPERTY GRAPH social
VERTEX TABLES (
  people AS persons KEY (id) LABEL Person PROPERTIES (name, age)
)
EDGE TABLES (
  follows KEY (id)
    SOURCE KEY (src) REFERENCES persons (id)
    DESTINATION KEY (dst) REFERENCES persons (id)
    LABEL FOLLOWS PROPERTIES (since)
);
```

`KEY` is required for each mapped vertex and edge source. Composite keys are
ordered; endpoints must match the complete referenced key and its column types.
Separate edge keys preserve parallel relationships. Keys do not create constraints.

`AS` names a source alias. `PROPERTIES (column AS property, ...)` selects and
renames columns. Omitting `PROPERTIES`, or using `PROPERTIES ALL COLUMNS`, exposes
current columns; `NO PROPERTIES` exposes none. Each label maps to one source.

Graph names may be `catalog.schema.graph`. Unqualified sources resolve relative
to the graph's catalog and schema. Use source views to cast unsupported types.

For advanced existing mappings and RDF sources, use the [mapping protocol](sql-compiler.md).
This DDL is inspired by SQL/PGQ and does not claim full SQL/PGQ compatibility.
