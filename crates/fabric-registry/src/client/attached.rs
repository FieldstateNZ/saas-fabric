//! Finding the component descriptor attached to an image digest (ADR 0026
//! section 4).
//!
//! # Why the document is not parsed here
//!
//! Whether a document is a component descriptor Fabric reads is
//! `fabric-component`'s question, answered in the domain. This adapter takes
//! only the family's name and its size bound from that crate: enough to
//! choose which referrers to fetch and how much of one to read, and nothing
//! that would make it a second reader of the contract.

mod candidate;
mod document;
mod listing;
mod shape;
#[cfg(test)]
mod shape_tests;
mod tag_schema;

use std::collections::BTreeSet;

use fabric_platform_management::{Attached, RegistryError, Unusable};

use self::candidate::Candidate;
use self::tag_schema::TagSchema;
use crate::client::digest::sha256;
use crate::client::wire::Descriptor;
use crate::client::{bounds, OciRegistry};

impl OciRegistry {
    /// What is attached to `subject`, by the rules
    /// [`Attached`] documents.
    ///
    /// # Errors
    ///
    /// [`RegistryError`] if the registry could not be asked, if the
    /// repository does not exist, if `subject` is not a `sha256` digest, or
    /// if bytes did not hash to their digest.
    pub(super) async fn attached(&self, repository: &str, subject: &str) -> Result<Attached, RegistryError> {
        let subject = sha256(subject, "a component descriptor's subject")?;

        let mut listed: Vec<Descriptor> = self.referrers_api(repository, subject).await?.unwrap_or_default();
        match self.referrers_tag(repository, subject).await? {
            TagSchema::Nothing => {}
            TagSchema::Listed(entries) => listed.extend(entries),
            TagSchema::NotAnIndex => {
                return Ok(Attached::Unusable {
                    reason: Unusable::NotAnIndex,
                })
            }
        }

        // Merged by digest, and kept by family whatever a registry says it
        // filtered.
        let candidates: BTreeSet<String> = listed
            .into_iter()
            .filter(|entry| {
                entry
                    .artifact_type
                    .as_deref()
                    .and_then(fabric_component::family_version)
                    .is_some()
            })
            .map(|entry| entry.digest)
            .collect();

        if candidates.len() > bounds::CANDIDATES {
            // Never fetched, so none of these digests was computed here, and
            // none is carried.
            return Ok(Attached::Several { digests: Vec::new() });
        }

        let mut attached = Vec::new();
        for digest in &candidates {
            match self.candidate(repository, subject, digest).await? {
                Candidate::Missing => {}
                Candidate::Unusable(reason) => return Ok(Attached::Unusable { reason }),
                Candidate::Usable(checked) => attached.push(checked),
            }
        }

        match attached.len() {
            0 => Ok(Attached::Nothing),
            1 => match attached.pop() {
                Some(checked) => self.document(repository, checked).await,
                None => Ok(Attached::Nothing),
            },
            _ => Ok(Attached::Several {
                digests: attached.into_iter().map(|checked| checked.digest).collect(),
            }),
        }
    }
}
