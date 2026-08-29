//! Acting on the identity provider with an operator's own authority.
//!
//! # Why this is a factory and not a provider
//!
//! The platform used to hold one provider, built once at startup from a
//! service account's client secret. That secret was a standing machine
//! credential: it could create a realm at three in the morning with nobody
//! having asked, and its existence meant the platform's authority was
//! independent of any human's.
//!
//! It is now built **per operator**, from the bearer they presented. Permission
//! to create a realm belongs to a person in the master realm, and this is the
//! seam through which their permission reaches the request that uses it. There
//! is no credential of the platform's own to leak, rotate, or reason about.
//!
//! # What it costs
//!
//! Convergence can only happen while an operator's authority is available, so
//! there is no unattended sweep any more. ADR 0012 records why that trade is
//! acceptable here: the identity provider's own console is published on no
//! plane, so changes made outside SaaS Fabric are largely prevented rather than
//! merely noticed afterwards.

use std::sync::Arc;

use fabric_reconciliation::IdentityProvider;

use crate::operator::OperatorToken;

/// Builds a provider that acts as one operator.
pub trait IdentityProviderFactory: Send + Sync {
    /// A provider carrying this operator's authority.
    ///
    /// Cheap, and called per operation rather than held: the authority it
    /// wraps expires, and a provider cached beyond one request would outlive
    /// the permission that justified it.
    fn acting_as(&self, authority: &OperatorToken) -> Arc<dyn IdentityProvider>;

    /// A short description of the provider, for the startup log.
    ///
    /// Names an endpoint and never a credential — there is no longer one to
    /// name.
    fn describe(&self) -> String;
}
