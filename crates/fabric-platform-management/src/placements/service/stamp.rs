//! Stamping a new record with when it was placed.

use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

use crate::placements::service::Placements;
use crate::DesiredStateError;

impl Placements {
    /// Now, as the record stamps it.
    ///
    /// Mirrors `PlatformManagement::stamp` (`service/brake.rs`) in shape,
    /// not in fallibility: that one is a display-only hold reason and
    /// falls back to an empty string. `placed_at` is the record the
    /// runtime reads, so a clock this cannot format as RFC 3339 refuses
    /// the placement rather than write one out with no timestamp at all
    /// (N4).
    ///
    /// # Errors
    ///
    /// [`DesiredStateError::Unavailable`] if the clock's time cannot be
    /// represented as RFC 3339 -- out of the representable calendar range,
    /// in practice never for a real clock.
    pub(super) fn stamp(&self) -> Result<String, DesiredStateError> {
        OffsetDateTime::from_unix_timestamp(i64::try_from(self.clock.now_unix_seconds()).unwrap_or(i64::MAX))
            .ok()
            .and_then(|at| at.format(&Rfc3339).ok())
            .ok_or_else(|| DesiredStateError::Unavailable {
                detail: "the clock's time could not be formatted as RFC 3339".to_owned(),
            })
    }
}
