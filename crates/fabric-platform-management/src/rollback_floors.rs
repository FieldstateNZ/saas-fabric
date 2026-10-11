//! The oldest version each component may be rolled back to (D01-21).

use std::collections::BTreeMap;

use crate::{ArtifactSource, History, PlatformError, Version};

/// Rollback floors, by the image repository a component pulls from.
///
/// A floor is a fact about the software, not about an environment. Below it,
/// a release misreads state that newer releases have already written. So it
/// is keyed by repository, which names the software, rather than by the
/// component name an environment's desired state happens to use.
///
/// Empty means no component has a floor. That is the honest answer for any
/// component that has not declared one. SaaS Fabric declares its own in
/// `compatibility.yaml`, and [`Self::for_this_release`] reads it from the
/// constants that file is held to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RollbackFloors {
    by_repository: BTreeMap<String, Version>,
}

impl RollbackFloors {
    /// These floors, plus `minimum` for every component that pulls from
    /// `repository`.
    #[must_use]
    pub fn with(mut self, repository: impl Into<String>, minimum: Version) -> Self {
        self.by_repository.insert(repository.into(), minimum);
        self
    }

    /// SaaS Fabric's own floor, as this binary was built with it.
    ///
    /// `None` only if the built-in floor is not a version, which a test on
    /// every pull request rules out. The composition root refuses to start
    /// rather than run without it.
    #[must_use]
    pub fn for_this_release() -> Option<Self> {
        let minimum = Version::parse(fabric_component::MINIMUM_ROLLABLE_VERSION)?;
        Some(Self::default().with(fabric_component::RUNTIME_REPOSITORY, minimum))
    }

    /// `history` without the releases below `source`'s floor, if it has one.
    #[must_use]
    pub(crate) fn cut(&self, source: &ArtifactSource, history: History) -> History {
        match self.of(source) {
            Some(floor) => history.at_or_above(floor),
            None => history,
        }
    }

    /// Refuses `version` of `component` if it is below `source`'s floor.
    ///
    /// A version that does not parse is left to the resolution that follows,
    /// which refuses it as not rollable.
    ///
    /// # Errors
    ///
    /// [`PlatformError::BelowRollbackFloor`] naming the floor.
    pub(crate) fn admit(
        &self,
        source: &ArtifactSource,
        component: &str,
        version: &str,
    ) -> Result<(), PlatformError> {
        match self.of(source) {
            Some(floor) if Version::parse_chart(version).is_some_and(|asked| asked < *floor) => {
                Err(PlatformError::BelowRollbackFloor {
                    component: component.to_owned(),
                    version: version.to_owned(),
                    floor: floor.as_str().to_owned(),
                })
            }
            _ => Ok(()),
        }
    }

    /// The floor for a component published from `source`: the highest floor
    /// among the repositories it pulls from, if any has one.
    ///
    /// A chart names no image repository, so it has no floor here.
    #[must_use]
    pub(crate) fn of(&self, source: &ArtifactSource) -> Option<&Version> {
        let repositories = match source {
            ArtifactSource::Oci { repositories } | ArtifactSource::Described { repositories, .. } => {
                repositories
            }
            ArtifactSource::Helm { .. } => return None,
        };
        repositories
            .values()
            .filter_map(|repository| self.by_repository.get(repository))
            .max()
    }
}

impl History {
    /// This listing without the releases below `floor`.
    ///
    /// `more` is cleared once the floor is reached, whether a release was cut
    /// or the oldest one listed is the floor itself: every older version is
    /// below it, so there is nothing further an operator could be offered.
    #[must_use]
    pub(crate) fn at_or_above(mut self, floor: &Version) -> Self {
        let listed = self.releases.len();
        self.releases.retain(|release| release.version() >= floor);
        let reached = self.releases.len() < listed
            || self
                .releases
                .last()
                .is_some_and(|oldest| oldest.version() == floor);
        self.more = self.more && !reached;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn images(repositories: &[(&str, &str)]) -> ArtifactSource {
        ArtifactSource::Oci {
            repositories: repositories
                .iter()
                .map(|(role, repository)| ((*role).to_owned(), (*repository).to_owned()))
                .collect(),
        }
    }

    #[test]
    fn this_release_floors_the_component_that_pulls_saas_fabrics_runtime_image() {
        let floors = RollbackFloors::for_this_release().unwrap();
        let fabric = images(&[("runtime", fabric_component::RUNTIME_REPOSITORY)]);

        assert_eq!(
            floors.of(&fabric),
            Some(&version(fabric_component::MINIMUM_ROLLABLE_VERSION))
        );
    }

    #[test]
    fn a_component_that_pulls_none_of_the_floored_repositories_has_no_floor() {
        let floors = RollbackFloors::for_this_release().unwrap();

        assert_eq!(floors.of(&images(&[("app", "ghcr.io/example/app")])), None);
        let chart = ArtifactSource::Helm {
            repository: "https://charts.example".to_owned(),
            chart: "saas-fabric".to_owned(),
        };
        assert_eq!(floors.of(&chart), None);
    }

    #[test]
    fn the_highest_floor_among_a_components_repositories_applies() {
        let floors = RollbackFloors::default()
            .with("ghcr.io/example/a", version("1.0.0-preview.3"))
            .with("ghcr.io/example/b", version("1.0.0-preview.10"));

        let both = images(&[("a", "ghcr.io/example/a"), ("b", "ghcr.io/example/b")]);

        assert_eq!(floors.of(&both), Some(&version("1.0.0-preview.10")));
    }
}
