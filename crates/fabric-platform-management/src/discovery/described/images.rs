//! Steps 4 and 5 of the rule: every image exists, carries one commit and
//! the version, and the images and the component descriptor name one
//! commit.

use std::collections::{BTreeMap, BTreeSet};

use super::claimed::{complete, Claimed};
use super::{Evaluation, InvalidReason, RevisionOf};
use crate::{Provenance, Registry, RegistryError, Resolved};

/// Steps 4 and 5, in four passes, in ADR 0026 section 3's order, the first
/// failure deciding.
///
/// 1. Step 4: every image other than the primary exists at its digest —
///    else [`MissingImage`](InvalidReason::MissingImage).
/// 2. Step 4: every other image's repository still tags the version at that
///    digest — else [`Incoherent`](Evaluation::Incoherent).
/// 3. Step 5: every image, the others in role order and then the primary,
///    has exactly one revision — else
///    [`NoSingleRevision`](InvalidReason::NoSingleRevision).
/// 4. Step 5: the component descriptor names its revision — else
///    [`NoSingleRevision`](InvalidReason::NoSingleRevision) of it — and it
///    and every image name one commit — else
///    [`Incoherent`](Evaluation::Incoherent).
///
/// # Why existence outranks everything after it
///
/// An image that does not exist is reported before one that exists with no
/// single revision, whichever role sorts first, and before any commit is
/// compared. It is the more basic fact, and a missing image beside a
/// disagreeing commit is first of all a release with a hole in it.
///
/// # One after another, for now
///
/// The images other than the primary are asked sequentially, two reads
/// each. That is the obvious place to add concurrency when the rollback
/// listing's five-version bound is measured again on this path (ADR 0026
/// section 9): the reads are independent, and only the order in which
/// failures are *reported* has to stay the one above.
pub(super) async fn check(registry: &dyn Registry, claimed: Claimed) -> Result<Evaluation, RegistryError> {
    let images = &claimed.descriptor.spec().images;
    let others: Vec<_> = images
        .iter()
        .filter(|(role, _)| role.as_str() != claimed.primary)
        .collect();

    let mut found: BTreeMap<String, Resolved> = BTreeMap::new();
    for (role, image) in &others {
        let exists = registry
            .resolve(image.repository.as_str(), image.digest.as_str())
            .await?
            .filter(|resolved| resolved.digest == image.digest.as_str());
        let Some(resolved) = exists else {
            return Ok(Evaluation::Invalid(InvalidReason::MissingImage {
                role: role.as_str().to_owned(),
            }));
        };
        found.insert(role.as_str().to_owned(), resolved);
    }

    for (_, image) in &others {
        let tagged = registry
            .resolve(image.repository.as_str(), claimed.version.as_str())
            .await?;
        if tagged.is_none_or(|tagged| tagged.digest != image.digest.as_str()) {
            return Ok(Evaluation::Incoherent);
        }
    }

    let mut revisions = BTreeSet::new();
    let every = found
        .iter()
        .chain(std::iter::once((&claimed.primary, &claimed.primary_image)));
    for (role, resolved) in every {
        match &resolved.provenance {
            Provenance::Agreed(revision) if !revision.is_empty() => {
                revisions.insert(revision.clone());
            }
            Provenance::Agreed(_) | Provenance::Absent | Provenance::Disagreed => {
                return Ok(no_single_revision(RevisionOf::Image { role: role.clone() }));
            }
        }
    }

    let Some(own) = claimed.attached.revision.clone().filter(|own| !own.is_empty()) else {
        return Ok(no_single_revision(RevisionOf::ComponentDescriptor));
    };
    revisions.insert(own);

    // One commit, or it is not one release.
    let mut revisions = revisions.into_iter();
    let (Some(source_revision), None) = (revisions.next(), revisions.next()) else {
        return Ok(Evaluation::Incoherent);
    };

    Ok(Evaluation::Complete(Box::new(complete(
        claimed,
        found,
        source_revision,
    ))))
}

/// The answer for something that names no single commit.
fn no_single_revision(of: RevisionOf) -> Evaluation {
    Evaluation::Invalid(InvalidReason::NoSingleRevision { of })
}
