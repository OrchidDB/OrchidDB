use super::*;
use reqwest::{Client, Method, Url};

#[derive(Clone)]
pub struct OrchidCatalog {
    client: Client,
    endpoint: Url,
    auth: CatalogAuth,
    scope: String,
    graph: String,
    revision: Option<i64>,
    bindings: Value,
    refresh_interval: std::time::Duration,
}
impl OrchidCatalog {
    pub fn new(
        endpoint: &str,
        token: impl Into<String>,
        scope: impl Into<String>,
        graph: impl Into<String>,
    ) -> Result<Self, String> {
        Self::with_auth(
            endpoint,
            CatalogAuth::bearer(Credential::value(token.into())),
            scope,
            graph,
        )
    }
    pub fn with_auth(
        endpoint: &str,
        auth: CatalogAuth,
        scope: impl Into<String>,
        graph: impl Into<String>,
    ) -> Result<Self, String> {
        let endpoint = Url::parse(endpoint).map_err(|e| e.to_string())?;
        let loopback = endpoint.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if (endpoint.scheme() != "https" && !(endpoint.scheme() == "http" && loopback))
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
        {
            return Err(
                "catalog endpoint requires HTTPS except on loopback, without credentials, query or fragment"
                    .into(),
            );
        }
        let scope = scope.into();
        let graph = graph.into();
        identifier(&scope)?;
        identifier(&graph)?;
        Ok(Self {
            client: {
                static CLIENT: std::sync::OnceLock<Result<Client, String>> =
                    std::sync::OnceLock::new();
                CLIENT
                    .get_or_init(|| {
                        Client::builder()
                            .no_proxy()
                            .timeout(std::time::Duration::from_secs(30))
                            .redirect(reqwest::redirect::Policy::none())
                            .build()
                            .map_err(|_| "catalog HTTP client setup failed".into())
                    })
                    .clone()?
            },
            endpoint,
            auth,
            scope,
            graph,
            revision: None,
            bindings: serde_json::json!({}),
            refresh_interval: std::time::Duration::from_millis(super::default_refresh_interval_ms()),
        })
    }
    pub fn from_env(endpoint: &str, scope: &str, graph: &str) -> Result<Self, String> {
        super::CatalogReference {
            endpoint: endpoint.into(),
            scope: scope.into(),
            graph: graph.into(),
            token_env: "ORCHID_CATALOG_TOKEN".into(),
            revision: None,
            auth: None,
            refresh_interval_ms: super::default_refresh_interval_ms(),
        }
        .client()
    }
    pub async fn edges(&self, search: Option<&str>) -> Result<Value, String> {
        let mut result = self.discover(search).await?;
        let objects = result["objects"]
            .as_array_mut()
            .ok_or("invalid catalog discovery response")?;
        objects.retain(|object| {
            matches!(
                object["definition"]["kind"].as_str(),
                Some("relationship" | "cypher_relationship")
            )
        });
        Ok(result)
    }
    pub async fn register_edge(
        &self,
        id: &str,
        expected_version: i64,
        edge: CypherRelationship,
    ) -> Result<Value, String> {
        edge.validate()?;
        let mut definition = serde_json::to_value(edge).map_err(|e| e.to_string())?;
        definition["kind"] = "cypher_relationship".into();
        self.register_object(id, expected_version, definition).await
    }
    pub fn with_bindings(mut self, bindings: Value) -> Result<Self, String> {
        Schema::from_value(serde_json::json!({"tables":[]}))?.with_bindings(&bindings)?;
        self.bindings = bindings;
        Ok(self)
    }
    pub fn at_revision(mut self, revision: i64) -> Result<Self, String> {
        if revision < 1 {
            return Err("catalog revision must be positive".into());
        }
        self.revision = Some(revision);
        Ok(self)
    }
    async fn request(
        &self,
        method: Method,
        path: &[&str],
        body: Option<Value>,
        query: &[(&str, String)],
    ) -> Result<Value, String> {
        if self.revision.is_some() && method != Method::GET {
            return Err("a catalog pinned to a publication is read-only".into());
        }
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .map_err(|_| "invalid catalog endpoint")?
            .pop_if_empty()
            .extend(["v1", &self.scope])
            .extend(path);
        url.query_pairs_mut()
            .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        let token = self
            .auth
            .access_token(&self.client, &self.endpoint, &self.scope)
            .await?;
        let mut request = self.client.request(method, url).bearer_auth(token);
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "catalog transport failed".to_string())?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|_| "catalog response could not be read".to_string())?;
        if !status.is_success() {
            return Err(format!("catalog HTTP {}", status.as_u16()));
        }
        if bytes.is_empty() {
            Ok(Value::Null)
        } else {
            serde_json::from_slice(&bytes).map_err(|_| "invalid catalog response".to_string())
        }
    }
    pub fn with_refresh_interval(mut self, interval: std::time::Duration) -> Self {
        self.refresh_interval = interval;
        self
    }
    pub async fn refresh(&self) -> Result<i64, String> {
        Ok(self.cached_snapshot(true).await?.revision)
    }
    async fn cached_snapshot(&self, force: bool) -> Result<CatalogSnapshot, String> {
        let identity = super::cache::Identity {
            endpoint: self.endpoint.as_str(),
            scope: &self.scope,
            graph: &self.graph,
            revision: self.revision,
            interval: self.refresh_interval,
        };
        let entry = super::cache::entry(&identity, &self.auth.cache_identity()?)?;
        super::cache::resolve(
            &entry,
            &identity,
            force,
            std::time::Instant::now(),
            |known| async move {
                let mut query = self
                    .revision
                    .map(|r| vec![("revision", r.to_string())])
                    .unwrap_or_default();
                if let Some(revision) = known {
                    query.push(("known_revision", revision.to_string()));
                }
                self.request(
                    Method::GET,
                    &["graphs", &self.graph, "resolve"],
                    None,
                    &query,
                )
                .await
            },
        )
        .await
    }
    pub async fn resolve(&self) -> Result<ResolvedCatalog, String> {
        let snapshot = self.cached_snapshot(false).await?;
        Ok(ResolvedCatalog {
            manifest: snapshot
                .manifest
                .as_ref()
                .ok_or("missing catalog manifest")?
                .as_ref()
                .clone(),
            principal: snapshot.principal.ok_or("missing catalog principal")?,
        })
    }
    pub async fn discover(&self, search: Option<&str>) -> Result<Value, String> {
        let mut query = self
            .revision
            .map(|r| vec![("revision", r.to_string())])
            .unwrap_or_default();
        if let Some(search) = search {
            query.push(("q", search.into()));
        }
        self.request(
            Method::GET,
            &["graphs", &self.graph, "discover"],
            None,
            &query,
        )
        .await
    }
    pub async fn objects(&self) -> Result<Value, String> {
        self.request(Method::GET, &["objects"], None, &[]).await
    }
    pub async fn object(&self, id: &str) -> Result<CatalogRecord, String> {
        identifier(id)?;
        serde_json::from_value(
            self.request(Method::GET, &["objects", id], None, &[])
                .await?,
        )
        .map_err(|e| e.to_string())
    }
    pub async fn register_object(
        &self,
        id: &str,
        expected_version: i64,
        definition: Value,
    ) -> Result<Value, String> {
        identifier(id)?;
        self.request(
            Method::PUT,
            &["objects", id],
            Some(serde_json::json!({"expected_version":expected_version,"definition":definition})),
            &[],
        )
        .await
    }
    pub async fn register_graph(
        &self,
        expected_version: i64,
        graph: CatalogGraph,
    ) -> Result<Value, String> {
        self.request(
            Method::PUT,
            &["graphs", &self.graph],
            Some(serde_json::json!({"expected_version":expected_version,"definition":graph})),
            &[],
        )
        .await
    }
    pub async fn draft(&self) -> Result<Value, String> {
        self.request(Method::GET, &["graphs", &self.graph, "draft"], None, &[])
            .await
    }
    pub async fn graphs(&self) -> Result<Value, String> {
        self.request(Method::GET, &["graphs"], None, &[]).await
    }
    pub async fn grants(&self) -> Result<Value, String> {
        self.request(Method::GET, &["graphs", &self.graph, "grants"], None, &[])
            .await
    }
    pub async fn set_grants(
        &self,
        expected_version: i64,
        grants: CatalogGrants,
    ) -> Result<Value, String> {
        self.request(
            Method::PUT,
            &["graphs", &self.graph, "grants"],
            Some(serde_json::json!({"expected_version":expected_version,"definition":grants})),
            &[],
        )
        .await
    }
    pub async fn publish(&self, publication: CatalogPublish) -> Result<Value, String> {
        self.request(
            Method::POST,
            &["graphs", &self.graph, "publish"],
            Some(serde_json::to_value(publication).map_err(|e| e.to_string())?),
            &[],
        )
        .await
    }
    pub async fn principals(&self) -> Result<Value, String> {
        self.request(Method::GET, &["principals"], None, &[]).await
    }
    pub async fn principal(&self, id: &str) -> Result<Value, String> {
        principal_identifier(id)?;
        self.request(Method::GET, &["principals", id], None, &[])
            .await
    }
    pub async fn register_principal(
        &self,
        id: &str,
        expected_version: i64,
        principal: CatalogPrincipal,
        enabled: bool,
    ) -> Result<Value, String> {
        principal_identifier(id)?;
        self.request(
            Method::PUT,
            &["principals", id],
            Some(serde_json::json!({"expected_version":expected_version,
            "definition":{"principal":principal,"enabled":enabled}})),
            &[],
        )
        .await
    }
    pub async fn audit(&self) -> Result<Value, String> {
        self.request(Method::GET, &["audit"], None, &[]).await
    }
}
#[async_trait]
impl Catalog for OrchidCatalog {
    async fn snapshot(&self) -> Result<CatalogSnapshot, String> {
        let mut snapshot = self.cached_snapshot(false).await?;
        if self.bindings.as_object().is_some_and(|bindings| !bindings.is_empty()) {
            snapshot.schema = snapshot.schema.with_bindings(&self.bindings)?;
        }
        Ok(snapshot)
    }
}
fn identifier(value: &str) -> Result<(), String> {
    let mut bytes = value.bytes();
    if value.len() > 128
        || !bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err("invalid catalog identifier".into());
    }
    Ok(())
}

fn principal_identifier(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 256 || value.chars().any(|c| c.is_control() || c == '/') {
        return Err("invalid catalog principal identifier".into());
    }
    Ok(())
}
