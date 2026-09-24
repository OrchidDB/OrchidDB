Bind application input as typed Cypher values and reuse query text across calls.

## Bind a read parameter

Parameters are passed in a `BTreeMap<String, Value>` and referenced by `$name` in query text:

```rust
use std::collections::BTreeMap;
use new_graph::ir::Value;

let params = BTreeMap::from([
    ("name".into(), Value::String("Alice".into())),
]);
let result = graph.cypher_with_params(
    "MATCH (p:Person) WHERE p.name = $name RETURN p.name, p.age",
    &params,
).await?;
```

Map keys contain the name without `$`. `GraphEngine` and `MappedGraphEngine` both provide this method.

## Bind a write parameter

For a managed graph:

```rust
let params = BTreeMap::from([
    ("name".into(), Value::String("Alice".into())),
    ("age".into(), Value::Int(30)),
]);
graph.cypher_with_params(
    "CREATE (:Person {name:$name, age:$age})",
    &params,
).await?;
```

For a mapped property update, pass the same map shape to `cypher_update_with_params`.

## Choose value types

| Rust value | Meaning |
| --- | --- |
| `Value::String(String)` | Text. |
| `Value::Int(i64)` | Signed integer. |
| `Value::Float(f64)` | Floating-point number. |
| `Value::Bool(bool)` | Boolean. |
| `Value::Null` | Null value. |
| `Value::List(Vec<Value>)` | List of values. |

Use a value type that matches the property being compared or assigned. The source table's types remain part of the mapped graph contract.

## Query text and values

Keep the query structure fixed and bind values separately. This preserves types and avoids constructing query strings from application input. Labels, relationship types, property names, and other structural choices belong in application-controlled query text.

Supply every referenced parameter before executing. Binding happens before query execution, and a missing parameter produces an error identifying the missing input.

## Nulls and lists

Use Cypher `IS NULL` and `IS NOT NULL` when testing for missing values. For a list parameter, use a list-oriented query expression such as membership:

```rust
let params = BTreeMap::from([(
    "names".into(),
    Value::List(vec![
        Value::String("Alice".into()),
        Value::String("Bob".into()),
    ]),
)]);
let result = graph.cypher_with_params(
    "MATCH (p:Person) WHERE p.name IN $names RETURN p.name ORDER BY p.name",
    &params,
).await?;
```
