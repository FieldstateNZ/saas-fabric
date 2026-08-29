//! Reading and writing one entry in a version 2 key-value mount.

use crate::client::OpenBao;

/// What an entry read returned.
pub(crate) enum Read {
    /// The entry's fields.
    Found(serde_json::Map<String, serde_json::Value>),

    /// No such entry. **Not an error**: a platform that has never connected
    /// has never written one, and that is an ordinary state.
    Absent,
}

impl OpenBao {
    /// Reads one entry beneath this instance's partition.
    pub(crate) async fn read(&self, name: &str) -> Result<Read, String> {
        let response = self
            .send(reqwest::Method::GET, &self.data_url(name), None)
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(Read::Absent);
        }

        if !response.status().is_success() {
            return Err(format!("the secret store answered {}", response.status()));
        }

        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|_| "the secret store's answer could not be read".to_owned())?;

        // Version 2 nests the entry twice: the envelope's `data` holds the
        // version metadata, and *its* `data` holds what was written.
        body.get("data")
            .and_then(|envelope| envelope.get("data"))
            .and_then(serde_json::Value::as_object)
            .cloned()
            .map(Read::Found)
            .ok_or_else(|| "the secret store's answer had no entry in it".to_owned())
    }

    /// Writes one entry, replacing whatever was there.
    pub(crate) async fn write(&self, name: &str, fields: serde_json::Value) -> Result<(), String> {
        let body = serde_json::json!({ "data": fields });

        let response = self
            .send(reqwest::Method::POST, &self.data_url(name), Some(body))
            .await?;

        if !response.status().is_success() {
            return Err(format!(
                "the secret store refused the write ({})",
                response.status()
            ));
        }

        Ok(())
    }

    /// Removes an entry and every version of it.
    ///
    /// The metadata path rather than the data path: deleting through `data`
    /// marks the latest version deleted and leaves the previous ones readable,
    /// which for a credential is not deletion at all.
    pub(crate) async fn remove(&self, name: &str) -> Result<(), String> {
        let response = self
            .send(reqwest::Method::DELETE, &self.metadata_url(name), None)
            .await?;

        if response.status() == reqwest::StatusCode::NOT_FOUND || response.status().is_success() {
            return Ok(());
        }

        Err(format!(
            "the secret store refused the removal ({})",
            response.status()
        ))
    }
}
