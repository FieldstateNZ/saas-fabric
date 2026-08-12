//! What a type must supply to live in a registry.

use std::fmt::Display;
use std::hash::Hash;

use fabric_core::BindingRevision;

/// A reconciled runtime resource.
///
/// Two things are required, and they are the two the lifecycle turns on: an
/// identity to look the resource up by, and a revision to decide whether an
/// incoming copy is newer than the one held (§20).
///
/// `KIND` exists so that log lines and errors can say *what* was not found
/// without the generic code knowing anything else about the type.
pub trait RegistryResource: Clone + Send + Sync + 'static {
    /// How this resource is addressed.
    type Key: Clone + Ord + Hash + Display + Send + Sync + 'static;

    /// A human-readable name for this kind of resource, such as `tenant`.
    const KIND: &'static str;

    /// This resource's identity.
    fn key(&self) -> &Self::Key;

    /// This resource's revision. Only ever moves forward.
    fn revision(&self) -> BindingRevision;
}
