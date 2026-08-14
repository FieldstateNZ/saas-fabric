//! Who is actually bound to each DataSource.

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use fabric_core::{DataSourceId, TenantId};

use crate::TenantRuntimeBinding;

/// Which tenants each DataSource currently carries.
///
/// # Why this is worth deriving
///
/// Structural isolation ([`IsolationModel::Database`] and
/// [`IsolationModel::Schema`](fabric_connector::IsolationModel::Schema))
/// contributes no predicate; its separation is supposed to come from the
/// connection reaching somewhere else. A DataSource carries one connection,
/// shared by every tenant on it. So the question that decides whether such a
/// binding isolates anything is **how many tenants are bound to this
/// DataSource** — a fact the runtime observes, rather than a
/// [`PlacementClass`](crate::PlacementClass) label it is asked to believe.
///
/// A tenant with two logical names pointing at one DataSource counts once.
/// That is one tenant reaching its own data twice, which is what `primary`
/// plus `audit` on one database looks like, and refusing it would break a
/// legitimate arrangement.
///
/// [`IsolationModel::Database`]: fabric_connector::IsolationModel::Database
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CoTenancy {
    occupancy: HashMap<DataSourceId, Occupancy>,
}

/// Who is bound to one DataSource.
///
/// The sole occupant is named rather than counted because the question asked
/// of it is "anyone *other than* this tenant?" — and for the common case of a
/// dedicated DataSource, the answer depends on which tenant is asking.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Occupancy {
    /// Exactly one tenant, named.
    Sole(TenantId),
    /// More than one, so at least one of them is not whoever is asking.
    Many,
}

impl CoTenancy {
    /// Derives the fact from a whole tenant snapshot.
    ///
    /// Needs no DataSource registry: a binding names the DataSource it points
    /// at, so occupancy is answerable inside the tenant snapshot alone. That
    /// is what lets the check stay correct while the two registries refresh
    /// independently — neither derivation waits on the other.
    #[must_use]
    pub fn derive(bindings: &HashMap<TenantId, Arc<TenantRuntimeBinding>>) -> Self {
        let mut occupancy: HashMap<DataSourceId, Occupancy> = HashMap::new();

        for binding in bindings.values() {
            let distinct: BTreeSet<&DataSourceId> =
                binding.data.values().map(|data| &data.data_source).collect();

            for id in distinct {
                occupancy
                    .entry(id.clone())
                    .and_modify(|held| *held = Occupancy::Many)
                    .or_insert_with(|| Occupancy::Sole(binding.tenant.clone()));
            }
        }

        Self { occupancy }
    }

    /// Whether any tenant besides `tenant` is bound to this DataSource.
    ///
    /// `false` for a DataSource nobody is bound to: an empty destination is
    /// not shared, and a tenant arriving on it later changes this answer on
    /// the very next snapshot.
    #[must_use]
    pub fn has_tenants_other_than(&self, data_source: &DataSourceId, tenant: &TenantId) -> bool {
        match self.occupancy.get(data_source) {
            None => false,
            Some(Occupancy::Many) => true,
            Some(Occupancy::Sole(occupant)) => occupant != tenant,
        }
    }
}
