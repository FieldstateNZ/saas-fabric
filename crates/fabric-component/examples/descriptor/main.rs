//! Renders and checks a component descriptor from a `component.yaml`, with
//! the renderer this repository builds -- the command a release runs (ADR
//! 0026 section 10).
//!
//! ```text
//! descriptor check  <component.yaml>
//! descriptor images <component.yaml>
//! descriptor render <component.yaml> --version <v> --digest <role>=<digest>... [--out <file>]
//! ```
//!
//! `check` renders at version `0.0.0` with a synthetic digest for every
//! role and reads the result back through the parser a server uses.
//! `images` prints each role and its repository, one per line, for a
//! workflow to read. `render` writes the canonical bytes to standard output,
//! or to `--out`.
//!
//! # Why an example, and not a binary
//!
//! It is a thin command over the library's own renderer, needed by this
//! repository's release and by nothing it ships. An example builds with the
//! crate, adds no dependency and no installable binary, and is run with
//! `cargo run -q -p fabric-component --example descriptor -- …`. Arguments
//! are parsed by hand for the same reason. Every failure is a message on
//! standard error and exit status 2; nothing panics.
mod render;
use fabric_component::{ComponentDescriptor, ComponentSource, Digest};
use std::io::Write;
use std::process::ExitCode;

/// The digest `check` gives every role: well-formed, and naming nothing.
const SYNTHETIC_DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

/// How to call this.
pub(crate) const USAGE: &str = "usage: descriptor check <component.yaml>\n       descriptor images <component.yaml>\n       descriptor render <component.yaml> --version <v> --digest <role>=<digest> [--digest ...] [--out <file>]";

fn main() -> ExitCode {
    // `std::env::args` panics on an argument that is not UTF-8; reading them
    // as `OsString`s turns that into a refusal like any other.
    let arguments: Result<Vec<String>, String> = std::env::args_os()
        .skip(1)
        .map(|argument| {
            argument
                .into_string()
                .map_err(|argument| format!("an argument is not UTF-8: {}", argument.to_string_lossy()))
        })
        .collect();
    match arguments.and_then(|arguments| run(&arguments)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("descriptor: {message}");
            ExitCode::from(2)
        }
    }
}

/// Dispatches on the command.
fn run(arguments: &[String]) -> Result<(), String> {
    let [command, path, rest @ ..] = arguments else {
        return Err(USAGE.to_owned());
    };
    let source = read_source(path)?;
    match (command.as_str(), rest) {
        ("check", []) => check(path, &source),
        ("images", []) => {
            let lines: Vec<String> = source
                .images()
                .map(|(role, repository)| format!("{role} {repository}\n"))
                .collect();
            write_stdout(lines.concat().as_bytes())
        }
        ("render", options) => render::render(&source, options),
        _ => Err(USAGE.to_owned()),
    }
}

/// Reads and parses `component.yaml`.
fn read_source(path: &str) -> Result<ComponentSource, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    ComponentSource::from_yaml(&text).map_err(|error| format!("{path}: {error}"))
}

/// Renders with synthetic digests and reads the result back.
fn check(path: &str, source: &ComponentSource) -> Result<(), String> {
    let digest = Digest::try_new(SYNTHETIC_DIGEST).map_err(|error| error.to_string())?;
    let digests = source
        .images()
        .map(|(role, _)| (role.clone(), digest.clone()))
        .collect();
    let descriptor = source
        .render("0.0.0", &digests)
        .map_err(|error| format!("{path}: {error}"))?;
    let bytes = descriptor.to_json();
    let read = ComponentDescriptor::from_json(&bytes).map_err(|error| format!("{path}: {error}"))?;
    if read != descriptor {
        return Err(format!("{path}: the rendered descriptor did not read back equal"));
    }
    let line = format!(
        "{path}: {} renders a valid component descriptor naming {} images in {} bytes\n",
        descriptor.spec().name,
        descriptor.spec().images.len(),
        bytes.len()
    );
    write_stdout(line.as_bytes())
}

/// Writes to standard output, reporting a closed pipe rather than
/// panicking as `print!` would.
pub(crate) fn write_stdout(bytes: &[u8]) -> Result<(), String> {
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(bytes)
        .and_then(|()| stdout.flush())
        .map_err(|error| format!("standard output: {error}"))
}
