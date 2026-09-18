//! What a connected platform repository is: every port, one adapter.

use crate::{
    DataSourceDeclaration, DataSourceState, DesiredRevision, DesiredState, DesiredStateError,
    PlacementRecord, PlacementState,
};

/// A repository this binding can hold.
///
/// # Why a combined trait, and not a supertrait on `DesiredState`
///
/// `environments/ENV/data-sources.yaml` and `environments/ENV/placements.yaml`
/// live beside `components.yaml` in the same platform repository, written by
/// the same credential (ADR 0023 parts 1 and 2) -- so one connected
/// repository must answer every port. Making either a supertrait of
/// `DesiredState` said that once, cheaply, but paid for it everywhere
/// `DesiredState` is named: every existing implementor -- including test
/// fakes that are never connected to a binding at all -- would have to grow
/// a half nothing uses. This trait keeps the "one adapter, every port"
/// requirement where it actually applies: to what `connect` accepts.
///
/// # Not a pure blanket trait any more
///
/// [`write_environment`](Self::write_environment) has no honest default: a
/// generic implementation over three independent ports cannot know how to
/// make two documents land in one atomic commit, and pretending otherwise
/// with two separate writes is exactly the cross-file race ADR 0023 part 2
/// closes this method to prevent (`place` reading data sources and writing
/// placements while `remove` reads placements and writes data sources could
/// interleave and leave a placement naming a source nothing declares).
/// So every connectable type names this trait and supplies its own atomic
/// write -- `PlatformGitRepository` over `update_files_atomically`, the
/// late-bound binding by delegation -- and a fixture in a test does the
/// same, deliberately: a fake that only recorded two independent writes
/// would prove nothing about the property this method exists for.
#[async_trait::async_trait]
pub trait PlatformRepository: DesiredState + DataSourceState + PlacementState {
    /// Writes an environment's data sources and placements in one atomic
    /// commit, each at the revision it was read.
    ///
    /// Both documents are rendered and written together even when only one
    /// changed -- the unchanged one is re-rendered from the list the caller
    /// read. What that costs the sibling's own revision depends on what
    /// wrote the file last, and is worth stating plainly rather than
    /// leaving an operator to discover it from a moved `ETag`:
    ///
    /// - **A file only Fabric has ever written**: re-rendering the same
    ///   list byte-for-byte reproduces the file exactly, so a
    ///   content-addressed adapter gives it back the same revision it had
    ///   -- `PlatformGitRepository` never mints a new blob for identical
    ///   bytes. The sibling's revision does not move.
    /// - **A file under break-glass with a hand-written comment inside the
    ///   entry list** (rather than in the preserved header): the header
    ///   survives, exactly as a single-document write already preserves
    ///   it, but a comment *inside* the list is not part of any entry this
    ///   crate models, so re-rendering the list it read drops it. The
    ///   bytes differ from what was on disk, so the sibling's revision
    ///   *does* move -- not because this call changed what the sibling
    ///   means, but because it is not a byte-for-byte copy of a file a
    ///   human had added a comment to.
    ///
    /// `expected` on either half is `None` to create that file and
    /// `Some(revision)` to replace it, refused unless the file is still at
    /// that revision -- the same compare-and-swap
    /// [`write_data_sources`](DataSourceState::write_data_sources) and
    /// [`write_placements`](PlacementState::write_placements) each make for
    /// their own document, applied to both at once so neither can move
    /// between the other's read and this write.
    ///
    /// # Errors
    ///
    /// `DesiredStateError::Conflict` if either half's revision does not
    /// match what is held, and the other `DesiredStateError` variants for
    /// what they name.
    async fn write_environment(
        &self,
        environment: &str,
        write: EnvironmentWrite<'_>,
        message: &str,
    ) -> Result<(), DesiredStateError>;
}

/// The two documents [`PlatformRepository::write_environment`] writes
/// together, each with the revision it was read at.
///
/// A borrowing struct rather than two positional arguments: `data_sources`
/// and `placements` read the same both times they appear (the field name
/// and the tuple's first element), where two bare slice-and-`Option`
/// parameters would leave a call site guessing which pair is which.
pub struct EnvironmentWrite<'a> {
    /// The complete list of declared data sources to write, and the
    /// revision it was read at.
    pub data_sources: (&'a [DataSourceDeclaration], Option<&'a DesiredRevision>),

    /// The complete list of recorded placements to write, and the revision
    /// it was read at.
    pub placements: (&'a [PlacementRecord], Option<&'a DesiredRevision>),
}
