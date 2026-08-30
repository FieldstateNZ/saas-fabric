//! Every way a secret operation can fail.

/// A secret operation that could not be completed.
///
/// Messages are safe to return to an operator: they describe the client and
/// the path they named, and no address, credential or store vocabulary.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretsError {
    /// The client has no secret boundary declared.
    ///
    /// Refused rather than guessed. Deriving a boundary from the client id is
    /// right until the day it is not, and on that day it reads another
    /// client's secrets.
    #[error("this client has no secret boundary yet")]
    NoBoundary,

    /// No secret at that path.
    #[error("no secret at that path")]
    NotFound,

    /// Somebody wrote a newer version while this operator was looking at an
    /// older one.
    ///
    /// The reason versions are not flattened away: this is a refusal the
    /// operator can see and act on, where the alternative is a silent
    /// overwrite of somebody else's change.
    #[error("that secret has changed since it was read; reload and try again")]
    Conflict,

    /// The store refused the operation.
    ///
    /// A credential or policy problem, and therefore this platform's rather
    /// than the operator's.
    #[error("the secret store refused the operation")]
    Refused,

    /// The store could not be reached.
    #[error("the secret store is unavailable")]
    Unavailable,
}
