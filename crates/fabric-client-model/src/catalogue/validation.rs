//! Structural validation shared by persisted and submitted product definitions.
//!
//! The rules for the shapes a component descriptor also declares -- text,
//! uniqueness, configuration fields, application resources -- live in
//! `fabric-component` (ADR 0026 section 2) and are called from here, with
//! their messages unchanged. What only the catalogue can check stays here.
mod application;
mod described;
mod fields;
mod hostname;
mod resource;
use super::{Catalogue, ConfigurationValues};
use crate::DesiredStateError;
use fields::check_key;
pub(super) use fields::{is_timezone, validate_fields, values};
pub(super) use resource::check_cross_application_conflicts;

pub(crate) fn invalid(detail: impl Into<String>) -> DesiredStateError {
    DesiredStateError::InvalidField {
        field: "catalogue",
        detail: detail.into(),
    }
}
/// `fabric_component::text`, the rule itself, as a desired-state error.
pub(super) fn text(value: &str, label: &str, required: bool, max: usize) -> Result<(), DesiredStateError> {
    Ok(fabric_component::text(value, label, required, max)?)
}
/// `fabric_component::unique`, the rule itself, as a desired-state error.
pub(super) fn unique<'a>(
    values: impl Iterator<Item = &'a str>,
    label: &str,
) -> Result<(), DesiredStateError> {
    Ok(fabric_component::unique(values, label)?)
}
pub(super) fn settings(settings: &super::ConsoleSettings) -> Result<(), DesiredStateError> {
    text(&settings.platform_name, "Platform name", true, 128)?;
    text(&settings.default_region, "Region", true, 128)?;
    text(&settings.timezone, "Timezone", true, 128)?;
    if !is_timezone(&settings.timezone) {
        return Err(invalid("Timezone must be UTC or an Area/Location name"));
    }
    Ok(())
}
pub(super) fn config(values: &ConfigurationValues) -> Result<(), DesiredStateError> {
    for (key, value) in values {
        check_key(key)?;
        text(key, "Configuration key", true, 128)?;
        text(value, "Configuration value", false, 4096)?;
    }
    Ok(())
}
impl Catalogue {
    /// Validates the catalogue and every saved application snapshot.
    /// # Errors
    /// Rejects duplicates, dangling references and invalid configuration.
    pub fn validate(&self) -> Result<(), DesiredStateError> {
        settings(&self.settings)?;
        validate_fields(&self.client_fields)?;
        unique(self.applications.iter().map(|a| a.id.as_str()), "application")?;
        unique(self.environments.iter().map(|e| e.id.as_str()), "environment")?;
        for app in &self.applications {
            app.draft.validate(false)?;
            let mut prior = 0;
            for release in &app.releases {
                if release.version <= prior {
                    return Err(invalid("Release versions must increase"));
                }
                prior = release.version;
                release.definition.validate(true)?;
            }
        }
        for environment in &self.environments {
            text(&environment.name, "Environment name", true, 128)?;
            text(&environment.description, "Environment description", false, 1024)?;
            let host = environment
                .console_url
                .strip_prefix("https://")
                .and_then(|url| url.strip_suffix('/').or(Some(url)));
            if host.is_none_or(|host| crate::Host::try_new(host).is_err()) {
                return Err(invalid(
                    "Environment console must be an HTTPS origin with a valid hostname",
                ));
            }
        }
        Ok(())
    }
}
