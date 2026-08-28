//! An application client within a realm.

use crate::{OidcClientId, RedirectUri};

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

    /// Where the identity provider may redirect back to after authentication.
    ///
    /// Must not be empty — a client with no permitted callback cannot complete
    /// a browser flow, so an empty list is a document that reconciles into
    /// something unusable rather than an intentional state.
    pub redirect_uris: Vec<RedirectUri>,
}
