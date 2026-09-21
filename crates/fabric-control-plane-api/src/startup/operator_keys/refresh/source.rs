//! Where the refresh loop reads the provider's signing keys from, and why
//! that is a trait at all.
//!
//! [`RealmSignIn::signing_keys`](fabric_keycloak::RealmSignIn::signing_keys)
//! is a concrete method with no trait behind it -- Keycloak is the only realm
//! this platform talks to, so `fabric-keycloak` never needed one. That is not
//! quite why this loop had no test of its own before this seam existed,
//! though: `RealmSignIn::new` is `pub` and takes plain URLs, so a stub HTTP
//! server could already have stood in for the realm. What nothing could do
//! is make the adapter *panic* on command, which is the property
//! `refresh`'s own test needs. [`SigningKeySource`] exists to let a test
//! substitute a fake with that property; in production the realm is the only
//! implementation, and nothing else is expected to implement it outside
//! `#[cfg(test)]`.

use async_trait::async_trait;
use fabric_keycloak::RealmSignIn;

/// Where this loop reads the provider's signing-key document from.
///
/// The seam exists so the refresh loop can be driven by a fake in tests --
/// see the module doc for why. The production source is the realm, reached
/// through [`RealmSignIn`]'s own inherent `signing_keys` method; nothing else
/// implements this trait outside `#[cfg(test)]`.
///
/// Visible to all of `operator_keys`, not just `refresh`: `establish`, in
/// `operator_keys.rs`, needs to name this type to hand `spawn` a
/// `RealmSignIn` as one. `pub(super)` here would not do: a re-export
/// cannot widen an item, so `refresh.rs`'s `pub(super) use` of a
/// `pub(super)` trait fails with E0365, and `establish`'s own use of it
/// with E0603. The item itself has to reach `operator_keys`.
#[async_trait]
pub(in crate::startup::operator_keys) trait SigningKeySource:
    Send + Sync
{
    /// Reads the document the provider currently publishes its signing keys
    /// in, exactly as `RealmSignIn::signing_keys` does.
    async fn read_signing_keys(&self) -> Result<String, String>;
}

#[async_trait]
impl SigningKeySource for RealmSignIn {
    async fn read_signing_keys(&self) -> Result<String, String> {
        // Named differently from the inherent method it delegates to on
        // purpose. `self.signing_keys()` here would resolve to the inherent
        // method today -- Rust prefers an inherent method over a trait
        // method of the same name at a call site -- but only for as long as
        // that inherent method keeps exactly this name. If
        // `fabric-keycloak` ever renamed or dropped it, this call would
        // silently re-resolve to the trait method itself: infinite
        // recursion that overflows the stack and aborts the process rather
        // than unwinding, which `tick::survive_panic` cannot catch. Naming
        // this method differently, and calling the inherent one through its
        // fully qualified path, removes the ambiguity outright -- a rename
        // upstream fails this file to compile instead of recursing at
        // runtime.
        RealmSignIn::signing_keys(self).await
    }
}
