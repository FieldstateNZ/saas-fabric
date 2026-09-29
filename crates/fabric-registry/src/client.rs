//! The registry client.

mod answer;
mod attached;
mod blob;
mod bounds;
mod build;
mod by_tag;
mod call;
mod challenge;
#[cfg(test)]
mod challenge_tests;
mod digest;
#[cfg(test)]
mod digest_tests;
mod fetch;
mod http;
mod link;
#[cfg(test)]
mod link_tests;
mod media;
mod naming;
mod port;
mod presented;
mod prove;
mod provenance;
mod readable;
mod realm;
#[cfg(test)]
mod realm_tests;
mod reference;
mod resolve;
mod revisions;
#[cfg(test)]
mod revisions_tests;
mod scope;
mod send;
mod tags;
mod token;
#[cfg(test)]
mod token_tests;
mod verified;
#[cfg(test)]
mod verified_tests;
mod wire;

pub use prove::Proof;
pub use readable::Readability;

/// Reads an OCI registry over HTTPS, hashing everything it records, with the
/// credential it was given presented only where its kind's rules allow.
///
/// Built from [`RegistrySettings`](crate::RegistrySettings) by
/// [`with_settings`](Self::with_settings), or for the deployment's registry
/// by [`new`](Self::new). Replacing a credential is building another: no
/// token, and no refusal, outlives the client that obtained it.
pub struct OciRegistry {
    /// For manifests, tags, referrers and tokens: follows a redirect only to
    /// the origin the request went to, so a registry can never steer one of
    /// these — or what it carries — somewhere else.
    pub(crate) api: reqwest::Client,

    /// For blobs, which every hosted registry serves from a CDN on another
    /// host: follows no redirect itself. Each hop is followed by hand, and
    /// what the repository holds is attached only to a hop on the registry's
    /// own origin — never judged against the hop before, which is how
    /// `reqwest` would judge it.
    pub(crate) blobs: reqwest::Client,

    /// The transport rule every blob redirect hop and realm is held to.
    pub(crate) transport: crate::transport::Transport,

    /// The address policy every URL this follows is held to.
    pub(crate) address: crate::address::Address,

    /// Where to talk to it, scheme and all, with no trailing `/`.
    pub(crate) base_url: String,

    /// The same address, parsed: the origin a `Link` must stay on, and the
    /// only one a credential or token is sent to.
    pub(crate) origin: reqwest::Url,

    /// How repositories are *named*, which is not always where they are
    /// served from: `docker.io/library/nginx` is served by
    /// `registry-1.docker.io`, and a test points a client at a socket without
    /// renaming every image in the fixture.
    pub(crate) registry_host: String,

    /// Which realm may issue its tokens, and what it has recorded.
    pub(crate) realm: realm::Realm,

    /// Whether a `Basic` challenge is answered: a `distribution` registry's
    /// alone.
    pub(crate) honours_basic: bool,

    /// The credential, if one was given, and whether its realm refused it.
    pub(crate) credential: Option<presented::Presented>,

    /// What each repository holds — a token, or `Basic` — by whether it was
    /// credentialed.
    ///
    /// A credential, not an answer: nothing about *what was found* is
    /// remembered between passes. Held without an expiry: a token that has
    /// aged out comes back as `401` with a fresh challenge, which is cheaper
    /// to notice than to predict.
    pub(crate) tokens: scope::Holdings,

    /// Bytes this client has hashed, by digest: content, never an answer.
    pub(crate) verified: verified::Verified,
}
