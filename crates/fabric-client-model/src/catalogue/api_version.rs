//! Which `apiVersion` a catalogue document is written at (ADR 0026 section
//! 7).
use super::{Catalogue, ComponentKind};
use crate::{ClientId, DesiredStateError};

/// The catalogue versions this build reads.
///
/// # Why the lowest that expresses it, computed on every render
///
/// `v2` exists for one reason: a described component, which a build that
/// reads only `v1` cannot read. Writing `v2` only while a draft or a release
/// holds one keeps every other catalogue readable by such a build -- it
/// refuses a catalogue it cannot read by naming the version it found, never
/// by misreading it -- and a catalogue whose last described component is
/// gone is written at `v1` again. Nothing about the version is stored apart
/// from the document, so nothing can drift from what the document holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ApiVersion {
    /// Every catalogue written before ADR 0026, and every one since that
    /// holds no described component.
    V1,
    /// A catalogue holding a described component in a draft or a release.
    V2,
}

impl ApiVersion {
    /// Every version, oldest first.
    pub(super) const ALL: [Self; 2] = [Self::V1, Self::V2];

    /// The `apiVersion` as written.
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "fabric.fieldstate.nz/v1",
            Self::V2 => "fabric.fieldstate.nz/v2",
        }
    }

    /// The version `text` names, if this build reads it.
    pub(super) fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|version| version.as_str() == text)
    }

    /// The lowest version that expresses `catalogue`.
    pub(super) fn lowest_for(catalogue: &Catalogue) -> Self {
        if catalogue.first_described().is_some() {
            Self::V2
        } else {
            Self::V1
        }
    }

    /// Refuses a `v1` document that holds a described component, naming the
    /// component and both versions: a hand edit, or a writer that is not
    /// this model, since this model never writes one.
    pub(super) fn check_expresses(self, catalogue: &Catalogue) -> Result<(), DesiredStateError> {
        match (self, catalogue.first_described()) {
            (Self::V1, Some((application, component))) => Err(DesiredStateError::CatalogueMalformed {
                detail: format!(
                    "component '{component}' of application '{application}' is described, which {} \
                     cannot express; a catalogue holding one is {}",
                    Self::V1.as_str(),
                    Self::V2.as_str()
                ),
            }),
            (Self::V1 | Self::V2, _) => Ok(()),
        }
    }
}

impl Catalogue {
    /// The first described component in any draft or release, as its
    /// application and component ids.
    fn first_described(&self) -> Option<(&ClientId, &ClientId)> {
        self.applications.iter().find_map(|application| {
            std::iter::once(&application.draft)
                .chain(application.releases.iter().map(|release| &release.definition))
                .flat_map(|definition| definition.components.iter())
                .find(|component| component.kind == ComponentKind::Described)
                .map(|component| (&application.id, &component.id))
        })
    }
}
