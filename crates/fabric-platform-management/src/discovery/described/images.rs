//! Steps 4 and 5 of the rule: every image exists, carries one commit and
//! the version, and the images and the component descriptor name one
//! commit.

use std::collections::BTreeSet;

use fabric_component::check_revision;

use super::claimed::{complete, Claimed};
use super::standing::found;
use super::{Evaluation, InvalidReason, RevisionOf};
use crate::{Provenance, Registry, RegistryError};

/// Steps 4 and 5, in four passes, in ADR 0026 section 3's order, the first
/// failure deciding.
///
/// 1. Step 4: every image other than the primary exists at its digest —
///    else [`MissingImage`](InvalidReason::MissingImage).
/// 2. Step 4: every other image's repository still tags the version at that
///    digest — else [`Incoherent`](Evaluation::Incoherent).
/// 3. Step 5: every image, the others in role order and then the primary,
///    has exactly one revision, and it is text a catalogue can record
///    ([`check_revision`]) — else
///    [`NoSingleRevision`](InvalidReason::NoSingleRevision).
/// 4. Step 5: the component descriptor names its revision, held to the
///    same rule — else
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
/// The images other than the primary are asked concurrently, and the
/// answers read in the rule's order: see [`found`].
///
/// Dropping the evaluation drops every read still in flight, which is what
/// lets the catalogue bound a selection by a deadline (ADR 0026 section 7).
pub(super) async fn check(registry: &dyn Registry, claimed: Claimed) -> Result<Evaluation, RegistryError> {
    let images = &claimed.descriptor.spec().images;
    let others: Vec<_> = images
        .iter()
        .filter(|(role, _)| role.as_str() != claimed.primary)
        .collect();
    let found = match found(registry, &others, claimed.version.as_str()).await? {
        Ok(found) => found,
        Err(decided) => return Ok(decided),
    };

    let mut revisions = BTreeSet::new();
    let every = found
        .iter()
        .chain(std::iter::once((&claimed.primary, &claimed.primary_image)));
    for (role, resolved) in every {
        match &resolved.provenance {
            Provenance::Agreed(revision) if check_revision(revision).is_ok() => {
                revisions.insert(revision.clone());
            }
            Provenance::Agreed(_) | Provenance::Absent | Provenance::Disagreed => {
                return Ok(no_single_revision(RevisionOf::Image { role: role.clone() }));
            }
        }
    }

    let Some(own) = claimed
        .attached
        .revision
        .clone()
        .filter(|own| check_revision(own).is_ok())
    else {
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
