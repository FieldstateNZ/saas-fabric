//! Why a version was not selected, as the states the console words.

/// Why a version was not selected: serialized as `state`, and for `invalid`
/// a `reason` beside it.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum DiagnosticState {
    /// `publishing`: some of its images are not there yet, and it is expected
    /// to become available on a later pass.
    Publishing,

    /// `undescribed`: its primary image is tagged and no component descriptor
    /// is attached to the digest the tag resolves to. Fabric cannot tell a
    /// publication in progress from one that will never attach one, and says
    /// only what it saw (ADR 0026 section 3); every pass asks again.
    Undescribed,

    /// `incoherent`: built more than once. Its images were built from
    /// different commits, or, for a described component, its images and its
    /// component descriptor name different commits, or another image no
    /// longer carries the version at the digest the component descriptor
    /// names. Waiting will not fix it.
    Incoherent,

    /// `invalid`: a component descriptor is attached and cannot be used.
    Invalid {
        /// Why, as `InvalidReason::code` spells it: a stable code from a
        /// closed list, which the console keys its wording by.
        ///
        /// The code, and not most of the detail a few reasons carry: a role
        /// or a repository is text a publisher wrote, and would sit beside
        /// that wording unexamined.
        reason: &'static str,

        /// The format version found, beside `unsupportedVersion` only, where
        /// naming it is what the answer means (ADR 0026 section 2).
        ///
        /// Unlike a role or a repository it is not free text: a reader only
        /// ever produces `v` and digits, and a longer run than sixteen
        /// characters is left out rather than trusted.
        #[serde(skip_serializing_if = "Option::is_none")]
        found: Option<String>,
    },
}
