//! Which DataSources do not have their physical destination to themselves.

use std::collections::HashMap;
use std::sync::Arc;

use fabric_connector::{ConnectionName, ConnectionSelector, ConnectorId, SecretRef};
use fabric_core::DataSourceId;

use crate::DataSource;

/// For each DataSource, the others that select the same connector *and* the
/// same connection.
///
/// # Why the id is not the destination
///
/// Two DataSources are two ids and two revisions. They are not necessarily two
/// databases. `sales-01` and `sales-02` may both name connector
/// `postgres-au-east` and connection `primary`, in which case there is one
/// physical database wearing two names, and structural isolation across them
/// separates nothing.
///
/// [`ConnectionSelector::Default`] is the sharpest case, because it is what an
/// operator gets by not choosing: it means "the connector's one configured
/// database", so *any* two DataSources on one connector that both select it
/// are the same database by definition. Its own documentation calls that a
/// precondition; this is the part of the precondition the runtime can check.
///
/// # Peers, not a verdict
///
/// This names the peers rather than flagging the DataSource, because sharing a
/// destination is only a problem when the *tenants* differ. One tenant with a
/// writable and a read-only DataSource over one database is a legitimate
/// arrangement, and a flag would refuse it. Pairing this with
/// [`CoTenancy`](crate::CoTenancy) is what tells the two apart.
///
/// # What this cannot see
///
/// Equality of the selector, never of the destination behind it. Two distinct
/// [`ConnectionSelector::Named`] values pointing at one database, or two
/// [`SecretRef`]s resolving to one credential, look like two destinations from
/// here. The connector knows better and the runtime never asks it — §6 keeps
/// the request path out of the control plane, and a connector round trip on
/// every resolution is exactly what that rule forbids.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DestinationReuse {
    peers: HashMap<DataSourceId, Vec<DataSourceId>>,
}

/// One physical destination, as far as configuration can express it.
///
/// Borrowed rather than owned so deriving the fact allocates nothing per
/// DataSource. A private enum rather than a formatted string because a string
/// key would have to invent a separator no identifier could contain, and a
/// collision here means refusing a tenant that was fine.
#[derive(PartialEq, Eq, Hash)]
enum Destination<'a> {
    /// The connector's single configured connection.
    Default,
    /// A connection the connector holds configuration for.
    Named(&'a ConnectionName),
    /// A connection built from a resolved credential.
    Secret(&'a SecretRef),
}

impl DestinationReuse {
    /// Derives the fact from a whole DataSource snapshot.
    ///
    /// Needs no tenant registry, which is what lets it stay correct while the
    /// two registries refresh on their own schedules. Only DataSources that
    /// actually have peers are recorded, so the common case — every DataSource
    /// on its own connection — costs one empty map.
    #[must_use]
    pub fn derive(sources: &HashMap<DataSourceId, Arc<DataSource>>) -> Self {
        let mut claims: HashMap<(&ConnectorId, Destination<'_>), Vec<&DataSourceId>> = HashMap::new();

        for source in sources.values() {
            claims
                .entry((&source.connector, destination(&source.connection)))
                .or_default()
                .push(&source.id);
        }

        let mut peers: HashMap<DataSourceId, Vec<DataSourceId>> = HashMap::new();

        for claimants in claims.into_values().filter(|claimants| claimants.len() > 1) {
            for id in &claimants {
                let others = claimants
                    .iter()
                    .filter(|other| *other != id)
                    .map(|other| (*other).clone());

                peers.entry((*id).clone()).or_default().extend(others);
            }
        }

        Self { peers }
    }

    /// The other DataSources selecting this one's connector and connection.
    ///
    /// Empty for a DataSource that has its destination to itself, which is
    /// every DataSource in a correctly modelled deployment.
    #[must_use]
    pub fn peers(&self, data_source: &DataSourceId) -> &[DataSourceId] {
        self.peers.get(data_source).map_or(&[], Vec::as_slice)
    }
}

/// Reduces a selector to the destination it names.
const fn destination(selector: &ConnectionSelector) -> Destination<'_> {
    match selector {
        ConnectionSelector::Default => Destination::Default,
        ConnectionSelector::Named { name } => Destination::Named(name),
        ConnectionSelector::Secret { reference } => Destination::Secret(reference),
    }
}
