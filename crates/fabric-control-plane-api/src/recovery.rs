//! `fabric-control-plane-api held-publication`: the operator's recovery
//! command for a held tenants or data-sources document the publisher cannot
//! parse (gap G5, publisher side).
//!
//! ```text
//! fabric-control-plane-api held-publication inspect  <target>
//! fabric-control-plane-api held-publication validate <target>
//! fabric-control-plane-api held-publication restore  <target> --document <tenants|data-sources>
//!     --from <file> --audit <file> --operator <who> --reason <why>
//!
//! <target>: --tenants <file> --data-sources <file> --catalog <file>
//!         | --namespace <namespace>
//! ```
//!
//! It reads no configuration and starts no telemetry. `inspect` and
//! `validate` read every held part raw, take no lock and write nothing; they
//! report digests, revisions and typed reasons, never a held value, and
//! `validate` fails unless the held evidence is clear.
//!
//! `restore` is filesystem only. On Kubernetes no exclusive, fenced
//! publisher ownership exists to bind a write to the record that authorises
//! it, so a restore there is refused before anything is read or audited.
//! On the filesystem it puts back only the exact bytes the held record
//! names, at the revision it names, for a document that blocks publication
//! ([`fabric_runtime_publication::plan_restore`]), under the publication
//! lock. Before anything is written it refuses a source or audit log that
//! is, or resolves to, a published file or each other; it appends and syncs
//! an operation's intent before the first write, and its completion,
//! failure or uncertain outcome after. The bytes already restored are a
//! no-op. Nothing here discards a held document, lowers a revision, or
//! writes content the record does not name. A digest match shows agreement
//! with the record held now, not signed history.
//!
//! Output is one line of JSON on stdout. Exit status: 0 on success (for
//! `validate`, clear held evidence; for `restore`, completed or already
//! restored); 1 when `validate` finds a problem or `restore` is refused,
//! with nothing written; 2 for a usage error, a target, source or audit log
//! that cannot be used, a failed audit, or a failed or uncertain write.

mod action;
mod arguments;
#[cfg(test)]
mod arguments_tests;
mod audit;
mod audit_file;
mod identity;
mod restore;
mod run;

pub use action::{Action, Target};
pub use arguments::parse;
pub use run::{run, Outcome};

/// The subcommand, as the first argument.
pub const COMMAND: &str = "held-publication";

/// Whether `arguments` (without the program name) ask for this command.
pub fn requested<I>(arguments: I) -> bool
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    arguments
        .into_iter()
        .next()
        .is_some_and(|first| first.as_ref() == COMMAND)
}
