//! Connection schema configuration. Query text is supplied separately at execution.
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

/// Reusable graph mappings and source metadata; never a query request.
#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct Schema(Map<String, Value>);
impl Schema {
    pub fn from_json(text: &str) -> Result<Self, String> {
        serde_json::from_str(text).map_err(|e| e.to_string())
    }
    pub fn from_value(value: Value) -> Result<Self, String> {
        let object = value.as_object().ok_or("schema must be an object")?;
        for key in [
            "query",
            "language",
            "parameters",
            "bindings",
            "authorization",
            "op",
            "version",
            "dialect",
        ] {
            if object.contains_key(key) {
                return Err(format!(
                    "{key} belongs to the connection or query, not the schema"
                ));
            }
        }
        if object.contains_key("catalog") && object.keys().any(|key| !matches!(key.as_str(),"catalog"|"functions"|"procedures"|"engines"|"execution_engine")) {
            return Err("catalog references accept only host function, procedure and engine bindings".into());
        }
        // Reuse the core's field validation; schema files cannot hide ignored query fields.
        let mut request = object.clone();
        request.extend(serde_json::json!({"version":1,"dialect":"duckdb","language":"cypher","query":"RETURN 1"}).as_object().unwrap().clone());
        let validated = serde_json::from_value::<crate::compiler::CompileRequest>(Value::Object(request))
            .map_err(|e| e.to_string())?;
        for relationship in &validated.cypher_relationships { relationship.validate()?; }
        Ok(Self(object.clone()))
    }
    pub async fn resolve(&self) -> Result<Self, String> {
        match self.0.get("catalog") {
            Some(reference) => {
                let reference: crate::catalog::CatalogReference = serde_json::from_value(reference.clone()).map_err(|e|e.to_string())?;
                let snapshot = reference.resolve().await?;
                let bindings = self.0.iter().filter(|(key,_)| key.as_str() != "catalog").map(|(key,value)|(key.clone(),value.clone())).collect();
                snapshot.schema.with_bindings(&Value::Object(bindings))
            }
            None => Ok(self.clone()),
        }
    }
    pub fn with_bindings(&self, bindings: &Value) -> Result<Self, String> {
        let bindings = bindings.as_object().ok_or("catalog bindings must be an object")?;
        let mut schema = self.0.clone();
        for (name,value) in bindings {
            if !matches!(name.as_str(),"functions"|"procedures"|"engines"|"execution_engine") {
                return Err(format!("unsupported host catalog binding `{name}`"));
            }
            if matches!(name.as_str(),"procedures"|"engines") {
                let target = schema.entry(name.clone()).or_insert_with(||Value::Object(Map::new())).as_object_mut().ok_or("catalog bindings must be objects")?;
                target.extend(value.as_object().ok_or("catalog bindings must be objects")?.clone());
            } else { schema.insert(name.clone(),value.clone()); }
        }
        Self::from_value(Value::Object(schema))
    }
    /// Internal adapter input; customer APIs execute Query separately from Schema.
    #[doc(hidden)]
    pub fn request(
        &self,
        dialect: &str,
        query: &Query,
    ) -> Result<crate::compiler::CompileRequest, String> {
        let mut input = self.0.clone();
        input.extend(
            serde_json::json!({"version":1,"dialect":dialect,"language":query.language,
            "query":query.text,"parameters":query.parameters,"authorization":query.authorization})
            .as_object()
            .unwrap()
            .clone(),
        );
        serde_json::from_value(Value::Object(input)).map_err(|e| e.to_string())
    }
}
impl<'de> Deserialize<'de> for Schema {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_value(Value::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}

/// Query text, language and per-execution parameters. Contains no schema.
#[derive(Clone, Debug)]
pub struct Query {
    pub text: String,
    pub language: String,
    pub parameters: std::collections::BTreeMap<String, Value>,
    pub authorization: Option<crate::compiler::Authorization>,
}
impl Query {
    pub fn cypher(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            language: "cypher".into(),
            parameters: Default::default(),
            authorization: None,
        }
    }
    pub fn gremlin(text: impl Into<String>) -> Self {
        Self {
            language: "gremlin".into(),
            ..Self::cypher(text)
        }
    }
    pub fn sparql(text: impl Into<String>) -> Self {
        Self {
            language: "sparql".into(),
            ..Self::cypher(text)
        }
    }
    pub fn parameter(mut self, name: impl Into<String>, value: impl Into<Value>) -> Self {
        self.parameters.insert(name.into(), value.into());
        self
    }
}
