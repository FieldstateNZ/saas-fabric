//! `fabric-api last-known-good`: the operator's recovery command for the
//! reader's persisted last-known-good copy (gap G5).
//!
//! ```text
//! fabric-api last-known-good inspect  --dir <directory>
//! fabric-api last-known-good validate --dir <directory>
//! fabric-api last-known-good discard  --dir <directory> --operator <who> --reason <why>
//! ```
//!
//! It reads only the directory it is given: no configuration, no network,
//! no published files. `inspect` and `validate` never write. `discard`
//! appends an audit record and then moves the copy aside; it is the only way
//! a copy is removed, and it never writes a generation in its place. There
//! is deliberately no command that writes, restores or promotes a copy: the
//! only generation a copy can hold is one the reader read from the published
//! files and validated.
//!
//! Output is one line of JSON on stdout. Exit status: 0 on success (for
//! `validate`, a valid copy); 1 when `validate` finds no valid copy or
//! `discard` finds none to discard; 2 for a usage error or a failed write.

mod arguments;
#[cfg(test)]
mod arguments_tests;
mod run;

pub use arguments::{parse, Action};
pub use run::{run, Outcome};

/// The subcommand, as the first argument.
pub const COMMAND: &str = "last-known-good";

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
