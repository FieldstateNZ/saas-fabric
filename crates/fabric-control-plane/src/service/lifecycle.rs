//! A client's lifecycle writes (D01-14): tombstoning it, confirming its
//! deletion, and restoring it. Each prepares one transition at the revision
//! the operator read (`write`), then records it through the continuity
//! store when there is one (`record`).

mod create;
mod record;
mod replay;
mod restore;
mod retire;
mod write;
