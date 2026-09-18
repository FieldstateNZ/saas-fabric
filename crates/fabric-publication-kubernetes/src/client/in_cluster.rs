//! The transport a pod builds from its own mounted identity.

use std::time::Duration;

use super::Client;

impl Client {
    /// The in-cluster transport: cluster CA, projected token, no redirects,
    /// bounded time.
    ///
    /// # Errors
    ///
    /// A message when the trust root cannot be read; never a path in it.
    pub(crate) fn in_cluster() -> Result<Self, String> {
        let root = "/var/run/secrets/kubernetes.io/serviceaccount";
        let pem = std::fs::read(format!("{root}/ca.crt"))
            .map_err(|_| "publication requires the cluster CA certificate")?;
        let ca = reqwest::Certificate::from_pem(&pem)
            .map_err(|_| "publication cluster CA certificate is invalid")?;
        let http = reqwest::Client::builder()
            .add_root_certificate(ca)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(4))
            .build()
            .map_err(|_| "could not build the publication client")?;
        Ok(Self {
            http,
            base: "https://kubernetes.default.svc".into(),
            token_file: format!("{root}/token").into(),
        })
    }
}
