//! Reading the document a realm publishes its signing keys in.
//!
//! Returned as it was served. Parsing it is a JOSE concern rather than a
//! Keycloak one and belongs to whoever verifies with the keys — which keeps
//! this crate's promise that no caller needs to know where a realm keeps
//! anything.

use crate::RealmSignIn;

impl RealmSignIn {
    /// Reads the document in which this realm publishes its signing keys.
    ///
    /// Returns the document as it was served. Parsing it is a JOSE concern
    /// rather than a Keycloak one, and belongs to whoever verifies with the
    /// keys — which keeps this crate's promise that no caller needs to know
    /// where a realm keeps anything.
    ///
    /// # Errors
    ///
    /// Returns a message if the endpoint could not be reached or did not
    /// answer successfully.
    pub async fn signing_keys(&self) -> Result<String, String> {
        let response = self
            .http
            .get(&self.jwks_endpoint)
            .send()
            .await
            .map_err(|error| format!("the signing keys could not be fetched: {error}"))?;

        if !response.status().is_success() {
            return Err(format!("the signing key request answered {}", response.status()));
        }

        response
            .text()
            .await
            .map_err(|error| format!("the signing keys could not be read: {error}"))
    }
}
