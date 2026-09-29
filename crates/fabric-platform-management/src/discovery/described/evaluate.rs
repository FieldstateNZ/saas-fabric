//! Steps 1 to 3 of the rule: the tag, the one component descriptor, and
//! what it says.

use fabric_component::{ComponentDescriptor, ContractError, Digest, Repository};

use super::claimed::Claimed;
use super::images;
use super::{expectation, Evaluation, Expectation, InvalidReason};
use crate::{Attached, Registry, RegistryError, Unusable, Version};

/// Decides whether `version` of the component whose primary image is
/// published to `repository` is a release unit (ADR 0026 section 3).
///
/// In order, the first failure deciding the answer:
///
/// 1. `repository`'s tag `version` resolves to a digest — if not, the answer
///    is [`NotTagged`](Evaluation::NotTagged);
/// 2. exactly one component descriptor is attached to that digest — none is
///    [`Undescribed`](Evaluation::Undescribed), more than one is
///    [`Several`](InvalidReason::Several), one the adapter could not use is
///    [`Unreadable`](InvalidReason::Unreadable);
/// 3. it is a component descriptor of a version this build reads
///    ([`UnsupportedVersion`](InvalidReason::UnsupportedVersion)), with
///    every image on one registry
///    ([`OtherRegistry`](InvalidReason::OtherRegistry) — step 4's rule, but
///    one the document's own version refuses, so it is found here), or
///    [`Unreadable`](InvalidReason::Unreadable) for anything else wrong with
///    it; its `spec.version` and its `version` annotation, when present, are
///    `version` byte for byte ([`WrongVersion`](InvalidReason::WrongVersion));
///    it names the image it is attached to, at `repository` and that digest,
///    as one of its roles ([`PrimaryNotNamed`](InvalidReason::PrimaryNotNamed));
///    and it is what the `expectation` says the component must be
///    ([`NotPinned`](InvalidReason::NotPinned) or
///    [`NotRegistered`](InvalidReason::NotRegistered));
/// 4. and 5. every image exists, carries one commit and the version, and
///    the images and the component descriptor name one commit — see
///    `images::check`.
///
/// Nothing about an answer is remembered: every call asks again.
///
/// # Errors
///
/// [`RegistryError`] if the registry could not be asked. That is never an
/// answer: a rate limit must not read as *undescribed*, and an image that
/// cannot be read is not an image that does not exist.
pub async fn evaluate(
    registry: &dyn Registry,
    repository: &str,
    version: &str,
    expectation: Expectation<'_>,
) -> Result<Evaluation, RegistryError> {
    let Some(primary_image) = registry.resolve(repository, version).await? else {
        return Ok(Evaluation::NotTagged);
    };

    let attached = match registry
        .component_descriptor(repository, &primary_image.digest)
        .await?
    {
        Attached::Nothing => return Ok(Evaluation::Undescribed),
        Attached::One(attached) => attached,
        Attached::Several { .. } => return Ok(Evaluation::Invalid(InvalidReason::Several)),
        // Written out rather than caught together, so a new way of being
        // unusable is a decision here and not an inheritance.
        Attached::Unusable {
            reason: Unusable::NotAnIndex | Unusable::OtherSubject | Unusable::Malformed,
        } => return Ok(Evaluation::Invalid(InvalidReason::Unreadable)),
    };

    let descriptor = match ComponentDescriptor::from_artifact(&attached.artifact_type, &attached.document) {
        Ok(descriptor) => descriptor,
        Err(ContractError::UnsupportedVersion { found }) => {
            return Ok(Evaluation::Invalid(InvalidReason::UnsupportedVersion { found }));
        }
        Err(ContractError::OtherRegistry { .. }) => {
            return Ok(Evaluation::Invalid(InvalidReason::OtherRegistry))
        }
        Err(ContractError::Invalid { .. }) => return Ok(Evaluation::Invalid(InvalidReason::Unreadable)),
    };

    // One digest has one version. A version this platform cannot order is
    // not the version it was asked about either: the descriptor's grammar
    // is the stricter, so this only refuses a number too large to compare.
    let names_version = descriptor.spec().version.as_str() == version
        && attached
            .version
            .as_deref()
            .is_none_or(|annotated| annotated == version);
    let Some(parsed) = Version::parse(version).filter(|_| names_version) else {
        return Ok(Evaluation::Invalid(InvalidReason::WrongVersion));
    };

    // The document does not name its primary: the digest it is attached to
    // does, and the document must name that image as one of its own.
    let primary = Repository::try_new(repository)
        .ok()
        .zip(Digest::try_new(&primary_image.digest).ok())
        .and_then(|(repository, digest)| descriptor.role_of(&repository, &digest).cloned());
    let Some(primary) = primary else {
        return Ok(Evaluation::Invalid(InvalidReason::PrimaryNotNamed));
    };

    if let Some(reason) = expectation::check(&descriptor, &primary, expectation) {
        return Ok(Evaluation::Invalid(reason));
    }

    images::check(
        registry,
        Claimed {
            version: parsed,
            primary: primary.as_str().to_owned(),
            primary_image,
            descriptor,
            attached,
        },
    )
    .await
}
