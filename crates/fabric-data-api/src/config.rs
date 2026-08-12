//! Configuration for the Data API.

/// Limits and defaults applied to Data API requests.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct DataApiConfig {
    /// Page size when a caller does not ask for one.
    pub default_limit: u32,

    /// The largest page a caller may request.
    ///
    /// Clamped rather than rejected: a caller asking for a million rows gets
    /// the maximum, not a 400. This is a resource guard, and a hard ceiling on
    /// how much work one request can ask a shared database for matters more in
    /// a multi-tenant system than in a single-tenant one — one tenant's
    /// unbounded scan is every co-tenant's latency.
    pub max_limit: u32,
}

impl Default for DataApiConfig {
    fn default() -> Self {
        Self {
            default_limit: 50,
            max_limit: 1000,
        }
    }
}

impl DataApiConfig {
    /// Applies the configured default and ceiling to a requested page size.
    #[must_use]
    pub const fn effective_limit(&self, requested: Option<u32>) -> u32 {
        match requested {
            Some(limit) if limit > self.max_limit => self.max_limit,
            // A caller explicitly asking for zero rows gets zero rows; it is a
            // legitimate way to probe for existence cheaply.
            Some(limit) => limit,
            None => self.default_limit,
        }
    }

    /// Checks the configuration before anything is built.
    ///
    /// # Errors
    ///
    /// Returns a message if the limits are unusable.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_limit == 0 {
            return Err("data_api.max_limit must be greater than zero".to_owned());
        }

        if self.default_limit > self.max_limit {
            return Err("data_api.default_limit must not exceed max_limit".to_owned());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_limit_uses_the_default() {
        assert_eq!(DataApiConfig::default().effective_limit(None), 50);
    }

    #[test]
    fn an_excessive_limit_is_clamped_rather_than_rejected() {
        assert_eq!(DataApiConfig::default().effective_limit(Some(1_000_000)), 1000);
    }

    #[test]
    fn a_reasonable_limit_is_honoured() {
        assert_eq!(DataApiConfig::default().effective_limit(Some(25)), 25);
    }

    #[test]
    fn a_zero_limit_is_honoured_as_an_existence_probe() {
        assert_eq!(DataApiConfig::default().effective_limit(Some(0)), 0);
    }

    #[test]
    fn a_default_above_the_ceiling_is_rejected_at_startup() {
        let config = DataApiConfig {
            default_limit: 100,
            max_limit: 10,
        };

        assert!(config.validate().is_err());
    }
}
