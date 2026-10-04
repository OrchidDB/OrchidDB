# JSON function and collection reference

Use the [JSON guide](../website/docs/content/json.md) for document columns,
`JSON '...'` literals, ordinary function calls, and a complete JSON-array-to-edge
mapping. The public library is `json.*`; existing engine-native functions keep
their original names and semantics.

## Document types and nulls

JSON is a logical datatype, not an alias for text or a graph property map.
Compiler schemas use `"data_type": "json"`. Rust callers use
`ir::functions::domain::json_type()` and `json_scalar(text)` when constructing
Arrow input. PostgreSQL stores documents as JSONB; DuckDB uses JSON. Adapter
codecs preserve document identity through SQL islands and nested collections.

There are three distinct states:

| State | `json.query` | `json.exists` | `json.value` |
| --- | --- | --- | --- |
| Present scalar | JSON value | true | Scalar value |
| Present JSON null | JSON null | true | SQL null |
| Missing path | SQL null | false | SQL null |
| SQL-null document | SQL null | SQL null | SQL null |

`json.type` reports the literal string `null` for JSON null; a missing path or
SQL-null input returns SQL null. Outer Arrow validity represents SQL null, so
JSON null is never inferred from a null storage field.

`json.parse` validates text and produces a document. `json.valid` tests validity
without raising a parse error; SQL-null text returns SQL null. Parsing duplicate
object keys keeps the last value. `json.stringify` produces valid JSON text;
whitespace and object-key order are not a cross-engine byte-level contract.
Object key order is not significant in equality. JSON numbers compare by exact
decimal value natively, so `1` equals `1.0` and large integers are not rounded
through float64. Backend numeric representation limits still apply.

## Selection and conversion

- `json.query(document, path)` preserves JSON types. A definite path returns the
  selected value. A path that can select multiple values returns an array even
  when exactly one value matches. No matches return SQL null.
- `json.value(document, path[, type])` extracts a scalar. The default type is
  string. A third argument must be a constant type name. Objects, arrays, JSON
  null, and missing values return typed SQL null; invalid conversions fail.
- `json.exists(document, path)` tests presence, including JSON-null values.
- `json.type(document[, path])` reports `null`, `boolean`, `number`, `string`,
  `array`, or `object`.
- `json.keys(document[, path])` requires an object and returns a string list.
- `json.array_length(document[, path])` requires an array and returns uint64.

Optional paths default to `$`. Functions expecting one value reject a path that
matches multiple values. Wrong-kind inputs to `keys` and `array_length` are
errors, not silently empty collections.

`json.transform(document, schema)` returns a typed Arrow struct/list/scalar.
The schema must be a constant JSON value or constant JSON text:

```cypher
RETURN json.transform(
  JSON '{"id":7,"tags":["graph",null],"active":true}',
  JSON '{"id":"int64","tags":["string"],"active":"boolean"}'
)
```

Schema objects declare named fields; a one-element array declares a list.
Leaf strings name types, such as `string`, `boolean`, `int64`, `float64`, or
`json`. Missing object fields become typed nulls. Extra document fields are
ignored. Numeric overflow and incompatible conversions raise errors. Conversion
schemas are planning-time constants because they determine the output schema.

## Native path grammar

Paths are parsed once per evaluation into structured steps. Supported forms:

| Form | Example |
| --- | --- |
| Document root | `$`, or empty JSON Pointer |
| Object field | `$.name`, `$."a.b"`, `$["a.b"]`, `$['a.b']` |
| Array element | `$[0]`, `$[-1]`, `$[#-1]` |
| JSON Pointer | `/friends/0/name`, `/a~1b/~0key` |
| Children | `$.*`, `$[*]` |
| Recursive descent | `$..name`, `$..*` |
| Selector union | `$[0,2]`, `$['a','b']` |
| Slice | `$[1:5]`, `$[::2]`, `$[::-1]` |
| Filter | `$.items[?(@.price >= 10 && @.active == true)]` |

Array indices are zero based. Negative JSONPath indices count from the end.
JSON Pointer array tokens must be nonnegative decimal indices without leading
zeros; numeric object keys remain object keys. Pointer escapes are `~0` for `~`
and `~1` for `/`.

Filter left operands are `@` paths; comparisons accept JSON literals on the
right. Operators are `==`, `!=`, `<`, `<=`, `>`, and `>=`, with `&&`, `||`, `!`,
and parentheses. A bare `@` path tests existence. Regex, embedded function calls,
script evaluation, and unrecognized path syntax are errors. A path selecting a
field from the wrong container has no match. Definite SQL path support is
separate from the broader native grammar; unsupported SQL combinations are
reported during compilation.

## Construction and aggregation

`json.array(values...)` and `json.object(key, value, ...)` accept typed values.
JSON arguments embed their document value, strings remain strings, native lists
and structs become JSON arrays and objects, and SQL-null values become JSON
null. Object keys must be non-null strings. Duplicate keys keep the last value.
Empty constructors return `[]` and `{}`.

`json.array_agg(value)` and `json.object_agg(key, value)` aggregate input rows.
SQL-null values remain JSON null. Empty input returns SQL null; null object keys
are errors. Array aggregation retains input ordering, and the last encountered
value wins for duplicate object keys. Queries requiring a particular order must
supply that order; an unordered relational input has no stable encounter order.
Native aggregate states preserve the same behavior when partial states merge.

## Equality and containment

`json.equals(left, right)` compares JSON structure. Arrays are ordered and keep
duplicates; objects are key-order independent; numbers compare by value.
SQL-null inputs produce SQL null.

`json.contains(document, candidate)` uses structural containment:

- An object contains another object when its requested keys recursively match.
- An array contains another array when each candidate member has a containing
  member in the document array; order and repeated requested values do not matter.
- At the root, an array can contain a primitive scalar member.
- Other scalar values require equality. Containment does not search arbitrary
  descendants or unwrap arrays inside object fields.

Examples:

```text
{"a":[1,2,3]} contains {"a":[2,2]}     true
{"a":{"b":1}} contains {"b":1}        false
{"a":[1]} contains {"a":1}             false
[1,2] contains 2                         true
[{"a":1}] contains {"a":1}             false
```

These functions can appear in ordinary query predicates or computed relationship
predicates. They are explicit functions; document properties are not implicitly
compared or expanded by ordinary edge traversal.

## Document changes

Functions return new documents; they do not mutate the database:

| Function | Behavior |
| --- | --- |
| `json.set(document, path, value)` | Replace or create the final member. |
| `json.insert(document, path, value)` | Add a missing object key; insert before an array position. |
| `json.replace(document, path, value)` | Replace only an existing member. |
| `json.remove(document, paths...)` | Remove selected members in argument order. |
| `json.merge_patch(document, patch)` | Apply JSON merge-patch semantics. |

Mutation paths must be definite fields/indices. Missing intermediate containers
leave the document unchanged. For set/insert, out-of-range positive array
positions append and out-of-range negative JSONPath positions prepend. Existing
object keys are unchanged by insert. SQL-null replacement values become JSON
null; SQL-null document or path arguments yield SQL null. Set/replace at `$`
replace the whole document; insert at `$` leaves it unchanged; removing `$`
is an error.

Merge patch recursively merges objects, removes keys whose patch value is JSON
null, and replaces arrays and other values as a whole. An SQL-null patch returns
SQL null; a JSON-null patch produces a JSON-null document.

## Row functions and collection mappings

`json.elements(document[, path])`, `json.entries(document[, path])`, and
`json.tree(document[, path])` return `list<struct<...>>` when called as scalar
functions. Collection-source expansion exposes the same records as rows.

| Field | Meaning |
| --- | --- |
| `value` | JSON value, including JSON null. |
| `index` | Zero-based array index, otherwise SQL null. |
| `key` | Object-member key, otherwise SQL null. |
| `path` | Absolute path from the original document root. |
| `parent_path` | Parent path, null for the document root. |
| `depth` | Number of path steps from the document root. |

Elements requires an array; entries requires an object. Tree includes the
selected root and all descendants in preorder. Array order is preserved;
object-member order is not guaranteed. Repeated values remain repeated rows.
Missing paths, SQL-null input, and empty arrays/objects produce no elements or
entries. Tree still emits a row for a present JSON-null or scalar root.

Collection descriptors accept `expand`, `as`, `parent_columns`, `fields`,
`outer`, and `ordinality`. Field expressions can use the row alias and typed JSON
functions. `ordinality` starts at one and is null on synthetic outer rows;
`index` remains zero based. Nested collections expand one mapped relation into
the next. Endpoint keys and ordinary node/edge mappings determine graph identity;
use an embedded stable key when identity must persist through array edits.

## SQL engines and native execution

Native execution provides the full function catalog and path grammar above.
Engine adapters lower supported fragments into SQL expressions, subqueries,
aggregates, and relational expansion. The shared relational machinery preserves
correlation, inner/outer expansion, output schema, and source ownership. JSON is
not a separate execution backend.

For PostgreSQL and DuckDB, definite paths are the portable starting point.
The adapters validate available operations and path syntax instead of emitting
an engine function with a guessed name. Engines may use a different SQL shape
for the same operation. Unsupported SQL combinations fail explicitly; the
native implementation remains available when planning native execution.
Runtime SQL errors are propagated rather than retried.

The bundled adapters currently provide these SQL transformations. Every row in
this table also has a native implementation.

| Function family | PostgreSQL SQL | DuckDB SQL |
| --- | --- | --- |
| Parse and stringify | Yes | Yes |
| Validate text with `json.valid` | Native execution | Yes |
| Query, value, exists, type, keys, array length | Constant definite paths | Constant definite paths |
| Array and object constructors | Yes | Yes |
| Equality and containment | Yes | Yes |
| Merge patch | Yes | Yes |
| Set, insert, replace, remove | Constant definite paths | Constant definite paths |
| Transform to scalar or list | Yes, including nested lists | Yes |
| Transform to struct | Native execution | Yes, including nested fields and lists; empty anonymous structs require native execution |
| Array and object aggregates | Yes, preserving aggregate ordering | Yes, preserving aggregate ordering |
| Elements, entries, tree as collection sources | Yes, constant definite paths | Yes, constant definite paths |

A row function used directly as a scalar list expression executes natively.
Dynamic paths, wildcards, recursive descent, unions, slices, and filters use the
native evaluator. The planner can retain such expressions in a native region;
requesting a single SQL expression for an unsupported combination reports the
capability limitation before execution.

The PostgreSQL adapter targets PostgreSQL 14, which has no built-in safe JSON
text validator for arbitrary malformed text. Its `json.valid` implementation
therefore remains native. PostgreSQL JSONB's numeric range and DuckDB's numeric
representation constrain values those engines can store and compute, even
though native comparison preserves arbitrary decimal values. Engine integration
tests exercise the supported transformations and their null, ordering, and
conversion behavior.

## Implementation extension points

- `src/ir/functions/json.rs`: public native scalar functions and typed results.
- `src/ir/functions/json/path.rs`: shared structured path parser/evaluator.
- `src/ir/functions/json/aggregate.rs`: native aggregate state and merging.
- `src/ir/functions/domain.rs`: nominal logical types and Arrow storage envelopes.
- `src/ir/rel/collection_source.rs`: general expression-based collection mappings.
- `src/ir/rel/sql/json.rs`: engine-owned JSON expression transformations.
- `src/ir/rel/sql/json_rows.rs`: row-function SQL lowering.
- `src/ir/rel/sql/json_transform.rs`: recursive typed conversion SQL lowering.

The same domain and relational hooks can support future types such as geometry.
This JSON feature does not add geographic functions or change graph-map semantics.
