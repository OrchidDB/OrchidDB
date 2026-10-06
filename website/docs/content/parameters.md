# Parameters and values

Native Cypher parameters use your DuckDB driver's ordinary named bindings:

```python
rows = connection.execute(
    "CYPHER social MATCH (p:Person) WHERE p.name = $name RETURN p.age",
    {"name": "Alice"},
).fetchall()
```

Binding specializes compilation to the supplied values. Repeated executions bind
fresh values. Null, Boolean, signed integers, finite floating-point values, strings,
and nested lists/structs are supported; unsupported values fail explicitly.

Advanced requests also carry the existing typed Gremlin bindings. Values enter the
shared frontend as data, not query text. See [mapping protocol](sql-compiler.md).
