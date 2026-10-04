# JSON documents and relationships

```cypher
MATCH (person:Person)-[link:KNOWS]->(friend:Person)
RETURN person.name, friend.name, link.since
```

A document's array can supply the rows behind a relationship. Map the parent ID,
extract each array element's target ID and properties, then traverse those edges
with ordinary Cypher or Gremlin. JSON functions also work directly in expressions:

```cypher
RETURN json.value(JSON '{"author":{"name":"Ada"}}', '$.author.name') AS author,
       json.value(JSON '{"count":7}', '$.count', 'int64') AS count
```

JSON values have their own type. Use a `JSON '...'` literal or `json.parse(text)`;
ordinary strings remain strings. Register a document column with schema type
`json` in a [compiler request](sql-compiler.md), or use
`ir::functions::domain::json_type()` for an Arrow schema in Rust.

## Turn an array into edges

Suppose each person stores a document like this in `profile`:

```json
{"friends":[{"id":2,"since":2020},{"id":3,"since":2023}]}
```

Use `jsonb` for the PostgreSQL column or `JSON` for the DuckDB column:

```sql
-- PostgreSQL
CREATE TABLE people (id BIGINT PRIMARY KEY, name TEXT, profile JSONB);
```

```sql
-- DuckDB
CREATE TABLE people (id BIGINT PRIMARY KEY, name VARCHAR, profile JSON);
```

Both tables can hold these example rows:

```sql
INSERT INTO people VALUES
  (1, 'Ada', '{"friends":[{"id":2,"since":2020},{"id":3,"since":2023}]}'),
  (2, 'Grace', '{"friends":[]}'),
  (3, 'Katherine', '{"friends":[]}');
```

The mapping below expands `profile.friends` into a named relation. Its fields
become the source ID, target ID, and relationship properties:

```toml
[node.Person]
table = "people"
id = "id"

[node.Person.properties]
name = "name"
profile = "profile"

[collection_sources]
catalog = '''[
  {
    "name": "friend_links",
    "table": "people",
    "expand": "json.elements(profile, '$.friends')",
    "as": "item",
    "parent_columns": {"source_id": "id"},
    "ordinality": "position",
    "fields": {
      "target_id": "json.value(item.value, '$.id', 'int64')",
      "since": "json.value(item.value, '$.since', 'int64')"
    }
  }
]'''

[edge.KNOWS]
table = "friend_links"
src = "source_id"
dst = "target_id"
src_label = "Person"
dst_label = "Person"
edge_id = ["source_id", "position"]

[edge.KNOWS.properties]
since = "since"
```

Load the mapping with `GraphMapping::from_toml` and register the `people` schema
or provider, as in the [mapping reference](mapping-reference.md). For the JSON
compiler, put the same collection descriptor in the `collection_sources` array
and use `data_type: "json"` for `profile`.

The opening query now returns Ada's two friendships. Repeated array elements
remain separate occurrences. `position` starts at one and distinguishes those
edges; use an embedded stable relationship ID instead if identity must survive
array reordering. `json.elements` itself exposes a zero-based `index` field.

Missing paths, SQL-null documents, and empty arrays produce no rows. A present
value of the wrong kind is an error: `json.elements` requires an array. Set
`"outer": true` on a collection source to retain parents with no child rows;
the generated child fields and ordinality are null. Null endpoints do not form
ordinary graph edges. A collection can use another collection as its source to
expand nested documents.

## Read and construct documents

```cypher
MATCH (p:Person)
WHERE json.exists(p.profile, '$.friends')
RETURN p.name,
       json.array_length(p.profile, '$.friends') AS friend_count,
       json.query(p.profile, '$.friends') AS friends
```

`json.query` returns JSON. `json.value` returns a scalar string by default;
pass a literal type name such as `int64`, `float64`, or `boolean` for a typed
result. Missing paths return SQL null. An existing JSON null is preserved by
`json.query`, while `json.value` returns SQL null for JSON null, objects, and arrays.

Construct values without string concatenation:

```cypher
RETURN json.object('name', 'Ada', 'tags', json.array('graph', 'sql')) AS document
```

Strings stay JSON strings; JSON arguments embed their document value. SQL-null
constructor values become JSON null. Object keys must be non-null strings.
Duplicate object keys keep the last value.

`json.transform` converts a document to an explicitly typed nested value:

```cypher
RETURN json.transform(
  JSON '{"id":7,"tags":["graph","sql"]}',
  JSON '{"id":"int64","tags":["string"]}'
) AS record
```

A schema object declares fields, a one-element schema array declares a list,
and a type-name string declares a scalar. Missing fields become typed nulls. Invalid type
conversions produce an error. The schema can also be supplied as a constant string.

## Use documents in relationship rules

For example, map a document node's `payload` and a requirement node's `criteria`
as JSON properties, then define containment:

```toml
[edge.MATCHES]
source = "Document"
target = "Requirement"
predicate = "json.contains(source.payload, target.criteria)"
```

```cypher
MATCH (document:Document)-[:MATCHES]->(requirement:Requirement)
RETURN document.id, requirement.id
```

Containment checks the requested structure. Objects match a subset of their
fields; arrays match an unordered subset, ignoring repeated requested values.
`{"a":{"b":1}}` does not contain `{"b":1}` at the root. Use `json.query` first
when containment should be tested within a particular subtree.

## Function catalog

| Functions | Result |
| --- | --- |
| `json.parse(text)`, `json.stringify(document)` | Parse text as JSON; serialize JSON as text. |
| `json.valid(text)` | Whether text is valid JSON. |
| `json.query(document, path)` | JSON at a path; multiple-selection paths return a JSON array. |
| `json.value(document, path[, type])` | Typed scalar; default type is string. |
| `json.exists(document, path)` | Whether the path exists, including JSON-null values. |
| `json.type(document[, path])` | `null`, `boolean`, `number`, `string`, `array`, or `object`. |
| `json.keys(document[, path])` | Object keys as a list of strings. |
| `json.array_length(document[, path])` | Array length. |
| `json.elements(document[, path])` | Array elements as rows. |
| `json.entries(document[, path])` | Object members as rows. |
| `json.tree(document[, path])` | Selected value and its descendants as rows. |
| `json.array(values...)`, `json.object(key, value, ...)` | Construct JSON. |
| `json.transform(document, schema)` | Typed fields and lists. |
| `json.set(document, path, value)` | Set an existing value or add a final member. |
| `json.insert(document, path, value)` | Add a missing object member or insert before an array position. |
| `json.replace(document, path, value)` | Replace only an existing value. |
| `json.remove(document, paths...)` | Remove selected members or array elements. |
| `json.merge_patch(document, patch)` | Apply a JSON merge patch; null patch fields remove object members. |
| `json.equals(left, right)`, `json.contains(document, candidate)` | Structural comparison. |
| `json.array_agg(value)`, `json.object_agg(key, value)` | Aggregate rows into JSON. |

The row functions return lists of records when used as scalar expressions and
can supply collection-source rows. Each record contains `value`, `index`, `key`,
`path`, `parent_path`, and `depth`. Aggregates retain null inputs as JSON null;
empty input returns SQL null. Specify ordering when array order or the last
value of a repeated object key matters.

## Execution and supported paths

OrchidDB has native implementations of these functions. PostgreSQL and DuckDB
lower supported operations into their own JSON expressions and row-producing
operations, so document work can compose with surrounding joins and filters.
Both adapters support document reads, constructors, equality, containment,
merge patch, aggregates, and collection expansion. DuckDB also lowers nested
`json.transform` results and text validation. PostgreSQL lowers path mutations
and scalar/list transforms; transforms containing structs execute natively.
DuckDB path mutations currently execute natively. SQL generation reports
unsupported combinations explicitly; backend errors are not retried with
different semantics.

Use definite paths such as `$.friends[0].id`, `$["key.with.dots"]`, or JSON Pointer
`/friends/0/id` for portable SQL examples. The native path evaluator also supports
wildcards, recursive descent, unions, slices, and basic filters. Array positions
are zero based; negative JSONPath positions count backward. Mutation functions
require definite paths. See the [detailed JSON reference](https://github.com/OrchidDB/OrchidDB/blob/main/docs/json.md)
for path grammar, null behavior, mutation rules, and engine capability limits.
