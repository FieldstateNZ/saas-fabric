//! A thin driver over `std::process::Command` for the `docker` CLI.
//!
//! Deliberately not `testcontainers` (lead decision 2): the lifecycle this
//! harness needs is one network and a couple of containers, and this
//! workspace already hand-rolls its test-convenience layer rather than add
//! one (`TempDir` in `fabric-runtime-publication/tests/support/mod.rs`
//! instead of `tempfile`). Every function here that can fail returns a
//! [`DockerError`] carrying the exact command line and `stderr` -- nothing
//! is swallowed, because a container harness that fails silently just looks
//! like a flaky test to whoever hits it next.

use std::io::Write as _;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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
    pub mount_ro: Option<(PathBuf, String)>,
    /// Arguments appended after the image reference.
    pub command: Vec<String>,
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

/// Whether `reference` is present locally under that exact name (tag or
/// tag@digest). Never attempts a pull: this machine's daemon may not be able
/// to (see `images.rs`), and CI, which can, is left to `run`'s own pull
/// fallback via a plain `docker run`.
///
/// # Errors
///
/// A [`DockerError`] only if `docker` itself could not be started.
pub fn image_present(reference: &str) -> Result<bool, DockerError> {
    let output = spawn(&["image".to_owned(), "inspect".to_owned(), reference.to_owned()])?;
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
fn resolve_runnable_reference(reference: &str) -> Result<String, DockerError> {
    if image_present(reference)? {
        return Ok(reference.to_owned());
    }

    if let Some((tag, _digest)) = reference.split_once('@') {
        if image_present(tag)? {
            eprintln!(
                "fabric-ndc-acceptance: {reference} is not present locally by digest; \
                 falling back to the bare tag {tag} (see images.rs for why)"
            );
            return Ok(tag.to_owned());
        }
    }

    Ok(reference.to_owned())
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

    run_checked(&args)?;
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
    let output = run_checked(&args)?;

    let text = String::from_utf8_lossy(&output.stdout);
    let first_line = text.lines().next().unwrap_or_default();
    first_line
        .rsplit(':')
        .next()
        .and_then(|port_text| port_text.trim().parse::<u16>().ok())
        .ok_or_else(|| DockerError {
            command: describe(&args),
            detail: format!("could not parse a host port from `{first_line}`"),
        })
}

/// Runs `docker exec <container> <args...>`, returning the raw output.
///
/// Deliberately not checked against a zero exit status: a poll loop (`pg_isready`,
/// `ndc-postgres check-health`) wants a non-zero exit to mean "not ready
/// yet", not a hard error. Callers that need a one-shot command to fail
/// loudly pass the result to [`ensure_success`].
///
/// # Errors
///
/// A [`DockerError`] only if `docker` itself could not be started.
pub fn exec(container: &Container, args: &[&str]) -> Result<Output, DockerError> {
    let mut full = vec!["exec".to_owned(), container.name.clone()];
    full.extend(args.iter().map(|argument| (*argument).to_owned()));
    spawn(&full)
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
        .map_err(|source| DockerError {
            command: describe(&full),
            detail: format!("could not start `docker`: {source}"),
        })?;

    let mut stdin = child
        .stdin
        .take()
        .unwrap_or_else(|| unreachable!("stdin was requested as piped"));
    stdin.write_all(input).map_err(|source| DockerError {
        command: describe(&full),
        detail: format!("could not write to `docker exec`'s stdin: {source}"),
    })?;
    drop(stdin);

    child.wait_with_output().map_err(|source| DockerError {
        command: describe(&full),
        detail: format!("could not read `docker exec`'s output: {source}"),
    })
}

/// Turns a completed command's exit status into a `Result`, carrying
/// `description` and `stderr` when it failed. Every "did this actually
/// work" check that [`exec`] itself deliberately leaves open goes through
/// here, so a swallowed failure is a missing call to this, not a missing
/// feature.
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

/// Reads `container`'s combined stdout/stderr log so far.
///
/// # Errors
///
/// A [`DockerError`] if `docker logs` failed.
pub fn logs(container: &Container) -> Result<String, DockerError> {
    let output = run_checked(&["logs".to_owned(), container.name.clone()])?;
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
    run_checked(&["stop".to_owned(), container.name.clone()]).map(|_| ())
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
    run_checked(&["rm".to_owned(), "-f".to_owned(), name.to_owned()]).map(|_| ())
}

/// Creates a Docker network.
///
/// # Errors
///
/// A [`DockerError`] if `docker network create` failed.
pub fn network_create(name: &str) -> Result<(), DockerError> {
    run_checked(&["network".to_owned(), "create".to_owned(), name.to_owned()]).map(|_| ())
}

/// Removes a Docker network.
///
/// # Errors
///
/// A [`DockerError`] if `docker network rm` failed.
pub fn network_rm(name: &str) -> Result<(), DockerError> {
    run_checked(&["network".to_owned(), "rm".to_owned(), name.to_owned()]).map(|_| ())
}

/// Container names currently matching `prefix`, running or not.
///
/// # Errors
///
/// A [`DockerError`] if `docker ps` failed.
pub fn container_names_with_prefix(prefix: &str) -> Result<Vec<String>, DockerError> {
    let output = run_checked(&[
        "ps".to_owned(),
        "-a".to_owned(),
        "--filter".to_owned(),
        format!("name={prefix}"),
        "--format".to_owned(),
        "{{.Names}}".to_owned(),
    ])?;
    Ok(lines(&output))
}

/// Network names currently matching `prefix`.
///
/// # Errors
///
/// A [`DockerError`] if `docker network ls` failed.
pub fn network_names_with_prefix(prefix: &str) -> Result<Vec<String>, DockerError> {
    let output = run_checked(&[
        "network".to_owned(),
        "ls".to_owned(),
        "--filter".to_owned(),
        format!("name={prefix}"),
        "--format".to_owned(),
        "{{.Name}}".to_owned(),
    ])?;
    Ok(lines(&output))
}

/// Calls `attempt` in a loop until it returns `true` or `deadline` has
/// elapsed since this call started, sleeping briefly in between. The
/// deadline bounds how long polling continues; it is never itself the
/// signal that the condition holds.
#[must_use]
pub fn poll_until(deadline: Duration, mut attempt: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if attempt() {
            return true;
        }
        if start.elapsed() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

/// Non-empty, trimmed lines of `output`'s stdout.
fn lines(output: &Output) -> Vec<String> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// `docker <args...>`, as a string, for error messages.
fn describe(args: &[String]) -> String {
    format!("docker {}", args.join(" "))
}

/// Runs `docker <args...>`, erroring on a non-zero exit as well as a spawn
/// failure.
fn run_checked(args: &[String]) -> Result<Output, DockerError> {
    let output = spawn(args)?;
    ensure_success(&describe(args), &output)?;
    Ok(output)
}

/// Runs `docker <args...>`, erroring only if the process could not be
/// started at all.
fn spawn(args: &[String]) -> Result<Output, DockerError> {
    Command::new("docker")
        .args(args)
        .output()
        .map_err(|source| DockerError {
            command: describe(args),
            detail: format!("could not run the `docker` binary: {source}"),
        })
}
