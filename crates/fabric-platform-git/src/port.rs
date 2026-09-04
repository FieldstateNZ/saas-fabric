//! Implementing the desired-state port over the platform repository.

use fabric_platform_management::{
    ArtifactSource, ComponentDesired, DesiredRevision, DesiredState, DesiredStateError, Hold, Release,
    ReleaseUnit,
};

use crate::{PlatformGitError, PlatformGitRepository};

mod wanted;

use wanted::{unit_from, wanted_from};

#[async_trait::async_trait]
impl DesiredState for PlatformGitRepository {
    async fn components(&self, environment: &str) -> Result<Vec<String>, DesiredStateError> {
        let manifest = self.components_manifest(environment).await?;

        Ok(manifest.components.keys().cloned().collect())
    }

    async fn component(
        &self,
        environment: &str,
        component: &str,
    ) -> Result<ComponentDesired, DesiredStateError> {
        let read = self.read_manifest(environment).await?;
        let manifest_revision = read.stored.revision;
        let manifest = read.document.manifest;

        let entry = manifest
            .components
            .get(component)
            .ok_or_else(|| DesiredStateError::NotFound {
                what: format!("{component} in {environment}"),
            })?;

        // A version the manifest carries that this cannot parse is a refusal,
        // not a default. Guessing would mean deciding what to advance *from*
        // on the strength of something nobody wrote deliberately. Which
        // grammar applies is the artifact's to say — see `Artifact::parse_version`.
        let version = entry
            .artifact
            .parse_version(&entry.desired.version)
            .ok_or_else(|| DesiredStateError::Refused {
                detail: format!("{component} in {environment} is at a version this cannot read"),
            })?;

        Ok(ComponentDesired {
            revision: DesiredRevision::new(manifest_revision.as_str()),
            version,
            channel: entry.channel,
            policy: entry.update,
            hold: entry.hold.clone(),
            source: match &entry.artifact {
                crate::Artifact::Oci { images, .. } => ArtifactSource::Oci {
                    repositories: images
                        .iter()
                        .map(|(role, image)| (role.clone(), image.repository.clone()))
                        .collect(),
                },
                crate::Artifact::Helm { repository, chart } => ArtifactSource::Helm {
                    repository: repository.clone(),
                    chart: chart.clone(),
                },
            },
        })
    }

    async fn advance(
        &self,
        environment: &str,
        component: &str,
        release: &Release,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.set_component_desired_state(environment, component, &wanted_from(release), at, message)
            .await?;

        Ok(())
    }

    async fn roll_back(
        &self,
        environment: &str,
        component: &str,
        unit: &ReleaseUnit,
        hold: &Hold,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.roll_back_component(environment, component, &unit_from(unit), hold, at, message)
            .await?;

        Ok(())
    }

    async fn pause(
        &self,
        environment: &str,
        component: &str,
        hold: &Hold,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.set_component_hold(environment, component, Some(hold), at, message)
            .await?;

        Ok(())
    }

    async fn resume(
        &self,
        environment: &str,
        component: &str,
        at: &DesiredRevision,
        message: &str,
    ) -> Result<(), DesiredStateError> {
        self.set_component_hold(environment, component, None, at, message)
            .await?;

        Ok(())
    }
}

/// Maps this adapter's failures into the port's vocabulary.
///
/// A free function's worth of translation, and the distinctions that matter
/// survive it. `Conflict` in particular has to: it is not a failure of the
/// component but an instruction to decide again, and a caller that could not
/// tell it from an outage would either retry forever or give up on a race it
/// was always going to lose once.
impl From<PlatformGitError> for DesiredStateError {
    fn from(error: PlatformGitError) -> Self {
        match error {
            PlatformGitError::Conflict { .. } => Self::Conflict,
            PlatformGitError::Contended => Self::Unavailable {
                detail: "the platform repository is busy".to_owned(),
            },
            PlatformGitError::NotFound { what } => Self::NotFound { what },
            PlatformGitError::NotPermitted => Self::Refused {
                detail: "the platform repository refused the platform's credential".to_owned(),
            },
            PlatformGitError::Unavailable { detail } => Self::Unavailable { detail },
            PlatformGitError::Rejected { detail } => Self::Refused { detail },
        }
    }
}
