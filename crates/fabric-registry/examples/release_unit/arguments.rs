//! The evaluator's command line, read by hand and checked before anything
//! is asked of a registry.

use std::ffi::OsString;

use fabric_component::{ComponentVersion, Repository};

/// What to ask, from the command line.
pub(crate) struct Arguments {
    pub(crate) repository: String,
    pub(crate) version: String,
    pub(crate) base_url: String,
    pub(crate) host: String,
}

/// Reads the arguments by hand: two positional, two optional flags.
///
/// # Why each is checked here
///
/// A repository or a version this platform could never name would otherwise
/// reach a registry as a path or a query and come back as an answer about
/// the release — `not tagged`, or a refusal — and exit `1`, as though the
/// release were at fault. A host other than the repository's would strip
/// nothing from its path and read every tag as missing.
pub(crate) fn arguments(given: impl Iterator<Item = OsString>) -> Result<Arguments, String> {
    let mut given = given
        .map(|argument| {
            argument
                .into_string()
                .map_err(|_| "an argument is not UTF-8".to_owned())
        })
        .collect::<Result<Vec<String>, String>>()?
        .into_iter();
    let mut positional = Vec::new();
    let mut base_url = None;
    let mut host = None;

    while let Some(argument) = given.next() {
        match argument.as_str() {
            "--base-url" => base_url = Some(given.next().ok_or("--base-url needs a value")?),
            "--host" => host = Some(given.next().ok_or("--host needs a value")?),
            flag if flag.starts_with("--") => return Err(format!("unknown option {flag}")),
            _ => positional.push(argument),
        }
    }

    let [repository, version]: [String; 2] = positional
        .try_into()
        .map_err(|_| "expected a repository and a version".to_owned())?;
    let named = Repository::try_new(&repository).map_err(|problem| format!("<repository>: {problem}"))?;
    ComponentVersion::try_new(&version).map_err(|problem| format!("<version>: {problem}"))?;

    let host = host.unwrap_or_else(|| named.host().to_owned());
    if named.host() != host {
        return Err(format!("the repository is not on {host}"));
    }
    let base_url = base_url.unwrap_or_else(|| format!("https://{host}"));

    Ok(Arguments {
        repository,
        version,
        base_url,
        host,
    })
}
