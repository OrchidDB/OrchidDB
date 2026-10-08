use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(
    tag = "source",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Credential {
    Value(String),
    Env(String),
    File(String),
}
impl std::fmt::Debug for Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Credential([redacted])")
    }
}
impl Credential {
    pub fn value(value: impl Into<String>) -> Self {
        Self::Value(value.into())
    }
    pub fn env(name: impl Into<String>) -> Self {
        Self::Env(name.into())
    }
    pub fn file(path: impl Into<String>) -> Self {
        Self::File(path.into())
    }
    #[cfg(feature = "orchid-catalog")]
    fn read(&self) -> Result<String, String> {
        let value = match self {
            Self::Value(value) => value.clone(),
            Self::Env(name) => std::env::var(name)
                .map_err(|_| "catalog credential environment variable is unavailable")?,
            Self::File(path) => std::fs::read_to_string(path)
                .map_err(|_| "catalog credential file is unavailable")?
                .trim_end_matches(['\n', '\r'])
                .into(),
        };
        if value.is_empty() {
            return Err("catalog credential is empty".into());
        }
        Ok(value)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CatalogAuth {
    Bearer {
        token: Credential,
    },
    ClientCredentials {
        client_id: String,
        client_secret: Credential,
        #[serde(default)]
        token_endpoint: Option<String>,
        #[serde(default)]
        issuer: Option<String>,
        #[serde(default = "default_scope")]
        scope: String,
    },
    TokenExchange {
        subject_token: Credential,
        #[serde(default)]
        token_endpoint: Option<String>,
        #[serde(default = "default_scope")]
        scope: String,
    },
}
fn default_scope() -> String {
    "PRINCIPAL_ROLE:ALL".into()
}
impl CatalogAuth {
    pub fn bearer(token: Credential) -> Self {
        Self::Bearer { token }
    }
    pub fn client_credentials(client_id: impl Into<String>, client_secret: Credential) -> Self {
        Self::ClientCredentials {
            client_id: client_id.into(),
            client_secret,
            token_endpoint: None,
            issuer: None,
            scope: default_scope(),
        }
    }
    pub fn token_exchange(subject_token: Credential) -> Self {
        Self::TokenExchange {
            subject_token,
            token_endpoint: None,
            scope: default_scope(),
        }
    }
    pub fn with_token_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        match &mut self {
            Self::ClientCredentials { token_endpoint, .. }
            | Self::TokenExchange { token_endpoint, .. } => *token_endpoint = Some(endpoint.into()),
            _ => (),
        }
        self
    }
    pub fn with_issuer(mut self, value: impl Into<String>) -> Self {
        if let Self::ClientCredentials { issuer, .. } = &mut self {
            *issuer = Some(value.into());
        }
        self
    }
    pub fn with_scope(mut self, value: impl Into<String>) -> Self {
        match &mut self {
            Self::ClientCredentials { scope, .. } | Self::TokenExchange { scope, .. } => {
                *scope = value.into()
            }
            _ => (),
        }
        self
    }
}

#[cfg(feature = "orchid-catalog")]
mod transport {
    use super::*;
    use reqwest::{Client, Url};
    use serde_json::Value;
    use sha2::{Digest, Sha256};
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex, OnceLock},
        time::{Duration, Instant},
    };
    use tokio::sync::Mutex as AsyncMutex;
    struct Token {
        value: String,
        expires: Instant,
        renew: Instant,
    }
    type Cache = HashMap<[u8; 32], Arc<AsyncMutex<Option<Token>>>>;
    static TOKENS: OnceLock<Mutex<Cache>> = OnceLock::new();
    pub(super) async fn token(
        auth: &CatalogAuth,
        client: &Client,
        endpoint: &Url,
        catalog_scope: &str,
    ) -> Result<String, String> {
        if let CatalogAuth::Bearer { token } = auth {
            return token.read();
        }
        let (secret, configured_endpoint, issuer, role_scope) = match auth {
            CatalogAuth::ClientCredentials {
                client_secret,
                token_endpoint,
                issuer,
                scope,
                ..
            } => (
                client_secret.read()?,
                token_endpoint,
                issuer.as_deref(),
                scope,
            ),
            CatalogAuth::TokenExchange {
                subject_token,
                token_endpoint,
                scope,
            } => (subject_token.read()?, token_endpoint, None, scope),
            _ => unreachable!(),
        };
        if configured_endpoint.is_some() && issuer.is_some() {
            return Err("choose either an OAuth token endpoint or an OIDC issuer".into());
        }
        let key: [u8; 32] = Sha256::digest(
            serde_json::to_vec(&(auth, &secret, endpoint.as_str(), catalog_scope))
                .map_err(|_| "invalid catalog authentication")?,
        )
        .into();
        let entry = {
            let mut cache = TOKENS
                .get_or_init(Default::default)
                .lock()
                .map_err(|_| "catalog credential cache unavailable")?;
            cache.retain(|_, value| match value.try_lock() {
                Ok(token) => {
                    token.as_ref().is_some_and(|t| t.expires > Instant::now())
                        || Arc::strong_count(value) > 1
                }
                Err(_) => true,
            });
            cache.entry(key).or_default().clone()
        };
        let mut cached = entry.lock().await;
        if let Some(token) = cached.as_ref().filter(|t| t.renew > Instant::now()) {
            return Ok(token.value.clone());
        }
        let url = if let Some(value) = configured_endpoint {
            secure_url(value)?
        } else if let Some(issuer) = issuer {
            let mut discovery = secure_url(issuer)?;
            discovery.set_path(&format!(
                "{}/.well-known/openid-configuration",
                discovery.path().trim_end_matches('/')
            ));
            let metadata: Value = client
                .get(discovery)
                .send()
                .await
                .map_err(|_| "OIDC discovery failed")?
                .error_for_status()
                .map_err(|_| "OIDC discovery failed")?
                .json()
                .await
                .map_err(|_| "invalid OIDC discovery response")?;
            if metadata["issuer"].as_str() != Some(issuer) {
                return Err("OIDC issuer mismatch".into());
            }
            secure_url(
                metadata["token_endpoint"]
                    .as_str()
                    .ok_or("OIDC token endpoint missing")?,
            )?
        } else {
            let mut url = endpoint.clone();
            url.path_segments_mut()
                .map_err(|_| "invalid catalog endpoint")?
                .pop_if_empty()
                .extend(["v1", catalog_scope, "oauth", "tokens"]);
            url
        };
        let mut form = vec![("scope", role_scope.clone())];
        match auth {
            CatalogAuth::ClientCredentials { client_id, .. } => {
                form.extend([
                    ("grant_type", "client_credentials".into()),
                    ("client_id", client_id.clone()),
                    ("client_secret", secret),
                ]);
            }
            CatalogAuth::TokenExchange { .. } => {
                let subject = cached
                    .as_ref()
                    .filter(|t| t.expires > Instant::now())
                    .map(|t| t.value.clone())
                    .unwrap_or(secret);
                form.extend([
                    (
                        "grant_type",
                        "urn:ietf:params:oauth:grant-type:token-exchange".into(),
                    ),
                    ("subject_token", subject),
                    (
                        "subject_token_type",
                        "urn:ietf:params:oauth:token-type:access_token".into(),
                    ),
                ]);
            }
            _ => unreachable!(),
        }
        let started = Instant::now();
        let response = client
            .post(url)
            .form(&form)
            .send()
            .await
            .map_err(|_| "catalog token request failed")?;
        if !response.status().is_success() {
            *cached = None;
            return Err(format!(
                "catalog authentication HTTP {}",
                response.status().as_u16()
            ));
        }
        let response: Value = response
            .json()
            .await
            .map_err(|_| "invalid catalog token response")?;
        if !response["token_type"]
            .as_str()
            .is_some_and(|t| t.eq_ignore_ascii_case("bearer"))
        {
            return Err("unsupported catalog token type".into());
        }
        let value = response["access_token"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or("catalog access token missing")?
            .to_owned();
        if let Some(seconds) = response["expires_in"].as_u64().filter(|v| *v > 0) {
            let expires = started
                .checked_add(Duration::from_secs(seconds))
                .ok_or("invalid token lifetime")?;
            let renew = started
                .checked_add(Duration::from_secs_f64(seconds as f64 * 0.5))
                .ok_or("invalid token lifetime")?;
            *cached = Some(Token {
                value: value.clone(),
                expires,
                renew,
            });
        } else {
            *cached = None;
        }
        Ok(value)
    }
    pub(crate) fn secure_url(value: &str) -> Result<Url, String> {
        let url = Url::parse(value).map_err(|_| "invalid catalog authentication URL")?;
        let loopback = url.host_str().is_some_and(|h| {
            h.eq_ignore_ascii_case("localhost")
                || h.trim_matches(['[', ']'])
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|ip| ip.is_loopback())
        });
        if (url.scheme() != "https" && !(url.scheme() == "http" && loopback))
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("catalog authentication requires HTTPS except on loopback, without URL credentials, query or fragment".into());
        }
        Ok(url)
    }
}
#[cfg(feature = "orchid-catalog")]
impl CatalogAuth {
    pub(crate) async fn access_token(
        &self,
        client: &reqwest::Client,
        endpoint: &reqwest::Url,
        scope: &str,
    ) -> Result<String, String> {
        transport::token(self, client, endpoint, scope).await
    }
}
