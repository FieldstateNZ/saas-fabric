//! A request before anything is attached to it, and what came back.

use reqwest::{Method, Response};

/// Which of the two clients a request goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Via {
    /// Same-origin redirects only.
    Api,

    /// No redirects: a blob's are followed by hand, for its CDN.
    Blob,
}

/// One request, before anything is attached to it.
pub(super) struct Call<'a> {
    /// `GET` or `HEAD`.
    pub(super) method: Method,
    /// Which client.
    pub(super) via: Via,
    /// What is being done, for messages.
    pub(super) operation: &'a str,
    /// Where, on the registry's own origin.
    pub(super) url: &'a str,
    /// What is accepted.
    pub(super) accept: &'a str,
}

/// A response, and the realm origin a challenge named on the way to it.
pub(super) struct Exchange {
    /// The last response.
    pub(super) response: Response,
    /// The realm's origin, when a `Bearer` challenge was answered.
    pub(super) realm: Option<String>,
}
