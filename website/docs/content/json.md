# JSON documents and relationships

Use DuckDB views to expose JSON fields as typed graph columns. Explicit casts make
keys and endpoint types clear during schema discovery. For example, a view can
select `CAST(json_extract_string(payload, '$.id') AS BIGINT) AS id` and
`json_extract_string(payload, '$.name') AS name` from an application table.

Map the view with `CREATE PROPERTY GRAPH`, just like a physical table. Flatten
nested relationships in another view and provide stable edge keys and endpoint
columns. Orchid reuses its existing mapping compiler; DuckDB executes the JSON SQL.
See [mapping reference](mapping-reference.md) and [views](views.md).
