//! Parsing the recovery command's arguments.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fabric_runtime_publication::DocumentKind;

use super::{Action, Target};

const OPTIONS: [&str; 9] = [
    "--tenants",
    "--data-sources",
    "--catalog",
    "--namespace",
    "--document",
    "--from",
    "--audit",
    "--operator",
    "--reason",
];

/// Parses the arguments after the program name, `held-publication` first.
///
/// # Errors
///
/// A usage message for an unknown action or option, a missing, repeated or
/// blank value, a target that is neither three paths nor a namespace, or a
/// restore without every one of its options.
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
    let mut values: BTreeMap<&'static str, String> = BTreeMap::new();
    while let Some(option) = words.next() {
        let Some(name) = OPTIONS.iter().find(|name| **name == option) else {
            return Err(format!("unknown option {option}\n{}", usage()));
        };
        let value = words
            .next()
            .ok_or_else(|| format!("{option} needs a value\n{}", usage()))?;
        if value.trim().is_empty() {
            return Err(format!("{option} needs a non-blank value"));
        }
        if values.insert(name, value.clone()).is_some() {
            return Err(format!("{option} given twice"));
        }
    }
    let target = target(&mut values)?;
    let action = match action.as_str() {
        "inspect" => Action::Inspect { target },
        "validate" => Action::Validate { target },
        "restore" => {
            let mut take = |name: &str| {
                values.remove(name).ok_or_else(|| {
                    "restore needs --document, --from, --audit, --operator and --reason; the last three are audited"
                        .to_owned()
                })
            };
            let document = match take("--document")?.as_str() {
                "tenants" => DocumentKind::Tenants,
                "data-sources" => DocumentKind::DataSources,
                other => return Err(format!("--document is tenants or data-sources, not {other}")),
            };
            Action::Restore {
                target,
                document,
                from: PathBuf::from(take("--from")?),
                audit: PathBuf::from(take("--audit")?),
                operator: take("--operator")?,
                reason: take("--reason")?,
            }
        }
        _ => return Err(format!("unknown action {action}\n{}", usage())),
    };
    match values.keys().next() {
        Some(extra) => Err(format!("{extra} is not an option of this action\n{}", usage())),
        None => Ok(action),
    }
}

fn target(values: &mut BTreeMap<&'static str, String>) -> Result<Target, String> {
    let paths = (
        values.remove("--tenants"),
        values.remove("--data-sources"),
        values.remove("--catalog"),
    );
    match (paths, values.remove("--namespace")) {
        ((Some(tenants), Some(data_sources), Some(catalog)), None) => Ok(Target::Filesystem {
            tenants: tenants.into(),
            data_sources: data_sources.into(),
            catalog: catalog.into(),
        }),
        ((None, None, None), Some(namespace)) => Ok(Target::Kubernetes { namespace }),
        _ => Err(format!(
            "the target is --tenants, --data-sources and --catalog together, or --namespace alone\n{}",
            usage()
        )),
    }
}

fn usage() -> String {
    "usage: fabric-control-plane-api held-publication <inspect|validate> <target>\n       \
     fabric-control-plane-api held-publication restore <target> --document <tenants|data-sources> \
     --from <file> --audit <file> --operator <who> --reason <why>\n\
     target: --tenants <file> --data-sources <file> --catalog <file> | --namespace <namespace>"
        .to_owned()
}
