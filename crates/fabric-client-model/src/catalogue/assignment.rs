//! Resolves requests to immutable entitlements before writing a client.
//!
//! In the 121–150 line band. The reason is `resolve` itself: ADR 0021 §3's
//! keep-the-stored-copy rule, the timezone check, and the plan and
//! configuration lookups all decide the one `ClientProduct` a write is
//! about to commit, and none of them is a rule a second caller would ever
//! want on its own. `components`/`navigation` stay beside it because they
//! read the same `ApplicationAssignment` this function produces — moving
//! them elsewhere would separate a type from the two projections that only
//! make sense once it exists.
use super::validation::{invalid, is_timezone, text, unique, values};
use super::{ApplicationAssignment, Catalogue, ClientProduct, ClientProductRequest};
use crate::DesiredStateError;
impl Catalogue {
    /// Resolves every requested assignment, keeping a client's existing copy
    /// of a release its request does not change.
    ///
    /// # Why `previous` is here at all
    ///
    /// ADR 0021 §3 pins a client to the exact release it was assigned —
    /// "whatever the catalogue later says" is exactly what a client must
    /// *not* silently start following. Looking every assignment up in `self`
    /// on every save would do precisely that: a release is meant to be
    /// immutable once published, but nothing stops an operator editing the
    /// stored catalogue by hand, and doing so used to reach every client
    /// holding that release on their very next unrelated save — or, if a
    /// release was removed altogether, refuse it. `previous` is this
    /// client's own last-resolved assignments; an entry unchanged in
    /// `application_id` and `version` keeps its stored release exactly,
    /// rather than re-deriving one from `self` that may no longer agree
    /// with it or may no longer exist. Only a genuinely new or upgraded
    /// assignment is looked up here, against the catalogue as it is now.
    ///
    /// Pass an empty slice when there is no previous state to keep — a
    /// brand-new client, where every assignment is necessarily new.
    /// # Errors
    /// Rejects missing releases, plans and invalid or missing configuration.
    pub fn resolve(
        &self,
        request: &ClientProductRequest,
        previous: &[ApplicationAssignment],
    ) -> Result<ClientProduct, DesiredStateError> {
        text(&request.display_name, "Display name", true, 128)?;
        text(&request.legal_name, "Legal name", true, 256)?;
        text(&request.region, "Region", true, 128)?;
        text(&request.timezone, "Timezone", true, 128)?;
        if !is_timezone(&request.timezone) {
            return Err(invalid("Timezone must be UTC or an Area/Location name"));
        }
        unique(request.hosts.iter().map(crate::Host::as_str), "hostname")?;
        unique(
            request.applications.iter().map(|a| a.application_id.as_str()),
            "assignment",
        )?;
        let mut assignments = Vec::new();
        for assignment in &request.applications {
            let kept = previous.iter().find(|existing| {
                existing.application_id == assignment.application_id
                    && existing.release.version == assignment.version
            });
            let release = if let Some(existing) = kept {
                existing.release.clone()
            } else {
                let app = self
                    .applications
                    .iter()
                    .find(|app| app.id == assignment.application_id)
                    .ok_or_else(|| invalid("Unknown application"))?;
                app.releases
                    .iter()
                    .find(|release| release.version == assignment.version)
                    .cloned()
                    .ok_or_else(|| invalid("Only published application versions can be assigned"))?
            };
            if !release
                .definition
                .plans
                .iter()
                .any(|plan| plan.id == assignment.plan_id)
            {
                return Err(invalid("The plan is not part of this published definition"));
            }
            let configuration = values(&release.definition.fields, &assignment.configuration)?;
            assignments.push(ApplicationAssignment {
                application_id: assignment.application_id.clone(),
                release,
                plan_id: assignment.plan_id.clone(),
                configuration,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::{
        Application, ApplicationDefinition, ApplicationPlan, ApplicationRelease, AssignmentRequest,
    };
    use crate::ClientId;

    fn plan() -> ApplicationPlan {
        ApplicationPlan {
            id: ClientId::try_new("standard").unwrap(),
            name: "Standard".into(),
            description: String::new(),
            features: vec![],
            configuration: crate::catalogue::ConfigurationValues::default(),
        }
    }

    fn catalogue_with_one_release(note: &str) -> Catalogue {
        let definition = ApplicationDefinition {
            name: note.into(),
            plans: vec![plan()],
            ..ApplicationDefinition::default()
        };
        Catalogue {
            applications: vec![Application {
                id: ClientId::try_new("analytics").unwrap(),
                draft: definition.clone(),
                releases: vec![ApplicationRelease {
                    version: 1,
                    note: note.into(),
                    published_at: 0,
                    definition,
                }],
            }],
            ..Catalogue::default()
        }
    }

    fn request() -> ClientProductRequest {
        ClientProductRequest {
            display_name: "Acme".into(),
            hosts: vec![],
            legal_name: "Acme Ltd".into(),
            region: "NZ".into(),
            timezone: "Pacific/Auckland".into(),
            configuration: crate::catalogue::ConfigurationValues::default(),
            applications: vec![AssignmentRequest {
                application_id: ClientId::try_new("analytics").unwrap(),
                version: 1,
                plan_id: ClientId::try_new("standard").unwrap(),
                configuration: crate::catalogue::ConfigurationValues::default(),
            }],
        }
    }

    #[test]
    fn an_unchanged_assignment_keeps_its_stored_release_even_once_the_catalogue_disagrees() {
        let original = catalogue_with_one_release("Initial release");
        let first = original.resolve(&request(), &[]).unwrap();

        // The catalogue's own copy of v1 has since changed shape (edited by
        // hand, say) — re-resolving against `self` would notice.
        let edited = catalogue_with_one_release("Edited by hand");

        let second = edited.resolve(&request(), &first.applications).unwrap();

        assert_eq!(
            second.applications[0].release.definition.name, "Initial release",
            "the client's own copy must not have moved just because the catalogue's did"
        );
    }

    #[test]
    fn an_unchanged_assignment_survives_the_release_being_removed_from_the_catalogue() {
        let original = catalogue_with_one_release("Initial release");
        let first = original.resolve(&request(), &[]).unwrap();

        let emptied = Catalogue::default();

        let second = emptied.resolve(&request(), &first.applications).unwrap();

        assert_eq!(second.applications[0].release.definition.name, "Initial release");
    }

    #[test]
    fn a_fresh_resolve_against_a_catalogue_missing_the_release_is_refused() {
        let emptied = Catalogue::default();

        assert!(emptied.resolve(&request(), &[]).is_err());
    }
}
