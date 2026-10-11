//! What this binary reports for `--contracts`.
//!
//! A release holds each shipped image to `compatibility.yaml` by running the
//! built binary, because only the binary knows what it was compiled with
//! (`scripts/release_compatibility.py binary-contracts`).
//!
//! The runtime plane reads the publication record and never writes it, so it
//! reports the one contract it owns: the reader contract. The publisher
//! contract and the rollback floor are not here on purpose. They live in
//! `fabric-runtime-publication` and `fabric-component`, which no runtime-plane
//! crate may reach (ADR 0018), and the control plane's binary reports those.

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
        "binary": "fabric-api",
        "readerContract": fabric_tenant_runtime::READER_CONTRACT,
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
        assert!(requested(["/etc/fabric/config.toml", "--contracts"]));
        assert!(!requested(["/etc/fabric/config.toml"]));
        assert!(!requested(Vec::<String>::new()));
    }

    #[test]
    fn the_report_names_the_reader_contract_the_runtime_enforces() {
        let report: serde_json::Value = serde_json::from_str(&report()).unwrap();
        assert_eq!(report["binary"], "fabric-api");
        assert_eq!(report["readerContract"], fabric_tenant_runtime::READER_CONTRACT);
        assert_eq!(report["version"], env!("CARGO_PKG_VERSION"));
    }
}
