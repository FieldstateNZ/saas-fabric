//! Which Git integration a record belongs to.

#[cfg(test)]
#[path = "kind/kind_tests.rs"]
mod kind_tests;

/// The Git integrations this platform can have, independently of each other.
///
/// # Why this is a closed enum and not a name
///
/// A string would let a caller invent an integration, and the first place that
/// would show is a store writing a record to a path nobody chose. There are
/// exactly two, they are known at compile time, and a request reaches one by
/// hitting the route for it rather than by naming it.
///
/// # Why there are two at all
///
/// They are the same *mechanism* — a GitHub App, an installation, a chosen
/// repository — serving two unrelated purposes, and the specification requires
/// them to be separately installable, configurable and removable. Connecting
/// client configuration must not connect platform management, and disconnecting
/// either must leave the other exactly as it was.
///
/// One App with both permissions would be smaller and would mean an operator
/// who wanted to manage clients had to grant write access to the platform
/// repository as well.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IntegrationKind {
    /// Where clients' desired state lives.
    ClientConfiguration,

    /// Where the platform's own composition lives.
    PlatformManagement,
}

impl IntegrationKind {
    /// The name this integration's private key is stored under.
    ///
    /// # The client path is the one that already exists
    ///
    /// `git/app-private-key` is where a connected `LucentRoot` keeps its key
    /// today. It stays exactly where it is: moving a live credential to make a
    /// naming scheme symmetrical is a migration with nothing to gain and a
    /// disconnected platform to lose.
    ///
    /// The asymmetry is the honest record of which came first, and costs
    /// nothing but the look of it.
    #[must_use]
    pub const fn private_key(self) -> &'static str {
        match self {
            Self::ClientConfiguration => "git/app-private-key",
            Self::PlatformManagement => "integrations/platform-management/app-private-key",
        }
    }

    /// What this integration is called, for a log line or a message.
    #[must_use]
    pub const fn describe(self) -> &'static str {
        match self {
            Self::ClientConfiguration => "client configuration",
            Self::PlatformManagement => "platform management",
        }
    }
}
