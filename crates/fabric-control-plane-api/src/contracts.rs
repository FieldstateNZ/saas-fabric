//! What this binary reports for `--contracts`.
//!
//! A release holds each shipped image to `compatibility.yaml` by running the
//! built binary, because only the binary knows what it was compiled with
//! (`scripts/release_compatibility.py binary-contracts`).
//!
//! The control plane is the publisher, so it reports the contract it writes,
//! the oldest reader that can serve it, and the rollback floor it enforces.
//! The reader contract is the runtime plane's and is reported by `fabric-api`:
//! this plane may not link `fabric-tenant-runtime`.

use fabric_component::MINIMUM_ROLLABLE_VERSION;
use fabric_runtime_publication::{CONTRACT_VERSION, MINIMUM_READER_CONTRACT};
use serde_json::json;

/// The flag that prints [`report`] and exits.
pub const FLAG: &str = "--contracts";

/// Whether `arguments` (without the program name) ask for the report.
pub fn requested<I>(arguments: I) -> bool
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    arguments.into_iter().any(|argument| argument.as_ref() == FLAG)
}

/// The report: one line of JSON, keys in a fixed order, so the same build
/// always prints the same bytes.
#[must_use]
pub fn report() -> String {
    json!({
        "binary": "fabric-control-plane-api",
        "minimumReaderContract": MINIMUM_READER_CONTRACT,
        "minimumRollableVersion": MINIMUM_ROLLABLE_VERSION,
        "publisherContract": CONTRACT_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_flag_is_recognised_anywhere_among_the_arguments() {
        assert!(requested(["--contracts"]));
        assert!(requested(["/etc/fabric/control-plane.toml", "--contracts"]));
        assert!(!requested(["/etc/fabric/control-plane.toml"]));
        assert!(!requested(Vec::<String>::new()));
    }

    #[test]
    fn the_report_carries_the_constants_the_publisher_is_built_with() {
        let report: serde_json::Value = serde_json::from_str(&report()).unwrap();
        assert_eq!(report["binary"], "fabric-control-plane-api");
        assert_eq!(report["publisherContract"], CONTRACT_VERSION);
        assert_eq!(report["minimumReaderContract"], MINIMUM_READER_CONTRACT);
        assert_eq!(report["minimumRollableVersion"], MINIMUM_ROLLABLE_VERSION);
        assert_eq!(report["version"], env!("CARGO_PKG_VERSION"));
    }
}
