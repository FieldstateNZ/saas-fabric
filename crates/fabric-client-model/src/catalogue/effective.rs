//! What an application's definition puts in effect: the fields and
//! resources the operator authored, and the ones each described component
//! declares (ADR 0026 section 8).
use super::{ApplicationComponent, ApplicationDefinition, ApplicationResource, ConfigurationField};

impl ApplicationDefinition {
    /// The configuration fields in effect: the authored ones, followed by
    /// each described component's declared ones in component order.
    ///
    /// # Why every reader uses this, and not `fields`
    ///
    /// A declared field applies to every client of the application, as an
    /// authored one does, whatever its plan grants. A reader that took
    /// `fields` alone -- a client's configuration check, say -- would accept
    /// a client that omits a field the software says it needs. Definition
    /// validation refuses a key in effect twice, so the order only decides
    /// how a console lists them.
    ///
    /// Owned rather than borrowed because every reader wants a slice, and
    /// the declared ones live inside each component's frozen descriptor;
    /// the copy is bounded by 64 fields a component.
    #[must_use]
    pub fn effective_fields(&self) -> Vec<ConfigurationField> {
        let declared = self
            .described()
            .flat_map(|(_, descriptor)| descriptor.spec().fields.iter());
        self.fields.iter().chain(declared).cloned().collect()
    }

    /// The Data API resources in effect: the authored ones, followed by each
    /// described component's declared ones in component order. Every reader
    /// uses it -- definition validation, the cross-application check on both
    /// sides, and the runtime catalogue -- for the reason
    /// [`effective_fields`](Self::effective_fields) gives.
    #[must_use]
    pub fn effective_resources(&self) -> Vec<ApplicationResource> {
        let declared = self
            .described()
            .flat_map(|(_, descriptor)| descriptor.spec().resources.iter());
        self.resources.iter().chain(declared).cloned().collect()
    }

    /// Each component carrying a resolution, with its frozen descriptor, in
    /// component order. Validation holds that exactly the described ones
    /// carry one.
    pub(super) fn described(
        &self,
    ) -> impl Iterator<Item = (&ApplicationComponent, &fabric_component::ComponentDescriptor)> {
        self.components.iter().filter_map(|component| {
            component
                .resolution
                .as_ref()
                .map(|resolution| (component, &resolution.descriptor))
        })
    }
}
