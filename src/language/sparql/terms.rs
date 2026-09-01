use spargebra::term::{GroundTerm, Literal, NamedNodePattern, TermPattern, Variable};

use crate::ir::expr::Lit;
use crate::ir::plan::RdfTerm;
use crate::ir::value::Value;

pub(crate) fn binding(variable: &Variable) -> String {
    format!("?{}", variable.as_str())
}

pub(crate) fn named_term(term: &NamedNodePattern) -> RdfTerm {
    match term {
        NamedNodePattern::NamedNode(value) => RdfTerm::Iri(value.as_str().into()),
        NamedNodePattern::Variable(value) => RdfTerm::Variable(binding(value)),
    }
}

pub(crate) fn term(term: &TermPattern) -> RdfTerm {
    match term {
        TermPattern::NamedNode(value) => RdfTerm::Iri(value.as_str().into()),
        TermPattern::BlankNode(value) => RdfTerm::BlankNode(value.as_str().into()),
        TermPattern::Literal(value) => literal(value),
        TermPattern::Variable(value) => RdfTerm::Variable(binding(value)),
    }
}

pub(crate) fn literal(value: &Literal) -> RdfTerm {
    if let Some(language) = value.language() {
        return RdfTerm::LanguageTagged {
            value: value.value().into(),
            lang: language.into(),
        };
    }
    let datatype = value.datatype().as_str();
    match datatype {
        "http://www.w3.org/2001/XMLSchema#string" => {
            RdfTerm::Literal(Lit::String(value.value().into()))
        }
        "http://www.w3.org/2001/XMLSchema#boolean" => {
            RdfTerm::Literal(Lit::Bool(value.value() == "true" || value.value() == "1"))
        }
        "http://www.w3.org/2001/XMLSchema#integer" => value
            .value()
            .parse()
            .map(|v| RdfTerm::Literal(Lit::Int(v)))
            .unwrap_or_else(|_| typed(value, datatype)),
        "http://www.w3.org/2001/XMLSchema#double" | "http://www.w3.org/2001/XMLSchema#decimal" => {
            value
                .value()
                .parse()
                .map(|v| RdfTerm::Literal(Lit::Float(v)))
                .unwrap_or_else(|_| typed(value, datatype))
        }
        _ => typed(value, datatype),
    }
}

fn typed(value: &Literal, datatype: &str) -> RdfTerm {
    RdfTerm::Typed {
        lexical: value.value().into(),
        datatype: datatype.into(),
    }
}

pub(crate) fn ground_value(term: &GroundTerm) -> Value {
    match term {
        GroundTerm::NamedNode(value) => Value::String(value.as_str().into()),
        GroundTerm::Literal(value) => rdf_value(&literal(value)),
    }
}

pub(crate) fn rdf_value(term: &RdfTerm) -> Value {
    match term {
        RdfTerm::Iri(value) => Value::String(value.clone()),
        RdfTerm::BlankNode(value) => Value::String(format!("_:{value}")),
        RdfTerm::LanguageTagged { value, lang } => Value::String(format!("{value}@{lang}")),
        RdfTerm::Typed { lexical, datatype } => Value::String(format!("{lexical}^^{datatype}")),
        RdfTerm::Literal(Lit::Null) => Value::Null,
        RdfTerm::Literal(Lit::Bool(value)) => Value::Bool(*value),
        RdfTerm::Literal(Lit::Int(value)) => Value::Int(*value),
        RdfTerm::Literal(Lit::Float(value)) => Value::Float(*value),
        RdfTerm::Literal(Lit::String(value)) => Value::String(value.clone()),
        RdfTerm::Variable(value) => Value::String(value.clone()),
    }
}
