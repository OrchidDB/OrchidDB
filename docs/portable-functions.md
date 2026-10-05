# Portable scalar functions

Use `fn.<name>(...)` to call scalar functions in your queries. These functions
cover numbers, text, dates and times, arrays, maps, and records. OrchidDB can
execute supported calls in DuckDB or PostgreSQL and evaluate other calls in
its query runtime when using mixed execution.

Browse the [function catalog](portable-function-catalog.md) for all 165 functions
and their DuckDB and PostgreSQL support.

## Call a function

In Cypher, use functions in expressions such as the values returned by a query:

```cypher
RETURN fn.sqrt(9.0) AS root, fn.upper('hello') AS greeting
```

| root | greeting |
| --- | --- |
| 3.0 | HELLO |

Pass a property to apply a function to each matching row:

```cypher
MATCH (d:Document)
RETURN d.title AS title, fn.upper(d.title) AS uppercase_title
```

Function aliases are supported. For example, `fn.char_length` and
`fn.character_length` refer to the same function.

The same `fn.` names are available in SQL expressions used by query-backed
mappings and computed relationships. Existing language-specific function names
keep their own behavior. This catalog covers scalar functions; aggregates,
window functions, and table functions use their existing catalogs.

## Choose a function

| Work with | Examples |
| --- | --- |
| Numbers | `fn.abs`, `fn.round`, `fn.sqrt` |
| Text | `fn.upper`, `fn.lower`, `fn.character_length`, `fn.replace` |
| Missing values | `fn.coalesce`, `fn.nullif` |
| Arrays | `fn.array_length`, `fn.array_distinct`, `fn.array_slice` |
| Maps and records | `fn.map`, `fn.struct`, `fn.named_struct` |
| Hashes | `fn.md5`, `fn.sha256`, `fn.sha512` |

See the [function catalog](portable-function-catalog.md) for the supported
argument types and options for each engine.

## Where functions execute

OrchidDB checks each call's arguments before placing it in SQL. All 165
functions have a DuckDB and PostgreSQL SQL path, but some argument types or
options require evaluation in OrchidDB. Functions that inspect types or
metadata can be resolved while the query is compiled.

For example, a simple, case-sensitive `fn.regexp_like` pattern such as
`'[a-z]+'` can execute in either database. Patterns with Unicode character
classes require evaluation in OrchidDB because the databases interpret those
classes differently.

In mixed execution, a call without a compatible SQL mapping runs in OrchidDB's
query runtime. SQL-only compilation reports an error identifying the function,
engine, and unsupported case. It cannot use runtime evaluation to complete the
query.

SQL support preserves the function's result types and behavior. Some functions
use SQL expressions containing several operations, so their cost can differ
from a database's built-in function.

## Compatibility limits

Consult the catalog before relying on a specific SQL overload. Restrictions
can depend on the argument types, number of arguments, and constant options.

- **Regular expressions:** SQL mappings support a case-sensitive subset of
  constant patterns. Captures, alternation, Unicode classes, lazy quantifiers,
  and empty matches require native evaluation.
- **Arrays:** Many SQL mappings support flat arrays of booleans, signed
  integers, or text. Support for nested arrays and other element types varies
  by function.
- **PostgreSQL values:** PostgreSQL does not support NUL characters in text
  or arbitrary nanosecond timestamp precision. Database value ranges also apply.
- **Text:** Unicode case conversion can produce multiple output characters for
  one input character. Padding and translation use grapheme boundaries;
  edit distance counts Unicode code points.

The [function catalog](portable-function-catalog.md) lists the restrictions for
each function so you can check whether a call can run in your selected engine.
