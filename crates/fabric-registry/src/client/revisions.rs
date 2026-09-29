//! Which revisions an image carries, and the verdict over several images'.

use std::collections::{BTreeMap, BTreeSet};

use fabric_platform_management::Provenance;

use crate::client::wire::Descriptor;

/// The label and annotation a build stamps its source commit into.
const REVISION: &str = "org.opencontainers.image.revision";

/// The revision in a map of labels or annotations, as a set of zero or one.
///
/// An empty value is no revision: a build that stamped `""` stamped nothing.
pub(in crate::client) fn revisions_in(values: Option<&BTreeMap<String, String>>) -> BTreeSet<String> {
    values
        .and_then(|values| values.get(REVISION))
        .map(|revision| revision.trim())
        .filter(|revision| !revision.is_empty())
        .map(ToOwned::to_owned)
        .into_iter()
        .collect()
}

/// Whether an index entry names a concrete runtime platform.
pub(super) fn deployable(entry: &Descriptor) -> bool {
    entry
        .platform
        .as_ref()
        .is_some_and(|platform| platform.os != "unknown" && platform.architecture != "unknown")
}

/// The verdict over each deployable image's set of revisions.
///
/// Order-independent: any image with more than one value, or two images
/// with different values, is [`Disagreed`](Provenance::Disagreed) — which
/// no waiting resolves, so it wins over an image with none, which is
/// [`Absent`](Provenance::Absent). No images at all is `Absent` too.
pub(super) fn verdict(images: &[BTreeSet<String>]) -> Provenance {
    let mut every = BTreeSet::new();
    let mut any_without = images.is_empty();

    for revisions in images {
        if revisions.len() > 1 {
            return Provenance::Disagreed;
        }
        any_without |= revisions.is_empty();
        every.extend(revisions.iter().cloned());
    }

    if every.len() > 1 {
        return Provenance::Disagreed;
    }

    match every.pop_first() {
        Some(revision) if !any_without => Provenance::Agreed(revision),
        _ => Provenance::Absent,
    }
}
