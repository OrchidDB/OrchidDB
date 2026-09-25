//! Read-only relational RDF dataset adapter.
//!
//! The compatibility source contract stores IRI-only quads as UTF-8 strings;
//! typed sources add kind, datatype, and language columns to each term.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use arrow::datatypes::DataType;
use datafusion::catalog::TableProvider;
use datafusion::common::ScalarValue;
use datafusion::datasource::provider_as_source;
use datafusion::functions::string::expr_fn as df_string;
use datafusion::logical_expr::{Expr, LogicalPlan, LogicalPlanBuilder};
use datafusion::prelude::lit;

use crate::ir::plan::{RdfGraphScope, RdfTerm};

use super::{LoweredNode, LoweringContext, RelError, RelResult, col_exact, resolve_column_name};

/// An existing table or view containing RDF quads. Without typed term columns,
/// the subject, predicate, and object fields are treated as IRIs. The optional
/// graph column is nullable: NULL denotes the default graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IriQuadSource {
    pub table: String,
    pub subject_column: String,
    pub predicate_column: String,
    pub object_column: String,
    pub graph_column: Option<String>,
    /// Optional term metadata for subject, predicate, and object. When absent,
    /// all three columns are treated as IRIs. `value` stays user-visible;
    /// the remaining columns carry RDF identity through joins.
    pub typed_terms: Option<[RdfTermColumns; 3]>,
}

/// Physical columns describing one RDF term. `kind` contains `IRI`, `BLANK`,
/// or `LITERAL`; datatype and language are nullable UTF-8. Literal strings use
/// the XSD string datatype, language literals use rdf:langString plus a tag,
/// and other typed literals carry their datatype IRI. The value column is the
/// lexical value returned to SPARQL clients.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RdfTermColumns {
    pub value: String,
    pub kind: String,
    pub datatype: Option<String>,
    pub language: Option<String>,
}

const IDENTITY_PREFIX: &str = "__rdf:term:";

/// Reserved output aliases used internally to carry the non-lexical identity
/// of a SPARQL variable between adjacent quad patterns.
pub(crate) fn binding_identity_columns(binding: &str) -> [String; 3] {
    ["kind", "datatype", "language"].map(|part| format!("{IDENTITY_PREFIX}{part}:{binding}"))
}

impl RdfTermColumns {
    pub fn new(value: impl Into<String>, kind: impl Into<String>) -> Self {
        Self {
            value: value.into(),
            kind: kind.into(),
            datatype: None,
            language: None,
        }
    }

    pub fn datatype(mut self, column: impl Into<String>) -> Self {
        self.datatype = Some(column.into());
        self
    }

    pub fn language(mut self, column: impl Into<String>) -> Self {
        self.language = Some(column.into());
        self
    }
}

impl IriQuadSource {
    pub fn table(
        table: impl Into<String>,
        subject_column: impl Into<String>,
        predicate_column: impl Into<String>,
        object_column: impl Into<String>,
    ) -> Self {
        Self {
            table: table.into(),
            subject_column: subject_column.into(),
            predicate_column: predicate_column.into(),
            object_column: object_column.into(),
            graph_column: None,
            typed_terms: None,
        }
    }

    pub fn graph_column(mut self, column: impl Into<String>) -> Self {
        self.graph_column = Some(column.into());
        self
    }

    pub fn typed_term_columns(
        mut self,
        subject: RdfTermColumns,
        predicate: RdfTermColumns,
        object: RdfTermColumns,
    ) -> Self {
        self.typed_terms = Some([subject, predicate, object]);
        self
    }
}

/// Dataset names from `SparqlPlanner::new` resolve to one or more registered
/// quad sources. The table providers supply schema for SQL lowering and
/// actual data for in-process DataFusion execution. For DuckDB execution the
/// same table names must exist in DuckDB; pass `physical_table_names()` to
/// `sql::prepare_with_external`.
#[derive(Default)]
pub struct RdfDatasetMapping {
    sources: BTreeMap<String, Vec<IriQuadSource>>,
    tables: BTreeMap<String, Arc<dyn TableProvider>>,
    graph_tables: BTreeMap<String, (String, String)>,
}

impl fmt::Debug for RdfDatasetMapping {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RdfDatasetMapping")
            .field("sources", &self.sources)
            .field("tables", &self.tables.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl RdfDatasetMapping {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_table(
        &mut self,
        name: impl Into<String>,
        provider: Arc<dyn TableProvider>,
    ) -> &mut Self {
        self.tables.insert(name.into(), provider);
        self
    }

    pub fn map_iri_quads(
        &mut self,
        dataset: impl Into<String>,
        source: IriQuadSource,
    ) -> &mut Self {
        self.sources.entry(dataset.into()).or_default().push(source);
        self
    }

    /// Register a source whose subject, predicate, and object have typed RDF
    /// metadata configured through `IriQuadSource::typed_term_columns`.
    pub fn map_typed_quads(
        &mut self,
        dataset: impl Into<String>,
        source: IriQuadSource,
    ) -> &mut Self {
        self.sources.entry(dataset.into()).or_default().push(source);
        self
    }

    pub fn physical_table_names(&self) -> BTreeSet<String> {
        self.tables.keys().cloned().collect()
    }

    /// Map the named graph registry to an existing table and IRI column.
    /// This preserves graph identity even when a named graph has no triples.
    pub fn map_named_graphs(&mut self, dataset: impl Into<String>, table: impl Into<String>,
        column: impl Into<String>) -> &mut Self {
        self.graph_tables.insert(dataset.into(), (table.into(), column.into()));
        self
    }

    pub fn has_typed_sources(&self) -> bool {
        self.sources
            .values()
            .flatten()
            .any(|source| source.typed_terms.is_some())
    }

    fn source_plan(&self, source: &IriQuadSource) -> RelResult<LogicalPlan> {
        let provider = self.tables.get(&source.table).ok_or_else(|| {
            RelError::Unsupported(format!(
                "RDF source table `{}` has no registered provider/schema",
                source.table
            ))
        })?;
        Ok(LogicalPlanBuilder::scan(
            source.table.clone(),
            provider_as_source(Arc::clone(provider)),
            None,
        )?
        .build()?)
    }
}

fn iri_column(plan: &LogicalPlan, name: &str) -> RelResult<Expr> {
    let resolved = resolve_column_name(plan, name).ok_or_else(|| {
        RelError::Unsupported(format!(
            "RDF source column `{name}` is missing or ambiguous"
        ))
    })?;
    let field = plan.schema().field_with_unqualified_name(&resolved)?;
    if !matches!(
        field.data_type(),
        DataType::Utf8 | DataType::LargeUtf8 | DataType::Utf8View
    ) {
        return Err(RelError::Unsupported(format!(
            "RDF IRI column `{name}` must be UTF-8, got {:?}",
            field.data_type()
        )));
    }
    Ok(col_exact(resolved))
}

fn null_safe_eq(left: Expr, right: Expr) -> Expr {
    left.clone()
        .eq(right.clone())
        .or(left.is_null().and(right.is_null()))
}

fn constant_identity_conditions(
    value_column: &str,
    identity: &[String; 3],
    kind: &str,
    lexical: &str,
    datatype: Option<&str>,
    language: Option<&str>,
) -> Vec<Expr> {
    let datatype = datatype
        .map(|value| lit(value.to_string()))
        .unwrap_or_else(|| lit(ScalarValue::Utf8(None)));
    let language = language
        .map(|value| lit(value.to_ascii_lowercase()))
        .unwrap_or_else(|| lit(ScalarValue::Utf8(None)));
    vec![
        col_exact(value_column).eq(lit(lexical.to_string())),
        col_exact(&identity[0]).eq(lit(kind.to_string())),
        null_safe_eq(col_exact(&identity[1]), datatype),
        null_safe_eq(
            df_string::lower(col_exact(&identity[2])),
            df_string::lower(language),
        ),
    ]
}

fn literal_identity(value: &crate::ir::expr::Lit) -> (String, String) {
    use crate::ir::expr::Lit;
    match value {
        Lit::Null => (
            String::new(),
            "http://www.w3.org/2001/XMLSchema#string".into(),
        ),
        Lit::Bool(value) => (
            value.to_string(),
            "http://www.w3.org/2001/XMLSchema#boolean".into(),
        ),
        Lit::Int(value) => (
            value.to_string(),
            "http://www.w3.org/2001/XMLSchema#integer".into(),
        ),
        Lit::Float(value) => (
            value.to_string(),
            "http://www.w3.org/2001/XMLSchema#double".into(),
        ),
        Lit::String(value) => (
            value.clone(),
            "http://www.w3.org/2001/XMLSchema#string".into(),
        ),
    }
}

fn graph_in(graph_column: &str, allowed: &[String]) -> Expr {
    allowed.iter().fold(lit(false), |condition, iri| {
        condition.or(col_exact(graph_column).eq(lit(iri.clone())))
    })
}

/// A deduplicated, graph-scoped relation over every mapped quad source of
/// one dataset. Column names are unique to this scan.
pub(super) struct QuadSource {
    pub(super) plan: LogicalPlan,
    /// Value columns for graph, subject, predicate, and object.
    pub(super) names: [String; 4],
    /// Kind, datatype, and language columns for the same four roles.
    pub(super) identity: [[String; 3]; 4],
    /// Whether any source carries typed term metadata.
    pub(super) typed: bool,
}

pub(super) fn quad_source(
    ctx: &mut LoweringContext<'_>,
    dataset: &str,
    graph_scope: &RdfGraphScope,
) -> RelResult<QuadSource> {
    if let RdfGraphScope::NamedGraph(term) = graph_scope {
        if !matches!(term, RdfTerm::Iri(_)) {
            return Err(RelError::Unsupported(format!(
                "named RDF graph scope requires an IRI; got {term:?}"
            )));
        }
    }
    if matches!(graph_scope, RdfGraphScope::ActiveGraph) {
        return Err(RelError::Unsupported(
            "active RDF graph scope requires dataset context".into(),
        ));
    }
    let mapping = ctx.options.rdf_datasets.as_ref().ok_or_else(|| {
        RelError::Unsupported(format!(
            "RDF dataset `{dataset}` has no quad mapping in RelBackendOptions"
        ))
    })?;
    let sources = mapping
        .sources
        .get(dataset)
        .ok_or_else(|| RelError::Unsupported(format!("RDF dataset `{dataset}` is not mapped")))?;
    if sources.is_empty() {
        return Err(RelError::Unsupported(format!(
            "RDF dataset `{dataset}` has no quad sources"
        )));
    }
    let typed_dataset = sources.iter().any(|source| source.typed_terms.is_some());
    ctx.rdf_typed_terms_used |= typed_dataset;

    let scan_id = ctx.scan_counter;
    ctx.scan_counter += 1;
    let names =
        ["graph", "subject", "predicate", "object"].map(|part| format!("__w_rdf_{scan_id}_{part}"));
    let identity_names = std::array::from_fn::<_, 4, _>(|role| {
        ["kind", "datatype", "language"].map(|part| format!("__w_rdf_{scan_id}_term_{role}_{part}"))
    });
    let mut branches = Vec::with_capacity(sources.len());
    for source in sources {
        let source_plan = mapping.source_plan(source)?;
        let graph = match &source.graph_column {
            Some(column) => iri_column(&source_plan, column)?,
            None => lit(ScalarValue::Utf8(None)),
        };
        let configured_values = source.typed_terms.as_ref().map(|terms| {
            [
                terms[0].value.as_str(),
                terms[1].value.as_str(),
                terms[2].value.as_str(),
            ]
        });
        let value_columns = configured_values.unwrap_or([
            source.subject_column.as_str(),
            source.predicate_column.as_str(),
            source.object_column.as_str(),
        ]);
        let mut expressions = vec![
            graph.alias(&names[0]),
            lit("IRI").alias(&identity_names[0][0]),
            lit(ScalarValue::Utf8(None)).alias(&identity_names[0][1]),
            lit(ScalarValue::Utf8(None)).alias(&identity_names[0][2]),
        ];
        for (role, value_column) in value_columns.iter().enumerate() {
            expressions.push(iri_column(&source_plan, value_column)?.alias(&names[role + 1]));
            let metadata = source.typed_terms.as_ref().map(|terms| &terms[role]);
            let kind = match metadata {
                Some(columns) => iri_column(&source_plan, &columns.kind)?,
                None => lit("IRI"),
            };
            let datatype = match metadata.and_then(|columns| columns.datatype.as_deref()) {
                Some(column) => iri_column(&source_plan, column)?,
                None => lit(ScalarValue::Utf8(None)),
            };
            // Language tags compare case-insensitively; normalize once at
            // the source so joins, DISTINCT, and results agree.
            let language = match metadata.and_then(|columns| columns.language.as_deref()) {
                Some(column) => df_string::lower(iri_column(&source_plan, column)?),
                None => lit(ScalarValue::Utf8(None)),
            };
            expressions.extend([
                kind.alias(&identity_names[role + 1][0]),
                datatype.alias(&identity_names[role + 1][1]),
                language.alias(&identity_names[role + 1][2]),
            ]);
        }
        branches.push(
            LogicalPlanBuilder::from(source_plan)
                .project(expressions)?
                .build()?,
        );
    }
    let mut source = branches.remove(0);
    for branch in branches {
        source = LogicalPlanBuilder::from(source)
            .union_by_name(branch)?
            .build()?;
    }
    // Restrict the source before correlation. FROM merges selected named
    // graphs into one default graph, so the graph identity must be dropped
    // before DISTINCT; the same triple in two selected graphs occurs once.
    let graph_filter = match graph_scope {
        RdfGraphScope::DefaultGraph => col_exact(&names[0]).is_null(),
        RdfGraphScope::NamedGraph(RdfTerm::Iri(iri)) => col_exact(&names[0]).eq(lit(iri.clone())),
        RdfGraphScope::NamedGraphVariable(_) => col_exact(&names[0]).is_not_null(),
        RdfGraphScope::DatasetDefaultGraph(graphs) => graph_in(&names[0], graphs),
        RdfGraphScope::DatasetNamedGraph { iri, allowed } => {
            if allowed.contains(iri) {
                col_exact(&names[0]).eq(lit(iri.clone()))
            } else {
                lit(false)
            }
        }
        RdfGraphScope::DatasetNamedGraphVariable { allowed, .. } => graph_in(&names[0], allowed),
        _ => unreachable!("validated RDF graph scope"),
    };
    source = LogicalPlanBuilder::from(source)
        .filter(graph_filter)?
        .build()?;
    if matches!(graph_scope, RdfGraphScope::DatasetDefaultGraph(_)) {
        let mut projections = vec![lit(ScalarValue::Utf8(None)).alias(&names[0])];
        projections.extend(names[1..].iter().map(|name| col_exact(name)));
        projections.extend(identity_names.iter().flatten().map(|name| col_exact(name)));
        source = LogicalPlanBuilder::from(source)
            .project(projections)?
            .build()?;
    }
    // RDF graphs are sets. Deduplicate before the join so equal input
    // solution mappings retain their correct multiplicity.
    source = LogicalPlanBuilder::from(source).distinct()?.build()?;

    Ok(QuadSource {
        plan: source,
        names,
        identity: identity_names,
        typed: typed_dataset,
    })
}

/// Enumerate the named graph domain independently of triple cardinality.
pub(super) fn named_graphs(ctx: &mut LoweringContext<'_>, dataset: &str,
    scope: &RdfGraphScope) -> RelResult<(LogicalPlan, String)> {
    let quads = quad_source(ctx, dataset, &RdfGraphScope::NamedGraphVariable("?__graph".into()))?;
    let name = quads.names[0].clone();
    let mut plan = LogicalPlanBuilder::from(quads.plan).project(vec![col_exact(&name)])?.build()?;
    let mapping = ctx.options.rdf_datasets.as_ref().unwrap();
    if let Some((table, column)) = mapping.graph_tables.get(dataset) {
        let provider = mapping.tables.get(table).ok_or_else(|| RelError::Unsupported(
            format!("named graph registry table `{table}` has no registered provider/schema")))?;
        let scan = LogicalPlanBuilder::scan(table.clone(), provider_as_source(Arc::clone(provider)), None)?.build()?;
        let value = iri_column(&scan, column)?;
        let registry = LogicalPlanBuilder::from(scan).filter(value.clone().is_not_null())?
            .project(vec![value.alias(&name)])?.build()?;
        plan = LogicalPlanBuilder::from(plan).union_by_name(registry)?.build()?;
    }
    // Keep graph filtering and DISTINCT above the UNION. The SQL unparser
    // requires a derived-table boundary for modifiers on a set expression.
    plan = LogicalPlanBuilder::from(plan).alias(format!("__w_sql_cte_graph_names_{}", ctx.scan_counter))?.build()?;
    ctx.scan_counter += 1;
    let filter = match scope {
        RdfGraphScope::NamedGraph(RdfTerm::Iri(iri)) => col_exact(&name).eq(lit(iri.clone())),
        RdfGraphScope::NamedGraphVariable(_) => lit(true),
        RdfGraphScope::DatasetNamedGraph { iri, allowed } =>
            if allowed.contains(iri) { col_exact(&name).eq(lit(iri.clone())) } else { lit(false) },
        RdfGraphScope::DatasetNamedGraphVariable { allowed, .. } => graph_in(&name, allowed),
        _ => return Err(RelError::Unsupported("named graph enumeration requires a named graph scope".into())),
    };
    Ok((LogicalPlanBuilder::from(plan).filter(filter)?.distinct()?.build()?, name))
}

pub(super) fn lower_iri_quad_pattern(
    ctx: &mut LoweringContext<'_>,
    dataset: &str,
    graph_scope: &RdfGraphScope,
    subject: &RdfTerm,
    predicate: &RdfTerm,
    object: &RdfTerm,
    _outputs: &[String],
) -> RelResult<LoweredNode> {
    let QuadSource {
        plan: source,
        names,
        identity: identity_names,
        typed: typed_dataset,
    } = quad_source(ctx, dataset, graph_scope)?;
    if !typed_dataset
        && [subject, predicate, object]
            .iter()
            .any(|term| !matches!(term, RdfTerm::Variable(_) | RdfTerm::Iri(_)))
    {
        return Err(RelError::Unsupported(
            "IRI-only RDF quad mapping cannot match literal or blank-node terms; a typed RDF term source is required".into(),
        ));
    }
    let scan_id = ctx.scan_counter;
    ctx.scan_counter += 1;

    let left = ctx.correlate_plan.clone();
    let mut combined = if let Some(left) = &left {
        LogicalPlanBuilder::from(left.clone())
            .cross_join(source)?
            .build()?
    } else {
        source
    };
    let mut conditions = vec![
        col_exact(&names[1]).is_not_null(),
        col_exact(&names[2]).is_not_null(),
        col_exact(&names[3]).is_not_null(),
    ];

    let mut variables = BTreeMap::<String, String>::new();
    let mut variable_identities = BTreeMap::<String, [String; 3]>::new();
    for (term, column) in [
        (subject, &names[1]),
        (predicate, &names[2]),
        (object, &names[3]),
    ] {
        let role = names.iter().position(|name| name == column).unwrap() - 1;
        let identity = &identity_names[role + 1];
        match term {
            RdfTerm::Iri(iri) => conditions.extend(constant_identity_conditions(
                column, identity, "IRI", iri, None, None,
            )),
            RdfTerm::Literal(value) => {
                let (lexical, datatype) = literal_identity(value);
                conditions.extend(constant_identity_conditions(
                    column,
                    identity,
                    "LITERAL",
                    &lexical,
                    Some(&datatype),
                    None,
                ));
            }
            RdfTerm::LanguageTagged { value, lang } => {
                conditions.extend(constant_identity_conditions(
                    column,
                    identity,
                    "LITERAL",
                    value,
                    Some("http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"),
                    Some(lang),
                ))
            }
            RdfTerm::Typed { lexical, datatype } => {
                conditions.extend(constant_identity_conditions(
                    column,
                    identity,
                    "LITERAL",
                    lexical,
                    Some(datatype),
                    None,
                ))
            }
            RdfTerm::BlankNode(value) => conditions.extend(constant_identity_conditions(
                column, identity, "BLANK", value, None, None,
            )),
            RdfTerm::Variable(variable) => {
                if let Some(previous) = variables.get(variable) {
                    conditions.push(col_exact(column).eq(col_exact(previous)));
                    let previous_identity = &variable_identities[variable];
                    for (current, previous) in identity.iter().zip(previous_identity) {
                        conditions.push(null_safe_eq(col_exact(current), col_exact(previous)));
                    }
                } else if left
                    .as_ref()
                    .is_some_and(|plan| resolve_column_name(plan, variable).is_some())
                {
                    conditions.push(col_exact(column).eq(col_exact(variable)));
                    let previous_identity = binding_identity_columns(variable);
                    if left.as_ref().is_some_and(|plan| {
                        previous_identity
                            .iter()
                            .all(|name| resolve_column_name(plan, name).is_some())
                    }) {
                        for (current, previous) in identity.iter().zip(previous_identity.iter()) {
                            conditions.push(null_safe_eq(col_exact(current), col_exact(previous)));
                        }
                    } else if typed_dataset {
                        return Err(RelError::Unsupported(format!(
                            "RDF variable `{variable}` is already bound without term identity metadata; typed joins cannot safely compare it"
                        )));
                    }
                } else {
                    variables.insert(variable.clone(), column.clone());
                    variable_identities.insert(variable.clone(), identity.clone());
                }
            }
        }
    }
    if let Some(variable) = match graph_scope {
        RdfGraphScope::NamedGraphVariable(variable)
        | RdfGraphScope::DatasetNamedGraphVariable { variable, .. } => Some(variable),
        _ => None,
    } {
        if let Some(previous) = variables.get(variable) {
            conditions.push(col_exact(&names[0]).eq(col_exact(previous)));
            let previous_identity = variable_identities.get(variable).ok_or_else(|| {
                RelError::Unsupported(format!(
                    "RDF graph variable `{variable}` lacks term identity metadata"
                ))
            })?;
            for (current, previous) in identity_names[0].iter().zip(previous_identity) {
                conditions.push(null_safe_eq(col_exact(current), col_exact(previous)));
            }
        } else if left
            .as_ref()
            .is_some_and(|plan| resolve_column_name(plan, variable).is_some())
        {
            conditions.push(col_exact(&names[0]).eq(col_exact(variable)));
            let previous_identity = binding_identity_columns(variable);
            if left.as_ref().is_some_and(|plan| {
                previous_identity
                    .iter()
                    .all(|name| resolve_column_name(plan, name).is_some())
            }) {
                for (current, previous) in identity_names[0].iter().zip(previous_identity.iter()) {
                    conditions.push(null_safe_eq(col_exact(current), col_exact(previous)));
                }
            } else if typed_dataset {
                return Err(RelError::Unsupported(format!(
                    "RDF graph variable `{variable}` is bound without term identity metadata"
                )));
            }
        } else {
            variables.insert(variable.clone(), names[0].clone());
            variable_identities.insert(variable.clone(), identity_names[0].clone());
        }
    }
    for condition in conditions {
        combined = LogicalPlanBuilder::from(combined)
            .filter(condition)?
            .build()?;
    }
    let mut projections = left
        .as_ref()
        .map(|plan| {
            plan.schema()
                .fields()
                .iter()
                .map(|field| col_exact(field.name()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    projections.extend(
        variables
            .into_iter()
            .map(|(name, column)| col_exact(column).alias(name)),
    );
    for (variable, columns) in &variable_identities {
        let aliases = binding_identity_columns(variable);
        projections.extend(
            columns
                .iter()
                .zip(aliases)
                .map(|(column, alias)| col_exact(column).alias(alias)),
        );
    }
    if projections.is_empty() {
        projections.push(lit(1_i64).alias(format!("__w_rdf_{scan_id}_match")));
    }
    combined = LogicalPlanBuilder::from(combined)
        .project(projections)?
        .build()?;
    Ok(LoweredNode::new(combined))
}
