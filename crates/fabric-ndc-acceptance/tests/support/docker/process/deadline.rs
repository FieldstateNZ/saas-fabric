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

/// How often [`poll_to_exit_or_kill`] checks whether the child has exited,
/// and how often [`join_readers_bounded`] checks whether its reader threads
/// have finished. Short enough that either wait is noticed promptly without
/// spinning the CPU on a process or thread that is almost always still
/// running or reading.
const POLL_INTERVAL: Duration = Duration::from_millis(200);

/// How long, once `docker`'s own exit status is known, [`join_readers_bounded`]
/// waits for the stdout/stderr reader threads to finish on their own before
/// concluding that something other than `docker` itself is still holding a
/// pipe open. See that function's doc for the concrete case this guards.
const READER_JOIN_DEADLINE: Duration = Duration::from_secs(3);

/// How much of a killed pull's stderr [`run_with_deadline`] folds into its
/// timeout error. Enough to show the registry's or credential helper's own
/// last words -- a TLS failure, a redirect notice, an auth prompt -- without
/// repeating a pull's entire progress stream, which can run to megabytes.
const STDERR_TAIL_BYTES: usize = 400;

/// Runs `docker <args...>`, killing it if it has not exited by `deadline`.
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
/// A [`DockerError`] if `docker` could not be started, exited with a
/// non-zero status, or did not exit within `deadline` -- in the timeout
/// case, naming `deadline` itself, whether the `kill` utility used to stop
/// the whole process group could even be run (see [`kill_process_group`]),
/// and the tail of whatever stderr had already been read before the kill
/// (see [`STDERR_TAIL_BYTES`]).
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

    let outcome = poll_to_exit_or_kill(&mut child, pgid, deadline);
    // Bounded even on the success path: `child`'s own exit does not close a
    // pipe a grandchild still holds open (see `join_readers_bounded`'s doc).
    let (stdout, stderr) = join_readers_bounded(pgid, stdout_reader, stderr_reader);

    let status = match outcome {
        ExitOutcome::Exited(status) => status,
        ExitOutcome::TimedOut(kill_result) => {
            // `kill_result` is the *group* kill's own success at being
            // started, not a claim that everything it targeted is dead --
            // see `kill_process_group`'s doc. An `Err` here means the `kill`
            // utility itself could not be run at all (missing from `PATH`,
            // no permission to exec it, ...), which is worth knowing: it is
            // the one case where the grandchild that doc describes is not
            // just probably dead, but never asked to die by this call.
            let kill_detail = match kill_result {
                Ok(_) => String::new(),
                Err(error) => format!(
                    "; the `kill` utility itself could not be run ({error}), so a lingering \
                     grandchild may not have been killed -- see kill_process_group's doc"
                ),
            };
            return Err(DockerError::from_parts(
                describe(args),
                format!(
                    "did not complete within {deadline:?} and was killed{kill_detail}; stderr \
                     so far: {}",
                    tail(&stderr, STDERR_TAIL_BYTES)
                ),
            ));
        }
    };

    let output = Output {
        status,
        stdout,
        stderr,
    };
    ensure_success(&describe(args), &output)?;
    Ok(output)
}

/// What became of waiting for `child` to exit within its deadline.
enum ExitOutcome {
    /// Exited on its own, in time.
    Exited(ExitStatus),
    /// Did not exit in time and was killed. Carries the result of *starting*
    /// [`kill_process_group`]'s `kill` utility -- not a guarantee about what
    /// that utility went on to do -- so [`run_with_deadline`] can say, in the
    /// rare case the utility itself could not run at all, that its usual
    /// cleanup of a lingering grandchild did not happen either.
    TimedOut(std::io::Result<ExitStatus>),
}

/// Polls `child` for exit at [`POLL_INTERVAL`] until it exits or `deadline`
/// elapses since this call started.
///
/// A timeout kills `child`'s whole process group ([`kill_process_group`]),
/// also kills `child` itself directly via [`Child::kill`] (belt and braces:
/// that std API does not depend on the external `kill` binary being on
/// `PATH` the way [`kill_process_group`] does, though on its own it would
/// still leave a grandchild holding the pipes open -- see that function's
/// doc), and reaps `child` before returning, so the caller never leaves a
/// zombie process behind -- killing alone does not reap; the following
/// `wait` does. [`Child::kill`]'s own result is not worth surfacing: an
/// `Err` from it means `child` had already exited, which is not a failure
/// worth reporting on a path that is about to kill it anyway.
fn poll_to_exit_or_kill(child: &mut Child, pgid: u32, deadline: Duration) -> ExitOutcome {
    let start = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return ExitOutcome::Exited(status);
        }
        if start.elapsed() >= deadline {
            let kill_result = kill_process_group(pgid);
            let _ = child.kill();
            let _ = child.wait();
            return ExitOutcome::TimedOut(kill_result);
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
///
/// # Errors
///
/// An [`std::io::Error`] only if the `kill` utility itself could not be
/// started -- not if it ran and reported nothing left to kill, which is the
/// ordinary case once `pgid` is already fully reaped and not itself a sign
/// anything is wrong.
fn kill_process_group(pgid: u32) -> std::io::Result<ExitStatus> {
    Command::new("kill").arg("-KILL").arg(format!("-{pgid}")).status()
}

/// Joins both reader threads, tolerating a pipe that `child`'s own exit did
/// not close.
///
/// `child` exiting reaps only *its* copy of a pipe's write end, not a
/// grandchild's: [`kill_process_group`]'s doc describes the concrete case --
/// `docker pull`'s own credential-helper process, still holding the pipe
/// open after `docker pull` itself has already returned. A reader thread
/// blocked in `read_to_end` on that pipe then never sees an EOF, so joining
/// it without a bound here would trade the very hang [`run_with_deadline`]
/// exists to prevent for an identical one one step later, on the success
/// path this time -- a pull that finished fine, immediately followed by a
/// join that never does.
///
/// Polling [`JoinHandle::is_finished`] for [`READER_JOIN_DEADLINE`] costs an
/// ordinary reader nothing: with nothing left to drain it finishes within
/// milliseconds of `child` exiting, well inside the bound. Only when a
/// reader is still running after that does this fall back to killing
/// `pgid`'s whole group -- the same mechanism a timeout already uses --
/// which closes every remaining copy of the write end and is what actually
/// lets the read return. That fallback kill's own result is not surfaced
/// the way [`ExitOutcome::TimedOut`]'s is: this path only runs after `child`
/// has already exited normally, so there is no timeout error for it to
/// attach to, and a `kill` that could not be started here still leaves
/// [`join_reader`]'s `unwrap_or_default` as a safe (if silent) way out
/// rather than hanging.
fn join_readers_bounded(
    pgid: u32,
    stdout_reader: JoinHandle<Vec<u8>>,
    stderr_reader: JoinHandle<Vec<u8>>,
) -> (Vec<u8>, Vec<u8>) {
    let both_finished =
        |out: &JoinHandle<Vec<u8>>, err: &JoinHandle<Vec<u8>>| out.is_finished() && err.is_finished();

    let start = Instant::now();
    while start.elapsed() < READER_JOIN_DEADLINE && !both_finished(&stdout_reader, &stderr_reader) {
        thread::sleep(POLL_INTERVAL);
    }

    if !both_finished(&stdout_reader, &stderr_reader) {
        let _ = kill_process_group(pgid);
    }

    (join_reader(stdout_reader), join_reader(stderr_reader))
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

/// The last `max_bytes` of `bytes`, lossily decoded as UTF-8 and trimmed.
///
/// Used to fold a killed pull's stderr into [`run_with_deadline`]'s timeout
/// error without repeating a pull's entire (potentially large) progress
/// output -- the tail is where a registry's or credential helper's own last
/// words actually are. `bytes.get(start..)` rather than `bytes[start..]`:
/// this workspace denies `clippy::indexing_slicing`, and `start` is computed
/// from `bytes.len()` so the `get` can never actually miss, but the
/// `unwrap_or` fallback keeps that a property of the arithmetic below rather
/// than a panic waiting to be wrong.
fn tail(bytes: &[u8], max_bytes: usize) -> String {
    let start = bytes.len().saturating_sub(max_bytes);
    String::from_utf8_lossy(bytes.get(start..).unwrap_or(bytes))
        .trim()
        .to_owned()
}
