//! Where a DataSource's data physically lives.

/// The region and jurisdiction a DataSource sits in.
///
/// # What these values are
///
/// **Resolved physical state, declared by the DataSource.** Not desired
/// placement, not a policy, not an aspiration: this says where the data
/// actually is, as reconciled. Whoever provisions the DataSource is asserting
/// it, and it is expected to match reality.
///
/// That framing matters because it decides who consumes it:
///
/// | Consumer | Use |
/// |---|---|
/// | Reconciliation / placement | Satisfying a tenant's residency policy when choosing a DataSource |
/// | Operators and audit | "Which tenants have data in the EU?" |
/// | Platform telemetry | A label on internal spans (§29) |
///
/// # The runtime never decides anything from it
///
/// By the time a binding exists the placement decision has been made. Reading
/// residency on the request path to pick or re-pick a DataSource would be
/// placement, which belongs to the control plane — and would be both wasteful
/// and too late. The runtime carries the value and reports it; nothing branches
/// on it.
///
/// A tenant's residency *requirement* is deliberately not modelled here. That
/// is tenant policy, it lives in the tenant definition, and reconciliation is
/// what matches the two.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataResidency {
    /// The region this DataSource runs in, such as `au-east`.
    pub region: String,

    /// The legal jurisdiction governing the data, where it differs usefully
    /// from the region — `AU`, `EU`, `US`.
    #[serde(default)]
    pub jurisdiction: Option<String>,
}

impl DataResidency {
    /// Residency in a region, with no jurisdiction recorded.
    #[must_use]
    pub fn in_region(region: impl Into<String>) -> Self {
        Self {
            region: region.into(),
            jurisdiction: None,
        }
    }

    /// A short label for telemetry: `au-east` or `au-east/AU`.
    #[must_use]
    pub fn telemetry_label(&self) -> String {
        match &self.jurisdiction {
            Some(jurisdiction) => format!("{}/{jurisdiction}", self.region),
            None => self.region.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_alone_labels_as_the_region() {
        assert_eq!(DataResidency::in_region("au-east").telemetry_label(), "au-east");
    }

    #[test]
    fn a_jurisdiction_is_appended_when_present() {
        let residency = DataResidency {
            region: "au-east".to_owned(),
            jurisdiction: Some("AU".to_owned()),
        };

        assert_eq!(residency.telemetry_label(), "au-east/AU");
    }
}
