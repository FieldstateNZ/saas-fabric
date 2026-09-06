//! Container lifecycle: starting one, reading it back, and removing it.
//!
//! The "containers" concept the parent `docker.rs` module doc names. Builds
//! on [`process`](super::process) for the actual `docker` invocations and
//! knows nothing about networks beyond the name a [`RunSpec`] joins.

use std::io::Write as _;
use std::process::{Command, Output, Stdio};

use crate::support::gate;

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

/// Whether `reference` is present locally under that exact name (tag or
/// tag@digest). Never attempts a pull: this machine's daemon may not be able
/// to (see `images.rs`), and CI, which can, is left to `run`'s own pull
/// fallback via a plain `docker run`.
///
/// # Errors
///
/// A [`DockerError`] only if `docker` itself could not be started.
pub fn image_present(reference: &str) -> Result<bool, DockerError> {
    let output = process::spawn(&["image".to_owned(), "inspect".to_owned(), reference.to_owned()])?;
    Ok(output.status.success())
}

/// Resolves `reference` to the string `run` should actually pass to
/// `docker run`.
///
/// A digest-qualified reference (`image:tag@sha256:...`) is used as-is when
/// present locally. When it is not -- a daemon that cannot pull, with only
/// the bare tag loaded by hand rather than through a registry pull, which is
/// this repository's own situation for `images::NDC_POSTGRES` today; see
/// that constant's doc comment -- this falls back to the bare tag, loudly,
/// on one stderr line, rather than silently running different bytes than
/// the pin names. Neither present: returns the original reference unchanged,
/// so `docker run`'s own error (an attempted pull, and whatever that does on
/// this daemon) is what the caller sees, honestly.
///
/// # The required mode disables the fallback
///
/// [`gate::REQUIRE_ENV`] set to `1` -- which CI's `connector-acceptance` job
/// always does -- turns a missing digest-qualified image into an outright
/// failure naming the digest, never a silent fallback to the bare tag. See
/// `gate.rs`'s module doc for why: the fallback exists for exactly one
/// situation, a sandboxed developer machine whose daemon cannot pull, with
/// the tag loaded by hand under a different digest. CI's daemon pulls
/// normally, so a pinned digest genuinely absent there is not that situation
/// -- it is drift between the pin and what the registry now serves, or a
/// typo in the pin -- and running whatever the bare tag happens to resolve
/// to would be running an unpinned image while a pinned-looking test result
/// reported success. Required mode is what makes that impossible instead of
/// merely unlikely.
///
/// # Errors
///
/// A [`DockerError`] only if `docker` itself could not be started, or if
/// [`gate::REQUIRE_ENV`] is `1` and `reference` names a digest that is not
/// present locally.
fn resolve_runnable_reference(reference: &str) -> Result<String, DockerError> {
    if image_present(reference)? {
        return Ok(reference.to_owned());
    }

    let Some((tag, digest)) = reference.split_once('@') else {
        return Ok(reference.to_owned());
    };

    if std::env::var(gate::REQUIRE_ENV).as_deref() == Ok("1") {
        return Err(DockerError::required_digest_missing(reference, digest));
    }

    if image_present(tag)? {
        eprintln!(
            "fabric-ndc-acceptance: {reference} is not present locally by digest; \
             falling back to the bare tag {tag} (see images.rs for why)"
        );
        return Ok(tag.to_owned());
    }

    Ok(reference.to_owned())
}

impl DockerError {
    /// Built by [`resolve_runnable_reference`] when the required mode
    /// refuses the bare-tag fallback. A free function on [`DockerError`]
    /// rather than another `spawn`/`run_checked` path: no `docker` process
    /// runs for this failure at all, so there is no command line or `stderr`
    /// to carry -- only the digest the caller needs to go pull by hand, or
    /// to update if the registry has genuinely moved on.
    fn required_digest_missing(reference: &str, digest: &str) -> Self {
        Self::from_parts(
            format!("docker image inspect {reference}"),
            format!(
                "not present locally by digest {digest}, and {}=1 disables the bare-tag \
                 fallback -- pull it, or update the pin in images.rs if the registry has moved on",
                gate::REQUIRE_ENV
            ),
        )
    }
}

/// Starts a detached container from `spec`.
///
/// # Errors
///
/// A [`DockerError`] naming the full `docker run` invocation and `stderr` if
/// the container could not be started.
pub fn run(spec: &RunSpec) -> Result<Container, DockerError> {
    let image = resolve_runnable_reference(&spec.image)?;

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

/// Container names currently matching `prefix`, running or not.
///
/// # Errors
///
/// A [`DockerError`] if `docker ps` failed.
pub fn container_names_with_prefix(prefix: &str) -> Result<Vec<String>, DockerError> {
    let output = process::run_checked(&[
        "ps".to_owned(),
        "-a".to_owned(),
        "--filter".to_owned(),
        format!("name={prefix}"),
        "--format".to_owned(),
        "{{.Names}}".to_owned(),
    ])?;
    Ok(process::lines(&output))
}
