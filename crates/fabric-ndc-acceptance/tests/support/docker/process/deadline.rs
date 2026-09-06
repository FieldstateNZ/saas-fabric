//! Running a `docker` command with a hard wall-clock deadline.
//!
//! [`super::run_checked`] and [`super::spawn`] never time out: they are
//! right for every `docker` subcommand this harness runs against the local
//! daemon (`run`, `stop`, `rm`, `exec`, `inspect`, `network ...`), which all
//! return promptly or not at all. `docker pull` is the one exception -- a
//! registry the daemon cannot reach makes it hang forever, and that hang is
//! exactly what motivates this file: [`run_with_deadline`] is the only
//! function in this harness that can turn "no answer" into a returned
//! [`DockerError`] instead of a stuck test process. Nothing else in
//! `support/` needs it, so nothing else calls it.

use std::io::Read;
use std::os::unix::process::CommandExt as _;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::{describe, ensure_success, DockerError};

/// How often [`run_with_deadline`] checks whether the child has exited yet.
/// Short enough that a command finishing well inside its deadline is still
/// noticed promptly, without spinning the CPU polling a process that is
/// almost always still running.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Runs `docker <args...>`, killing it and returning a [`DockerError`]
/// naming the command and `deadline` if it has not exited by then.
///
/// Reads stdout and stderr on their own threads while polling, rather than
/// waiting for the process to exit first: `docker pull`'s progress output
/// can exceed the OS pipe buffer, and a child blocked writing to a full,
/// unread pipe would make this function report a timeout on output alone --
/// for a pull that was otherwise about to succeed.
///
/// Spawns `docker` as the leader of its own process group
/// ([`process_group(0)`](std::os::unix::process::CommandExt::process_group))
/// so a timeout can kill that whole group, not just the one pid -- see
/// [`kill_process_group`] for why that turned out to be load-bearing, not
/// defensive. This assumes a Unix-like host, which the rest of this harness
/// already does (bind-mount syntax, `127.0.0.1`-only port publishing): the
/// developer machine this was built against is macOS, and CI is
/// `ubuntu-latest`.
///
/// # Errors
///
/// A [`DockerError`] if `docker` could not be started, did not exit within
/// `deadline`, or exited with a non-zero status.
pub(in super::super) fn run_with_deadline(
    args: &[String],
    deadline: Duration,
) -> Result<Output, DockerError> {
    let mut child = Command::new("docker")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .map_err(|source| {
            DockerError::from_parts(
                describe(args),
                format!("could not run the `docker` binary: {source}"),
            )
        })?;
    let pgid = child.id();

    let stdout_reader = spawn_reader(child.stdout.take());
    let stderr_reader = spawn_reader(child.stderr.take());

    let status = poll_to_exit_or_kill(&mut child, pgid, deadline);

    let stdout = join_reader(stdout_reader);
    let stderr = join_reader(stderr_reader);

    let Some(status) = status else {
        return Err(DockerError::from_parts(
            describe(args),
            format!("did not complete within {deadline:?} and was killed"),
        ));
    };

    let output = Output {
        status,
        stdout,
        stderr,
    };
    ensure_success(&describe(args), &output)?;
    Ok(output)
}

/// Polls `child` for exit at [`POLL_INTERVAL`] until it exits or `deadline`
/// elapses since this call started.
///
/// A timeout kills `child`'s whole process group ([`kill_process_group`])
/// and reaps `child` before returning `None`, so the caller never leaves a
/// zombie process behind -- killing alone does not reap; the following
/// `wait` does.
fn poll_to_exit_or_kill(child: &mut Child, pgid: u32, deadline: Duration) -> Option<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if start.elapsed() >= deadline {
            kill_process_group(pgid);
            let _ = child.wait();
            return None;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Sends `SIGKILL` to every process in `pgid`'s group, not only `pgid`
/// itself.
///
/// Observed directly on this repository's own blocked-registry machine:
/// `docker pull`'s credential lookup runs as a further child of `docker`
/// (`docker-credential-desktop get`), and *that* grandchild -- not the
/// `docker pull` process itself -- is what actually blocks forever when the
/// registry route is unreachable. Killing only the immediate child (a plain
/// [`Child::kill`]) leaves that grandchild running, and it inherited this
/// function's stdout/stderr pipes from its parent -- so it keeps their
/// write ends open after the pid we did kill is gone. A reader thread
/// blocked in `read_to_end` on the other end of that pipe then never sees
/// an EOF, which turns a bounded pull into an unbounded hang one process
/// down instead of ending it. [`run_with_deadline`] spawns `docker` as its
/// own process-group leader (`process_group(0)`) precisely so `pgid` here
/// -- the child's own pid, doubling as the group id -- reaches that
/// grandchild too: sending the signal to `-pgid` targets the whole group,
/// the grandchild dies with the child, and its copy of the pipe closes,
/// which is what actually lets the reader threads return.
///
/// Shells out to the `kill` utility rather than a libc binding: this crate
/// has no C-interop dependency today (`Cargo.toml` carries only
/// `[dev-dependencies]`), and every other function in this module already
/// drives a process by name the same way.
fn kill_process_group(pgid: u32) {
    let _ = Command::new("kill").arg("-KILL").arg(format!("-{pgid}")).status();
}

/// Spawns a thread that drains `pipe` to completion, so a child writing more
/// than the OS pipe buffer holds never blocks on output nobody is reading
/// while [`poll_to_exit_or_kill`] is busy polling instead.
fn spawn_reader(pipe: Option<impl Read + Send + 'static>) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut buffer);
        }
        buffer
    })
}

/// Joins a reader thread. `read_to_end` does not panic, so the only way
/// `join` fails is a bug elsewhere; treating that as empty output here keeps
/// it from masking whichever error [`run_with_deadline`] would otherwise
/// report.
fn join_reader(handle: JoinHandle<Vec<u8>>) -> Vec<u8> {
    handle.join().unwrap_or_default()
}
