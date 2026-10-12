//! Which derived resources a tenant cannot reach, because it binds no data
//! source for their logical data source (G8, ADR 0023 "A resource on a
//! logical data source a client never asked for").

use std::collections::{BTreeMap, BTreeSet};

use fabric_core::LogicalDataSourceName;

use super::{DerivedCatalogue, DerivedResource};

/// One logical data source the catalogue names and a tenant does not bind,
/// and every resource on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnboundDataSource {
    /// The logical data source no placement of the tenant binds.
    pub logical: LogicalDataSourceName,
    /// The catalogue's resources on it, sorted by name. Never empty.
    pub resources: Vec<DerivedResource>,
}

impl DerivedCatalogue {
    /// Every logical data source a resource in this catalogue names that
    /// `bound` does not hold, sorted by name.
    ///
    /// The runtime serves one catalogue to every tenant, so each of these
    /// resources answers `UnboundDataSource` (500) to a tenant whose binding
    /// is `bound` (ADR 0018 part 4). The publisher refuses only an entry
    /// that no tenant binds, so these pairs are published and are only
    /// visible here.
    #[must_use]
    pub fn unbound_for(&self, bound: &BTreeSet<LogicalDataSourceName>) -> Vec<UnboundDataSource> {
        let mut unbound: BTreeMap<&LogicalDataSourceName, Vec<DerivedResource>> = BTreeMap::new();
        for resource in &self.resources {
            let logical = &resource.definition.data_source;
            if !bound.contains(logical) {
                unbound.entry(logical).or_default().push(resource.clone());
            }
        }
        unbound
            .into_iter()
            .map(|(logical, resources)| UnboundDataSource {
                logical: logical.clone(),
                resources,
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "unbound_resources_tests.rs"]
mod tests;
