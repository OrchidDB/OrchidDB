# Results and native values

Read results using ordinary DuckDB APIs: rows, Arrow batches, or a dataframe.
Mapped scalar projections preserve SQL types whenever the compiled plan can do so.

Graph elements, paths, heterogeneous collections, and managed dynamic values use
Orchid's shared typed carrier. Apply `__orchiddb_value_json(value)` to inspect its
typed JSON representation from SQL. The representation preserves graph identity,
nested values, and value kinds; it is not just a stringified display value.

The bridge supports Boolean and numeric values, decimal, strings/binary, date,
microsecond time/timestamp, interval, JSON, lists/arrays, and structs. Cast unsupported
source types in a view. RDF mappings preserve kind, datatype, and language metadata.
