//! Parsing the recovery command's arguments.

use std::path::PathBuf;

use fabric_tenant_runtime::DiscardRequest;

/// What the operator asked for.
#[derive(Debug, Clone)]
pub enum Action {
    /// Report the copy, valid or not. Writes nothing.
    Inspect {
        /// The last-known-good directory.
        directory: PathBuf,
    },
    /// Report the copy and exit non-zero unless a reader would serve it.
    /// Writes nothing.
    Validate {
        /// The last-known-good directory.
        directory: PathBuf,
    },
    /// Audit, then move the copy aside.
    Discard {
        /// The last-known-good directory.
        directory: PathBuf,
        /// Who and why, both required.
        request: DiscardRequest,
    },
}

/// Parses the arguments after the program name, `last-known-good` first.
///
/// # Errors
///
/// A usage message for an unknown action, a missing or repeated option, an
/// unknown option, or a blank `--operator` or `--reason`.
pub fn parse<I>(arguments: I) -> Result<Action, String>
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let arguments: Vec<String> = arguments
        .into_iter()
        .map(|argument| argument.as_ref().to_owned())
        .collect();
    let mut words = arguments.iter().skip(1);
    let action = words.next().ok_or_else(usage)?.clone();
    let (mut directory, mut operator, mut reason) = (None, None, None);
    while let Some(option) = words.next() {
        let slot = match option.as_str() {
            "--dir" => &mut directory,
            "--operator" => &mut operator,
            "--reason" => &mut reason,
            other => return Err(format!("unknown option {other}\n{}", usage())),
        };
        let value = words
            .next()
            .ok_or_else(|| format!("{option} needs a value\n{}", usage()))?;
        if slot.replace(value.clone()).is_some() {
            return Err(format!("{option} given twice"));
        }
    }
    let directory = PathBuf::from(directory.ok_or_else(|| format!("--dir is required\n{}", usage()))?);
    match (action.as_str(), operator, reason) {
        ("inspect", None, None) => Ok(Action::Inspect { directory }),
        ("validate", None, None) => Ok(Action::Validate { directory }),
        ("discard", Some(operator), Some(reason)) => {
            if operator.trim().is_empty() || reason.trim().is_empty() {
                return Err("discard needs a non-blank --operator and --reason; both are audited".to_owned());
            }
            Ok(Action::Discard {
                directory,
                request: DiscardRequest { operator, reason },
            })
        }
        ("discard", _, _) => Err("discard needs --operator and --reason; both are audited".to_owned()),
        ("inspect" | "validate", _, _) => Err(format!("{action} takes only --dir")),
        _ => Err(format!("unknown action {action}\n{}", usage())),
    }
}

fn usage() -> String {
    "usage: fabric-api last-known-good <inspect|validate> --dir <directory>\n       \
     fabric-api last-known-good discard --dir <directory> --operator <who> --reason <why>"
        .to_owned()
}
