//! Container lifecycle: starting one, reading it back, and removing it.
//!
//! The "containers" concept the parent `docker.rs` module doc names. Builds
//! on [`process`](super::process) for the actual `docker` invocations and
//! knows nothing about networks beyond the name a [`RunSpec`] joins. Deciding
//! *which* image reference to actually pass to `docker run` -- presence
//! checks, pulling, and the required-mode-versus-fallback policy -- is a
//! separate concept, [`image_reference`](super::image_reference); this file
//! only calls it.

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

use super::image_reference;
use super::process::{self, DockerError};

/// A container this harness started, identified by the name it was given.
///
/// Deliberately the name this harness chose, not the ID `docker run` prints
/// to stdout: every later operation (`exec`, `port`, `stop`, `rm`) addresses
/// a container by name, and [`crate::support::names::sweep_stale`] matches
/// stale ones the same way.
pub struct Container {
    name: String,
}

impl Container {
    /// The name Docker knows this container by.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// What [`run`] needs to start one container.
pub struct RunSpec {
    /// The name to give the container.
    pub name: String,
    /// The image reference to run, digest-pinned (see `images.rs`).
    pub image: String,
    /// The Docker network to attach to.
    pub network: String,
    /// Environment variables, as `(name, value)` pairs.
    pub env: Vec<(String, String)>,
    /// A container port to publish on an ephemeral `127.0.0.1` host port, if
    /// any -- read back afterwards with [`port`].
    pub publish: Option<u16>,
    /// A host directory to bind-mount read-only, as `(host_dir, container_dir)`.
    pub mount_ro: Option<(std::path::PathBuf, String)>,
    /// Arguments appended after the image reference.
    pub command: Vec<String>,
}

/// Starts a detached container from `spec`.
///
/// # Errors
///
/// A [`DockerError`] naming the full `docker run` invocation and `stderr` if
/// the container could not be started, or propagated from
/// [`image_reference::resolve_runnable_reference`] if `spec.image`'s pinned
/// digest is absent and could not be pulled.
pub fn run(spec: &RunSpec) -> Result<Container, DockerError> {
    let image = image_reference::resolve_runnable_reference(&spec.image)?;

    let mut args = vec![
        "run".to_owned(),
        "-d".to_owned(),
        "--name".to_owned(),
        spec.name.clone(),
        "--network".to_owned(),
        spec.network.clone(),
    ];
    for (key, value) in &spec.env {
        args.push("-e".to_owned());
        args.push(format!("{key}={value}"));
    }
    if let Some(container_port) = spec.publish {
        args.push("-p".to_owned());
        args.push(format!("127.0.0.1:0:{container_port}"));
    }
    if let Some((host_dir, container_dir)) = &spec.mount_ro {
        args.push("-v".to_owned());
        args.push(format!("{}:{container_dir}:ro", host_dir.display()));
    }
    args.push(image);
    args.extend(spec.command.iter().cloned());

    process::run_checked(&args)?;
    Ok(Container {
        name: spec.name.clone(),
    })
}

/// Reads back the ephemeral host port Docker chose for `container_port`.
///
/// # Errors
///
/// A [`DockerError`] if `docker port` failed, or its output did not carry a
/// parseable `host:port`.
pub fn port(container: &Container, container_port: u16) -> Result<u16, DockerError> {
    let args = vec![
        "port".to_owned(),
        container.name.clone(),
        container_port.to_string(),
    ];
    let output = process::run_checked(&args)?;

    let text = String::from_utf8_lossy(&output.stdout);
    let first_line = text.lines().next().unwrap_or_default();
    first_line
        .rsplit(':')
        .next()
        .and_then(|port_text| port_text.trim().parse::<u16>().ok())
        .ok_or_else(|| {
            DockerError::from_parts(
                process::describe(&args),
                format!("could not parse a host port from `{first_line}`"),
            )
        })
}

/// Runs `docker exec <container> <args...>`, returning the raw output.
///
/// Deliberately not checked against a zero exit status: a poll loop (`pg_isready`,
/// `ndc-postgres check-health`) wants a non-zero exit to mean "not ready
/// yet", not a hard error. Callers that need a one-shot command to fail
/// loudly pass the result to [`super::process::ensure_success`].
///
/// # Errors
///
/// A [`DockerError`] only if `docker` itself could not be started.
pub fn exec(container: &Container, args: &[&str]) -> Result<Output, DockerError> {
    let mut full = vec!["exec".to_owned(), container.name.clone()];
    full.extend(args.iter().map(|argument| (*argument).to_owned()));
    process::spawn(&full)
}

/// Like [`exec`], piping `input` to the command's stdin -- for `psql` reading
/// a script rather than a `-c` argument.
///
/// # Errors
///
/// A [`DockerError`] if `docker` could not be started, its stdin could not
/// be written, or its output could not be read back.
pub fn exec_with_stdin(container: &Container, args: &[&str], input: &[u8]) -> Result<Output, DockerError> {
    let mut full = vec!["exec".to_owned(), "-i".to_owned(), container.name.clone()];
    full.extend(args.iter().map(|argument| (*argument).to_owned()));

    let mut child = Command::new("docker")
        .args(&full)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|source| {
            DockerError::from_parts(
                process::describe(&full),
                format!("could not start `docker`: {source}"),
            )
        })?;

    let mut stdin = child
        .stdin
        .take()
        .unwrap_or_else(|| unreachable!("stdin was requested as piped"));
    stdin.write_all(input).map_err(|source| {
        DockerError::from_parts(
            process::describe(&full),
            format!("could not write to `docker exec`'s stdin: {source}"),
        )
    })?;
    drop(stdin);

    child.wait_with_output().map_err(|source| {
        DockerError::from_parts(
            process::describe(&full),
            format!("could not read `docker exec`'s output: {source}"),
        )
    })
}

/// Reads `container`'s combined stdout/stderr log so far.
///
/// # Errors
///
/// A [`DockerError`] if `docker logs` failed.
pub fn logs(container: &Container) -> Result<String, DockerError> {
    let output = process::run_checked(&["logs".to_owned(), container.name.clone()])?;
    Ok(format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    ))
}

/// Stops `container` without removing it.
///
/// # Errors
///
/// A [`DockerError`] if `docker stop` failed.
pub fn stop(container: &Container) -> Result<(), DockerError> {
    process::run_checked(&["stop".to_owned(), container.name.clone()]).map(|_| ())
}

/// Force-removes `container`, stopping it first if still running.
///
/// # Errors
///
/// A [`DockerError`] if `docker rm -f` failed.
pub fn rm(container: &Container) -> Result<(), DockerError> {
    rm_by_name(&container.name)
}

/// Like [`rm`], for a container this process did not itself start -- used
/// by [`crate::support::names::sweep_stale`] to remove a prior run's
/// leftovers by name alone.
///
/// # Errors
///
/// A [`DockerError`] if `docker rm -f` failed.
pub fn rm_by_name(name: &str) -> Result<(), DockerError> {
    process::run_checked(&["rm".to_owned(), "-f".to_owned(), name.to_owned()]).map(|_| ())
}

/// Containers currently matching `prefix`, running or not: each one's name
/// paired with `docker ps`'s own `{{.CreatedAt}}` reading for it.
///
/// The pair, not the name alone, is what [`crate::support::names::sweep_stale`]
/// needs to tell a concurrent run's fresh container from a hard-killed run's
/// stale one.
///
/// # Errors
///
/// A [`DockerError`] if `docker ps` failed.
pub fn container_summaries_with_prefix(prefix: &str) -> Result<Vec<(String, String)>, DockerError> {
    let output = process::run_checked(&[
        "ps".to_owned(),
        "-a".to_owned(),
        "--filter".to_owned(),
        format!("name={prefix}"),
        "--format".to_owned(),
        "{{.Names}}\t{{.CreatedAt}}".to_owned(),
    ])?;

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, created_at)| (name.trim().to_owned(), created_at.trim().to_owned()))
        .collect())
}
