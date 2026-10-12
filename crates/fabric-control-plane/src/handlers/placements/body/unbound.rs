//! The catalogue resources a client's tenant cannot reach, as the console
//! reads them (G8).

use std::collections::BTreeMap;

use fabric_client_model::catalogue::UnboundDataSource;
use fabric_core::LogicalDataSourceName;

/// Whether the client's placements were checked against the environment's
/// derived runtime catalogue, and what the check found.
#[derive(Debug, serde::Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub(crate) enum UnboundBody {
    /// Checked. Empty when every resource's logical data source has a
    /// placement for this client.
    Checked {
        /// One entry per logical data source the catalogue names and this
        /// client's tenant does not bind, sorted by name.
        #[serde(rename = "dataSources")]
        data_sources: Vec<UnboundDataSourceRow>,
    },

    /// The catalogue could not be read, or two applications declare one
    /// resource, so nothing was checked.
    Unchecked {
        /// Why, with no repository detail.
        reason: String,
    },
}

/// One logical data source the catalogue names and the client's tenant does
/// not bind.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnboundDataSourceRow {
    /// The logical data source, such as `audit`.
    pub(crate) logical: String,

    /// Whether the client's document asks for it (`spec.data.<logical>`).
    /// When it does, placing that intent binds it. When it does not, the
    /// document has to ask for it first.
    pub(crate) declared: bool,

    /// Every resource on it, each of which answers `UnboundDataSource` to
    /// this client.
    pub(crate) resources: Vec<UnboundResourceRow>,
}

/// One resource the client's tenant cannot reach, and the release that
/// declares it.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnboundResourceRow {
    /// The name callers address it by.
    pub(crate) resource: String,

    /// The application whose newest published release declares it.
    pub(crate) application: String,

    /// That release's version.
    pub(crate) version: u32,
}

impl UnboundBody {
    /// Renders what [`DerivedCatalogue::unbound_for`](fabric_client_model::catalogue::DerivedCatalogue::unbound_for)
    /// found, marking each logical data source the client's document
    /// declares.
    pub(in crate::handlers::placements) fn checked(
        unbound: &[UnboundDataSource],
        intents: &BTreeMap<LogicalDataSourceName, fabric_client_model::DataIntent>,
    ) -> Self {
        Self::Checked {
            data_sources: unbound
                .iter()
                .map(|entry| UnboundDataSourceRow {
                    logical: entry.logical.to_string(),
                    declared: intents.contains_key(&entry.logical),
                    resources: entry
                        .resources
                        .iter()
                        .map(|resource| UnboundResourceRow {
                            resource: resource.name.to_string(),
                            application: resource.application.to_string(),
                            version: resource.version,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}
