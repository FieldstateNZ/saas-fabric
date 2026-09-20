//! What [`establish`](super::establish) builds.

use fabric_control_plane::{PlatformBinding, PublicationSink};

/// Platform Management, and where this deployment publishes runtime state --
/// two independent absences, not one.
///
/// A deployment can manage a platform repository and publish no runtime
/// state (no `publication` section), and -- though nothing today constructs
/// it this way -- the reverse shape is representable without meaning
/// anything, since a publisher needs `platform` to publish through. Kept as
/// two fields rather than nesting `publication` inside `platform` because
/// [`PublicationSink`] is handed to `ControlPlaneDeps.publication`, a
/// sibling of `ControlPlaneDeps.platform`, not a part of the platform
/// binding itself -- see `PlatformBinding::publisher`'s own rustdoc for why
/// the two meet only inside `build_control_plane`.
pub struct Established {
    /// Platform Management, if this deployment manages a platform
    /// repository at all.
    pub platform: Option<PlatformBinding>,

    /// Where this deployment publishes the runtime's three documents, if it
    /// publishes them at all (ADR 0023 part 4).
    pub publication: Option<PublicationSink>,
}
