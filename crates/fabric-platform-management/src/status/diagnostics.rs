//! Why the versions that were not selected were not.

use crate::{InvalidVersion, Version};

/// Versions that exist and were not selected, and why.
///
/// `not_yet` is transient — images still publishing — and is expected to
/// empty itself. `incoherent` is not: those versions were built more than
/// once, and no waiting fixes them. `undescribed` and `invalid` are a
/// described component's (ADR 0026 section 9): no component descriptor
/// attached, and one attached that cannot be used, with why. Reported so an
/// environment jumping `preview.2` to `preview.4` can say what happened to
/// `preview.3`, and each in its own list so none is worded as another.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diagnostics {
    /// Still publishing.
    pub not_yet: Vec<Version>,

    /// Built from more than one commit.
    pub incoherent: Vec<Version>,

    /// No component descriptor attached, which Fabric cannot tell from one
    /// still to come.
    pub undescribed: Vec<Version>,

    /// A component descriptor attached that cannot be used, and why.
    pub invalid: Vec<InvalidVersion>,
}
