use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogRecord {
    pub version: i64,
    pub definition: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogPrincipal {
    pub subject: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub tenant: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CatalogManifest {
    pub protocol_version: u32,
    pub scope: String,
    pub graph: String,
    pub revision: i64,
    pub graph_version: i64,
    pub description: String,
    pub published_at: String,
    pub objects: BTreeMap<String, CatalogRecord>,
    pub schema: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedCatalog {
    pub manifest: CatalogManifest,
    pub principal: CatalogPrincipal,
}

impl CatalogManifest {
    pub fn schema(&self) -> Result<Schema, String> {
        if !matches!(self.protocol_version, 1 | 2) {
            return Err(format!(
                "unsupported catalog protocol {}",
                self.protocol_version
            ));
        }
        if self.revision < 1 || self.graph_version < 1 {
            return Err("invalid catalog revision".into());
        }
        let mut schema = self.schema.clone();
        if !schema.is_object() {
            return Err("catalog schema must be an object".into());
        }
        if schema.get("catalog").is_some() {
            return Err("resolved catalog schemas cannot contain another catalog reference".into());
        }
        if self.protocol_version == 1 && schema.get("cypher_relationships").is_some() {
            return Err("Cypher relationships require catalog protocol 2".into());
        }
        let mut relationships = Vec::new();
        for (id, record) in &self.objects {
            if record.version < 1 {
                return Err(format!("invalid version for catalog object `{id}`"));
            }
            if record.definition["kind"] != "cypher_relationship" {
                continue;
            }
            if self.protocol_version != 2 {
                return Err("Cypher relationships require catalog protocol 2".into());
            }
            let mut definition = record.definition.clone();
            definition.as_object_mut().unwrap().remove("kind");
            for endpoint in ["source", "target"] {
                let reference = definition[endpoint]
                    .as_str()
                    .ok_or("missing relationship endpoint")?;
                let entity = self
                    .objects
                    .get(reference)
                    .ok_or_else(|| format!("unknown relationship endpoint `{reference}`"))?;
                if entity.definition["kind"] != "entity" {
                    return Err(format!(
                        "relationship endpoint `{reference}` is not an entity"
                    ));
                }
                let label = entity.definition["mapping"]["label"]
                    .as_str()
                    .ok_or("entity has no label")?;
                definition[endpoint] = label.into();
            }
            let relationship: CypherRelationship = serde_json::from_value(definition)
                .map_err(|e| format!("relationship `{id}`: {e}"))?;
            relationship.validate()?;
            relationships.push(serde_json::to_value(relationship).map_err(|e| e.to_string())?);
        }
        if !relationships.is_empty() {
            schema["cypher_relationships"] = Value::Array(relationships);
        }
        Schema::from_value(schema)
    }
}

impl ResolvedCatalog {
    pub fn snapshot(self) -> Result<CatalogSnapshot, String> {
        let schema = self.manifest.schema()?;
        Ok(CatalogSnapshot {
            schema,
            revision: self.manifest.revision,
            manifest: Some(Arc::new(self.manifest)),
            principal: Some(self.principal),
        })
    }
}
