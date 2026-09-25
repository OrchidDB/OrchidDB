use std::sync::Arc;

use arrow::array::{ArrayRef, RecordBatch, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use datafusion::datasource::MemTable;

use orchiddb::ir::PropertyGraph;
use orchiddb::ir::rel::rdf::{IriQuadSource, RdfDatasetMapping, RdfTermColumns};
use orchiddb::ir::rel::{RelBackend, RelBackendOptions};
use orchiddb::language::sparql::SparqlPlanner;

fn typed_backend() -> RelBackend {
    let schema = Arc::new(Schema::new(
        [
            "g", "s", "s_kind", "s_dt", "s_lang", "p", "p_kind", "p_dt", "p_lang", "o", "o_kind",
            "o_dt", "o_lang",
        ]
        .into_iter()
        .map(|name| Field::new(name, DataType::Utf8, true))
        .collect::<Vec<_>>(),
    ));
    let arrays = schema
        .fields()
        .iter()
        .map(|_| Arc::new(StringArray::from(Vec::<Option<&str>>::new())) as ArrayRef)
        .collect();
    let batch = RecordBatch::try_new(Arc::clone(&schema), arrays).unwrap();
    let mut mapping = RdfDatasetMapping::new();
    mapping
        .register_table(
            "terms",
            Arc::new(MemTable::try_new(schema, vec![vec![batch]]).unwrap()),
        )
        .map_typed_quads(
            "default",
            IriQuadSource::table("terms", "s", "p", "o")
                .graph_column("g")
                .typed_term_columns(
                    RdfTermColumns::new("s", "s_kind")
                        .datatype("s_dt")
                        .language("s_lang"),
                    RdfTermColumns::new("p", "p_kind")
                        .datatype("p_dt")
                        .language("p_lang"),
                    RdfTermColumns::new("o", "o_kind")
                        .datatype("o_dt")
                        .language("o_lang"),
                ),
        );
    RelBackend::with_options(RelBackendOptions {
        rdf_datasets: Some(Arc::new(mapping)),
        ..RelBackendOptions::default()
    })
}

#[test]
fn typed_constant_patterns_lower_with_rdf_identity_checks() {
    let backend = typed_backend();
    for query in [
        "ASK { <https://example.com/s> <https://example.com/p> \"plain\" }",
        "ASK { <https://example.com/s> <https://example.com/p> \"bonjour\"@FR }",
        "ASK { <https://example.com/s> <https://example.com/p> \"0042\"^^<http://www.w3.org/2001/XMLSchema#integer> }",
        "ASK { <https://example.com/s> <https://example.com/p> _:node }",
    ] {
        let plan = SparqlPlanner::default().plan_str(query).unwrap();
        backend.lower(&plan, &PropertyGraph::new()).unwrap();
    }
}

#[test]
fn typed_bgp_correlation_keeps_term_identity() {
    let ask = SparqlPlanner::default()
        .plan_str("ASK { ?s <https://example.com/name> ?x . ?s <https://example.com/alias> ?x }")
        .unwrap();
    typed_backend().lower(&ask, &PropertyGraph::new()).unwrap();

    let plan = SparqlPlanner::default()
        .plan_str(
            "SELECT ?s WHERE { ?s <https://example.com/name> ?x . ?s <https://example.com/alias> ?x }",
        )
        .unwrap();
    // Typed results keep the term identity after the visible value column.
    let lowered = typed_backend().lower(&plan, &PropertyGraph::new()).unwrap();
    assert_eq!(lowered.fields, vec!["?s"]);
    let columns: Vec<_> = lowered
        .plan
        .schema()
        .fields()
        .iter()
        .map(|field| field.name().clone())
        .collect();
    assert_eq!(
        columns,
        vec![
            "?s",
            "__rdf:term:kind:?s",
            "__rdf:term:datatype:?s",
            "__rdf:term:language:?s"
        ]
    );
}
