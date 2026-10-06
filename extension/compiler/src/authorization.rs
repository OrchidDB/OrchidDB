//! Graph policy mapping and the SpiceDB transport. Permission semantics belong to SpiceDB.
use crate::syntax::{Graph, ident, literal};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub provider: String,
    pub vertices: Vec<Rule>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub label: String,
    pub resource: Option<String>,
    pub key: Option<String>,
    pub permission: Option<String>,
}

pub fn apply(
    policy: &Policy,
    graph: &Graph,
    tables: &[Value],
    nodes: &mut [Value],
    edges: &mut [Value],
) -> Result<(), String> {
    if policy.provider.is_empty() {
        return Err("authorization provider cannot be empty".into());
    }
    let mut seen = BTreeSet::new();
    for rule in &policy.vertices {
        if !seen.insert(&rule.label) {
            return Err(format!("duplicate authorization rule for {}", rule.label));
        }
        let index = graph
            .vertices
            .iter()
            .position(|v| v.label == rule.label)
            .ok_or_else(|| format!("unknown authorization vertex {}", rule.label))?;
        match (&rule.resource, &rule.key, &rule.permission) {
            (None, None, None) => {}
            (Some(resource), Some(key), Some(permission))
                if !resource.is_empty() && !permission.is_empty() =>
            {
                let column = tables[index]["columns"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|c| {
                        c["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case(key))
                    })
                    .ok_or_else(|| format!("unknown authorization key {}.{key}", rule.label))?;
                if !matches!(
                    column["data_type"].as_str(),
                    Some(
                        "string"
                            | "int8"
                            | "int16"
                            | "int32"
                            | "int64"
                            | "uint8"
                            | "uint16"
                            | "uint32"
                            | "uint64"
                    )
                ) {
                    return Err("authorization keys must be strings or integers".into());
                }
                let key = ident(column["name"].as_str().unwrap());
                let table = tables[index]["name"].as_str().unwrap();
                nodes[index]["source_query"] = json!(format!(
                    "SELECT * FROM {table} WHERE __orchid_spicedb_check({}, {}, {}, CAST({key} AS VARCHAR))",
                    literal(&policy.provider),
                    literal(resource),
                    literal(permission)
                ));
            }
            _ => return Err("invalid authorization rule".into()),
        }
    }
    // An omitted rule is DENY, including newly added labels.
    for (index, vertex) in graph.vertices.iter().enumerate() {
        if !seen.contains(&vertex.label) {
            nodes[index]["source_query"] = json!(format!(
                "SELECT * FROM {} WHERE FALSE",
                tables[index]["name"].as_str().unwrap()
            ));
        }
    }
    // Standalone edge scans must enforce endpoint access too (g.E(), counts, etc.).
    for (index, edge) in graph.edges.iter().enumerate() {
        let mut predicates = Vec::new();
        for endpoint in [&edge.from, &edge.to] {
            let endpoint = endpoint.as_ref().ok_or("missing edge endpoint")?;
            let ni = graph
                .vertices
                .iter()
                .position(|v| v.alias.eq_ignore_ascii_case(&endpoint.node))
                .ok_or("unknown endpoint")?;
            let source = nodes[ni]["source_query"]
                .as_str()
                .map(|s| format!("({s})"))
                .unwrap_or_else(|| tables[ni]["name"].as_str().unwrap().to_owned());
            let joins = endpoint
                .columns
                .iter()
                .zip(&endpoint.references)
                .map(|(src, dst)| format!("e.{} = n.{}", ident(src), ident(dst)))
                .collect::<Vec<_>>()
                .join(" AND ");
            predicates.push(format!(
                "EXISTS (SELECT 1 FROM {source} AS n WHERE {joins})"
            ));
        }
        edges[index]["source_query"] = json!(format!(
            "SELECT e.* FROM {} AS e WHERE {}",
            tables[graph.vertices.len() + index]["name"]
                .as_str()
                .unwrap(),
            predicates.join(" AND ")
        ));
    }
    Ok(())
}

#[cfg(not(feature = "spicedb"))]
pub fn check(_: &Value) -> Result<Value, String> {
    Err("this Orchid build has no SpiceDB support".into())
}

#[cfg(feature = "spicedb")]
pub fn check(input: &Value) -> Result<Value, String> {
    use std::{io::Read, sync::OnceLock, time::Duration};
    static CLIENT: OnceLock<Result<reqwest::blocking::Client, String>> = OnceLock::new();
    let client = CLIENT
        .get_or_init(|| {
            reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .connect_timeout(Duration::from_secs(3))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|_| "could not initialize SpiceDB HTTP client".into())
        })
        .as_ref()
        .map_err(Clone::clone)?;
    let endpoint = input["endpoint"]
        .as_str()
        .ok_or("missing SpiceDB endpoint")?;
    let url = reqwest::Url::parse(endpoint).map_err(|_| "invalid SpiceDB endpoint")?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !(url.scheme() == "https"
            || (url.scheme() == "http"
                && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))))
    {
        return Err("SpiceDB requires HTTPS (HTTP is allowed only on loopback)".into());
    }
    let items = input["items"]
        .as_array()
        .ok_or("missing permission checks")?;
    if items.is_empty() || items.len() > 2048 {
        return Err("invalid SpiceDB batch size".into());
    }
    let consistency = if let Some(token) = input["revision"].as_str() {
        json!({"atExactSnapshot":{"token":token}})
    } else if let Some(token) = input["authorization"]["at_least_as_fresh"].as_str() {
        json!({"atLeastAsFresh":{"token":token}})
    } else {
        json!({"fullyConsistent":true})
    };
    let subject = json!({"object":{"objectType":input["authorization"]["subject_type"],"objectId":input["authorization"]["subject_id"]}});
    let checks = items.iter().map(|item|json!({"resource":{"objectType":item[0],"objectId":item[2]},"permission":item[1],"subject":subject,"context":input["authorization"].get("context").cloned().unwrap_or(json!({}))})).collect::<Vec<_>>();
    let response = client
        .post(format!(
            "{}/v1/permissions/checkbulk",
            endpoint.trim_end_matches('/')
        ))
        .bearer_auth(input["token"].as_str().ok_or("missing SpiceDB token")?)
        .json(&json!({"consistency":consistency,"items":checks}))
        .send()
        .map_err(|_| "SpiceDB permission request failed or timed out")?;
    if !response.status().is_success() {
        return Err(format!(
            "SpiceDB permission request failed (HTTP {})",
            response.status().as_u16()
        ));
    }
    let mut bytes = Vec::new();
    response
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "SpiceDB response read failed")?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("SpiceDB response exceeds limit".into());
    }
    let result: Value = serde_json::from_slice(&bytes).map_err(|_| "invalid SpiceDB response")?;
    let revision = result["checkedAt"]["token"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("missing SpiceDB revision")?;
    if input["revision"].as_str().is_some_and(|v| v != revision) {
        return Err("SpiceDB returned an inconsistent revision".into());
    }
    let pairs = result["pairs"]
        .as_array()
        .ok_or("missing SpiceDB decisions")?;
    if pairs.len() != items.len() {
        return Err("incomplete SpiceDB decisions".into());
    }
    let mut decisions = Vec::new();
    for (pair, expected) in pairs.iter().zip(&checks) {
        let request = &pair["request"];
        if request["resource"] != expected["resource"]
            || request["permission"] != expected["permission"]
            || request["subject"]["object"] != expected["subject"]["object"]
        {
            return Err("SpiceDB decision did not match requested resource/subject".into());
        }
        decisions.push(match pair["item"]["permissionship"].as_str() {
            Some("PERMISSIONSHIP_HAS_PERMISSION") => true,
            Some("PERMISSIONSHIP_NO_PERMISSION") => false,
            Some("PERMISSIONSHIP_CONDITIONAL_PERMISSION") => {
                return Err("SpiceDB permission requires additional caveat context".into());
            }
            _ => return Err("SpiceDB returned a permission error or invalid decision".into()),
        });
    }
    Ok(json!({"revision":revision,"decisions":decisions}))
}
