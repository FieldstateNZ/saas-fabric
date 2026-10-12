//! Authenticated GETs only; service-account rotation is read for every request.
use crate::wire::{Deployment, List};
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) struct Client {
    pub http: reqwest::Client,
    pub base: String,
    pub token_file: PathBuf,
}
impl Client {
    /// The API server this pod runs in, authenticated with its own service
    /// account, verified against the cluster CA, never following a redirect.
    pub fn in_cluster() -> Result<Self, String> {
        let root = "/var/run/secrets/kubernetes.io/serviceaccount";
        let pem = std::fs::read(format!("{root}/ca.crt"))
            .map_err(|_| "observation requires the cluster CA certificate")?;
        let ca = reqwest::Certificate::from_pem(&pem)
            .map_err(|_| "observation cluster CA certificate is invalid")?;
        let http = reqwest::Client::builder()
            .add_root_certificate(ca)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| "could not build the deployment observation client")?;
        Ok(Self {
            http,
            base: "https://kubernetes.default.svc".into(),
            token_file: format!("{root}/token").into(),
        })
    }
    pub async fn get<T: DeserializeOwned>(&self, path: &str, selector: Option<&str>) -> Result<T, String> {
        let token = tokio::fs::read_to_string(&self.token_file)
            .await
            .map_err(|_| "Deployment observation credential is unavailable.")?;
        let mut request = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token.trim());
        if let Some(selector) = selector {
            request = request.query(&[("labelSelector", selector), ("limit", "500")]);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "Deployment observation could not reach the cluster.")?;
        if !response.status().is_success() {
            return Err(match response.status().as_u16() {
                401 | 403 => "Deployment observation is not permitted.",
                404 => "The configured deployment was not found.",
                _ => "The cluster could not supply deployment evidence.",
            }
            .into());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Deployment evidence could not be read.")?
        {
            if body.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("Deployment evidence exceeded its size limit.".into());
            }
            body.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&body)
            .map_err(|_| "The cluster returned incomplete deployment evidence.".into())
    }
    pub async fn list<T: DeserializeOwned>(&self, path: &str, selector: &str) -> Result<Vec<T>, String> {
        let list: List<T> = self.get(path, Some(selector)).await?;
        if !list.metadata.continuation.is_empty() {
            return Err("Deployment evidence requires more than one page.".into());
        }
        Ok(list.items)
    }
}

/// The equality selector a Deployment's pods and replica sets carry.
pub(crate) fn selector_of(deployment: &Deployment) -> Result<String, String> {
    let selector = &deployment.spec.selector;
    if selector.match_labels.is_empty() || !selector.match_expressions.is_empty() {
        return Err("Deployment observation requires a nonempty equality label selector.".into());
    }
    Ok(selector
        .match_labels
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(","))
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
