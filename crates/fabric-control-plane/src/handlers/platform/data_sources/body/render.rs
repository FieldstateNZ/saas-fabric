//! Turning declared data sources into what the console reads.

use fabric_platform_management::{DataSourceDeclaration, DataSourcesRead, DesiredRevision};

use super::{CapabilitiesRow, DataSourceRow, DataSourcesBody, DiscriminatorRow, PoolRow, ResidencyRow};
use crate::handlers::platform::data_sources::placement;

impl DataSourcesBody {
    /// Renders an environment's declared data sources at the given
    /// revision -- see `current_revision` for why the caller, not this
    /// function, is what turns the read's `Option` into one.
    pub(in crate::handlers::platform::data_sources) fn of(
        environment: &str,
        revision: &DesiredRevision,
        read: &DataSourcesRead,
    ) -> Self {
        Self {
            environment: environment.to_owned(),
            revision: revision.as_str().to_owned(),
            data_sources: read.declarations.iter().map(DataSourceRow::of).collect(),
        }
    }
}

impl DataSourceRow {
    /// Renders one declaration.
    fn of(declaration: &DataSourceDeclaration) -> Self {
        Self {
            id: declaration.id.to_string(),
            revision: declaration.revision.get(),
            connector: declaration.connector.to_string(),
            connection: declaration.connection.clone(),
            placement: placement::console_word(declaration.placement),
            residency: ResidencyRow {
                region: declaration.residency.region.clone(),
                jurisdiction: declaration.residency.jurisdiction.clone(),
            },
            pool: PoolRow {
                max_connections: declaration.pool.max_connections,
                idle_timeout_seconds: declaration.pool.idle_timeout_seconds,
                acquire_timeout_seconds: declaration.pool.acquire_timeout_seconds,
            },
            capabilities: CapabilitiesRow {
                writable: declaration.capabilities.writable,
                accepts_new_tenants: declaration.capabilities.accepts_new_tenants,
            },
            discriminator: declaration
                .discriminator
                .as_ref()
                .map(|discriminator| DiscriminatorRow {
                    column: discriminator.column.to_string(),
                }),
            labels: declaration.labels.clone(),
        }
    }
}
