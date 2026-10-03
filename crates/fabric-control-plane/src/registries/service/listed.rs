//! A registry as it is listed: what was recorded, and what is true of it now.

use crate::registries::RegistryRecord;

/// A registry, and what is true of it now.
#[derive(Debug, Clone)]
pub(crate) struct Listed {
    /// What was recorded.
    pub(crate) record: RegistryRecord,

    /// Whether Platform Management reads through it now.
    pub(crate) installed: bool,

    /// How its credential, if it holds one, stands now.
    pub(crate) credential: CredentialState,

    /// Whether it is the deployment's own registry's host.
    pub(crate) deployment: bool,
}

/// How a recorded credential stands now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CredentialState {
    /// Presented where it is registered for — or no credential is held.
    Presented,

    /// Its realm refused it; it is not presented again until it is replaced
    /// or proven again.
    Refused,

    /// It could not be read at startup, so the registry is read anonymously
    /// until it is set again.
    Unreadable,
}
