//! What verification produces, and where each field came from.

use fabric_core::SubjectId;

/// A caller whose token has been verified, and the tenant it belongs to.
///
/// # One field comes from the token; the rest come from the registry
///
/// ```text
/// tenant     registry-derived
/// store      registry-derived
/// principal  registry-derived realm + verified `sub`
/// subject    the verified token's `sub`         ← the only claim read
/// ```
///
/// A token may carry a `tenant`, a `realm`, a `store_id`, or a `principal`
/// claim. It is not wrong to carry them — providers put all sorts of things in
/// tokens — but this verifier never reads them. Which tenant a caller belongs
/// to and which store answers for them are properties of *the registration
/// selected by the verified issuer*, and nothing a caller can influence
/// selects a different one.
///
/// That is the whole security argument for the type: it is not a bag of
/// claims, it is the answer to "who is this and where do they live", assembled
/// from a source the caller does not control.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    /// The tenant, from the registration the verified issuer selected.
    tenant: String,

    /// The subject exactly as the provider minted it.
    subject: String,

    /// Realm-qualified, and therefore meaningless outside its tenant.
    principal: SubjectId,

    /// The authorization store that answers for this tenant.
    store: String,
}

impl VerifiedIdentity {
    /// Assembles an identity from a registration and a verified subject.
    ///
    /// Deliberately crate-private: outside this crate there is no way to make
    /// one, so holding a `VerifiedIdentity` is evidence that a token was
    /// verified rather than a claim that it was.
    pub(crate) const fn new(tenant: String, subject: String, principal: SubjectId, store: String) -> Self {
        Self {
            tenant,
            subject,
            principal,
            store,
        }
    }

    /// The tenant this caller belongs to.
    #[must_use]
    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    /// The subject as the provider minted it, unqualified.
    ///
    /// Rarely what a caller wants: [`principal`](Self::principal) is the
    /// identifier an authorization decision is made about, because a bare
    /// subject means nothing outside its realm.
    #[must_use]
    pub fn subject(&self) -> &str {
        &self.subject
    }

    /// The identifier an authorization decision is made about.
    #[must_use]
    pub const fn principal(&self) -> &SubjectId {
        &self.principal
    }

    /// The authorization store that answers for this tenant.
    #[must_use]
    pub fn store(&self) -> &str {
        &self.store
    }
}
