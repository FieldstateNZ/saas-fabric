//! Who asked for a change, and what it was.

/// The attribution a repository records alongside a write.
///
/// # Why the repository is told rather than left to infer
///
/// A Git-backed repository commits as the platform's own machine identity, so
/// every commit has the same author. Without this, the audit trail in Git would
/// say *that* SaaS Fabric changed a client and never *who asked it to* — and
/// the control plane's own log would be the only record, which is exactly the
/// single point of failure §24 warns against.
///
/// # What must never be in here
///
/// A credential, a token, or anything the operator supplied that has not been
/// validated. This text is written into a commit message and a log line, both
/// of which are durable and widely readable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangeContext {
    /// The operator who requested the change, as the authenticator
    /// established them.
    pub requested_by: String,

    /// A one-line description of the domain operation, such as
    /// `update identity`.
    ///
    /// Domain vocabulary, not repository vocabulary: it says what changed
    /// about the client, not which file was rewritten.
    pub summary: String,
}
