Connect RDF vocabulary to property-graph labels, identities, properties, and relationship directions.

## Class mappings

`OntologyMapping::class(iri, label)` associates a class IRI with a graph label:

```rust
let ontology = OntologyMapping::new()
    .class("https://example.com/Person", "Person");
```

A pattern such as `?p a ex:Person` then selects nodes with the `Person` label.

## Subject identity

Use `class_with_identity` when a graph property provides the externally visible subject:

```rust
let ontology = OntologyMapping::new()
    .class_with_identity("https://example.com/Person", "Person", "iri");
```

Map that graph property to your physical identity column in the graph mapping:

```rust
mapping.map_node(
    NodeMapping::table("Person", "people", "id")
        .property("iri", "person_iri")
        .property("name", "full_name")
);
```

The relational ID identifies the node for graph joins; the identity property supplies the vocabulary-facing subject.

## Property predicates

`property(iri, domain_label, property)` maps a predicate to a graph property:

```rust
let ontology = OntologyMapping::new()
    .class("https://example.com/Person", "Person")
    .property("https://example.com/name", "Person", "name")
    .property("https://example.com/age", "Person", "age");
```

The graph mapping resolves `name` and `age` to physical columns. This keeps the RDF vocabulary independent of database column names.

## Relationship predicates

Use `relationship_between` to make both endpoint labels explicit:

```rust
use new_graph::ir::plan::Direction;

let ontology = OntologyMapping::new()
    .class("https://example.com/Person", "Person")
    .class("https://example.com/Order", "Order")
    .relationship_between(
        "https://example.com/ordered", "ORDERED", Direction::Out,
        "Person", "Order",
    );
```

The direction defines how a subject-to-object triple follows the graph relationship. Use `Direction::In` for a vocabulary predicate that follows the inverse graph direction.

`relationship(iri, rel_type, direction)` configures the predicate without explicit endpoint labels. Use the endpoint-aware builder when those labels are known in your vocabulary.

## Reuse one vocabulary

Pass the same ontology to managed or mapped graph SPARQL methods. This allows application queries to share vocabulary while the graph mapping determines physical storage.

| Method | Meaning |
| --- | --- |
| `class(iri, label)` | Map a class to a label. |
| `class_with_identity(iri, label, property)` | Also choose the subject identity property. |
| `property(iri, label, property)` | Map a predicate to a property. |
| `relationship(iri, type, direction)` | Map a predicate to a relationship. |
| `relationship_between(iri, type, direction, domain, range)` | Also specify endpoint labels. |
| `class_for_iri(iri)` / `class_for_label(label)` | Inspect a class mapping. |
| `predicate_for_iri(iri)` | Inspect a predicate mapping. |
