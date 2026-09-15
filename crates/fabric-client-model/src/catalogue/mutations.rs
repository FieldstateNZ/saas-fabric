//! Applies operator commands while owning version numbers and audit attribution.
use super::validation::{invalid, text};
use super::{
    Application, ApplicationDefinition, ApplicationRelease, Catalogue, CatalogueCommand, ProductActivity,
};
use crate::DesiredStateError;
impl Catalogue {
    /// Applies a command to a copy; errors leave the original snapshot unchanged.
    /// # Errors
    /// Rejects unknown resources, duplicate keys and invalid definitions.
    pub fn apply(
        &self,
        command: CatalogueCommand,
        operator: &str,
        at: u64,
    ) -> Result<Self, DesiredStateError> {
        let mut next = self.clone();
        let (action, resource) = match command {
            CatalogueCommand::CreateApplication { id, name } => {
                if next.applications.iter().any(|a| a.id == id) {
                    return Err(invalid("Application already exists"));
                }
                let resource = id.to_string();
                next.applications.push(Application {
                    id,
                    draft: ApplicationDefinition {
                        name,
                        ..ApplicationDefinition::default()
                    },
                    releases: vec![],
                });
                ("Application created", resource)
            }
            CatalogueCommand::SaveApplication { id, definition } => {
                let app = next
                    .applications
                    .iter_mut()
                    .find(|a| a.id == id)
                    .ok_or_else(|| invalid("Application does not exist"))?;
                app.draft = definition;
                ("Application draft saved", id.to_string())
            }
            CatalogueCommand::PublishApplication { id, note } => {
                text(&note, "Release note", true, 2048)?;
                let app = next
                    .applications
                    .iter_mut()
                    .find(|a| a.id == id)
                    .ok_or_else(|| invalid("Application does not exist"))?;
                app.draft.validate(true)?;
                if app.releases.last().is_some_and(|r| r.definition == app.draft) {
                    return Err(invalid("This definition is already published"));
                }
                let version = app
                    .releases
                    .last()
                    .map_or(0, |r| r.version)
                    .checked_add(1)
                    .ok_or_else(|| invalid("Version limit reached"))?;
                app.releases.push(ApplicationRelease {
                    version,
                    note,
                    published_at: at,
                    definition: app.draft.clone(),
                });
                ("Application definition published", id.to_string())
            }
            CatalogueCommand::SaveDefinition { fields } => {
                next.client_fields = fields;
                next.definition_version = next
                    .definition_version
                    .checked_add(1)
                    .ok_or_else(|| invalid("Version limit reached"))?;
                ("Client definition updated", "client-definition".into())
            }
            CatalogueCommand::SaveSettings { settings } => {
                next.settings = settings;
                ("Settings updated", "settings".into())
            }
            CatalogueCommand::SaveEnvironment { environment } => {
                let resource = environment.id.to_string();
                if let Some(existing) = next.environments.iter_mut().find(|e| e.id == environment.id) {
                    *existing = environment;
                } else {
                    next.environments.push(environment);
                }
                ("Environment registered", resource)
            }
        };
        next.activity.push(ProductActivity {
            at,
            operator: operator.into(),
            action: action.into(),
            resource,
        });
        next.validate()?;
        Ok(next)
    }
}
