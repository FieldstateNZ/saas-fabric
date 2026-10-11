//! Deciding the registry generation a publication carries.

use std::collections::BTreeSet;

use fabric_core::TenantId;

use super::{
    IssuerRegistration, IssuerRegistryDocument, RecordedRegistry, RegistryGeneration, RegistryRefusal,
};
use crate::digest::sha256_hex;
use crate::{DocumentKind, DocumentOutcome, PublicationError, RuntimeSnapshot};

/// The registry's decided fate in one publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryPlan {
    /// Whether the adapter must write it: whenever the held bytes differ
    /// from these, including after a publication interrupted between the
    /// registry and the record.
    pub outcome: DocumentOutcome,
    /// The registry this publication leaves published.
    pub document: IssuerRegistryDocument,
    /// Its canonical bytes.
    pub bytes: Vec<u8>,
}

impl RegistryPlan {
    /// What the publication record names this registry by.
    #[must_use]
    pub fn recorded(&self) -> RecordedRegistry {
        RecordedRegistry {
            generation: self.document.generation(),
            sha256: sha256_hex(&self.bytes),
        }
    }
}

/// Decides the registry generation for `snapshot`'s registry, against the
/// generation the held record names and the registry bytes actually held.
///
/// The generation stays where the record has it while the registrations
/// are the ones it recorded, and otherwise advances by one -- including
/// when the held bytes are an interrupted publication's, because the record
/// still names the generation readers hold. With no recorded registry, the
/// first is generation 1.
///
/// # Errors
///
/// A [`RegistryRefusal`]: a duplicate issuer, a tenant the snapshot's
/// tenants document does not bind, or no registry offered over one that is
/// recorded.
pub(crate) fn decide(
    recorded: Option<&RecordedRegistry>,
    held: Option<&[u8]>,
    snapshot: &RuntimeSnapshot,
) -> Result<Option<RegistryPlan>, PublicationError> {
    let Some(offered) = snapshot.registry.as_deref() else {
        return match recorded {
            Some(recorded) => Err(RegistryRefusal::Withdrawn {
                held: recorded.generation,
            }
            .into()),
            None => Ok(None),
        };
    };

    let bound: BTreeSet<&TenantId> = snapshot
        .tenants
        .payload
        .iter()
        .map(|binding| &binding.tenant)
        .collect();
    if let Some(unbound) = offered
        .iter()
        .find(|registration| !bound.contains(&registration.tenant))
    {
        return Err(RegistryRefusal::TenantUnbound {
            issuer: unbound.issuer.clone(),
            tenant: unbound.tenant.clone(),
        }
        .into());
    }

    let (document, bytes) = match recorded {
        Some(recorded) => {
            let (document, bytes) = render(recorded.generation, offered)?;
            if sha256_hex(&bytes) == recorded.sha256 {
                (document, bytes)
            } else {
                render(recorded.generation.next(), offered)?
            }
        }
        None => render(RegistryGeneration::FIRST, offered)?,
    };
    let outcome = if held == Some(bytes.as_slice()) {
        DocumentOutcome::Unchanged
    } else {
        DocumentOutcome::Written
    };
    Ok(Some(RegistryPlan {
        outcome,
        document,
        bytes,
    }))
}

fn render(
    generation: RegistryGeneration,
    offered: &[IssuerRegistration],
) -> Result<(IssuerRegistryDocument, Vec<u8>), PublicationError> {
    let document = IssuerRegistryDocument::new(generation, offered.to_vec())?;
    let bytes = document
        .canonical_json()
        .map_err(|cause| PublicationError::Unwritable {
            document: DocumentKind::Tenants,
            cause: Box::new(cause),
        })?;
    Ok((document, bytes))
}
