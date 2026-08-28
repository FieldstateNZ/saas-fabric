//! What the Git-backed repository needs to be told.

mod auth;

pub use auth::GitAuthConfig;

/// Where desired state lives, and how to reach it.
///
/// Every field is non-secret and belongs in a `ConfigMap`. The token itself is
/// named, never carried — see [`Self::auth`].
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct GitRepositoryConfig {
    /// The hosting API's base URL.
    ///
    /// Configurable rather than hard-coded so the same build serves
    /// `api.github.com` and an enterprise instance — and so tests can point it
    /// at a socket that answers like one.
    pub api_base_url: String,

    /// The account or organisation owning the repository.
    pub owner: String,

    /// The repository holding client desired state.
    pub repository: String,

    /// The branch reconciliation reads and operators write.
    pub branch: String,

    /// The directory clients live under.
    ///
    /// A client's document is `{path_prefix}/{client id}/{document_file}`. The
    /// layout is configuration rather than a constant because the
    /// desired-state repository is a separate repository with its own
    /// conventions, and a platform that hard-coded them could not follow a
    /// change to them without a release.
    pub path_prefix: String,

    /// The file within a client's directory that carries its desired state.
    pub document_file: String,

    /// How SaaS Fabric authenticates to the host.
    ///
    /// Names the secret it needs; never carries one. This application says
    /// which value it requires, and `saas-fabric-platform` decides how it
    /// arrives (§20, §21).
    pub auth: GitAuthConfig,

    /// The name commits are attributed to.
    ///
    /// The platform's own identity, not the operator's — every commit has the
    /// same author, which is exactly why the *audit record* has to carry who
    /// asked. See `ChangeContext` in `fabric-control-plane`.
    pub committer_name: String,

    /// The email address commits are attributed to.
    pub committer_email: String,

    /// How long a single API call may take, in seconds.
    pub http_timeout_seconds: u64,
}

impl Default for GitRepositoryConfig {
    fn default() -> Self {
        Self {
            api_base_url: "https://api.github.com".to_owned(),
            owner: "FieldstateNZ".to_owned(),
            repository: "saas-fabric-clients".to_owned(),
            branch: "main".to_owned(),
            path_prefix: "clients".to_owned(),
            document_file: "client.yaml".to_owned(),
            auth: GitAuthConfig::default(),
            committer_name: "SaaS Fabric".to_owned(),
            committer_email: "saas-fabric@users.noreply.github.com".to_owned(),
            http_timeout_seconds: 15,
        }
    }
}

impl GitRepositoryConfig {
    /// Checks the settings that would otherwise fail on the first request.
    ///
    /// # Errors
    ///
    /// Returns a message if the base URL is not absolute, or if any required
    /// name is empty. A control plane whose desired state is unreachable
    /// should refuse to start rather than serve an empty client list that
    /// looks like a platform with no clients.
    pub fn validate(&self) -> Result<(), String> {
        if !(self.api_base_url.starts_with("http://") || self.api_base_url.starts_with("https://")) {
            return Err(format!(
                "clients repository: api_base_url must be an absolute http(s) URL, got {}",
                self.api_base_url
            ));
        }

        for (name, value) in [
            ("owner", &self.owner),
            ("repository", &self.repository),
            ("branch", &self.branch),
            ("path_prefix", &self.path_prefix),
            ("document_file", &self.document_file),
        ] {
            if value.trim().is_empty() {
                return Err(format!("clients repository: {name} must not be empty"));
            }
        }

        self.auth.validate()
    }
}
