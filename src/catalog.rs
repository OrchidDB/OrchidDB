mod auth;
pub use auth::{CatalogAuth, Credential};
use crate::session::Schema;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

pub(crate) mod lowering;
mod manifest;
mod retrieval;
pub mod relationship;
#[cfg(feature = "orchid-catalog")]
mod remote;
#[cfg(feature = "orchid-catalog")]
mod cache;
pub use manifest::{CatalogManifest, CatalogPrincipal, CatalogRecord, ResolvedCatalog};
pub use relationship::{CypherRelationship, RelationshipParameter, RelationshipReturns};
#[cfg(feature = "orchid-catalog")]
pub use remote::OrchidCatalog;

#[derive(Clone, Debug)]
pub struct CatalogSnapshot {
    pub schema: Schema,
    pub revision: i64,
    pub manifest: Option<Arc<CatalogManifest>>,
    pub principal: Option<CatalogPrincipal>,
}

#[async_trait]
pub trait Catalog: Send + Sync {
    async fn snapshot(&self) -> Result<CatalogSnapshot, String>;
}

#[derive(Clone, Debug)]
pub struct InMemoryCatalog {
    state: Arc<RwLock<CatalogSnapshot>>,
}

impl InMemoryCatalog {
    pub fn new(schema: Schema) -> Self {
        Self {
            state: Arc::new(RwLock::new(CatalogSnapshot {
                schema,
                revision: 1,
                manifest: None,
                principal: None,
            })),
        }
    }
    pub fn from_resolved(resolved: ResolvedCatalog) -> Result<Self, String> {
        Ok(Self {
            state: Arc::new(RwLock::new(resolved.snapshot()?)),
        })
    }
    pub fn current(&self) -> Result<CatalogSnapshot, String> {
        self.state
            .read()
            .map(|state| state.clone())
            .map_err(|_| "catalog lock poisoned".into())
    }
    pub fn replace(&self, expected_revision: i64, schema: Schema) -> Result<i64, String> {
        let mut state = self.state.write().map_err(|_| "catalog lock poisoned")?;
        if state.revision != expected_revision {
            return Err("catalog revision conflict".into());
        }
        let revision = state
            .revision
            .checked_add(1)
            .ok_or("catalog revision overflow")?;
        *state = CatalogSnapshot {
            schema,
            revision,
            manifest: None,
            principal: None,
        };
        Ok(revision)
    }
    pub fn register_relationship(
        &self,
        expected_revision: i64,
        relationship: RelationshipDeclaration,
    ) -> Result<i64, String> {
        let snapshot = self.current()?;
        if snapshot.revision != expected_revision {
            return Err("catalog revision conflict".into());
        }
        let (field, name, definition) = relationship.into_schema_entry()?;
        let mut schema = serde_json::to_value(snapshot.schema).map_err(|e| e.to_string())?;
        for key in ["edges", "computed_relationships", "cypher_relationships"] {
            if schema[key].as_array().is_some_and(|entries| {
                entries
                    .iter()
                    .any(|entry| entry["label"] == name || entry["name"] == name)
            }) {
                return Err(format!("relationship `{name}` already exists"));
            }
        }
        schema
            .as_object_mut()
            .unwrap()
            .entry(field)
            .or_insert_with(|| Value::Array(Vec::new()))
            .as_array_mut()
            .ok_or("relationship declarations must be an array")?
            .push(definition);
        self.replace(expected_revision, Schema::from_value(schema)?)
    }
    pub fn remove_relationship(&self, expected_revision: i64, name: &str) -> Result<i64, String> {
        self.edit_relationship(expected_revision, name, None)
    }
    pub fn replace_relationship(
        &self,
        expected_revision: i64,
        name: &str,
        declaration: RelationshipDeclaration,
    ) -> Result<i64, String> {
        let (field, replacement_name, value) = declaration.into_schema_entry()?;
        if replacement_name != name {
            return Err("replacement must retain the relationship name".into());
        }
        self.edit_relationship(expected_revision, name, Some((field, value)))
    }
    fn edit_relationship(
        &self,
        expected_revision: i64,
        name: &str,
        replacement: Option<(&str, Value)>,
    ) -> Result<i64, String> {
        let snapshot = self.current()?;
        if snapshot.revision != expected_revision {
            return Err("catalog revision conflict".into());
        }
        let mut schema = serde_json::to_value(snapshot.schema).map_err(|e| e.to_string())?;
        let mut found = false;
        for key in ["edges", "computed_relationships", "cypher_relationships"] {
            if let Some(entries) = schema.get_mut(key).and_then(Value::as_array_mut) {
                entries.retain(|entry| {
                    let matches = entry["label"] == name || entry["name"] == name;
                    found |= matches;
                    !matches
                });
            }
        }
        if !found {
            return Err(format!("unknown relationship `{name}`"));
        }
        if let Some((field, value)) = replacement {
            schema
                .as_object_mut()
                .unwrap()
                .entry(field)
                .or_insert_with(|| Value::Array(vec![]))
                .as_array_mut()
                .ok_or("relationship declarations must be arrays")?
                .push(value);
        }
        self.replace(expected_revision, Schema::from_value(schema)?)
    }
}

#[async_trait]
impl Catalog for InMemoryCatalog {
    async fn snapshot(&self) -> Result<CatalogSnapshot, String> {
        self.current()
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", content = "definition", rename_all = "snake_case")]
pub enum RelationshipDeclaration {
    Stored(crate::compiler::Edge),
    Computed(crate::ir::rel::mapping::ComputedRelationship),
    Cypher(CypherRelationship),
}
impl RelationshipDeclaration {
    fn into_schema_entry(self) -> Result<(&'static str, String, Value), String> {
        let (field, name, value) = match self {
            Self::Stored(value) => ("edges", value.label.clone(), serde_json::to_value(value)),
            Self::Computed(value) => (
                "computed_relationships",
                value.name.clone(),
                serde_json::to_value(value),
            ),
            Self::Cypher(value) => {
                value.validate()?;
                (
                    "cypher_relationships",
                    value.name.clone(),
                    serde_json::to_value(value),
                )
            }
        };
        if name.is_empty() {
            return Err("relationship name cannot be empty".into());
        }
        Ok((field, name, value.map_err(|e| e.to_string())?))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogGraph {
    pub description: String,
    pub objects: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub execution_connector: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogPublish {
    pub expected_revision: i64,
    pub graph_version: i64,
    pub object_versions: BTreeMap<String, i64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogGrants {
    pub discover: Vec<String>,
    pub execute: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogReference {
    pub endpoint: String,
    pub scope: String,
    pub graph: String,
    #[serde(default = "default_token_env")]
    pub token_env: String,
    #[serde(default)]
    pub auth: Option<CatalogAuth>,
    #[serde(default)]
    pub revision: Option<i64>,
    #[serde(default = "default_refresh_interval_ms")]
    pub refresh_interval_ms: u64,
}
fn default_refresh_interval_ms() -> u64 { 5_000 }
fn default_token_env() -> String {
    "ORCHID_CATALOG_TOKEN".into()
}
impl CatalogReference {
    #[cfg(feature = "orchid-catalog")]
    pub fn client(&self) -> Result<OrchidCatalog, String> {
        let auth = self.auth.clone().unwrap_or_else(|| CatalogAuth::bearer(Credential::env(&self.token_env)));
        let mut catalog = OrchidCatalog::with_auth(&self.endpoint, auth, &self.scope, &self.graph)?;
        if let Some(revision) = self.revision {
            catalog = catalog.at_revision(revision)?;
        }
        Ok(catalog.with_refresh_interval(std::time::Duration::from_millis(self.refresh_interval_ms)))
    }
    pub async fn resolve(&self) -> Result<CatalogSnapshot, String> {
        #[cfg(feature = "orchid-catalog")]
        {
            self.client()?.snapshot().await
        }
        #[cfg(not(feature = "orchid-catalog"))]
        Err("remote catalogs require the orchid-catalog feature".into())
    }
}

pub(crate) async fn command(input: &Value) -> Result<Value, String> {
    #[cfg(feature = "orchid-catalog")]
    {
        let reference: CatalogReference =
            serde_json::from_value(input["catalog"].clone()).map_err(|e| e.to_string())?;
        let catalog = reference.client()?;
        let field = |name: &str| {
            input[name]
                .as_str()
                .ok_or_else(|| format!("missing catalog {name}"))
        };
        let version = || {
            input["expected_version"]
                .as_i64()
                .filter(|v| *v >= 0)
                .ok_or("expected_version must be a nonnegative integer".to_string())
        };
        match field("action")? {
            "refresh" => Ok(serde_json::json!({"revision": catalog.refresh().await?})),
            "discover" => catalog.discover(input["search"].as_str()).await,
            "edges" => catalog.edges(input["search"].as_str()).await,
            "object" => {
                serde_json::to_value(catalog.object(field("id")?).await?).map_err(|e| e.to_string())
            }
            "register_edge" => {
                let definition = &input["definition"];
                if !matches!(
                    definition["kind"].as_str(),
                    Some("relationship" | "cypher_relationship")
                ) {
                    return Err("an edge must be a relationship or cypher_relationship".into());
                }
                catalog
                    .register_object(field("id")?, version()?, definition.clone())
                    .await
            }
            "register_object" => {
                catalog
                    .register_object(field("id")?, version()?, input["definition"].clone())
                    .await
            }
            "register_graph" => {
                catalog
                    .register_graph(
                        version()?,
                        serde_json::from_value(input["definition"].clone())
                            .map_err(|e| e.to_string())?,
                    )
                    .await
            }
            "draft" => catalog.draft().await,
            "publish" => {
                catalog
                    .publish(
                        serde_json::from_value(input["publication"].clone())
                            .map_err(|e| e.to_string())?,
                    )
                    .await
            }
            "grants" => catalog.grants().await,
            "set_grants" => {
                catalog
                    .set_grants(
                        version()?,
                        serde_json::from_value(input["definition"].clone())
                            .map_err(|e| e.to_string())?,
                    )
                    .await
            }
            "graphs" => catalog.graphs().await,
            "audit" => catalog.audit().await,
            "principals" => catalog.principals().await,
            "principal" => catalog.principal(field("id")?).await,
            "register_principal" => catalog.register_principal(field("id")?,version()?,
                serde_json::from_value(input["principal"].clone()).map_err(|_|"invalid catalog principal")?,
                input["enabled"].as_bool().unwrap_or(true)).await,
            other => Err(format!("unknown catalog action {other}")),
        }
    }
    #[cfg(not(feature = "orchid-catalog"))]
    {
        let _ = input;
        Err("remote catalogs require the orchid-catalog feature".into())
    }
}
