//! Choosing the generation a registry is stamped with, so that no
//! generation is ever stamped on two different sets of bytes.

use super::{
    IssuerRegistration, IssuerRegistryDocument, RecordedRegistry, RegistryGeneration, RegistryRefusal,
};
use crate::digest::sha256_hex;
use crate::{DocumentKind, PublicationError};

/// The generation the offered registrations are stamped with. Never one
/// already stamped on different bytes, committed or not.
///
/// Two generations may be kept: the committed one, when the registrations
/// render to the digest the record names and no uncommitted registry is
/// ahead of it; and an uncommitted one an interrupted publication left on
/// disk, when the registrations render to exactly its bytes. Otherwise the
/// generation is one past the highest either shows. That keeps the held
/// file at or above every generation ever written, since nothing below it
/// is written, so an uncommitted generation is never reissued for other
/// registrations. Past `u64::MAX` the publication is refused.
pub(super) fn generation_for(
    recorded: Option<&RecordedRegistry>,
    held: Option<&[u8]>,
    offered: &[IssuerRegistration],
) -> Result<RegistryGeneration, PublicationError> {
    let held_generation = held.map(held_generation).transpose()?;
    let committed = recorded.map(|recorded| recorded.generation);
    let ahead = match (held_generation, committed) {
        (Some(held), Some(committed)) => (held > committed).then_some(held),
        (held, None) => held,
        (None, Some(_)) => None,
    };

    if let (Some(recorded), None) = (recorded, ahead) {
        if sha256_hex(&super::plan::render(recorded.generation, offered)?.1) == recorded.sha256 {
            return Ok(recorded.generation);
        }
    }
    if let (Some(generation), Some(held)) = (ahead, held) {
        if super::plan::render(generation, offered)?.1 == held {
            return Ok(generation);
        }
    }
    match held_generation.max(committed) {
        None => Ok(RegistryGeneration::FIRST),
        Some(highest) => highest
            .next()
            .ok_or_else(|| RegistryRefusal::GenerationExhausted { held: highest }.into()),
    }
}

/// The generation the held registry bytes are stamped with.
fn held_generation(held: &[u8]) -> Result<RegistryGeneration, PublicationError> {
    serde_json::from_slice::<IssuerRegistryDocument>(held)
        .map(|document| document.generation())
        .map_err(|cause| PublicationError::Unreadable {
            document: DocumentKind::Tenants,
            cause: Box::new(cause),
        })
}
