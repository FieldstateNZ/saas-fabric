//! An application client within a realm.

use crate::{OidcClientId, PkceMethod, RedirectStrategy};

/// The authentication protocol an application client speaks.
///
/// A single-variant enum rather than an absent field. The document says
/// `type: oidc` explicitly, so a future `saml` entry is an added variant that
/// old documents keep parsing — where inferring the protocol would mean every
/// existing document silently acquiring a meaning it never stated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientProtocol {
    /// OpenID Connect.
    Oidc,
}

/// An application belonging to a client, as SaaS Fabric declares it.
///
/// # What is deliberately not here
///
/// **A client secret.** Every client declared here is reconciled as a *public*
/// client. A confidential client needs a secret, secrets never enter desired
/// state (platform specification §4), and inventing a place for one in this
/// document would be the first step toward Git holding credentials. Supporting
/// confidential clients means designing secret delivery first — see ADR 0008's
/// "What this does not decide".
///
/// That is also why [`Self::pkce`] is required rather than defaulted. A public
/// client with no proof key is a client whose authorisation code is redeemable
/// by whoever intercepts it, and a defaulted field would be a meaning the
/// document acquired without saying it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OidcClient {
    /// The `client_id` the application presents.
    pub id: OidcClientId,

    /// The protocol it speaks.
    ///
    /// Named `type` in the document, which is a Rust keyword, hence the
    /// rename.
    #[serde(rename = "type")]
    pub protocol: ClientProtocol,

    /// The proof-key method the identity provider must require of it.
    ///
    /// Required in `v2`, and supplied as `S256` by the `v1` migrator, so no
    /// declared client can exist without one.
    pub pkce: PkceMethod,

    /// Which kind of callback this client is entitled to, and the callbacks
    /// themselves.
    ///
    /// Replaces `v1`'s flat `redirectUris` list. Every URI in that list was
    /// individually acceptable and the *set* still said nothing about what the
    /// client was, so a production client could quietly hold a development
    /// callback and pass every check.
    pub redirect: RedirectStrategy,
}
