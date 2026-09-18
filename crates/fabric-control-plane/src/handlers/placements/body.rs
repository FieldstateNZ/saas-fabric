//! The shape the console reads for a client's data placements.

mod render;

use fabric_platform_management::{
    ClientPlacements, DesiredRevision, DesiredStateError, IsolationModelDocument, PlatformError,
};

use crate::ControlPlaneError;

/// The revision every response about a client's placements carries, as
/// `revision` in the body and as `ETag`.
///
/// # Why this can fail
///
/// `PlacementState::read_placements` returns `Option` because an adapter's
/// truth about the file is genuinely optional at that layer -- see
/// `PlacementsRead`. The binding this deployment runs turns that option
/// into a tag before this code ever sees it, even for an environment with
/// nothing recorded yet, so in production this is always `Some`. The same
/// reasoning as `handlers::platform::data_sources::body::current_revision`,
/// for the sibling document.
pub(super) fn current_revision(placements: &ClientPlacements) -> Result<&DesiredRevision, ControlPlaneError> {
    placements.revision.as_ref().ok_or_else(|| {
        ControlPlaneError::Platform(PlatformError::DesiredState(DesiredStateError::Unavailable {
            detail: "the platform repository returned no revision for placements".to_owned(),
        }))
    })
}

/// Every logical data source a client's document names, and what is true
/// of each, as the console reads it.
///
/// The same shape answers `GET /api/clients/{clientId}/placements` and
/// `POST /api/clients/{clientId}/placements/{logical}`: a write's response
/// is what a following read would now see, the same property
/// `handlers::platform::data_sources`'s `DataSourcesBody` keeps for its own
/// pair of routes.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacementsBody {
    /// Which client. Always the one the request path named.
    pub(crate) client_id: String,

    /// Which environment. A deployment manages one, so this is never a
    /// caller's choice.
    pub(crate) environment: String,

    /// The placements document's revision, opaque and compared for
    /// equality only. Always present -- see [`current_revision`].
    pub(crate) revision: String,

    /// One entry per logical data source the client's document names,
    /// sorted the same way `spec.data` is.
    pub(crate) placements: Vec<PlacementRow>,
}

/// One logical data source: what the client's document asks for, and what
/// is true of it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacementRow {
    /// The logical data source this entry is about, such as `primary`.
    pub(crate) logical: String,

    /// What the client's document asks for.
    pub(crate) intent: IntentRow,

    /// Where this landed, when it is recorded. `null` when nothing is
    /// recorded yet, whether that is because placing it would succeed
    /// (`refusal` is also `null`) or because it would be refused (`refusal`
    /// names why).
    pub(crate) placed: Option<PlacedRow>,

    /// Why placing this would be refused, when nothing is recorded and
    /// nothing declared could admit the intent. `null` when this is
    /// already placed, or when it is not yet placed but placeable.
    pub(crate) refusal: Option<String>,
}

/// What `spec.data.<logical>` asks for, as the console reads it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IntentRow {
    /// The service class this intent asks for, in the console's word --
    /// `handlers::platform::data_sources::placement::console_word` reused,
    /// so a data source's `placement` and a client's `spec.data.<logical>.class`
    /// never disagree about how one is spelled.
    pub(crate) class: &'static str,

    /// Free text, carried and shown, never matched against anything
    /// declared.
    pub(crate) provider: Option<String>,

    /// Matched against a candidate data source's residency when present.
    pub(crate) region: Option<String>,
}

/// Where a logical data source landed, once it is recorded.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacedRow {
    /// The declared data source this tenant's logical data source landed
    /// on.
    pub(crate) data_source: String,

    /// How this tenant's rows are kept apart from another's on
    /// `data_source`. Rendered as the wire's own
    /// [`IsolationModelDocument`] spells it -- every one of its field
    /// names is already a single word, so the wire's own shape is the
    /// console's, the same way `DataSourceRow::connection` reuses
    /// `ConnectionSelectorDocument` directly.
    pub(crate) isolation: IsolationModelDocument,

    /// When this was placed, RFC 3339.
    pub(crate) placed_at: String,
}
