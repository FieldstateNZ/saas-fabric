//! The hosting provider's own representations.
//!
//! Nothing here is public. These types mirror the contents API's JSON, and
//! they exist so that nothing above this crate has to know a blob hash, a
//! commit, or a base64 payload is involved —
//! `scripts/check_architecture.py` fails the build if the names appear
//! elsewhere.

/// One entry in a directory listing, or the metadata of one file.
#[derive(Debug, serde::Deserialize)]
pub(super) struct ContentsEntry {
    /// `file`, `dir`, or something this crate ignores.
    #[serde(rename = "type")]
    pub(super) kind: String,

    /// The entry's own name within its directory.
    pub(super) name: String,

    /// The blob hash, which is what a revision is.
    pub(super) sha: String,

    /// The file's content, base64-encoded. Absent for a directory entry.
    #[serde(default)]
    pub(super) content: Option<String>,
}

/// A write to one file.
///
/// `sha` is what makes the write conditional: the host applies it only if that
/// is still the file's current blob hash, and refuses otherwise. Omitting it
/// would turn every write into an unconditional overwrite, which is precisely
/// the behaviour ADR 0008 rules out.
#[derive(Debug, serde::Serialize)]
pub(super) struct PutContents<'a> {
    /// The commit message.
    pub(super) message: &'a str,

    /// The new content, base64-encoded.
    pub(super) content: String,

    /// The blob hash the writer believes it is replacing.
    pub(super) sha: &'a str,

    /// The branch to commit on.
    pub(super) branch: &'a str,

    /// Who the commit is attributed to.
    pub(super) committer: Committer<'a>,
}

/// The identity a commit is attributed to.
///
/// The platform's own, always. The operator who asked is recorded in the
/// commit *message* and in the control plane's audit event, because a machine
/// identity cannot honestly claim to be a person.
#[derive(Debug, serde::Serialize)]
pub(super) struct Committer<'a> {
    /// The display name.
    pub(super) name: &'a str,

    /// The email address.
    pub(super) email: &'a str,
}

/// What a successful write returns.
#[derive(Debug, serde::Deserialize)]
pub(super) struct PutContentsResponse {
    /// The file as it now stands, including its new blob hash.
    pub(super) content: WrittenContent,
}

/// The file metadata inside a write response.
#[derive(Debug, serde::Deserialize)]
pub(super) struct WrittenContent {
    /// The blob hash the write produced, which becomes the new revision.
    pub(super) sha: String,
}

/// What the installation-token endpoint returns.
///
/// The one wire type here carrying a secret. It is never logged and never
/// returned upward — the port this crate implements has no operation that
/// could hand a token to a caller.
#[derive(serde::Deserialize)]
pub(super) struct InstallationToken {
    /// The bearer to present to the contents API.
    pub(super) token: String,

    /// When the host says it stops working, as an RFC 3339 timestamp.
    ///
    /// Read rather than assumed. An earlier version cached every token for a
    /// fixed fifty minutes on the grounds that GitHub issues them for an hour
    /// — which is true today, is not a promise, and left the platform holding
    /// a dead token for the remainder of the window if it ever stopped being
    /// true.
    pub(super) expires_at: String,
}

// No `Debug` on the type above: deriving one would put an installation token
// into any error or log line that formatted a value containing it.
