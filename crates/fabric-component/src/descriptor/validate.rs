//! The rules a component descriptor's `spec` satisfies beyond its types.
use super::constants::{MAX_DOCUMENT_BYTES, MAX_FIELDS, MAX_IMAGES, MAX_RESOURCES};
use super::format_characters::refuse_format_characters;
use super::{envelope, ComponentSpec};
use crate::errors::{invalid, ContractError};
use crate::validation::{text, unique, validate_fields, validate_resources};

/// Checks `spec` against every rule of ADR 0026 section 2, and last that
/// its canonical bytes fit [`MAX_DOCUMENT_BYTES`].
pub(super) fn validate(spec: &ComponentSpec) -> Result<(), ContractError> {
    text(&spec.title, "Component title", true, 128)?;
    text(&spec.description, "Component description", false, 2048)?;
    refuse_format_characters(&spec.title, "Component title")?;
    refuse_format_characters(&spec.description, "Component description")?;
    images(spec)?;
    unique(
        spec.images.values().map(|image| image.repository.as_str()),
        "image repository",
    )?;
    unique(
        spec.capabilities.iter().map(|capability| capability.as_str()),
        "capability",
    )?;
    if spec.fields.len() > MAX_FIELDS {
        return Err(invalid(format!(
            "A component declares at most {MAX_FIELDS} fields"
        )));
    }
    validate_fields(&spec.fields)?;
    declared_text(spec)?;
    if spec.resources.len() > MAX_RESOURCES {
        return Err(invalid(format!(
            "A component declares at most {MAX_RESOURCES} resources"
        )));
    }
    validate_resources(&spec.resources)?;
    canonical_size(spec)
}

/// The canonical bytes fit [`MAX_DOCUMENT_BYTES`].
///
/// # Why the canonical form, and not the bytes that were read
///
/// A reader bounds what it is given before parsing it, but the canonical
/// form writes every default an author may omit -- a resource's `keyField`,
/// `operations` and `queryableFields`, an empty `description` -- so a
/// document within the bound as read can be over it as written. Checking
/// the canonical size here, where every descriptor is made, means every
/// [`ComponentDescriptor`](super::ComponentDescriptor) has bytes that its own
/// `from_json` reads back.
fn canonical_size(spec: &ComponentSpec) -> Result<(), ContractError> {
    let size = serde_json::to_vec(&envelope::Written::wrapping(spec))
        .as_ref()
        .map_or(0, Vec::len);
    if size > MAX_DOCUMENT_BYTES {
        return Err(invalid(format!(
            "A component descriptor's canonical form is {size} bytes; at most {MAX_DOCUMENT_BYTES} are read"
        )));
    }
    Ok(())
}

/// Between one and [`MAX_IMAGES`] images, all on one registry: the same
/// host, port included.
fn images(spec: &ComponentSpec) -> Result<(), ContractError> {
    if spec.images.is_empty() || spec.images.len() > MAX_IMAGES {
        return Err(invalid(format!(
            "A component names between 1 and {MAX_IMAGES} images"
        )));
    }
    let mut hosts = spec.images.values().map(|image| image.repository.host());
    let first = hosts.next().unwrap_or_default();
    if let Some(other) = hosts.find(|host| *host != first) {
        return Err(ContractError::OtherRegistry {
            detail: format!("Every image of a component is on one registry: {first} and {other} are two"),
        });
    }
    Ok(())
}

/// No format character in any text a declared field shows an operator or a
/// client.
fn declared_text(spec: &ComponentSpec) -> Result<(), ContractError> {
    for field in &spec.fields {
        refuse_format_characters(&field.label, "Field label")?;
        refuse_format_characters(&field.description, "Field description")?;
        for option in &field.options {
            refuse_format_characters(option, "Choice")?;
        }
        if let Some(default) = &field.default {
            refuse_format_characters(default, "Field default")?;
        }
    }
    Ok(())
}
