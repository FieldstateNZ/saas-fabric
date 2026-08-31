//! Where the platform repository is, and which branch is its desired state.

/// The one repository, and the one branch, this adapter writes to.
///
/// There is no path-prefix field and no file list. Which files a change
/// touches is decided by the caller *per change*, and which files a component
/// maps to is the platform domain's business — but the repository and the
/// branch are configuration, because pointing this at a different branch is a
/// deployment decision rather than a request-time one.
#[derive(Debug, Clone)]
pub struct PlatformRepositoryConfig {
    /// The API root, so a test or an Enterprise host can be pointed elsewhere.
    pub api_base_url: String,

    /// The account the repository belongs to.
    pub owner: String,

    /// The repository name.
    pub repository: String,

    /// The branch the environment follows.
    pub branch: String,

    /// How long any one request may take.
    pub http_timeout_seconds: u64,
}

impl PlatformRepositoryConfig {
    /// Checks the configuration names a repository this can address.
    ///
    /// # Errors
    ///
    /// Returns a message naming the field, never its value: an API base URL
    /// can carry a host nobody meant to publish, and a message is a log line.
    pub fn validate(&self) -> Result<(), String> {
        for (field, value) in [
            ("api_base_url", &self.api_base_url),
            ("owner", &self.owner),
            ("repository", &self.repository),
            ("branch", &self.branch),
        ] {
            if value.trim().is_empty() {
                return Err(format!("platform repository: {field} is empty"));
            }
        }

        if !self.api_base_url.starts_with("http://") && !self.api_base_url.starts_with("https://") {
            return Err("platform repository: api_base_url is not an HTTP URL".to_owned());
        }

        if self.http_timeout_seconds == 0 {
            return Err("platform repository: http_timeout_seconds is zero".to_owned());
        }

        Ok(())
    }

    /// The `{owner}/{repository}` segment every URL starts with.
    pub(crate) fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.repository)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> PlatformRepositoryConfig {
        PlatformRepositoryConfig {
            api_base_url: "https://api.github.com".to_owned(),
            owner: "FieldstateNZ".to_owned(),
            repository: "saas-fabric-platform".to_owned(),
            branch: "main".to_owned(),
            http_timeout_seconds: 10,
        }
    }

    #[test]
    fn a_complete_configuration_is_accepted() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn every_required_field_is_actually_required() {
        for (field, blank) in [
            (
                "api_base_url",
                (|c: &mut PlatformRepositoryConfig| c.api_base_url = "  ".to_owned())
                    as fn(&mut PlatformRepositoryConfig),
            ),
            ("owner", |c| c.owner = String::new()),
            ("repository", |c| c.repository = String::new()),
            ("branch", |c| c.branch = String::new()),
        ] {
            let mut config = valid();
            blank(&mut config);

            let message = config.validate().expect_err(&format!("{field} was not required"));
            assert!(message.contains(field), "{message}");
        }
    }

    #[test]
    fn a_base_url_that_is_not_http_is_refused() {
        // Not pedantry: a value that is not a URL becomes a request to a path
        // relative to nothing, and the failure surfaces as an unreadable
        // transport error much later.
        let mut config = valid();
        config.api_base_url = "api.github.com".to_owned();

        assert!(config.validate().is_err());
    }

    #[test]
    fn a_zero_timeout_is_refused() {
        // reqwest reads zero as "no timeout", so this is the difference between
        // a bounded call and one that can hang a reconciliation forever.
        let mut config = valid();
        config.http_timeout_seconds = 0;

        assert!(config.validate().is_err());
    }

    #[test]
    fn a_message_never_carries_the_value_it_rejected() {
        // An API base URL can name a host nobody meant to publish, and a
        // validation message is a log line.
        let mut config = valid();
        config.api_base_url = "ftp://internal.example.invalid".to_owned();

        let message = config.validate().expect_err("should be refused");
        assert!(!message.contains("internal.example.invalid"), "{message}");
    }
}
