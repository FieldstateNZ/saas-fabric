//! The answers that are not content: a tag page, a token, an error body.

/// What `GET /v2/<name>/tags/list` answers with.
#[derive(Debug, serde::Deserialize)]
pub(in crate::client) struct TagList {
    /// The tags on this page. Absent rather than empty on some registries.
    #[serde(default)]
    pub(in crate::client) tags: Option<Vec<String>>,
}

/// What the token endpoint answers with: `token`, `access_token`, or both,
/// depending on the registry.
#[derive(Debug, serde::Deserialize)]
pub(in crate::client) struct PullToken {
    /// The bearer, as the distribution specification names it.
    #[serde(default)]
    token: Option<String>,

    /// The same bearer under its OAuth 2 name, which Docker Hub also sends.
    #[serde(default)]
    access_token: Option<String>,
}

impl PullToken {
    /// The bearer to present: `token` when both are given, as the
    /// specification says they are then the same.
    pub(in crate::client) fn bearer(self) -> Option<String> {
        self.token.or(self.access_token).filter(|token| !token.is_empty())
    }
}

/// A registry's error body: `{"errors":[{"code":"NAME_UNKNOWN",…}]}`.
#[derive(Debug, Default, serde::Deserialize)]
pub(in crate::client) struct Errors {
    /// Each error it reported.
    #[serde(default)]
    pub(in crate::client) errors: Vec<ErrorCode>,
}

/// One error a registry reported.
#[derive(Debug, serde::Deserialize)]
pub(in crate::client) struct ErrorCode {
    /// Its code, such as `NAME_UNKNOWN`.
    #[serde(default)]
    pub(in crate::client) code: String,
}
