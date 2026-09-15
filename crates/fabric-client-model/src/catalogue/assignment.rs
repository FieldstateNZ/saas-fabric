//! Resolves requests to immutable entitlements before writing a client.
use super::validation::{invalid, text, unique, values};
use super::{ApplicationAssignment, Catalogue, ClientProduct, ClientProductRequest};
use crate::DesiredStateError;
impl Catalogue {
    /// Resolves every requested plan against the exact published application version.
    /// # Errors
    /// Rejects missing releases, plans and invalid or missing configuration.
    pub fn resolve(&self, request: &ClientProductRequest) -> Result<ClientProduct, DesiredStateError> {
        text(&request.display_name, "Display name", true, 128)?;
        text(&request.legal_name, "Legal name", true, 256)?;
        text(&request.region, "Region", true, 128)?;
        text(&request.timezone, "Timezone", true, 128)?;
        unique(request.hosts.iter().map(crate::Host::as_str), "hostname")?;
        unique(
            request.applications.iter().map(|a| a.application_id.as_str()),
            "assignment",
        )?;
        let mut assignments = Vec::new();
        for assignment in &request.applications {
            let app = self
                .applications
                .iter()
                .find(|app| app.id == assignment.application_id)
                .ok_or_else(|| invalid("Unknown application"))?;
            let release = app
                .releases
                .iter()
                .find(|release| release.version == assignment.version)
                .ok_or_else(|| invalid("Only published application versions can be assigned"))?;
            if !release
                .definition
                .plans
                .iter()
                .any(|plan| plan.id == assignment.plan_id)
            {
                return Err(invalid("The plan is not part of this published definition"));
            }
            assignments.push(ApplicationAssignment {
                application_id: assignment.application_id.clone(),
                release: release.clone(),
                plan_id: assignment.plan_id.clone(),
                configuration: values(&release.definition.fields, &assignment.configuration)?,
            });
        }
        Ok(ClientProduct {
            legal_name: request.legal_name.clone(),
            region: request.region.clone(),
            timezone: request.timezone.clone(),
            configuration: values(&self.client_fields, &request.configuration)?,
            definition_version: self.definition_version,
            applications: assignments,
            activity: vec![],
        })
    }
}
impl ApplicationAssignment {
    /// Returns only components required by the selected plan and its features.
    #[must_use]
    pub fn components(&self) -> Vec<&super::ApplicationComponent> {
        let definition = &self.release.definition;
        let plan = definition.plans.iter().find(|p| p.id == self.plan_id);
        definition
            .components
            .iter()
            .filter(|component| {
                component.required
                    || component.kind == super::ComponentKind::Capability
                    || plan.is_some_and(|plan| {
                        definition.features.iter().any(|feature| {
                            plan.features.contains(&feature.id)
                                && feature.implemented_by.contains(&component.id)
                        })
                    })
            })
            .collect()
    }
    /// Returns navigation granted by the selected plan; application permissions still apply.
    #[must_use]
    pub fn navigation(&self) -> Vec<&super::NavigationItem> {
        let definition = &self.release.definition;
        let plan = definition.plans.iter().find(|p| p.id == self.plan_id);
        definition
            .navigation
            .iter()
            .filter(|item| {
                item.feature
                    .as_ref()
                    .is_none_or(|feature| plan.is_some_and(|p| p.features.contains(feature)))
            })
            .collect()
    }
}
