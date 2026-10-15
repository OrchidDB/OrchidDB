use super::*;
/// SpiceDB HTTP gateway. Checks use fully consistent reads; caveated/unknown
/// decisions fail closed. No credential or response body appears in errors.
pub struct SpiceDb {
    client: reqwest::Client,
    endpoint: reqwest::Url,
    token: String,
}
impl SpiceDb {
    pub fn new(endpoint: &str, token: String) -> Result<Self, String> {
        let mut endpoint = reqwest::Url::parse(endpoint).map_err(|_| "invalid SpiceDB URL")?;
        let loopback = endpoint.host_str().is_some_and(|h| {
            h == "localhost"
                || h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if token.is_empty()
            || (endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && loopback))
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err("SpiceDB requires a token and HTTPS (HTTP permitted on loopback)".into());
        }
        endpoint.set_path(&format!(
            "{}/v1/permissions/check",
            endpoint.path().trim_end_matches('/')
        ));
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|_| "SpiceDB client initialization failed")?;
        Ok(Self {
            client,
            endpoint,
            token,
        })
    }
}
#[async_trait]
impl Authorizer for SpiceDb {
    async fn check(
        &self,
        subject: &Authorization,
        resource_type: &str,
        resource_id: &str,
        permission: &str,
    ) -> Result<bool, String> {
        if [
            &subject.subject_type,
            &subject.subject_id,
            resource_type,
            resource_id,
            permission,
        ]
        .iter()
        .any(|s| s.is_empty())
        {
            return Err("empty authorization identity".into());
        }
        let response = self.client.post(self.endpoint.clone()).bearer_auth(&self.token).json(&serde_json::json!({
            "consistency":{"fullyConsistent":true},
            "resource":{"objectType":resource_type,"objectId":resource_id},
            "permission":permission,
            "subject":{"object":{"objectType":subject.subject_type,"objectId":subject.subject_id}}
        })).send().await.map_err(|_| "SpiceDB request failed")?;
        if !response.status().is_success() {
            return Err(format!("SpiceDB HTTP {}", response.status().as_u16()));
        }
        let body: Value = response
            .json()
            .await
            .map_err(|_| "invalid SpiceDB response")?;
        match body["permissionship"].as_str() {
            Some("PERMISSIONSHIP_HAS_PERMISSION") => Ok(true),
            Some("PERMISSIONSHIP_NO_PERMISSION" | "PERMISSIONSHIP_CONDITIONAL_PERMISSION") => {
                Ok(false)
            }
            _ => Err("unknown SpiceDB permission decision".into()),
        }
    }
    async fn check_many(
        &self,
        subject: &Authorization,
        checks: &[PermissionCheck<'_>],
    ) -> Result<Vec<bool>, String> {
        if subject.subject_type.is_empty() || subject.subject_id.is_empty() {
            return Err("empty authorization identity".into());
        }
        let mut decisions = Vec::with_capacity(checks.len());
        // Bound payloads and server work; each chunk gets a fresh fully-consistent check.
        for chunk in checks.chunks(512) {
            let mut items = Vec::with_capacity(chunk.len());
            for c in chunk {
                if [c.resource_type, c.resource_id, c.permission]
                    .iter()
                    .any(|s| s.is_empty())
                {
                    return Err("empty authorization identity".into());
                }
                items.push(serde_json::json!({"resource":{"objectType":c.resource_type,"objectId":c.resource_id},"permission":c.permission,"subject":{"object":{"objectType":subject.subject_type,"objectId":subject.subject_id}}}));
            }
            let mut endpoint = self.endpoint.clone();
            endpoint.set_path(&format!("{}bulk", self.endpoint.path()));
            let response = self
                .client
                .post(endpoint)
                .bearer_auth(&self.token)
                .json(&serde_json::json!({"consistency":{"fullyConsistent":true},"items":items}))
                .send()
                .await
                .map_err(|_| "SpiceDB bulk request failed")?;
            if !response.status().is_success() {
                return Err(format!("SpiceDB bulk HTTP {}", response.status().as_u16()));
            }
            let body: Value = response
                .json()
                .await
                .map_err(|_| "invalid SpiceDB bulk response")?;
            let pairs = body["pairs"]
                .as_array()
                .ok_or("missing SpiceDB bulk decisions")?;
            if pairs.len() != items.len() {
                return Err("incomplete SpiceDB bulk decisions".into());
            }
            for (pair, item) in pairs.iter().zip(&items) {
                let request = &pair["request"];
                if !pair["error"].is_null()
                    || request["resource"] != item["resource"]
                    || request["permission"] != item["permission"]
                    || request["subject"]["object"] != item["subject"]["object"]
                    || request["subject"]["optionalRelation"]
                        .as_str()
                        .is_some_and(|v| !v.is_empty())
                {
                    return Err("mismatched or failed SpiceDB bulk decision".into());
                }
                decisions.push(match pair["item"]["permissionship"].as_str() {
                    Some("PERMISSIONSHIP_HAS_PERMISSION") => true,
                    Some(
                        "PERMISSIONSHIP_NO_PERMISSION" | "PERMISSIONSHIP_CONDITIONAL_PERMISSION",
                    ) => false,
                    _ => return Err("unknown SpiceDB bulk permission decision".into()),
                });
            }
        }
        Ok(decisions)
    }
}
