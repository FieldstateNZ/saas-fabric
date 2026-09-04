//! Reading one component out of an environment's manifest.

use fabric_platform_management::{ArtifactSource, ComponentDesired, DesiredRevision, DesiredStateError};

use crate::PlatformGitRepository;

impl PlatformGitRepository {
    /// Reads one component out of the environment's manifest.
    ///
    /// Split out of the port so the operation budget wraps a plain future
    /// rather than a block that borrows every argument twice, and so `port.rs`
    /// stays a list of what each operation delegates to.
    pub(super) async fn read_component(
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
}
