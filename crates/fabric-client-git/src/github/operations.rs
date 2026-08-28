//! Reading and writing through the contents API.
//!
//! Split from the client's own file because these are two concerns that share a
//! struct: `http` owns how a request is made — the client, the credential, the
//! headers, the URLs — and this owns what the four operations are. The house
//! convention for that split is an impl block in its own module.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use fabric_client_model::{ClientId, ClientRevision};
use fabric_control_plane::RepositoryError;

use crate::github::contents::{DirectoryEntry, StoredFile};
use crate::github::decoding::decode;
use crate::github::errors::{status_failure, transport_failure};
use crate::github::http::GitHost;
use crate::github::wire::{Committer, ContentsEntry, PutContents, PutContentsResponse};

impl GitHost {
    /// Lists the entries directly under the clients directory.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError`] if the directory could not be read. A
    /// missing directory is reported as unavailable rather than as an empty
    /// platform: a repository whose layout has moved must not look like a
    /// repository with no clients.
    pub(crate) async fn list_directory(&self) -> Result<Vec<DirectoryEntry>, RepositoryError> {
        let url = self.contents_url(&self.config.path_prefix);
        let entries: Vec<ContentsEntry> = self.get("listing clients", url, None).await?;

        Ok(entries
            .into_iter()
            .map(|entry| DirectoryEntry {
                is_directory: entry.kind == "dir",
                name: entry.name,
            })
            .collect())
    }

    /// Reads one client's document.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::NotFound`] if there is no such file, or
    /// another variant if it could not be read or decoded.
    pub(crate) async fn read_document(&self, client: &ClientId) -> Result<StoredFile, RepositoryError> {
        let url = self.contents_url(&self.document_path(client));
        let entry: ContentsEntry = self.get("reading a client", url, Some(client)).await?;

        decode(entry)
    }

    /// Writes one client's document, but only if it is still at `expected`.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Conflict`] if the stored blob has moved on,
    /// or another variant if the write could not be made.
    pub(crate) async fn write_document(
        &self,
        client: &ClientId,
        text: &str,
        expected: &ClientRevision,
        message: &str,
    ) -> Result<ClientRevision, RepositoryError> {
        let body = PutContents {
            message,
            content: BASE64.encode(text),
            sha: expected.as_str(),
            branch: &self.config.branch,
            committer: Committer {
                name: &self.config.committer_name,
                email: &self.config.committer_email,
            },
        };

        let url = self.contents_url(&self.document_path(client));
        let response = self
            .send("writing a client", self.http.put(url).json(&body))
            .await?;

        if !response.status().is_success() {
            return Err(status_failure(
                "writing a client",
                response.status(),
                response.headers(),
                None,
            ));
        }

        let written: PutContentsResponse = response
            .json()
            .await
            .map_err(|error| transport_failure("writing a client", &error))?;

        ClientRevision::try_new(written.content.sha).map_err(|error| RepositoryError::Unavailable {
            detail: format!("the repository reported an unusable revision: {error}"),
        })
    }

    /// Issues a `GET` and decodes the response.
    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        operation: &str,
        url: String,
        client: Option<&ClientId>,
    ) -> Result<T, RepositoryError> {
        let response = self.send(operation, self.http.get(url)).await?;

        if !response.status().is_success() {
            return Err(status_failure(
                operation,
                response.status(),
                response.headers(),
                client,
            ));
        }

        response
            .json()
            .await
            .map_err(|error| transport_failure(operation, &error))
    }
}
