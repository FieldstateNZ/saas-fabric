//! Application references are checked before publication and assignment.
use super::{config, invalid, text, unique, validate_fields};
use crate::catalogue::{ApplicationDefinition, ComponentKind};
use crate::{DesiredStateError, Host};
impl ApplicationDefinition {
    /// Validates component, feature, plan and navigation references.
    /// # Errors
    /// Publication additionally requires a plan and pinned deployable versions.
    pub fn validate(&self, publishing: bool) -> Result<(), DesiredStateError> {
        text(&self.name, "Application name", true, 128)?;
        text(&self.description, "Description", false, 2048)?;
        if !self.domain.is_empty() && Host::try_new(self.domain.replace("{client}", "example")).is_err() {
            return Err(invalid("Invalid application hostname template"));
        }
        unique(self.components.iter().map(|c| c.id.as_str()), "component")?;
        unique(self.features.iter().map(|f| f.id.as_str()), "feature")?;
        unique(self.plans.iter().map(|p| p.id.as_str()), "plan")?;
        unique(
            self.navigation.iter().map(|n| n.route.as_str()),
            "navigation route",
        )?;
        validate_fields(&self.fields)?;
        for component in &self.components {
            text(&component.name, "Component name", true, 128)?;
            text(&component.reference, "Component reference", true, 1024)?;
            text(
                &component.version,
                "Component version",
                publishing && component.kind != ComponentKind::Capability,
                256,
            )?;
            if component.kind == ComponentKind::Capability
                && ![
                    "Identity",
                    "Database",
                    "Secrets",
                    "Authorization",
                    "Routing",
                    "Object storage",
                    "Messaging",
                ]
                .contains(&component.reference.as_str())
            {
                return Err(invalid("Unknown platform capability"));
            }
        }
        for feature in &self.features {
            text(&feature.name, "Feature name", true, 128)?;
            text(&feature.description, "Feature description", false, 1024)?;
            unique(
                feature.implemented_by.iter().map(crate::ClientId::as_str),
                "feature component",
            )?;
            if feature
                .implemented_by
                .iter()
                .any(|id| !self.components.iter().any(|c| &c.id == id))
            {
                return Err(invalid("A feature names an unknown component"));
            }
        }
        if publishing && self.plans.is_empty() {
            return Err(invalid("Add at least one plan before publishing"));
        }
        for plan in &self.plans {
            text(&plan.name, "Plan name", true, 128)?;
            text(&plan.description, "Plan description", false, 1024)?;
            unique(plan.features.iter().map(crate::ClientId::as_str), "plan feature")?;
            if plan
                .features
                .iter()
                .any(|id| !self.features.iter().any(|f| &f.id == id))
            {
                return Err(invalid("A plan names an unknown feature"));
            }
            config(&plan.configuration)?;
        }
        for item in &self.navigation {
            text(&item.label, "Navigation label", true, 128)?;
            text(&item.route, "Navigation route", true, 512)?;
            text(&item.permission, "Permission", false, 256)?;
            if !item.route.starts_with('/')
                || item.route.starts_with("//")
                || item.route.contains(['\\', '%', '?', '#'])
                || item.route.split('/').any(|s| s == ".." || s == ".")
            {
                return Err(invalid(
                    "Navigation must be a same-origin path without traversal, query or fragment",
                ));
            }
            if item
                .feature
                .as_ref()
                .is_some_and(|id| !self.features.iter().any(|f| &f.id == id))
            {
                return Err(invalid("Navigation names an unknown feature"));
            }
        }
        Ok(())
    }
}
