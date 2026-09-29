//! `render`: the canonical bytes for a real version and real digests.
use crate::{write_stdout, USAGE};
use fabric_component::{ComponentSource, Digest, Role};
use std::collections::BTreeMap;

/// Renders for a real version and real digests.
pub(crate) fn render(source: &ComponentSource, options: &[String]) -> Result<(), String> {
    let mut version = None;
    let mut out = None;
    let mut digests = BTreeMap::new();
    let mut remaining = options.iter();
    while let Some(option) = remaining.next() {
        let value = remaining
            .next()
            .ok_or_else(|| format!("{option} needs a value\n{USAGE}"))?;
        match option.as_str() {
            "--version" if version.is_none() => version = Some(value.as_str()),
            "--out" if out.is_none() => out = Some(value.as_str()),
            "--digest" => {
                let (role, digest) = parse_digest(value)?;
                if digests.insert(role, digest).is_some() {
                    return Err(format!("--digest {value} names a role twice"));
                }
            }
            _ => return Err(format!("unexpected {option}\n{USAGE}")),
        }
    }
    let version = version.ok_or_else(|| format!("--version is required\n{USAGE}"))?;
    let descriptor = source
        .render(version, &digests)
        .map_err(|error| error.to_string())?;
    let bytes = descriptor.to_json();
    match out {
        Some(path) => std::fs::write(path, bytes).map_err(|error| format!("{path}: {error}")),
        None => write_stdout(&bytes),
    }
}

/// Parses `<role>=<digest>`.
fn parse_digest(value: &str) -> Result<(Role, Digest), String> {
    let (role, digest) = value
        .split_once('=')
        .ok_or_else(|| format!("--digest {value} is not <role>=<digest>"))?;
    let role = Role::try_new(role).map_err(|error| error.to_string())?;
    let digest = Digest::try_new(digest).map_err(|error| error.to_string())?;
    Ok((role, digest))
}
