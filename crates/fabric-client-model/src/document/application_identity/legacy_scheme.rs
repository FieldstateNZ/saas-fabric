//! A stored document on `.internal` hosts that projects applications but
//! declares no scheme (D01-15).
//!
//! Such a document cannot be projected, so it reads as `unverifiable`, every
//! projected entry is withheld, and nothing is reinterpreted as either
//! scheme. This is the one question that lets an operator leave that state
//! through the product: whether the stored entries are exactly the
//! projection under **some** scheme, so a save that now declares one is not
//! silently overwriting a divergence (D01-19b). Which scheme the save
//! declares is the operator's, and need not be the one that matched.

use crate::{ClientDocument, PrivateNetworkScheme};

impl ClientDocument {
    /// Whether this document declares no scheme, is on `.internal` hosts
    /// with an application assigned, and holds stored projected entries
    /// that are exactly its projection under one of the two schemes.
    #[must_use]
    pub fn undeclared_scheme_is_otherwise_consistent(&self) -> bool {
        if self.client().private_network_scheme.is_some() || self.projected_clients().is_ok() {
            return false;
        }

        PrivateNetworkScheme::ALL.into_iter().any(|scheme| {
            self.divergences_over(Some(scheme))
                .is_ok_and(|divergences| divergences.is_empty())
        })
    }
}
