//! Configuration for the Data API.

/// Limits and defaults applied to Data API requests.
///
/// Two families live here. `default_limit`/`max_limit` bound how much data one
/// response can carry; everything else bounds how much *work* a single
/// request can demand before a connector is ever asked to do it — a
/// multi-tenant service has to protect co-tenants from one caller's
/// pathological request, not just from its own result size (§28).
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

    /// The largest number of equality filters one list request may supply.
    ///
    /// Unlike `max_limit`, this is rejected rather than clamped: there is no
    /// sensible way to silently drop a caller's filter and still answer the
    /// query they asked for.
    pub max_filters: u32,

    /// The largest number of `sort` fields one list request may supply.
    pub max_sort_fields: u32,

    /// The largest number of `select` (projection) fields one list request
    /// may supply.
    pub max_select_fields: u32,

    /// The deepest a filter tree may nest.
    ///
    /// The query language this crate parses is flat today — the deepest a
    /// filter gets is an `And` of `Compare` clauses, which is depth two — so
    /// this bound is inert in practice. It exists anyway so that if the query
    /// language ever grows nested predicates, there is already a ceiling in
    /// place rather than one added after the fact.
    pub max_filter_depth: u32,

    /// The largest request body, in bytes, this crate will read before
    /// rejecting a request.
    ///
    /// Enforced while the body is being read (`extraction::BoundedJson`), not
    /// by trusting a caller-supplied `Content-Length` — a body limit that only
    /// checks the header is not a body limit.
    pub max_request_body_bytes: u32,

    /// The largest number of rows one `POST` may create.
    pub max_mutation_batch_size: u32,
}

impl Default for DataApiConfig {
    fn default() -> Self {
        Self {
            default_limit: 50,
            max_limit: 1000,
            max_filters: 25,
            max_sort_fields: 5,
            max_select_fields: 50,
            max_filter_depth: 4,
            max_request_body_bytes: 1024 * 1024,
            max_mutation_batch_size: 500,
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
    /// Every bound below has to be usable — zero would mean every request of
    /// that shape is refused, which is almost certainly a misconfiguration
    /// rather than an intention, and is cheaper to catch here than from a
    /// flood of confused callers.
    ///
    /// # Errors
    ///
    /// Returns a message naming the first unusable bound.
    pub fn validate(&self) -> Result<(), String> {
        if self.max_limit == 0 {
            return Err("data_api.max_limit must be greater than zero".to_owned());
        }

        if self.default_limit > self.max_limit {
            return Err("data_api.default_limit must not exceed max_limit".to_owned());
        }

        for (name, value) in [
            ("max_filters", self.max_filters),
            ("max_sort_fields", self.max_sort_fields),
            ("max_select_fields", self.max_select_fields),
            ("max_filter_depth", self.max_filter_depth),
            ("max_request_body_bytes", self.max_request_body_bytes),
            ("max_mutation_batch_size", self.max_mutation_batch_size),
        ] {
            if value == 0 {
                return Err(format!("data_api.{name} must be greater than zero"));
            }
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
            ..DataApiConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn the_defaults_are_themselves_valid() {
        assert!(DataApiConfig::default().validate().is_ok());
    }

    #[test]
    fn a_zero_complexity_bound_is_rejected_at_startup() {
        let config = DataApiConfig {
            max_filters: 0,
            ..DataApiConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn a_zero_body_size_bound_is_rejected_at_startup() {
        let config = DataApiConfig {
            max_request_body_bytes: 0,
            ..DataApiConfig::default()
        };

        assert!(config.validate().is_err());
    }

    #[test]
    fn a_zero_batch_size_bound_is_rejected_at_startup() {
        let config = DataApiConfig {
            max_mutation_batch_size: 0,
            ..DataApiConfig::default()
        };

        assert!(config.validate().is_err());
    }
}
