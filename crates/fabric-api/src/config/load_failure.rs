//! Saying which of the two configuration sources actually failed.

use figment::Source;

use crate::config::env_namespace::ENV_PREFIX;

/// Describes a load failure, naming the source it came from.
///
/// # Why this is not one format string
///
/// Configuration is layered from a file and the environment, and the wrapper
/// used to name the file unconditionally: `could not load configuration from
/// /etc/fabric/config.toml: unknown field …`. When the environment was at
/// fault that sentence sent an operator to edit a file which did not contain
/// the setting and could not be changed to fix it.
///
/// figment records the provider on the error, so the distinction is available
/// rather than guessed: a file provider carries [`Source::File`], and the
/// environment provider carries no source at all.
pub(super) fn describe(error: &figment::Error, path: &str) -> String {
    if from_environment(error) {
        return format!(
            "could not load configuration from a {ENV_PREFIX}* environment variable — not from \
             {path}, which cannot fix this: {error}"
        );
    }

    format!("could not load configuration from {path}: {error}")
}

/// Whether the environment provider produced this error.
///
/// Absence of a file source is the signal. It is the right way round: a new
/// provider added later is attributed to the environment only if it genuinely
/// has no file behind it, and a file error can never be misreported as an
/// environment one.
fn from_environment(error: &figment::Error) -> bool {
    !matches!(
        error
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.source.as_ref()),
        Some(Source::File(_))
    )
}

#[cfg(test)]
mod tests {
    use figment::Metadata;

    use super::*;

    fn error_from(metadata: Metadata) -> figment::Error {
        let mut error = figment::Error::from("something went wrong".to_owned());
        error.metadata = Some(metadata);
        error
    }

    #[test]
    fn an_environment_failure_says_so_and_absolves_the_file() {
        let message = describe(
            &error_from(Metadata::named("environment variable(s)")),
            "/etc/f.toml",
        );

        assert!(message.contains("FABRIC_SETTING_* environment variable"));
        assert!(message.contains("cannot fix this"));
    }

    #[test]
    fn a_file_failure_still_names_the_file_and_not_the_environment() {
        let metadata = Metadata::from("TOML file", std::path::Path::new("/etc/f.toml"));

        let message = describe(&error_from(metadata), "/etc/f.toml");

        assert!(message.starts_with("could not load configuration from /etc/f.toml:"));
        assert!(!message.contains("environment variable"));
    }

    #[test]
    fn an_error_carrying_no_provider_is_not_blamed_on_the_file() {
        // Better to over-report the environment than to send an operator to
        // edit a file that has nothing to do with the failure.
        let message = describe(&figment::Error::from("no metadata".to_owned()), "/etc/f.toml");

        assert!(message.contains("environment variable"));
    }
}
