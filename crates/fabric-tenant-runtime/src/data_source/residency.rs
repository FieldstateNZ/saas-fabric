//! Where a DataSource's data physically lives.

/// The region and jurisdiction a DataSource sits in.
///
/// Data residency is a contractual and regulatory property, not a performance
/// hint. Recording it on the DataSource means it can be reported, audited, and
/// checked by reconciliation when placing a tenant — a tenant with an
/// `au-only` requirement must not be bound to a DataSource in `us-west`, and
/// that check needs somewhere to read the answer from.
///
/// The runtime does not enforce residency: by the time a binding exists the
/// placement decision has been made, and re-deciding it per request would be
/// both wasteful and too late. It exposes the value for telemetry and for the
/// admin surface.
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
