//! The Git host the platform creates its application on.

/// Where the Git host's API and website are.
///
/// Two URLs rather than one because they are genuinely different services on
/// GitHub — `api.github.com` and `github.com` — and on an enterprise host they
/// are two paths on one origin. A deployment that has to state one usually has
/// to state both.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GitHostConfig {
    /// Where the API lives.
    #[serde(default = "default_api_base_url")]
    pub api_base_url: String,

    /// Where the website lives, which is where an operator's browser is sent.
    #[serde(default = "default_web_base_url")]
    pub web_base_url: String,

    /// The name commits are attributed to.
    #[serde(default = "default_committer_name")]
    pub committer_name: String,

    /// The address commits are attributed to.
    ///
    /// A `noreply` address by default: commits are made by the platform, and
    /// attributing them to a person who did not make them is worse than
    /// attributing them to nobody. Who *asked* for a change is in the commit
    /// body and in the audit record.
    #[serde(default = "default_committer_email")]
    pub committer_email: String,

    /// How long a call to the host may take.
    ///
    /// It is also half of a rule startup enforces: this plus
    /// `platform_management.operation_timeout_seconds` must be strictly less
    /// than `request_timeout_seconds`. A platform operation runs for its budget
    /// plus the one call that budget cannot cut short, and that sum is the
    /// longest an operator's disconnect can wait for the binding to drain —
    /// bounded below one request, with headroom for the rest of the operation.
    #[serde(default = "default_timeout")]
    pub http_timeout_seconds: u64,
}

impl Default for GitHostConfig {
    fn default() -> Self {
        Self {
            api_base_url: default_api_base_url(),
            web_base_url: default_web_base_url(),
            committer_name: default_committer_name(),
            committer_email: default_committer_email(),
            http_timeout_seconds: default_timeout(),
        }
    }
}

/// GitHub's API.
fn default_api_base_url() -> String {
    "https://api.github.com".to_owned()
}

/// GitHub's website.
fn default_web_base_url() -> String {
    "https://github.com".to_owned()
}

/// How the platform names itself in a commit.
fn default_committer_name() -> String {
    "SaaS Fabric".to_owned()
}

/// The address commits are attributed to.
fn default_committer_email() -> String {
    "saas-fabric@users.noreply.github.com".to_owned()
}

/// Ten seconds, matching the other platform clients.
fn default_timeout() -> u64 {
    10
}
