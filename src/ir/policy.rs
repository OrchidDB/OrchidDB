//! `GraphPlanPolicy` — the semantic contract attached to every plan.
//!
//! Mirrors §0 of `docs/graph_ir_language_examples_v0_2_draft.md`. The policy
//! is part of the logical semantics; later rewrites must preserve it.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Cypher,
    Gql,
    Sparql,
    Gremlin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultForm {
    RowSet,
    TraverserStream,
    RdfGraph,
    Boolean,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Multiplicity {
    Bag,
    BulkAwareBag,
    Set,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyMissing {
    NullOnMissing,
    DropUnproductive,
    Unbound,
    Error,
    ProviderDefined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalMissing {
    Null,
    Unbound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathMode {
    None,
    Walk,
    Trail,
    Simple,
    Acyclic,
    /// Gremlin traverser path semantics: path history is the sequence of
    /// objects visited by the traverser, not necessarily a property-graph
    /// path value.
    TraverserHistory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    None,
    DifferentRelationships,
    RepeatableElements,
    ProviderDefined,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphScope {
    PropertyGraph,
    ActiveRdfGraph,
    DefaultRdfGraph,
    NamedRdfGraph(String),
    ProviderDefined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputNaming {
    SourceNames,
    AliasedNames,
    SyntheticNames,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderFeature {
    MultiLabelVertices,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPlanPolicy {
    pub language: Language,
    pub result_form: ResultForm,
    pub multiplicity: Multiplicity,
    pub property_missing: PropertyMissing,
    pub optional_missing: OptionalMissing,
    pub path_mode: PathMode,
    pub match_mode: MatchMode,
    pub graph_scope: GraphScope,
    pub output_naming: OutputNaming,
    pub provider_features: Vec<ProviderFeature>,
}

impl GraphPlanPolicy {
    pub fn cypher() -> Self {
        Self {
            language: Language::Cypher,
            result_form: ResultForm::RowSet,
            multiplicity: Multiplicity::Bag,
            property_missing: PropertyMissing::NullOnMissing,
            optional_missing: OptionalMissing::Null,
            path_mode: PathMode::Walk,
            match_mode: MatchMode::DifferentRelationships,
            graph_scope: GraphScope::PropertyGraph,
            output_naming: OutputNaming::AliasedNames,
            provider_features: Vec::new(),
        }
    }

    pub fn gremlin() -> Self {
        Self {
            language: Language::Gremlin,
            result_form: ResultForm::TraverserStream,
            multiplicity: Multiplicity::BulkAwareBag,
            property_missing: PropertyMissing::DropUnproductive,
            optional_missing: OptionalMissing::Null,
            path_mode: PathMode::TraverserHistory,
            match_mode: MatchMode::ProviderDefined,
            graph_scope: GraphScope::PropertyGraph,
            output_naming: OutputNaming::SourceNames,
            provider_features: Vec::new(),
        }
    }

    pub fn sparql() -> Self {
        Self {
            language: Language::Sparql,
            result_form: ResultForm::RowSet,
            multiplicity: Multiplicity::Bag,
            property_missing: PropertyMissing::Unbound,
            optional_missing: OptionalMissing::Unbound,
            path_mode: PathMode::None,
            match_mode: MatchMode::None,
            graph_scope: GraphScope::DefaultRdfGraph,
            output_naming: OutputNaming::SourceNames,
            provider_features: Vec::new(),
        }
    }
}
