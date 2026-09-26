# Glossary

A compact reference for the terms used throughout OrchidDB's documentation.

## Graph terms

| Term | Meaning |
| --- | --- |
| Node / vertex | An identified graph element with labels and properties. |
| Relationship / edge | A typed connection from a source node to a destination node. |
| Label | A name such as `Person` that identifies a group of nodes. |
| Relationship type | A name such as `FOLLOWS` that identifies a group of edges. |
| Property | A named value on a node or relationship. |
| Path | A sequence of connected nodes and relationships. |
| Traverser | The item of traversal state passed between Gremlin steps. |
| Binding | A named value produced during query evaluation. |

## Mapping terms

| Term | Meaning |
| --- | --- |
| Graph mapping | The contract between graph labels/properties and relational tables/columns. |
| Node mapping | A source relation, identity column, and property columns for a label. |
| Edge mapping | A source relation, endpoint columns and labels, and optional edge identity and properties. |
| Query-backed mapping | A graph source defined by a SQL query. |
| Schema provider | Table metadata used by the relational planner. |
| Ontology mapping | The association of RDF vocabulary with graph labels, properties, and relationships. |
| Quad source | A relational representation of RDF subject, predicate, object, and graph. |

## Execution terms

| Term | Meaning |
| --- | --- |
| Graph IR | The intermediate graph representation shared by language frontends. |
| Lowering | Transforming a plan into another representation while preserving its meaning. |
| SQL island | A region of the lowered relational plan executed as SQL in DuckDB. |
| Hybrid execution | A DataFusion execution DAG combining DuckDB SQL regions and native kernels. |
| Arrow batch | A columnar result represented by an Apache Arrow `RecordBatch`. |
| Checkpoint | A compact persisted snapshot of managed graph records. |
| Snapshot isolation | A transaction's consistent database view, combined with its own writes. |

## RDF terms

| Term | Meaning |
| --- | --- |
| IRI | An Internationalized Resource Identifier used to name an RDF resource. |
| Triple | An RDF statement with subject, predicate, and object. |
| Quad | A triple together with its graph. |
| Default graph | The graph matched by unqualified triple patterns in the active dataset. |
| Named graph | An RDF graph identified by an IRI. |
| Vocabulary | Class and predicate IRIs used to describe a domain. |

Return to the [introduction](index.md) to choose a guide, or use the search button to find a specific API or topic.
