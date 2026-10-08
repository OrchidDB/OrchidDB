use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipParameter {
    pub name: String,
    pub schema: Value,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "present_default"
    )]
    pub default: Option<Value>,
}
fn present_default<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RelationshipReturns {
    pub target: String,
    #[serde(default)]
    pub properties: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CypherRelationship {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub parameters: Vec<RelationshipParameter>,
    pub cypher: String,
    pub returns: RelationshipReturns,
}
struct LocalSchemas;
impl jsonschema::Retrieve for LocalSchemas {
    fn retrieve(
        &self,
        _: &jsonschema::Uri<String>,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        Err("external JSON Schema references are not supported".into())
    }
}
fn validator(schema: &Value) -> Result<jsonschema::Validator, String> {
    jsonschema::meta::validate(schema).map_err(|e| e.to_string())?;
    jsonschema::options()
        .with_retriever(LocalSchemas)
        .build(schema)
        .map_err(|e| e.to_string())
}
impl CypherRelationship {
    pub fn new(name: impl Into<String>, source: impl Into<String>, target: impl Into<String>,
        cypher: impl Into<String>, description: impl Into<String>) -> Self {
        Self { name: name.into(), source: source.into(), target: target.into(), cypher: cypher.into(),
            description: description.into(), parameters: Vec::new(),
            returns: RelationshipReturns { target: "target".into(), properties: BTreeMap::new() } }
    }
    pub fn with_property(mut self, name: impl Into<String>, schema: Value) -> Self {
        self.returns.properties.insert(name.into(), schema);
        self
    }
    pub fn with_parameter(mut self, parameter: RelationshipParameter) -> Self {
        self.parameters.push(parameter);
        self
    }
    pub fn validate(&self) -> Result<(), String> {
        if [&self.name, &self.source, &self.target, &self.returns.target]
            .iter()
            .any(|s| s.is_empty())
        {
            return Err("relationship names and endpoints cannot be empty".into());
        }
        if self.returns.properties.contains_key(&self.returns.target) {
            return Err("relationship target cannot also be a property".into());
        }
        let mut names = BTreeSet::new();
        for parameter in &self.parameters {
            if parameter.name.is_empty() || !names.insert(&parameter.name) {
                return Err("relationship parameter names must be nonempty and unique".into());
            }
            let validator = validator(&parameter.schema)?;
            if let Some(default) = &parameter.default {
                validator
                    .validate(default)
                    .map_err(|e| format!("default for {}: {e}", parameter.name))?;
            }
        }
        for schema in self.returns.properties.values() {
            validator(schema)?;
        }
        let query = crate::language::cypher::parser::parse_query(&self.cypher)
            .map_err(|e| e.to_string())?;
        validate_body(&query, &self.returns)?;
        Ok(())
    }
    pub fn arguments(
        &self,
        supplied: &BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Value>, String> {
        self.validate()?;
        for name in supplied.keys() {
            if !self.parameters.iter().any(|p| &p.name == name) {
                return Err(format!("unknown parameter `{name}` for {}", self.name));
            }
        }
        self.parameters
            .iter()
            .map(|parameter| {
                let value = supplied
                    .get(&parameter.name)
                    .or(parameter.default.as_ref())
                    .ok_or_else(|| {
                        format!("missing parameter `{}` for {}", parameter.name, self.name)
                    })?;
                validator(&parameter.schema)?
                    .validate(value)
                    .map_err(|e| format!("parameter {}: {e}", parameter.name))?;
                Ok((parameter.name.clone(), value.clone()))
            })
            .collect()
    }
}
fn validate_body(
    query: &crate::language::cypher::ast::Query,
    returns: &RelationshipReturns,
) -> Result<(), String> {
    use crate::language::cypher::ast::Clause;
    for clause in &query.clauses {
        if matches!(
            clause,
            Clause::Create(_) | Clause::Merge(_) | Clause::Set(_) | Clause::Delete(_)
        ) {
            return Err("relationship bodies must be read-only".into());
        }
    }
    let Some(Clause::Return(result)) = query.clauses.last() else {
        return Err("relationship bodies must end with RETURN".into());
    };
    let fields = result
        .projection
        .items
        .iter()
        .map(|item| {
            item.alias
                .clone()
                .or_else(|| {
                    if let crate::language::cypher::ast::Expr::Variable(name) = &item.expr {
                        Some(name.clone())
                    } else {
                        None
                    }
                })
                .ok_or("relationship return expressions require aliases")
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let expected = std::iter::once(returns.target.clone())
        .chain(returns.properties.keys().cloned())
        .collect();
    if result.projection.include_existing
        || fields != expected
        || fields.len() != result.projection.items.len()
    {
        return Err("relationship RETURN must match its declared target and properties".into());
    }
    for branch in &query.unions {
        validate_body(&branch.query, returns)?;
    }
    Ok(())
}

pub(crate) fn validate_target_entity(
    query: &crate::language::cypher::ast::Query,
    declaration: &CypherRelationship,
    declarations: &BTreeMap<String, CypherRelationship>,
) -> Result<(), String> {
    use crate::language::cypher::ast::{Clause, Expr};
    let mut labels = BTreeMap::from([(
        "source".to_string(),
        BTreeSet::from([declaration.source.clone()]),
    )]);
    for clause in &query.clauses {
        match clause {
            Clause::Match(m) => {
                for pattern in &m.patterns {
                    for node in std::iter::once(&pattern.element.start)
                        .chain(pattern.element.chains.iter().map(|chain| &chain.node))
                    {
                        if let Some(name) = &node.variable {
                            if !node.labels.is_empty() {
                                labels.insert(name.clone(), node.labels.iter().cloned().collect());
                            }
                        }
                    }
                }
            }
            Clause::With(with) => {
                let mut next = if with.projection.include_existing {
                    labels.clone()
                } else {
                    BTreeMap::new()
                };
                for item in &with.projection.items {
                    if let Expr::Variable(name) = &item.expr {
                        if let Some(entity) = labels.get(name) {
                            next.insert(
                                item.alias.clone().unwrap_or_else(|| name.clone()),
                                entity.clone(),
                            );
                        }
                    }
                }
                labels = next;
            }
            Clause::Call(call) => {
                if let Some(called) = declarations.get(&call.name) {
                    for item in &call.yields {
                        if item.field == called.returns.target {
                            labels.insert(
                                item.alias.clone(),
                                BTreeSet::from([called.target.clone()]),
                            );
                        }
                    }
                }
            }
            Clause::Return(ret) => {
                let target = ret.projection.items.iter().find(|item| {
                    item.alias.as_deref().or_else(|| {
                        if let Expr::Variable(name) = &item.expr {
                            Some(name.as_str())
                        } else {
                            None
                        }
                    }) == Some(&declaration.returns.target)
                });
                let actual = target.and_then(|item| {
                    if let Expr::Variable(name) = &item.expr {
                        labels.get(name)
                    } else {
                        None
                    }
                });
                if actual != Some(&BTreeSet::from([declaration.target.clone()])) {
                    return Err(format!(
                        "relationship {} must return a node of declared target entity {}",
                        declaration.name, declaration.target
                    ));
                }
            }
            _ => {}
        }
    }
    for branch in &query.unions {
        validate_target_entity(&branch.query, declaration, declarations)?;
    }
    Ok(())
}
