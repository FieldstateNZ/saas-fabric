//! Selecting a component's version: the catalogue's half of ADR 0026
//! section 7, once the server has resolved it.
//!
//! In the 121–150 line band. The reason is `select_component`: one `match`
//! on what the component id names is the section's whole table, and the
//! refusal a caller maps apart from the others is declared beside it, so a
//! new row and its answer are read in one place.
use super::validation::invalid;
use super::{
    ApplicationComponent, Catalogue, ComponentKind, ComponentResolution, ProductActivity, UpdatePolicy,
};
use crate::{ClientId, DesiredStateError};
use fabric_component::Digest;

/// Why a selection was not written.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ComponentSelectionError {
    /// The version resolved to the component descriptor the component
    /// already records. Its own variant because it is not a malformed
    /// request: a caller maps it to its own answer, and nothing is written,
    /// so a timestamp never becomes a change.
    #[error(
        "Component '{component}' of '{application}' already records the component descriptor {descriptor_digest}"
    )]
    AlreadySelected {
        /// The application.
        application: ClientId,
        /// The component.
        component: ClientId,
        /// The component descriptor it records, and the version resolved to.
        descriptor_digest: Digest,
    },
    /// The selection breaks a catalogue rule, or names what it cannot.
    #[error(transparent)]
    Refused(#[from] DesiredStateError),
}

impl Catalogue {
    /// Records `resolution` for `component` of `application`'s draft, as a
    /// copy of this catalogue; errors leave this one unchanged.
    ///
    /// Pure, like [`apply`](Self::apply): the server resolves the version
    /// against its registry first and hands the resolution here, which is
    /// why `apply` refuses the command itself. What the component id names
    /// decides what happens:
    ///
    /// | The id names | Selecting |
    /// |---|---|
    /// | nothing | adds a described component named by the descriptor's title, in no plan by default, with a manual policy |
    /// | a described component | re-resolves it, keeping its name, `required` and policy |
    /// | a `container` or `helm` component | converts it in place, keeping its id, name, `required` and policy, so the features naming it are kept |
    /// | a `capability` component | is refused |
    ///
    /// # Errors
    ///
    /// Returns [`ComponentSelectionError::AlreadySelected`] when the
    /// component already records `resolution`'s component descriptor, and
    /// [`ComponentSelectionError::Refused`] for an unknown application, a
    /// capability, or a catalogue that would not validate.
    pub fn select_component(
        &self,
        application: &ClientId,
        component: &ClientId,
        resolution: ComponentResolution,
        operator: &str,
        at: u64,
    ) -> Result<Self, ComponentSelectionError> {
        let mut next = self.clone();
        let draft = &mut next
            .applications
            .iter_mut()
            .find(|candidate| &candidate.id == application)
            .ok_or_else(|| invalid("Application does not exist"))?
            .draft;
        let reference = resolution.repository.to_string();
        let version = resolution.version.to_string();
        match draft
            .components
            .iter_mut()
            .find(|candidate| &candidate.id == component)
        {
            None => draft.components.push(ApplicationComponent {
                id: component.clone(),
                name: resolution.descriptor.spec().title.clone(),
                kind: ComponentKind::Described,
                reference,
                version,
                required: false,
                policy: UpdatePolicy::Manual,
                resolution: Some(resolution),
            }),
            Some(existing) => {
                match (existing.kind, &existing.resolution) {
                    (ComponentKind::Capability, _) => {
                        return Err(invalid(format!(
                            "Component '{component}' is a platform capability; a version cannot be selected for it"
                        ))
                        .into())
                    }
                    (ComponentKind::Described, Some(recorded))
                        if recorded.descriptor_digest == resolution.descriptor_digest =>
                    {
                        return Err(ComponentSelectionError::AlreadySelected {
                            application: application.clone(),
                            component: component.clone(),
                            descriptor_digest: resolution.descriptor_digest,
                        })
                    }
                    (ComponentKind::Described | ComponentKind::Container | ComponentKind::Helm, _) => {}
                }
                existing.kind = ComponentKind::Described;
                existing.reference = reference;
                existing.version = version;
                existing.resolution = Some(resolution);
            }
        }
        next.activity.push(ProductActivity {
            at,
            operator: operator.into(),
            action: "Component version selected".into(),
            resource: format!("{application}/{component}"),
        });
        next.validate()?;
        Ok(next)
    }
}
