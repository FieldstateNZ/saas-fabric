//! The evidence that turns saved intent into a usable client (D01-19a).

/// One condition a client must meet before it is usable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadinessCondition {
    /// Every logical data source the client declares is placed.
    Provisioning,
    /// The client's identity is applied at its current revision.
    Identity,
    /// The client's tenant binding is in the published runtime documents at
    /// the revision its placements add up to.
    Publication,
}

/// What is known about one condition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evidence {
    /// The condition is met.
    Met,
    /// It is not met, or nothing shows that it is. The detail says which.
    Unmet(String),
}

/// A condition that is not met, and why.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unmet {
    /// Which condition.
    pub condition: ReadinessCondition,
    /// Why, in an operator's words.
    pub detail: String,
}

/// The evidence for all three conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Readiness {
    unmet: Vec<Unmet>,
}

impl Readiness {
    /// Gathers the evidence for each condition.
    #[must_use]
    pub fn new(provisioning: Evidence, identity: Evidence, publication: Evidence) -> Self {
        let unmet = [
            (ReadinessCondition::Provisioning, provisioning),
            (ReadinessCondition::Identity, identity),
            (ReadinessCondition::Publication, publication),
        ]
        .into_iter()
        .filter_map(|(condition, evidence)| match evidence {
            Evidence::Met => None,
            Evidence::Unmet(detail) => Some(Unmet { condition, detail }),
        })
        .collect();

        Self { unmet }
    }

    /// Readiness nobody has assessed. Every condition is unmet, so a caller
    /// that did not gather evidence can never conclude a client is usable.
    #[must_use]
    pub fn unassessed() -> Self {
        let unassessed = || Evidence::Unmet("not assessed".to_owned());
        Self::new(unassessed(), unassessed(), unassessed())
    }

    /// The conditions that are not met, in a fixed order.
    #[must_use]
    pub fn unmet(&self) -> &[Unmet] {
        &self.unmet
    }

    pub(super) fn into_unmet(self) -> Vec<Unmet> {
        self.unmet
    }
}
