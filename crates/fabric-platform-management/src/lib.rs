//! Deciding which version of a component an environment should run.
//!
//! ```text
//! Available     discovered from artifact registries   ← this crate asks
//! Desired       the platform repository                ← this crate proposes
//! Running       the reconciliation system              ← read-only observation
//! ```
//!
//! Three states, and they are deliberately not one. A version that has been
//! published is not one that has been chosen, and a version that has been
//! chosen is not one that is serving traffic. Collapsing any pair of them
//! produces a console that reports success from a Git commit.
//!
//! # No transport
//!
//! This crate knows nothing about HTTP, registries or Git. It defines the
//! [`Registry`] port and is handed an implementation; where an artifact was
//! found and how it was authenticated to is somebody else's concern, and
//! deliberately so — the registry credential and the platform repository
//! credential are separate integrations and must stay separable.

mod artifact;
mod binding;
mod charts;
mod data_sources;
mod desired_state;
mod diagnostic;
mod discovery;
mod observation;
mod placements;
mod policy;
mod publication;
mod registry;
mod running_guard;
mod selector;
mod service;
mod status;
mod sweep;
mod version;

pub use artifact::{ArtifactKind, ArtifactSource, Release};
pub use binding::{EnvironmentWrite, PlatformDesiredState, PlatformRepository};
pub use charts::ChartIndex;
pub use data_sources::{
    DataSourceDeclaration, DataSourceRule, DataSourceState, DataSources, DataSourcesRead, Declared,
    Discriminator, PoolField,
};
pub use desired_state::{ComponentDesired, DesiredRevision, DesiredState, DesiredStateError, Hold};
pub use diagnostic::SafeDiagnostic;
pub use discovery::{Discovery, History, ReleaseUnit, ResolvedImage};
// The wire's own sub-types, re-exported because they are the field types of
// `DataSourceDeclaration` (ADR 0023 part 1) and `PlacementRecord` (ADR 0023
// part 2): a caller that builds or reads either has to name them, and the
// only crates allowed to depend on the wire directly are this one and the
// publisher's future caller. A re-export adds no Cargo edge for anyone.
pub use fabric_runtime_publication::{
    ConnectionName, ConnectionSelectorDocument, ConnectorId, DataResidencyDocument,
    DataSourceCapabilitiesDocument, DataSourceDocument, FieldName, IsolationModelDocument,
    PlacementClassDocument, PoolSettingsDocument,
};
pub use observation::{DeploymentHealth, DeploymentObservation, DeploymentObserver, WorkloadObservation};
pub use placements::{
    select, ClientPlacements, DataIntent, PlacementOutcome, PlacementRecord, PlacementRefusal,
    PlacementState, Placements, PlacementsRead,
};
pub use policy::UpdatePolicy;
pub use publication::{
    compose, CatalogueSourceError, ComposeError, LastPass, PassOutcome, PassResult, PublicationState,
    RuntimeCatalogueSource, RuntimePublisher, WaitingReason,
};
pub use registry::{Provenance, Registry, RegistryError, Resolved};
pub use selector::{decide, Decision, Reason};
pub use service::{PlatformError, PlatformManagement};
pub use status::{ComponentStatus, DesiredStateStatus, Diagnostics, Reconciliation, Running};
pub use sweep::{CheckOutcome, LastCheck, Sweep, SweepResult, SweepState, Swept};
pub use version::{Channel, Version};

/// Finds the releases an environment is allowed to move to, above and below
/// the version it runs, for either artifact kind.
pub use discovery::{chart_history, discover, discover_chart, history, resolve, resolve_chart};
