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
//! It reads no configuration and starts no telemetry: an operator runs it
//! against the published files, or from the control plane's pod against its
//! namespace. `inspect` and `validate` never write. `restore` puts back only
//! the bytes the held publication record names, at the revision it names,
//! for a document that blocks publication
//! ([`fabric_runtime_publication::plan_restore`]); it appends and syncs an
//! audit line first, and writes under the publisher's own exclusion (the
//! filesystem publication lock, or the object's `resourceVersion`). Nothing
//! here discards a held document, lowers a revision or writes content the
//! record does not name.
//!
//! Output is one line of JSON on stdout. Exit status: 0 on success (for
//! `validate`, nothing held blocks publication); 1 when `validate` finds
//! something blocking or `restore` is refused, with nothing written; 2 for
//! a usage error, a target that cannot be read, or a failed audit or write.

mod action;
mod arguments;
#[cfg(test)]
mod arguments_tests;
mod audit;
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
