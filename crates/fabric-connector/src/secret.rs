//! Secret references, the values they resolve to, and the seam between them.
//!
//! Three types that are deliberately not interchangeable. A [`SecretRef`] is a
//! path and may be logged; a [`ResolvedSecret`] is a credential and may not;
//! [`SecretResolver`] is the only thing that turns one into the other, and its
//! implementation is a deployment concern no caller can observe.

mod resolved_secret;
#[cfg(test)]
mod resolved_secret_tests;
mod secret_ref;
#[cfg(test)]
mod secret_ref_tests;
mod secret_resolver;

pub use resolved_secret::ResolvedSecret;
pub use secret_ref::SecretRef;
pub use secret_resolver::SecretResolver;
