//! Who is signed in, in the shape every instance's shell reads (ADR 0024 §3).

/// The signed-in user, projected from the verified token.
///
/// Absent claims are `null`, never omitted: the shell reads one shape on
/// every instance, and a field that comes and goes is a field every reader
/// has to guess about.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CurrentUserResponse {
    /// The token's `sub`, exactly: opaque and stable.
    pub(crate) subject: String,

    /// The token's `preferred_username`.
    pub(crate) user_name: Option<String>,

    /// How to show this user to a human. Never empty.
    pub(crate) display_name: String,

    /// The token's `email`.
    pub(crate) email: Option<String>,

    /// The realm tier: on the control-plane instance, the configured operator
    /// role alone.
    pub(crate) roles: Vec<String>,
}
