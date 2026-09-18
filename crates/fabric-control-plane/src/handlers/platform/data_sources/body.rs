//! The shape the console reads for an environment's declared data sources.

mod render;

use std::collections::BTreeMap;

use fabric_platform_management::{
    ConnectionSelectorDocument, DataSourcesRead, DesiredRevision, DesiredStateError, PlatformError,
};

use crate::ControlPlaneError;

/// The revision every response about an environment's data sources
/// carries, as `revision` in the body and as `ETag`.
///
/// # Why this can fail
///
/// `DataSourceState::read_data_sources` returns `Option` because an
/// adapter's truth about a file is genuinely optional at that layer --
/// see `DataSourcesRead`. The binding this deployment runs turns that
/// option into a tag before this code ever sees it, even for an
/// environment with nothing declared yet, so in production this is
/// always `Some`. Treating `None` as anything other than a platform
/// failure would answer a write with a body that carries nothing to
/// send back as `If-Match`.
pub(super) fn current_revision(read: &DataSourcesRead) -> Result<&DesiredRevision, ControlPlaneError> {
    read.revision.as_ref().ok_or_else(|| {
        ControlPlaneError::Platform(PlatformError::DesiredState(DesiredStateError::Unavailable {
            detail: "the platform repository returned no revision for data sources".to_owned(),
        }))
    })
}

/// Every data source an environment declares, as the console reads it.
///
/// The same shape answers `GET /api/platform/data-sources` and
/// `PUT /api/platform/data-sources/{dataSourceId}`: a write's response is
/// what a following read would now see, so the console never has to guess
/// whether to merge its own request into what it already had.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataSourcesBody {
    /// Which environment. A deployment manages one, so this is never a
    /// caller's choice — see `handlers::platform::get_platform` for why the
    /// same is true of the platform's own summary.
    pub(crate) environment: String,

    /// The document's revision, opaque and compared for equality only.
    /// Always present: the late-bound binding tags even an environment with
    /// no data sources declared yet, so there is always something to send
    /// back as `If-Match` on the very first declaration.
    pub(crate) revision: String,

    /// Every declared data source, sorted by id.
    pub(crate) data_sources: Vec<DataSourceRow>,
}

/// One declared data source.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataSourceRow {
    /// Its id, referenced by a tenant's placement and nothing else.
    pub(crate) id: String,

    /// The revision the runtime will see once this is published — bumped by
    /// Fabric on every change, never by the caller.
    pub(crate) revision: u64,

    /// The connector process this data source is configured on.
    pub(crate) connector: String,

    /// How the connector selects the connection: a name it already holds,
    /// or a reference to a secret — never a value. Rendered as the wire
    /// spells it (`{"kind": "named", "name": …}` or
    /// `{"kind": "secret", "reference": …}`): every one of its field names
    /// is already a single word, so the wire's own shape is the console's.
    /// The wire's third shape, `{"kind": "default"}`, is refused at
    /// declaration (`DataSourceRule::ConnectionKindNotDeclarable`) and never
    /// appears here: a declared source can only hold the two shapes above.
    pub(crate) connection: ConnectionSelectorDocument,

    /// The service class this data source provides: `shared`, `dedicated`,
    /// `highAvailability`, `regulated`, `development`, or `ephemeral`.
    pub(crate) placement: &'static str,

    /// Where the data physically lives.
    pub(crate) residency: ResidencyRow,

    /// Pool sizing, applied by reconciliation to the connector.
    pub(crate) pool: PoolRow,

    /// What the platform permits this data source to be used for.
    pub(crate) capabilities: CapabilitiesRow,

    /// The column every collection on this data source carries. `null` on
    /// anything but a shared data source, which `DataSourceRule` refuses to
    /// have one at all.
    pub(crate) discriminator: Option<DiscriminatorRow>,

    /// Operator-defined labels. `{}` rather than an absent key when empty,
    /// so the console never has to treat "no labels" as a special case.
    pub(crate) labels: BTreeMap<String, String>,
}

/// Where a data source's data physically lives.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResidencyRow {
    /// The region, such as `nz`.
    pub(crate) region: String,

    /// The legal jurisdiction, when it differs usefully from the region.
    /// `null`, not an absent key, when there is none to report.
    pub(crate) jurisdiction: Option<String>,
}

/// Connection pool sizing.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PoolRow {
    /// Maximum concurrent connections the connector may open, across every
    /// tenant bound to this data source.
    pub(crate) max_connections: u32,

    /// How long an idle connection is kept before eviction, in seconds.
    pub(crate) idle_timeout_seconds: u64,

    /// How long to wait for a connection before giving up, in seconds.
    pub(crate) acquire_timeout_seconds: u64,
}

/// What the platform permits a data source to be used for.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CapabilitiesRow {
    /// Whether write operations are permitted against this data source.
    pub(crate) writable: bool,

    /// Whether reconciliation may bind new tenants to this data source.
    pub(crate) accepts_new_tenants: bool,
}

/// The column every collection on a shared data source carries.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscriminatorRow {
    /// The column name.
    pub(crate) column: String,
}
