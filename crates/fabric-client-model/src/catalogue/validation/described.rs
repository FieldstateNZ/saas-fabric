//! The rules a described component and its resolution keep, and the one
//! rule the content in effect keeps across authored and declared (ADR 0026
//! sections 7 and 8).
use super::invalid;
use crate::catalogue::{ApplicationComponent, ApplicationDefinition, ComponentKind};
use crate::DesiredStateError;
use std::collections::BTreeMap;

impl ApplicationDefinition {
    /// Checks every component's resolution against its kind and itself, then
    /// that no field key and no resource name is in effect twice.
    ///
    /// # Why on every read, and not only when the server writes one
    ///
    /// A resolution is what the server observed, frozen; nothing in it was
    /// typed by an operator. A hand edit that makes a component and its
    /// resolution disagree -- another reference, another version, a
    /// descriptor that no longer names the primary image -- would otherwise
    /// be believed, so it makes the catalogue unreadable instead.
    pub(super) fn validate_described(&self) -> Result<(), DesiredStateError> {
        for component in &self.components {
            resolution(component)?;
        }
        let declared = |select: fn(&fabric_component::ComponentSpec) -> Vec<&str>| {
            self.described()
                .map(move |(component, descriptor)| (component, select(descriptor.spec())))
                .collect::<Vec<_>>()
        };
        in_effect_once(
            "field",
            self.fields.iter().map(|field| field.key.as_str()),
            &declared(|spec| spec.fields.iter().map(|field| field.key.as_str()).collect()),
        )?;
        in_effect_once(
            "resource",
            self.resources.iter().map(|resource| resource.name.as_str()),
            &declared(|spec| {
                spec.resources
                    .iter()
                    .map(|resource| resource.name.as_str())
                    .collect()
            }),
        )
    }
}

/// A described component has a resolution that agrees with it and with its
/// own frozen descriptor; no other kind has one.
fn resolution(component: &ApplicationComponent) -> Result<(), DesiredStateError> {
    let id = &component.id;
    let resolution = match (component.kind, &component.resolution) {
        (ComponentKind::Described, Some(resolution)) => resolution,
        (ComponentKind::Described, None) => {
            return Err(invalid(format!(
                "Component '{id}' is described and has no resolution; select a version for it"
            )))
        }
        (ComponentKind::Container | ComponentKind::Helm | ComponentKind::Capability, None) => return Ok(()),
        (ComponentKind::Container | ComponentKind::Helm | ComponentKind::Capability, Some(_)) => {
            return Err(invalid(format!(
                "Component '{id}' is not described and cannot carry a resolution"
            )))
        }
    };
    let repository = &resolution.repository;
    let version = &resolution.version;
    if component.reference != repository.as_str() || component.version != version.as_str() {
        return Err(invalid(format!(
            "Component '{id}' names {} at {} but was resolved from {repository} at {version}",
            component.reference, component.version
        )));
    }
    let described = &resolution.descriptor.spec().version;
    if described != version {
        return Err(invalid(format!(
            "Component '{id}' was resolved at {version} but its component descriptor describes {described}"
        )));
    }
    if resolution
        .descriptor
        .role_of(repository, &resolution.primary_digest)
        .is_none()
    {
        return Err(invalid(format!(
            "Component '{id}''s component descriptor does not name {repository} at {}",
            resolution.primary_digest
        )));
    }
    Ok(fabric_component::check_revision(&resolution.revision)?)
}

/// Refuses a name in effect twice, naming it and both of its sources. The
/// authored names are unique already (`validate_fields`,
/// `validate_resources`), and so are one descriptor's own; what is left is
/// a name two sources share.
fn in_effect_once<'a>(
    label: &str,
    authored: impl Iterator<Item = &'a str>,
    declared: &[(&ApplicationComponent, Vec<&'a str>)],
) -> Result<(), DesiredStateError> {
    let mut sources: BTreeMap<&str, String> = authored
        .map(|name| (name, "authored on the application".to_owned()))
        .collect();
    for (component, names) in declared {
        for name in names {
            let source = format!("declared by component '{}'", component.id);
            if let Some(first) = sources.insert(name, source.clone()) {
                return Err(invalid(format!(
                    "Duplicate {label}: {name} is {first} and {source}"
                )));
            }
        }
    }
    Ok(())
}
