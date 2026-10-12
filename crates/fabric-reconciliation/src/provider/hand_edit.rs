//! A hand edit the product does not support, and what a write leaves of it.
//!
//! D01-19b: a declared client is the product's projection into the identity
//! provider, and an edit made outside the product is either corrected (it
//! touches a product-owned field) or **held**: reported, and left in place
//! until an operator removes it or explicitly re-asserts the projection. See
//! `plan::classify` for which field is which, and why.

use std::collections::BTreeSet;

use fabric_client_model::{OidcClientId, RedirectUri};

/// Which unsupported hand edit a client carries.
///
/// Every variant **narrows** what the client can do. That is what makes it
/// safe to leave in place: an edit that widened a *managed* field — a
/// redirect URI nobody declared, a confidential secret, a downgraded PKCE
/// method, a client mapper injecting a claim — is product-owned drift and is
/// corrected. Fields the adapter does not read are not evaluated at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HandEditedField {
    /// The client was switched off in the provider. Possibly an operator
    /// containing an incident, which re-enabling it automatically would undo.
    ClientDisabled,

    /// The authorization-code flow was switched off for the client.
    StandardFlowDisabled,

    /// At least one declared redirect URI is no longer registered.
    DeclaredRedirectUriRemoved,
}

impl HandEditedField {
    /// A stable name, for log fields and audit records. The same string the
    /// API serialises.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ClientDisabled => "clientDisabled",
            Self::StandardFlowDisabled => "standardFlowDisabled",
            Self::DeclaredRedirectUriRemoved => "declaredRedirectUriRemoved",
        }
    }
}

/// One unsupported hand edit on one declared client.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
pub struct HandEdit {
    /// The declared application client that carries it.
    pub client: OidcClientId,

    /// What was edited.
    pub field: HandEditedField,
}

impl std::fmt::Display for HandEdit {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}:{}", self.client, self.field.as_str())
    }
}

/// The hand edits an update must leave exactly as the provider holds them.
///
/// Passed to [`IdentityProvider::update_oidc_client`](crate::IdentityProvider::update_oidc_client)
/// so that correcting a product-owned field on a client does not also
/// overwrite a held one: the adapter writes the declaration with these
/// narrowings applied. The default holds nothing, which is a full write of
/// the declaration — what a create, and an explicit re-assertion, send.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HeldEdits {
    /// Write the client disabled, because it is disabled now.
    pub client_disabled: bool,

    /// Write the standard flow off, because it is off now.
    pub standard_flow_disabled: bool,

    /// Declared redirect URIs to leave out, because they are absent now.
    pub removed_redirect_uris: BTreeSet<RedirectUri>,
}

impl HeldEdits {
    /// Whether nothing is held, so an update writes the whole declaration.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        !self.client_disabled && !self.standard_flow_disabled && self.removed_redirect_uris.is_empty()
    }

    /// The declared redirect URIs a write should carry: every one but those
    /// held as removed.
    ///
    /// Shared by every adapter so that "which callbacks does a held write
    /// keep" has one answer.
    pub fn redirect_uris_to_write<'a>(
        &'a self,
        declared: &'a [RedirectUri],
    ) -> impl Iterator<Item = &'a RedirectUri> + 'a {
        declared
            .iter()
            .filter(|uri| !self.removed_redirect_uris.contains(*uri))
    }

    /// The held edits as reportable fields, in a stable order.
    #[must_use]
    pub fn fields(&self) -> Vec<HandEditedField> {
        [
            (self.client_disabled, HandEditedField::ClientDisabled),
            (self.standard_flow_disabled, HandEditedField::StandardFlowDisabled),
            (
                !self.removed_redirect_uris.is_empty(),
                HandEditedField::DeclaredRedirectUriRemoved,
            ),
        ]
        .into_iter()
        .filter_map(|(held, field)| held.then_some(field))
        .collect()
    }
}
