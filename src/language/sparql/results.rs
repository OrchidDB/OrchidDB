//! Shared typed RDF result decoding for every execution host.
use crate::ir::policy::ResultForm;
use crate::ir::rel::rdf::binding_identity_columns;
use crate::ir::runtime::ReturnedBatches;
use arrow::array::{Array, BooleanArray, StringArray};

/// One RDF term in a query result.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum RdfTermValue {
    Iri(String),
    BlankNode(String),
    Literal {
        lexical: String,
        datatype: String,
        language: Option<String>,
    },
}

impl RdfTermValue {
    pub fn iri(value: impl Into<String>) -> Self {
        Self::Iri(value.into())
    }

    /// A simple literal (`xsd:string`).
    pub fn string(value: impl Into<String>) -> Self {
        Self::typed(value, "http://www.w3.org/2001/XMLSchema#string")
    }

    pub fn typed(value: impl Into<String>, datatype: impl Into<String>) -> Self {
        Self::Literal {
            lexical: value.into(),
            datatype: datatype.into(),
            language: None,
        }
    }

    pub fn lang(value: impl Into<String>, language: impl Into<String>) -> Self {
        Self::Literal {
            lexical: value.into(),
            datatype: "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString".into(),
            language: Some(language.into()),
        }
    }
}

/// Decoded SPARQL query results.
#[derive(Debug, Clone, PartialEq)]
pub enum SparqlResults {
    /// Variables (with their `?` prefix) and rows; `None` is unbound.
    Solutions {
        variables: Vec<String>,
        rows: Vec<Vec<Option<RdfTermValue>>>,
    },
    Boolean(bool),
    Graph(Vec<[RdfTermValue; 3]>),
}

pub fn decode_results(output: &ReturnedBatches) -> Result<SparqlResults, String> {
    let batch = &output.batch;
    if output.result_form == ResultForm::Boolean {
        let column = batch
            .column(0)
            .as_any()
            .downcast_ref::<BooleanArray>()
            .ok_or("ASK result is not boolean")?;
        return Ok(SparqlResults::Boolean(
            batch.num_rows() == 1 && !column.is_null(0) && column.value(0),
        ));
    }
    let text = |name: &str| -> Result<&StringArray, String> {
        let index = batch
            .schema()
            .index_of(name)
            .map_err(|_| format!("result column `{name}` is missing"))?;
        batch
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| format!("result column `{name}` is not text"))
    };
    let mut columns = Vec::new();
    for field in &output.fields {
        let [kind, datatype, language] = binding_identity_columns(field);
        columns.push([
            text(field)?,
            text(&kind)?,
            text(&datatype)?,
            text(&language)?,
        ]);
    }
    let cell = |columns: &[&StringArray; 4], row: usize| -> Result<Option<RdfTermValue>, String> {
        let [value, kind, datatype, language] = columns;
        if kind.is_null(row) {
            return Ok(None);
        }
        let lexical = value.value(row).to_string();
        Ok(Some(match kind.value(row) {
            "IRI" => RdfTermValue::Iri(lexical),
            "BLANK" => RdfTermValue::BlankNode(lexical),
            "LITERAL" => RdfTermValue::Literal {
                lexical,
                datatype: if datatype.is_null(row) {
                    "http://www.w3.org/2001/XMLSchema#string".into()
                } else {
                    datatype.value(row).to_string()
                },
                language: (!language.is_null(row)).then(|| language.value(row).to_string()),
            },
            other => return Err(format!("unknown RDF term kind `{other}`")),
        }))
    };
    if output.result_form == ResultForm::RdfGraph {
        let mut triples = Vec::new();
        for row in 0..batch.num_rows() {
            let mut terms = Vec::new();
            for column in &columns {
                terms.push(cell(column, row)?.ok_or("CONSTRUCT produced an unbound term")?);
            }
            let [s, p, o]: [RdfTermValue; 3] = terms.try_into().expect("three columns");
            triples.push([s, p, o]);
        }
        return Ok(SparqlResults::Graph(triples));
    }
    let mut rows = Vec::new();
    for row in 0..batch.num_rows() {
        rows.push(
            columns
                .iter()
                .map(|column| cell(column, row))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(SparqlResults::Solutions {
        variables: output.fields.clone(),
        rows,
    })
}
