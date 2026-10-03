//! Whether one version of a described component is a release unit, asked
//! the way discovery asks it (ADR 0026 section 3): the evaluator a release
//! job runs after pushing a component descriptor.
//!
//! ```text
//! cargo run -q -p fabric-registry --example release_unit -- \
//!     <repository> <version> [--base-url <https url>] [--host <host>]
//! ```
//!
//! `<repository>` is the primary image's, written in full, and `<version>`
//! a component version; either one malformed is a bad argument. The registry
//! is read anonymously, at `https://<the repository's host>` and named by
//! that host unless told otherwise — `https://ghcr.io` as `ghcr.io` for a
//! GHCR repository — with a ten-second timeout, and every repository counts as registered: this
//! asks whether the release is whole, not whether an environment pins it.
//! It prints the answer, the reason when invalid, and when complete the
//! component descriptor's digest and each role's; it exits `0` only when
//! complete, `1` otherwise, and `2` on arguments it does not understand. An
//! example rather than a binary: it ships in no image, and runs the very
//! `evaluate` discovery does.

#[path = "release_unit/arguments.rs"]
mod arguments;

use std::process::ExitCode;

use fabric_platform_management::{evaluate, Evaluation, Expectation};
use fabric_registry::OciRegistry;

use self::arguments::arguments;

/// How long any one request may take, in seconds.
const TIMEOUT_SECONDS: u64 = 10;

/// What to type.
const USAGE: &str = "usage: release_unit <repository> <version> [--base-url <https url>] [--host <host>]";

#[tokio::main]
async fn main() -> ExitCode {
    let asked = match arguments(std::env::args_os().skip(1)) {
        Ok(asked) => asked,
        Err(problem) => return refuse(&format!("{problem}\n{USAGE}")),
    };

    let registry = match OciRegistry::new(asked.base_url, asked.host, TIMEOUT_SECONDS) {
        Ok(registry) => registry,
        Err(problem) => return refuse(&problem),
    };

    let everything = |_: &str| true;
    let expectation = Expectation::Registered(&everything);
    let answer = evaluate(&registry, &asked.repository, &asked.version, expectation).await;

    match answer {
        Ok(Evaluation::Complete(release)) => {
            println!("complete");
            println!("component descriptor {}", release.descriptor_digest);
            println!("built from {}", release.unit.source_revision);
            for (role, image) in &release.unit.images {
                println!("{role} {}@{}", image.repository, image.digest);
            }
            ExitCode::SUCCESS
        }
        Ok(Evaluation::Invalid(reason)) => {
            println!("invalid: {} ({reason})", reason.code());
            ExitCode::FAILURE
        }
        Ok(Evaluation::Undescribed) => answer_only("undescribed"),
        Ok(Evaluation::Incoherent) => answer_only("incoherent"),
        Ok(Evaluation::NotTagged) => answer_only("not tagged"),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Prints an answer that says nothing more, and fails.
fn answer_only(answer: &str) -> ExitCode {
    println!("{answer}");
    ExitCode::FAILURE
}

/// Says what was wrong with the arguments, and exits `2`.
fn refuse(problem: &str) -> ExitCode {
    eprintln!("{problem}");
    ExitCode::from(2)
}
