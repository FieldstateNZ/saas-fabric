use fabric_platform_management::{Diagnostics, InvalidReason, InvalidVersion, Version};
use serde_json::json;

use super::DiagnosticRow;

fn version(text: &str) -> Version {
    Version::parse(text).unwrap()
}

#[test]
fn every_list_is_its_own_state_and_only_an_invalid_row_carries_a_reason() {
    let diagnostics = Diagnostics {
        not_yet: vec![version("0.3.0-preview.6")],
        incoherent: vec![version("0.3.0-preview.5")],
        undescribed: vec![version("0.3.0-preview.4")],
        invalid: vec![InvalidVersion {
            version: version("0.3.0-preview.3"),
            reason: InvalidReason::MissingImage {
                role: "console".to_owned(),
            },
        }],
    };

    let rows = serde_json::to_value(DiagnosticRow::every(&diagnostics)).unwrap();

    // The role stays behind: the console words the reason from its code.
    assert_eq!(
        rows,
        json!([
            { "version": "0.3.0-preview.6", "state": "publishing" },
            { "version": "0.3.0-preview.4", "state": "undescribed" },
            { "version": "0.3.0-preview.5", "state": "incoherent" },
            { "version": "0.3.0-preview.3", "state": "invalid", "reason": "missingImage" },
        ])
    );
}

#[test]
fn a_reason_is_sent_as_its_code() {
    let reasons = [
        InvalidReason::Unreadable,
        InvalidReason::UnsupportedVersion {
            found: "v2".to_owned(),
        },
        InvalidReason::NotPinned,
    ];

    for reason in reasons {
        let code = reason.code();
        let diagnostics = Diagnostics {
            invalid: vec![InvalidVersion {
                version: version("0.3.0-preview.3"),
                reason,
            }],
            ..Diagnostics::default()
        };

        let rows = serde_json::to_value(DiagnosticRow::every(&diagnostics)).unwrap();

        assert_eq!(rows[0]["reason"], code);
    }
}

#[test]
fn an_unsupported_version_names_the_version_found_and_nothing_else_does() {
    let row = |reason| {
        let diagnostics = Diagnostics {
            invalid: vec![InvalidVersion {
                version: version("0.3.0-preview.3"),
                reason,
            }],
            ..Diagnostics::default()
        };
        serde_json::to_value(DiagnosticRow::every(&diagnostics)).unwrap()[0].clone()
    };

    assert_eq!(
        row(InvalidReason::UnsupportedVersion {
            found: "v2".to_owned()
        }),
        json!({ "version": "0.3.0-preview.3", "state": "invalid", "reason": "unsupportedVersion", "found": "v2" })
    );
    assert_eq!(
        row(InvalidReason::UnsupportedVersion {
            found: format!("v{}", "9".repeat(40))
        })["found"],
        serde_json::Value::Null,
        "a run longer than any real version is left out"
    );
    assert_eq!(row(InvalidReason::OtherRegistry)["reason"], "otherRegistry");
    assert_eq!(row(InvalidReason::OtherRegistry).get("found"), None);
}

#[test]
fn nothing_passed_over_is_no_rows() {
    let rows = serde_json::to_value(DiagnosticRow::every(&Diagnostics::default())).unwrap();

    assert_eq!(rows, json!([]));
}
