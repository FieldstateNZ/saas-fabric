//! Shared kernel for the SaaS Fabric runtime plane.
//!
//! This crate holds the handful of types that every other crate needs and that
//! nothing in the platform can disagree about: the validated identifiers that
//! flow through tenant resolution, the structured event-ID scheme used by
//! logging, and the [`Clock`] seam that keeps time injectable in tests.
//!
//! It deliberately contains **no I/O**. If you find yourself wanting to add a
//! database call or an HTTP client here, it belongs in a domain crate instead —
//! every crate in the workspace depends on this one, so anything added here is
//! paid for everywhere.
//!
//! # The three names
//!
//! Three identifiers in this crate look alike and mean entirely different
//! things. Getting them straight is the single most useful thing to know about
//! the platform's model:
//!
//! ```text
//! LogicalResourceName      customers, orders, auditEvents
//!         ↓ catalogue                what an application addresses
//! LogicalDataSourceName    primary, audit, analytics
//!         ↓ tenant binding           which pool of data it belongs to
//! DataSourceId             sql-au-east-03, shared-postgres-02
//!         ↓ registry                 the configured physical resource
//! DataSource               connector, connection, pool, region, placement
//!         ↓
//! Connector
//! ```
//!
//! The first two are **intent** and are identical for every tenant. The third
//! is a **physical resource** and differs per tenant — that difference is the
//! whole of multi-tenancy in this platform, and it is confined to one hop.
//!
//! Applications see the first. They never see the third.

mod clock;
mod identifier_error;
mod ids;
mod logging;

/// The character-set rules behind the identifier newtypes.
///
/// Exposed so that crates further out — connector ids, collection names,
/// schema names — enforce exactly the same rules rather than growing their own
/// near-miss copies. Prefer an existing newtype where one fits; reach for these
/// functions only when defining a genuinely new kind of identifier.
pub mod naming {
    pub use crate::ids::slug::{parse_dns_label, parse_identifier, MAX_LENGTH};
}

pub use clock::{Clock, SystemClock};
pub use identifier_error::IdentifierError;
pub use ids::{BindingRevision, DataSourceId, LogicalDataSourceName, LogicalResourceName, TenantId};
pub use logging::{event_id, EventType};
