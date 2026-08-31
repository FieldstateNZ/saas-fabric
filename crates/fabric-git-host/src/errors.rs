//! What can go wrong obtaining a bearer.

/// A failure minting or presenting the host credential.
///
/// # Three variants, because they lead three different places
///
/// Each adapter maps these into its own vocabulary, and the mapping is only
/// possible if the distinctions survive. [`NotPermitted`](Self::NotPermitted)
/// means a human has to look at the App — the key, the installation, the
/// permissions. [`Unavailable`](Self::Unavailable) means wait and retry.
/// [`Rejected`](Self::Rejected) means the request was understood and refused,
/// which no retry fixes.
///
/// Collapsing them would put a misconfigured App into a retry loop, or send an
/// operator looking for a broken secret during a rate limit.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    /// The App's key, installation or permissions were refused.
    #[error("the Git host refused the platform's credential")]
    NotPermitted,

    /// The token endpoint could not be reached, or failed internally.
    #[error("the Git host is unavailable: {detail}")]
    Unavailable {
        /// What was observed. Never an upstream body, and never a credential.
        detail: String,
    },

    /// The host understood the request and refused it.
    #[error("the Git host refused the request: {detail}")]
    Rejected {
        /// What was observed. Never an upstream body, and never a credential.
        detail: String,
    },
}

/// The header the host reports remaining quota in.
const RATE_LIMIT_REMAINING: &str = "x-ratelimit-remaining";

/// Classifies a failure from `.send()`.
///
/// The message is this crate's own classification, never `reqwest::Error`'s
/// `Display`, which can carry the full URL.
pub(crate) fn transport_failure(error: &reqwest::Error) -> TokenError {
    let kind = if error.is_connect() {
        "could not connect"
    } else if error.is_timeout() {
        "timed out"
    } else if error.is_decode() {
        "returned a response that could not be read"
    } else {
        "failed"
    };

    TokenError::Unavailable {
        detail: format!("minting an installation token {kind}"),
    }
}

/// Classifies a status the token endpoint returned.
///
/// A `403` with no quota left is a rate limit, which is transient. Reporting
/// it as a refused credential would send an operator looking for a secret that
/// is perfectly fine, so the header is what distinguishes them.
pub(crate) fn status_failure(
    status: reqwest::StatusCode,
    headers: &reqwest::header::HeaderMap,
) -> TokenError {
    if status == reqwest::StatusCode::FORBIDDEN && quota_exhausted(headers) {
        return TokenError::Unavailable {
            detail: "minting an installation token was rate limited".to_owned(),
        };
    }

    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return TokenError::NotPermitted;
    }

    if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return TokenError::Unavailable {
            detail: format!("minting an installation token returned {}", status.as_u16()),
        };
    }

    TokenError::Rejected {
        detail: format!(
            "minting an installation token was refused with {}",
            status.as_u16()
        ),
    }
}

/// Whether the host reported no remaining quota.
fn quota_exhausted(headers: &reqwest::header::HeaderMap) -> bool {
    headers
        .get(RATE_LIMIT_REMAINING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.trim() == "0")
}
