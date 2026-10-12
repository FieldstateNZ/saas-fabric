//! A client's lifecycle writes (D01-14): tombstoning it, confirming its
//! deletion, and restoring it. Each prepares one transition at the revision
//! the operator read, then writes it (`write`).

mod restore;
mod retire;
mod write;
