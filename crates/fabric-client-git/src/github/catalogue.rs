//! Conditional creation and catalogue storage through the existing repository credential.
use super::{
    contents::StoredFile,
    decoding::decode,
    errors::{status_failure, transport_failure},
    http::GitHost,
    wire::{ContentsEntry, PutContentsResponse},
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use fabric_client_model::{ClientId, ClientRevision};
use fabric_control_plane::RepositoryError;
impl GitHost {
    pub(crate) async fn read_catalogue(&self) -> Result<Option<StoredFile>, RepositoryError> {
        let response = self
            .send(
                "reading catalogue",
                self.http.get(self.contents_url("fabric-catalogue.yaml")),
            )
            .await?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            // Distinguish an absent optional file from a repository/branch no longer accessible.
            let root = self
                .send("checking repository", self.http.get(self.contents_url("")))
                .await?;
            if !root.status().is_success() {
                return Err(status_failure(
                    "checking repository",
                    root.status(),
                    root.headers(),
                    None,
                ));
            }
            return Ok(None);
        }
        if !response.status().is_success() {
            return Err(status_failure(
                "reading catalogue",
                response.status(),
                response.headers(),
                None,
            ));
        }
        let entry: ContentsEntry = response
            .json()
            .await
            .map_err(|error| transport_failure("reading catalogue", &error))?;
        decode(entry).map(Some)
    }
    pub(crate) async fn create_document(
        &self,
        client: &ClientId,
        text: &str,
        message: &str,
    ) -> Result<ClientRevision, RepositoryError> {
        self.write_file(&self.document_path(client), text, None, message)
            .await
    }
    pub(crate) async fn write_catalogue(
        &self,
        text: &str,
        expected: Option<&ClientRevision>,
        message: &str,
    ) -> Result<ClientRevision, RepositoryError> {
        self.write_file("fabric-catalogue.yaml", text, expected, message)
            .await
    }
    async fn write_file(
        &self,
        path: &str,
        text: &str,
        expected: Option<&ClientRevision>,
        message: &str,
    ) -> Result<ClientRevision, RepositoryError> {
        let mut body = serde_json::json!({ "message": message, "content": BASE64.encode(text), "branch": self.config.branch, "committer": { "name": self.config.committer_name, "email": self.config.committer_email } });
        if let (Some(revision), Some(map)) = (expected, body.as_object_mut()) {
            map.insert("sha".into(), serde_json::json!(revision.as_str()));
        }
        let response = self
            .send(
                "writing desired state",
                self.http.put(self.contents_url(path)).json(&body),
            )
            .await?;
        if !response.status().is_success() {
            return Err(status_failure(
                "writing desired state",
                response.status(),
                response.headers(),
                None,
            ));
        }
        let written: PutContentsResponse = response
            .json()
            .await
            .map_err(|error| transport_failure("writing desired state", &error))?;
        ClientRevision::try_new(written.content.sha).map_err(|_| RepositoryError::Unavailable {
            detail: "Invalid written revision".into(),
        })
    }
}
