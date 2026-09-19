//! Authenticated calls to the API server; the token is read for every one.

mod in_cluster;

use std::path::PathBuf;

use fabric_runtime_publication::{DocumentKind, PublicationError};

use crate::errors::{unreadable, unwritable};
use crate::wire::ConfigMap;

/// Two megabytes: an object this adapter writes is capped at one, so a
/// response larger than this is not one of ours.
const RESPONSE_CAP: usize = 2 * 1024 * 1024;

/// The pod-identity transport. `base` and `token_file` are fields so a test
/// can point them at a local socket and a temporary file.
pub(crate) struct Client {
    pub(crate) http: reqwest::Client,
    pub(crate) base: String,
    pub(crate) token_file: PathBuf,
}

/// What a read found.
pub(crate) enum Found {
    /// The object, and the version it was read at.
    Object(ConfigMap),
    /// No such object.
    Absent,
}

impl Client {
    async fn token(&self, document: DocumentKind) -> Result<String, PublicationError> {
        tokio::fs::read_to_string(&self.token_file)
            .await
            .map(|token| token.trim().to_owned())
            .map_err(|_| unreadable(document, "the publication credential is unavailable"))
    }

    /// `GET` one object.
    pub(crate) async fn get(&self, path: &str, document: DocumentKind) -> Result<Found, PublicationError> {
        let token = self.token(document).await?;
        let response = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| unreadable(document, "the cluster could not be reached"))?;
        match response.status().as_u16() {
            404 => Ok(Found::Absent),
            200 => Ok(Found::Object(read_body(response, document).await?)),
            401 | 403 => Err(unreadable(
                document,
                "reading the published document is not permitted",
            )),
            _ => Err(unreadable(
                document,
                "the cluster could not supply the published document",
            )),
        }
    }

    /// `POST` (create) or `PUT` (replace) one object; a replace carries the
    /// version it was read at, so the API server refuses a write over a
    /// version this adapter did not see.
    pub(crate) async fn write(
        &self,
        path: &str,
        object: &ConfigMap,
        document: DocumentKind,
    ) -> Result<(), PublicationError> {
        let token = self
            .token(document)
            .await
            .map_err(|_| unwritable(document, "the publication credential is unavailable"))?;
        let url = format!("{}{path}", self.base);
        let request = if object.metadata.resource_version.is_some() {
            self.http.put(url)
        } else {
            self.http.post(url)
        };
        let response = request
            .bearer_auth(token)
            .json(object)
            .send()
            .await
            .map_err(|_| unwritable(document, "the cluster could not be reached"))?;
        match response.status().as_u16() {
            200 | 201 => Ok(()),
            409 => Err(unwritable(
                document,
                "another writer changed the published document; the next pass re-reads it",
            )),
            401 | 403 => Err(unwritable(
                document,
                "writing the published document is not permitted",
            )),
            422 => Err(unwritable(
                document,
                "the cluster refused the published document's shape",
            )),
            _ => Err(unwritable(
                document,
                "the cluster did not accept the published document",
            )),
        }
    }
}

async fn read_body(
    mut response: reqwest::Response,
    document: DocumentKind,
) -> Result<ConfigMap, PublicationError> {
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| unreadable(document, "the published document could not be read"))?
    {
        if body.len() + chunk.len() > RESPONSE_CAP {
            return Err(unreadable(
                document,
                "the published document exceeded its size limit",
            ));
        }
        body.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&body).map_err(|_| {
        unreadable(
            document,
            "the cluster returned an object this adapter cannot read",
        )
    })
}
