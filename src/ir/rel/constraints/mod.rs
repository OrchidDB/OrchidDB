//! Engine-neutral relational facts supplied by callers, never inferred from names.
//! `Enforced` is a caller contract valid for every execution. Snapshot facts are
//! enabled only by an explicitly matching scope; estimates never authorize rewrites.
#[cfg(feature = "duckdb")]
pub mod duckdb;
mod properties;
mod provider;
mod rewrite;
pub use properties::{PlanProperties, analyze};
pub(crate) use provider::{bind, ConstrainedProvider};
pub use rewrite::{ConstraintOptimizer, RewriteProof, optimize};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Evidence {
    /// The supplying engine/caller guarantees enforcement for the plan lifetime.
    Enforced,
    /// Valid only in the named immutable snapshot/transaction scope.
    Validated {
        scope: String,
    },
    #[default]
    Declared,
    Estimate,
}
impl Evidence {
    pub fn usable(&self, scope: Option<&str>) -> bool {
        matches!(self, Self::Enforced)
            || matches!(self, Self::Validated { scope: s } if !s.is_empty() && Some(s.as_str()) == scope)
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Fact {
    Unique {
        columns: Vec<String>,
        #[serde(default)]
        nulls_equal: bool,
    },
    NonNull {
        columns: Vec<String>,
    },
    ForeignKey {
        columns: Vec<String>,
        target: String,
        references: Vec<String>,
    },
    FunctionalDependency {
        determinant: Vec<String>,
        dependent: Vec<String>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constraint {
    pub name: String,
    pub fact: Fact,
    #[serde(default)]
    pub evidence: Evidence,
}
impl Constraint {
    pub fn enforced(name: impl Into<String>, fact: Fact) -> Self {
        Self {
            name: name.into(),
            fact,
            evidence: Evidence::Enforced,
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstraintCatalog {
    #[serde(default)]
    pub tables: BTreeMap<String, Vec<Constraint>>,
    /// A revision supplied by the caller; changing the catalog creates new plans.
    #[serde(default)]
    pub revision: String,
}
impl ConstraintCatalog {
    pub fn insert(&mut self, table: impl Into<String>, constraint: Constraint) -> &mut Self {
        self.tables
            .entry(table.into())
            .or_default()
            .push(constraint);
        self
    }
    /// Validate structure against registered schemas. Does not inspect source rows.
    pub fn validate(
        &self,
        schemas: &BTreeMap<String, arrow::datatypes::SchemaRef>,
    ) -> Result<(), String> {
        let columns = |table: &str, names: &[String]| -> Result<(), String> {
            let schema = schemas
                .get(table)
                .ok_or_else(|| format!("constraint references unknown table {table}"))?;
            if names.is_empty() || names.iter().collect::<BTreeSet<_>>().len() != names.len() {
                return Err(format!(
                    "constraint on {table} needs nonempty, distinct columns"
                ));
            }
            for name in names {
                schema
                    .index_of(name)
                    .map_err(|_| format!("constraint references missing column {table}.{name}"))?;
            }
            Ok(())
        };
        for (table, facts) in &self.tables {
            let mut names = BTreeSet::new();
            for c in facts {
                if c.name.is_empty() || !names.insert(&c.name) {
                    return Err(format!(
                        "constraint names on {table} must be nonempty and unique"
                    ));
                }
                match &c.fact {
                    Fact::Unique { columns: cols, .. } | Fact::NonNull { columns: cols } => {
                        columns(table, cols)?
                    }
                    Fact::FunctionalDependency {
                        determinant,
                        dependent,
                    } => {
                        columns(table, determinant)?;
                        columns(table, dependent)?;
                    }
                    Fact::ForeignKey {
                        columns: cols,
                        target,
                        references,
                    } => {
                        columns(table, cols)?;
                        columns(target, references)?;
                        if cols.len() != references.len() {
                            return Err(format!("foreign key {} has mismatched arity", c.name));
                        }
                        for (a, b) in cols.iter().zip(references) {
                            if schemas[table].field_with_name(a).unwrap().data_type()
                                != schemas[target].field_with_name(b).unwrap().data_type()
                            {
                                return Err(format!(
                                    "foreign key {} has incompatible column types",
                                    c.name
                                ));
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// Proven bounds, not estimates of typical degree. A missing lower bound does
/// not imply that a relationship is optional in the application schema.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelationshipMultiplicity {
    pub at_most_one_outgoing: bool,
    pub at_most_one_incoming: bool,
    pub source_endpoint_exists: bool,
    pub target_endpoint_exists: bool,
}
