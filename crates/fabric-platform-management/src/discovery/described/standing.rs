//! Step 4's two passes over the images other than the primary: asked all
//! at once, answered in the rule's order, and stopped the moment the answers
//! in so far decide (`images.rs` runs the passes after them).

use std::collections::BTreeMap;

use fabric_component::{ImageReference, Role};

use super::together::{together, Read};
use super::{Evaluation, InvalidReason};
use crate::{Registry, RegistryError, Resolved};

/// Passes 1 and 2: what each image other than the primary resolved to, by
/// role — or the evaluation the first failure decides.
///
/// # Every read at once, and every answer in the rule's order
///
/// The images other than the primary are asked concurrently: for each, by
/// digest and by the version tag, every read started before any answer is
/// looked at (`together`). The answers are read in the rule's order —
/// every read by digest, then every read by tag — the first failure deciding, exactly as if each had been asked one after
/// another — and the moment the answers in so far decide, the reads still
/// out are dropped. So a read that hangs cannot hold back an answer the
/// rule already has: an image found missing is *invalid* at once, as it was
/// asked one read at a time, and not a deadline reached. What concurrency
/// can still change is which reads were made: a read the sequential rule
/// would never have made — the tag of an image already found missing — may
/// have answered first, and its answer is ignored, a registry error in it
/// too, because the rule reads it only after everything before it.
pub(super) async fn found(
    registry: &dyn Registry,
    others: &[(&Role, &ImageReference)],
    version: &str,
) -> Result<Result<BTreeMap<String, Resolved>, Evaluation>, RegistryError> {
    let digests: Vec<&str> = others.iter().map(|(_, image)| image.digest.as_str()).collect();
    // The port's futures are already boxed, `Send`, and borrow only what
    // this function holds, which is the shape `together` takes.
    let by_digest = others.iter().map(|(_, image)| -> Read<'_, _> {
        registry.resolve(image.repository.as_str(), image.digest.as_str())
    });
    let by_tag = others
        .iter()
        .map(|(_, image)| -> Read<'_, _> { registry.resolve(image.repository.as_str(), version) });
    let reads = by_digest.chain(by_tag).collect();
    let settled = |so_far: &[Option<Answer>]| !matches!(standing(&digests, so_far), Standing::Waiting);
    let answers = together(reads, settled).await;

    match standing(&digests, &answers) {
        Standing::Failed(index) => {
            if let Some(Some(Err(error))) = answers.into_iter().nth(index) {
                return Err(error);
            }
            // Reads by digest come first: a failure among them is an image
            // that does not exist; one among the reads by tag, a tag that
            // does not point at the bytes the component descriptor names.
            Ok(Err(match others.get(index) {
                Some((role, _)) => Evaluation::Invalid(InvalidReason::MissingImage {
                    role: role.as_str().to_owned(),
                }),
                None => Evaluation::Incoherent,
            }))
        }
        Standing::Passed => Ok(Ok(others
            .iter()
            .zip(answers)
            .filter_map(|((role, _), answer)| match answer {
                Some(Ok(Some(resolved))) => Some((role.as_str().to_owned(), resolved)),
                Some(Ok(None) | Err(_)) | None => None,
            })
            .collect())),
        // `together` returns once every read has answered or the answers
        // settle, so this is never the standing it returns with; were it,
        // the reads were abandoned, and that is said, not guessed past.
        Standing::Waiting => Err(RegistryError::Unavailable {
            detail: "an image read was abandoned before it answered".to_owned(),
        }),
    }
}

/// One read's answer: what the registry resolved the reference to, if
/// anything.
type Answer = Result<Option<Resolved>, RegistryError>;

/// Where passes 1 and 2 stand on the answers in so far.
enum Standing {
    /// A read the rule looks at before any failure seen so far is still out.
    Waiting,

    /// The read at this index is the first, in the rule's order, that did
    /// not resolve to the digest the component descriptor names — or could
    /// not be answered.
    Failed(usize),

    /// Every read resolved to the digest the component descriptor names.
    Passed,
}

/// Passes 1 and 2 over `answers`, read in the rule's order: the reads by
/// digest, then the reads by tag, one of each per image in `digests`' order,
/// so the reads' expected digests are `digests` twice over.
fn standing(digests: &[&str], answers: &[Option<Answer>]) -> Standing {
    for (index, (answer, expected)) in answers.iter().zip(digests.iter().cycle()).enumerate() {
        let Some(answer) = answer else {
            return Standing::Waiting;
        };
        if !matches!(answer, Ok(Some(resolved)) if resolved.digest == *expected) {
            return Standing::Failed(index);
        }
    }
    Standing::Passed
}
