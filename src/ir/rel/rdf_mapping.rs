//! RDF vocabulary over ordinary relational sources. These are expressions over
//! the existing catalog, not a physical triple schema or a materialization.
use super::{RelError, RelResult, col_exact};
use arrow::datatypes::DataType;
use datafusion::common::ScalarValue;
use datafusion::logical_expr::{Cast, Expr, LogicalPlan, LogicalPlanBuilder};
use datafusion::prelude::lit;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
fn lexical(value: Expr, kind: &DataType) -> RelResult<Expr> {
    use datafusion::functions::string::expr_fn as string;
    let text = Expr::Cast(Cast::new(Box::new(value), DataType::Utf8));
    Ok(match kind {
        DataType::Timestamp(_, timezone) => {
            let text = string::replace(text, lit(" "), lit("T"));
            if timezone.is_some() {
                let tail = datafusion::functions::unicode::expr_fn::right(text.clone(), lit(3_i64));
                datafusion::logical_expr::when(
                    tail.clone().like(lit("+__")).or(tail.like(lit("-__"))),
                    string::concat(vec![text.clone(), lit(":00")]),
                )
                .otherwise(text)?
            } else {
                text
            }
        }
        DataType::Float32 | DataType::Float64 => {
            let lower = string::lower(text.clone());
            datafusion::logical_expr::when(lower.clone().eq(lit("inf")), lit("INF"))
                .when(lower.clone().eq(lit("-inf")), lit("-INF"))
                .when(lower.eq(lit("nan")), lit("NaN"))
                .otherwise(text)?
        }
        _ => text,
    })
}

fn default_dataset() -> String {
    "default".into()
}

/// IRI templates hex-encode scalar text (or original binary bytes), separated by `/`.
/// This is reversible, URI-safe, and independent of physical column names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RdfTermMapping {
    Iri {
        column: String,
    },
    Template {
        prefix: String,
        columns: Vec<String>,
    },
    Blank {
        scope: String,
        columns: Vec<String>,
    },
    Literal {
        column: String,
        #[serde(default)]
        datatype: Option<String>,
        #[serde(default)]
        language: Option<String>,
        #[serde(default)]
        language_column: Option<String>,
    },
    Constant {
        value: String,
        #[serde(default)]
        datatype: Option<String>,
        #[serde(default)]
        language: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RdfMapping {
    #[serde(default = "default_dataset")]
    pub dataset: String,
    pub table: String,
    pub subject: RdfTermMapping,
    pub predicate: RdfTermMapping,
    pub object: RdfTermMapping,
    #[serde(default)]
    pub graph: Option<RdfTermMapping>,
    /// A declared complete row key is required for relational writes.
    #[serde(default)]
    pub key: Vec<String>,
    #[serde(default)]
    pub writable: bool,
}
impl RdfTermMapping {
    pub fn iri(column: impl Into<String>) -> Self {
        Self::Iri {
            column: column.into(),
        }
    }
    pub fn constant(value: impl Into<String>) -> Self {
        Self::Constant {
            value: value.into(),
            datatype: None,
            language: None,
        }
    }
    pub fn template(
        prefix: impl Into<String>,
        columns: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self::Template {
            prefix: prefix.into(),
            columns: columns.into_iter().map(Into::into).collect(),
        }
    }
    pub fn literal(column: impl Into<String>) -> Self {
        Self::Literal {
            column: column.into(),
            datatype: None,
            language: None,
            language_column: None,
        }
    }
    pub(crate) fn columns(&self) -> Vec<&str> {
        match self {
            Self::Iri { column } => vec![column],
            Self::Literal {
                column,
                language_column,
                ..
            } => std::iter::once(column.as_str())
                .chain(language_column.as_deref())
                .collect(),
            Self::Template { columns, .. } | Self::Blank { columns, .. } => {
                columns.iter().map(String::as_str).collect()
            }
            Self::Constant { .. } => vec![],
        }
    }
    pub(crate) fn predicate_iri(&self) -> Option<&str> {
        match self {
            Self::Constant {
                value,
                datatype: None,
                language: None,
            } => Some(value),
            _ => None,
        }
    }
    pub(crate) fn expressions(&self, plan: &LogicalPlan) -> RelResult<[Expr; 4]> {
        use datafusion::functions::{encoding::expr_fn as encoding, string::expr_fn as string};
        let null = || lit(ScalarValue::Utf8(None));
        let text = |expr| Expr::Cast(Cast::new(Box::new(expr), DataType::Utf8));
        for column in self.columns() {
            plan.schema().field_with_unqualified_name(column)?;
        }
        Ok(match self {
            Self::Iri { column } => [text(col_exact(column)), lit("IRI"), null(), null()],
            Self::Constant {
                value,
                datatype,
                language,
            } => [
                lit(value.clone()),
                lit(if datatype.is_some() || language.is_some() {
                    "LITERAL"
                } else {
                    "IRI"
                }),
                language
                    .as_ref()
                    .map(|_| lit("http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"))
                    .or_else(|| datatype.as_ref().map(|d| lit(d.clone())))
                    .unwrap_or_else(null),
                language
                    .as_ref()
                    .map(|s| lit(s.to_ascii_lowercase()))
                    .unwrap_or_else(null),
            ],
            Self::Literal {
                column,
                datatype,
                language,
                language_column,
            } => {
                if language.is_some() && language_column.is_some() {
                    return Err(RelError::Unsupported(
                        "literal language and language_column are mutually exclusive".into(),
                    ));
                }
                let dt = match datatype {
                    Some(dt) => dt.clone(),
                    None => rdf_datatype(
                        plan.schema()
                            .field_with_unqualified_name(column)?
                            .data_type(),
                    )?,
                };
                let lang = language
                    .as_ref()
                    .map(|s| lit(s.to_ascii_lowercase()))
                    .or_else(|| {
                        language_column
                            .as_ref()
                            .map(|c| string::lower(text(col_exact(c))))
                    })
                    .unwrap_or_else(null);
                let dt = datafusion::logical_expr::when(
                    lang.clone().is_not_null(),
                    lit("http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"),
                )
                .otherwise(lit(dt))?;
                [
                    lexical(
                        col_exact(column),
                        plan.schema()
                            .field_with_unqualified_name(column)?
                            .data_type(),
                    )?,
                    lit("LITERAL"),
                    dt,
                    lang,
                ]
            }
            Self::Template { prefix, columns }
            | Self::Blank {
                scope: prefix,
                columns,
            } => {
                if columns.is_empty()
                    || columns.iter().collect::<BTreeSet<_>>().len() != columns.len()
                {
                    return Err(RelError::Unsupported(
                        "RDF identity requires nonempty distinct key columns".into(),
                    ));
                }
                let mut parts = vec![lit(prefix.clone())];
                let mut present = lit(true);
                for (i, column) in columns.iter().enumerate() {
                    let ty = plan
                        .schema()
                        .field_with_unqualified_name(column)?
                        .data_type();
                    if !super::mapping::is_scalar_identity_type(ty) {
                        return Err(RelError::Unsupported(
                            "RDF identity components must be scalar".into(),
                        ));
                    }
                    if i > 0 {
                        parts.push(lit("/"));
                    }
                    let value = col_exact(column);
                    present = present.and(value.clone().is_not_null());
                    let bytes = if matches!(
                        ty,
                        DataType::Binary
                            | DataType::LargeBinary
                            | DataType::BinaryView
                            | DataType::FixedSizeBinary(_)
                    ) {
                        value
                    } else {
                        lexical(value, ty)?
                    };
                    parts.push(encoding::encode(bytes, lit("hex")));
                }
                [
                    datafusion::logical_expr::when(present, string::concat(parts))
                        .otherwise(null())?,
                    lit(if matches!(self, Self::Blank { .. }) {
                        "BLANK"
                    } else {
                        "IRI"
                    }),
                    null(),
                    null(),
                ]
            }
        })
    }
}
pub(crate) fn rdf_datatype(kind: &DataType) -> RelResult<String> {
    let suffix = match kind {
        DataType::Boolean => "boolean",
        DataType::Int8 => "byte",
        DataType::Int16 => "short",
        DataType::Int32 => "int",
        DataType::Int64 => "integer",
        DataType::UInt8 => "unsignedByte",
        DataType::UInt16 => "unsignedShort",
        DataType::UInt32 => "unsignedInt",
        DataType::UInt64 => "unsignedLong",
        DataType::Float32 => "float",
        DataType::Float64 => "double",
        DataType::Decimal128(..) | DataType::Decimal256(..) => "decimal",
        DataType::Date32 | DataType::Date64 => "date",
        DataType::Time32(_) | DataType::Time64(_) => "time",
        DataType::Timestamp(..) => "dateTime",
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View => "string",
        _ => {
            return Err(RelError::Unsupported(format!(
                "declare an RDF datatype/lexical column for {kind}"
            )));
        }
    };
    Ok(format!("{XSD}{suffix}"))
}
impl RdfMapping {
    pub fn table(
        table: impl Into<String>,
        subject: RdfTermMapping,
        predicate: impl Into<String>,
        object: RdfTermMapping,
    ) -> Self {
        Self {
            dataset: default_dataset(),
            table: table.into(),
            subject,
            predicate: RdfTermMapping::constant(predicate),
            object,
            graph: None,
            key: vec![],
            writable: false,
        }
    }
    pub fn writable(mut self, key: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.writable = true;
        self.key = key.into_iter().map(Into::into).collect();
        self
    }
    pub fn dataset(mut self, dataset: impl Into<String>) -> Self {
        self.dataset = dataset.into();
        self
    }
    pub fn graph(mut self, graph: RdfTermMapping) -> Self {
        self.graph = Some(graph);
        self
    }
    pub(crate) fn project(
        &self,
        plan: LogicalPlan,
        names: &[String; 4],
        identity: &[[String; 3]; 4],
    ) -> RelResult<LogicalPlan> {
        let null = || lit(ScalarValue::Utf8(None));
        let mut expressions = Vec::new();
        for (i, term) in [
            self.graph.as_ref(),
            Some(&self.subject),
            Some(&self.predicate),
            Some(&self.object),
        ]
        .into_iter()
        .enumerate()
        {
            let [value, kind, dt, lang] = match term {
                Some(t) => t.expressions(&plan)?,
                None => [null(), lit("IRI"), null(), null()],
            };
            expressions.extend([
                value.alias(&names[i]),
                kind.alias(&identity[i][0]),
                dt.alias(&identity[i][1]),
                lang.alias(&identity[i][2]),
            ]);
        }
        for term in [&self.subject, &self.predicate]
            .into_iter()
            .chain(self.graph.as_ref())
        {
            if matches!(
                term,
                RdfTermMapping::Literal { .. }
                    | RdfTermMapping::Constant {
                        datatype: Some(_),
                        ..
                    }
                    | RdfTermMapping::Constant {
                        language: Some(_),
                        ..
                    }
            ) {
                return Err(RelError::Unsupported(
                    "RDF subject, predicate and graph cannot be literals".into(),
                ));
            }
        }
        if matches!(self.predicate, RdfTermMapping::Blank { .. })
            || matches!(self.graph, Some(RdfTermMapping::Blank { .. }))
        {
            return Err(RelError::Unsupported(
                "RDF predicate and graph must be IRIs".into(),
            ));
        }
        let mut present = datafusion::prelude::lit(true);
        for term in [&self.subject, &self.predicate, &self.object]
            .into_iter()
            .chain(self.graph.as_ref())
        {
            present = present.and(term.expressions(&plan)?[0].clone().is_not_null());
        }
        Ok(LogicalPlanBuilder::from(plan)
            .filter(present)?
            .project(expressions)?
            .build()?)
    }
}

/// Native scalar representation used by the legacy, untyped result APIs.
pub(crate) fn legacy_scalar_type(iri: &str) -> Option<DataType> {
    Some(match iri.strip_prefix(XSD)? {
        "boolean" => DataType::Boolean,
        "byte" => DataType::Int8,
        "short" => DataType::Int16,
        "int" => DataType::Int32,
        "integer" | "long" => DataType::Int64,
        "unsignedByte" => DataType::UInt8,
        "unsignedShort" => DataType::UInt16,
        "unsignedInt" => DataType::UInt32,
        "unsignedLong" => DataType::UInt64,
        "float" => DataType::Float32,
        "double" => DataType::Float64,
        _ => return None,
    })
}
