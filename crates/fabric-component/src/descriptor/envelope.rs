//! The component descriptor's outer shape -- `apiVersion`, `kind`, `spec` --
//! shared by the published document and its authored source.
use super::constants::{API_VERSION, KIND};
use crate::errors::{invalid, ContractError};
use serde::{Deserialize, Serialize};

/// The document as it is read: an envelope around a `spec` of type `S`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Read<S> {
    /// Checked by [`check`] before this is parsed; kept so the parse refuses
    /// nothing it names as unknown.
    #[allow(dead_code)]
    pub(super) api_version: String,
    /// As `api_version`.
    #[allow(dead_code)]
    pub(super) kind: String,
    /// The component itself.
    pub(super) spec: S,
}

/// The document as it is written. Field order is the envelope's canonical
/// order.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Written<'a, S> {
    /// Always [`API_VERSION`].
    pub(super) api_version: &'static str,
    /// Always [`KIND`].
    pub(super) kind: &'static str,
    /// The component itself.
    pub(super) spec: &'a S,
}

impl<'a, S> Written<'a, S> {
    /// Wraps `spec` in the v1 envelope.
    pub(super) fn wrapping(spec: &'a S) -> Self {
        Self {
            api_version: API_VERSION,
            kind: KIND,
            spec,
        }
    }
}

/// Refuses a document that is not a v1 component descriptor, *before* its
/// `spec` is parsed -- the discipline `fabric-client-model`'s catalogue
/// applies to its own envelope -- so the wrong document is refused as the
/// wrong document, and never as a component missing every field.
///
/// # Why a later version is its own answer
///
/// A `Component` of this group at another version is a component descriptor
/// this build does not read, and ADR 0026 section 2 has a reader say so,
/// naming the version it found; anything else is not a component descriptor
/// at all. The version is named as the part of `apiVersion` after the group,
/// `v2`, the spelling [`from_artifact`](super::ComponentDescriptor::from_artifact)
/// names an artifact type's version in too.
pub(super) fn check(api_version: Option<&str>, kind: Option<&str>) -> Result<(), ContractError> {
    if api_version == Some(API_VERSION) && kind == Some(KIND) {
        return Ok(());
    }
    let group = API_VERSION
        .rsplit_once('/')
        .map_or(API_VERSION, |(group, _)| group);
    if let (Some(found), Some(KIND)) = (api_version, kind) {
        let version = found
            .strip_prefix(group)
            .and_then(|rest| rest.strip_prefix('/'))
            .filter(|version| is_version(version));
        if let Some(version) = version {
            return Err(ContractError::UnsupportedVersion {
                found: version.to_owned(),
            });
        }
    }
    let found = if api_version.is_none() && kind.is_none() {
        "no apiVersion or kind at all".to_owned()
    } else {
        format!(
            "{}/{}",
            api_version.unwrap_or("(no apiVersion)"),
            kind.unwrap_or("(no kind)")
        )
    };
    Err(invalid(format!("expected {API_VERSION}/{KIND}, found {found}")))
}

/// A version as this group spells one: `v` and a number, without a leading
/// zero. Anything else after the group is not a version of this document.
fn is_version(text: &str) -> bool {
    text.strip_prefix('v').is_some_and(|number| {
        !number.is_empty()
            && number.bytes().all(|byte| byte.is_ascii_digit())
            && (number == "0" || !number.starts_with('0'))
    })
}
