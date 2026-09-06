//! Driving `docker` as a plain process: spawning it, reading its exit
//! status, and turning failure into one error type.
//!
//! This is the "process driving" concept the module doc of the parent
//! `docker.rs` names: nothing here knows what a container or a network is,
//! only how to run `docker <args...>` and turn the result into something
//! [`containers`](super::containers) and [`networks`](super::networks) can
//! build on.

use std::process::{Command, Output};

/// A `docker` invocation failed, or could not be started at all.
///
/// Carries the full command line and enough of the failure to act on
/// without re-running it by hand.
#[derive(Debug)]
pub struct DockerError {
    command: String,
    detail: String,
}

impl std::fmt::Display for DockerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "`{}` failed: {}", self.command, self.detail)
    }
}

impl std::error::Error for DockerError {}

impl DockerError {
    /// Builds a [`DockerError`] directly from its two parts, for a caller
    /// that already has a command description and a failure detail in hand
    /// -- a raw [`std::io::Error`] from [`std::process::Command`], or a
    /// required-mode digest refusal with no `docker` process behind it at
    /// all (`containers::resolve_runnable_reference`).
    pub(super) fn from_parts(command: String, detail: String) -> Self {
        Self { command, detail }
    }
}

/// Runs `docker version`, succeeding only if a daemon actually answered.
///
/// # Errors
///
/// A [`DockerError`] if the `docker` binary could not be run, or a daemon
/// did not answer -- the signal [`crate::support::gate`] treats as "Docker
/// is not available here".
pub fn version() -> Result<(), DockerError> {
    run_checked(&[
        "version".to_owned(),
        "--format".to_owned(),
        "{{.Server.Version}}".to_owned(),
    ])
    .map(|_| ())
}

/// Turns a completed command's exit status into a `Result`, carrying
/// `description` and `stderr` when it failed. Every "did this actually
/// work" check that [`containers::exec`](super::containers::exec) itself
/// deliberately leaves open goes through here, so a swallowed failure is a
/// missing call to this, not a missing feature.
///
/// # Errors
///
/// A [`DockerError`] if `output`'s exit status was not success.
pub fn ensure_success(description: &str, output: &Output) -> Result<(), DockerError> {
    if output.status.success() {
        return Ok(());
    }

    Err(DockerError {
        command: description.to_owned(),
        detail: format!(
            "exit {}: {}",
            output
                .status
                .code()
                .map_or_else(|| "terminated by signal".to_owned(), |code| code.to_string()),
            String::from_utf8_lossy(&output.stderr).trim()
        ),
    })
}

/// Non-empty, trimmed lines of `output`'s stdout.
pub(super) fn lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `docker <args...>`, as a string, for error messages.
pub(super) fn describe(args: &[String]) -> String {
    format!("docker {}", args.join(" "))
}

/// Runs `docker <args...>`, erroring on a non-zero exit as well as a spawn
/// failure.
pub(super) fn run_checked(args: &[String]) -> Result<Output, DockerError> {
    let output = spawn(args)?;
    ensure_success(&describe(args), &output)?;
    Ok(output)
}

/// Runs `docker <args...>`, erroring only if the process could not be
/// started at all.
pub(super) fn spawn(args: &[String]) -> Result<Output, DockerError> {
    Command::new("docker")
        .args(args)
        .output()
        .map_err(|source| DockerError {
            command: describe(args),
            detail: format!("could not run the `docker` binary: {source}"),
        })
}
