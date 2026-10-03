//! Where one of a component's images lives, written in full.
mod host;
mod path;
use crate::errors::{invalid, ContractError};

contract_newtype!(
    /// An image repository, fully qualified: `ghcr.io/fieldstatenz/saas-fabric`,
    /// `registry.example.com:5000/acme/reports`, `docker.io/library/nginx`.
    ///
    /// # Why nothing is implied
    ///
    /// A registry is named by its host (ADR 0026 section 5), and one image
    /// reference is compared with another as text. Docker's own shorthand --
    /// `nginx` meaning `docker.io/library/nginx`, `index.docker.io` meaning
    /// `docker.io`, `:443` meaning nothing -- would give one repository
    /// several spellings, so every one of them is refused rather than
    /// normalised: a repository has exactly one way to be written.
    ///
    /// The first path component is the registry host: it contains `.` or
    /// `:`, or is `localhost`; it is lower case and its labels are DNS
    /// labels; it is a name, not an IP address; and a port, if any, is not
    /// 443. The rest is the OCI distribution specification's path grammar.
    /// No scheme, tag or digest; at most 255 bytes.
    Repository,
    check
);

impl Repository {
    /// The registry host, with its port if it has one.
    #[must_use]
    pub fn host(&self) -> &str {
        self.0.split_once('/').map_or("", |(host, _)| host)
    }

    /// The repository's path on its registry.
    #[must_use]
    pub fn path(&self) -> &str {
        self.0.split_once('/').map_or("", |(_, path)| path)
    }
}

/// The longest repository, in bytes.
const MAX_LENGTH: usize = 255;

/// The rule.
fn check(value: &str) -> Result<(), ContractError> {
    if value.len() > MAX_LENGTH {
        return Err(invalid(format!(
            "A repository must be at most {MAX_LENGTH} bytes"
        )));
    }
    if value.contains("://") {
        return Err(invalid(format!(
            "A repository is written without a scheme: {value}"
        )));
    }
    if value.contains('@') {
        return Err(invalid(format!(
            "A repository is written without a digest: {value}"
        )));
    }
    let Some((host, path)) = value.split_once('/') else {
        return Err(invalid(format!(
            "A repository is written in full, starting with its registry host: {value}"
        )));
    };
    host::check(host, path)?;
    if path.contains(':') {
        return Err(invalid(format!("A repository is written without a tag: {value}")));
    }
    path::check(path)
}
