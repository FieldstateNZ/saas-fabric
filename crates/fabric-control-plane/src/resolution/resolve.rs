//! Applying the release-unit rule to a selection, under its deadline.
//!
//! In the 121–150 line band: the rule's every answer is mapped here, in
//! one exhaustive match, beside the resolution a complete one becomes.

use std::collections::BTreeSet;

use fabric_client_model::catalogue::ComponentResolution;
use fabric_component::{ComponentVersion, Digest, Repository};
use fabric_platform_management::{evaluate, DescribedRelease, Evaluation, Expectation, RegistryError};

use crate::resolution::{ResolutionService, SelectionRefusal, Unusable};
use crate::{ControlPlaneError, RegistryFailure};

impl ResolutionService {
    /// Resolves `version` of the component whose primary image is
    /// published to `repository`, by ADR 0026 section 3's rule with the
    /// catalogue's expectation: every repository the component descriptor
    /// names is registered.
    ///
    /// In order, all under the budget: `repository` must be registered and
    /// its registry read through, refused before any registry is asked;
    /// then the rule runs; then a complete answer becomes the resolution,
    /// stamped with when. The registry store's read is inside the budget
    /// too, so the budget bounds everything between the catalogue's read and
    /// its write, which is what startup's sum assumes.
    ///
    /// # Errors
    ///
    /// - [`SelectionRefusal::RepositoryNotRegistered`] for an unregistered
    ///   primary repository;
    /// - [`SelectionRefusal::VersionNotFound`] for a tag that does not
    ///   resolve;
    /// - [`SelectionRefusal::Unusable`] for *undescribed*, *incoherent* or
    ///   *invalid*, a sibling repository that is not registered included;
    /// - [`RegistryFailure::Refused`] when a registry refused a read or its
    ///   credential, [`RegistryFailure::Unavailable`] when it could not be
    ///   asked, is not being read through yet, or the budget ran out, and
    ///   the registry store's failures.
    pub(crate) async fn resolve(
        &self,
        repository: &Repository,
        version: &ComponentVersion,
    ) -> Result<ComponentResolution, ControlPlaneError> {
        let resolving = async {
            let Some(registered) = self.registries.registered_for(repository).await? else {
                return Err(ControlPlaneError::from(
                    SelectionRefusal::RepositoryNotRegistered {
                        repository: repository.to_string(),
                    },
                ));
            };
            let names: BTreeSet<String> = registered.iter().map(ToString::to_string).collect();
            let is_registered = move |candidate: &str| names.contains(candidate);
            let expectation = Expectation::Registered(&is_registered);
            evaluate(
                self.registry.as_ref(),
                repository.as_str(),
                version.as_str(),
                expectation,
            )
            .await
            .map_err(|error| reading(&error).into())
        };
        // Dropping `resolving` at the deadline drops every read in flight.
        let evaluation = tokio::time::timeout(self.budget, resolving).await.map_err(|_| {
            RegistryFailure::Unavailable(format!(
                "{repository} {version} was not resolved within the {}-second resolution budget",
                self.budget.as_secs()
            ))
        })??;

        let unusable = |answer| SelectionRefusal::Unusable {
            repository: repository.to_string(),
            version: version.to_string(),
            answer,
        };
        match evaluation {
            Evaluation::Complete(release) => {
                resolution(*release, repository, version, self.clock.now_unix_seconds())
            }
            Evaluation::NotTagged => Err(SelectionRefusal::VersionNotFound {
                repository: repository.to_string(),
                version: version.to_string(),
            }
            .into()),
            Evaluation::Undescribed => Err(unusable(Unusable::Undescribed).into()),
            Evaluation::Incoherent => Err(unusable(Unusable::Incoherent).into()),
            Evaluation::Invalid(reason) => Err(unusable(Unusable::Invalid(reason)).into()),
        }
    }
}

/// A registry that could not answer, as the registry routes word it: a
/// refusal of the request or of its credential is `502`, not retried; a
/// registry that could not be asked is `503`.
fn reading(error: &RegistryError) -> RegistryFailure {
    match error {
        RegistryError::Denied { .. } | RegistryError::Refused { .. } => {
            RegistryFailure::Refused(error.to_string())
        }
        RegistryError::Unavailable { .. } => RegistryFailure::Unavailable(error.to_string()),
    }
}

/// What the catalogue records of a complete release unit.
///
/// # Errors
///
/// [`RegistryFailure::Refused`] if the adapter reported a digest that is not
/// `sha256` — it computes every digest it reports, so this is an adapter
/// that broke its port's promise, and nothing it said is recorded.
fn resolution(
    release: DescribedRelease,
    repository: &Repository,
    version: &ComponentVersion,
    at: u64,
) -> Result<ComponentResolution, ControlPlaneError> {
    let DescribedRelease {
        unit,
        primary,
        descriptor_digest,
        descriptor,
    } = release;
    let primary_digest = unit
        .images
        .get(&primary)
        .and_then(|image| Digest::try_new(&image.digest).ok());
    let (Some(primary_digest), Ok(descriptor_digest)) = (primary_digest, Digest::try_new(&descriptor_digest))
    else {
        return Err(RegistryFailure::Refused(
            "the registry adapter reported a digest that is not sha256".to_owned(),
        )
        .into());
    };
    Ok(ComponentResolution {
        repository: repository.clone(),
        version: version.clone(),
        primary_digest,
        descriptor_digest,
        revision: unit.source_revision,
        resolved_at: at,
        descriptor,
    })
}
