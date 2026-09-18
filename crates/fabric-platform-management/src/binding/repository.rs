//! What a connected platform repository is: both ports, one adapter.

use crate::{DataSourceState, DesiredState};

/// A repository this binding can hold.
///
/// # Why a combined trait, and not a supertrait on `DesiredState`
///
/// `environments/ENV/data-sources.yaml` lives beside `components.yaml` in
/// the same platform repository, written by the same credential (ADR 0023
/// part 1) -- so one connected repository must answer both ports. Making
/// `DataSourceState` a supertrait of `DesiredState` said that once, cheaply,
/// but paid for it everywhere `DesiredState` is named: every existing
/// implementor -- including test fakes that are never connected to a
/// binding at all -- had to grow a `DataSourceState` half whether or not
/// anything used it. This trait keeps the "one adapter, both ports"
/// requirement where it actually applies: to what `connect` accepts.
///
/// The blanket implementation means nothing has to name this trait to
/// satisfy it -- any type that already implements both ports is a
/// `PlatformRepository` for free, exactly as it would have been a
/// `DesiredState` supertrait implementor for free before.
pub trait PlatformRepository: DesiredState + DataSourceState {}

impl<T: DesiredState + DataSourceState + ?Sized> PlatformRepository for T {}
