//! What a search found: the version to move to, and why the others were
//! not.

use crate::{InvalidReason, Version};

/// What a discovery pass found.
///
/// # Why the rejected versions are reported rather than dropped
///
/// `not_yet` is the case that must not be remembered. A component's images are
/// published by parallel jobs, so a version existing in two repositories and
/// not the third is normally a window of a minute or two — and a discovery
/// that recorded "0.3.0-preview.3 is not a thing" would still believe it an
/// hour later. Every pass recomputes from the registry, and a version listed
/// here is expected to move to `newer` on a later one.
///
/// `incoherent` is the opposite: images that all exist and disagree about
/// which commit they came from. That is one version built twice, and no
/// waiting fixes it.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Discovery {
    /// The newest complete, coherent version that sorts after the floor.
    ///
    /// # Not "the available version"
    ///
    /// It is the newest eligible version *newer than desired*, which is a
    /// narrower fact and needs the narrower name. Nothing here observes
    /// whether the desired version itself is still in the registry, so a
    /// broader name would be a claim this type is not entitled to make — and
    /// the console said exactly that for a while, rendering `Available —`
    /// about an environment running the newest preview there was.
    ///
    /// A `Latest available` worth the name arrives with a versions view, where
    /// Fabric enumerates what exists rather than inferring it from what it
    /// declined to advance to.
    pub newer: Option<crate::Release>,

    /// Newer versions that are still publishing. Transient — retried, never
    /// remembered.
    pub not_yet: Vec<Version>,

    /// Newer versions whose images disagree about their source commit — or,
    /// for a described component, whose images and component descriptor
    /// name different commits, or whose other images no longer carry the
    /// version at the digests it names.
    pub incoherent: Vec<Version>,

    /// Newer versions of a described component with no component descriptor
    /// attached. Not remembered: Fabric cannot tell a publication still in
    /// progress from one that will never attach one, and every pass asks
    /// again.
    pub undescribed: Vec<Version>,

    /// Newer versions of a described component whose component descriptor is
    /// present and cannot be used, each with why.
    pub invalid: Vec<InvalidVersion>,
}

/// A version whose component descriptor cannot be used, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidVersion {
    /// The version.
    pub version: Version,

    /// Why its component descriptor cannot be used.
    pub reason: InvalidReason,
}
