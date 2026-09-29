//! Which status, and which stable code, an image registry's failure carries
//! (ADR 0026 section 5).
//!
//! | Code | Status | When |
//! |---|---|---|
//! | `registry_not_found` | 404 | no registry for the host, or the repository is not registered under it |
//! | `registry_exists` | 409 | a registry is already registered for the host |
//! | `registry_invalid` | 400 | a request the rules refuse; the message names the rule |
//! | `registry_endpoint_differs` | 409 | the deployment's host, at another endpoint |
//! | `registry_credential_unreadable` | 409 | the recorded credential is not in the secret partition |
//! | `registry_not_proven` | 422 | the registry's `/v2/` endpoint did not prove |
//! | `repository_not_readable` | 422 | the repository's tag listing was refused |
//! | `registry_refused` | 502 | the registry's realm refused the credential |
//! | `registry_unavailable` | 503, `Retry-After` | the registry could not be asked |
//! | `registries_unavailable` | 503, `Retry-After` | the record set or a credential could not be read or written |
//! | `registries_invalid` | 500 | the stored record set does not parse |

use http::StatusCode;

use crate::RegistryFailure;

impl RegistryFailure {
    /// The status an operator's browser sees.
    #[allow(
        clippy::match_same_arms,
        reason = "arms are grouped by cause; the code keeps each apart"
    )]
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            // An absence, and no wait or grant fixes it.
            Self::NotFound { .. } | Self::RepositoryNotRegistered { .. } => StatusCode::NOT_FOUND,

            // The request is well-formed; what is already there is what
            // does not permit it. Remove the registry first, or register the
            // deployment's host at the deployment's endpoint.
            Self::Exists { .. } | Self::EndpointDiffers { .. } => StatusCode::CONFLICT,

            // A record naming a credential the secret partition no longer
            // holds: no retry brings it back, so not a 503. Setting the
            // credential again, or removing it, is the fix.
            Self::CredentialUnreadable { .. } => StatusCode::CONFLICT,

            // The operator's to correct, and the message says which rule.
            Self::Invalid(_) => StatusCode::BAD_REQUEST,

            // Understood, and what it names did not prove: the registry is
            // not one, or the repository is not readable through it. 422,
            // not 502: nothing upstream is misbehaving, and the fix is to
            // name something else or give a credential that reaches it.
            Self::NotProven(_) | Self::RepositoryNotReadable { .. } => StatusCode::UNPROCESSABLE_ENTITY,

            // An upstream that said no to what it was given. Not a 503: the
            // same credential is refused in five seconds, and advertising it
            // as transient invites the retry storm that locks an account.
            Self::Refused(_) => StatusCode::BAD_GATEWAY,

            // Transient: the registry, or the store, may answer shortly.
            Self::Unavailable(_) | Self::StoreUnavailable => StatusCode::SERVICE_UNAVAILABLE,

            // A record set somebody wrote over. The platform's problem, and
            // no retry fixes it — as a client document that will not parse.
            Self::StoreInvalid => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// A stable machine-readable code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            // One code for both absences: the message says which, and a
            // console does the same with either — look again.
            Self::NotFound { .. } | Self::RepositoryNotRegistered { .. } => "registry_not_found",
            Self::Exists { .. } => "registry_exists",
            Self::Invalid(_) => "registry_invalid",
            Self::EndpointDiffers { .. } => "registry_endpoint_differs",
            Self::CredentialUnreadable { .. } => "registry_credential_unreadable",
            Self::NotProven(_) => "registry_not_proven",
            Self::RepositoryNotReadable { .. } => "repository_not_readable",
            Self::Refused(_) => "registry_refused",
            Self::Unavailable(_) => "registry_unavailable",
            Self::StoreUnavailable => "registries_unavailable",
            Self::StoreInvalid => "registries_invalid",
        }
    }
}
