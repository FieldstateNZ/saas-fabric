//! The requested chart's own value within `entries`, and nothing else in it.

use serde::de::{DeserializeSeed, Error as DeError, IgnoredAny, MapAccess, Visitor};

/// One published release of a chart.
#[derive(serde::Deserialize)]
pub(in crate::charts::index) struct Entry {
    /// The chart version, which is what Argo pins.
    pub(in crate::charts::index) version: String,
}

/// Visits `entries`' mapping, deserialising only the requested chart's value.
pub(super) struct Entries<'a> {
    /// The chart this reader was asked for.
    pub(super) chart: &'a str,
}

impl<'de> DeserializeSeed<'de> for Entries<'_> {
    type Value = Vec<Entry>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(self)
    }
}

impl<'de> Visitor<'de> for Entries<'_> {
    type Value = Vec<Entry>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a mapping of chart name to its releases")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut found: Option<Vec<Entry>> = None;

        while let Some(key) = map.next_key::<String>()? {
            if key == self.chart {
                if found.is_some() {
                    // Last-wins would pick silently between two statements of
                    // what this chart's releases are; there is no reason to
                    // prefer either.
                    return Err(DeError::custom(format_args!(
                        "{} is listed under entries more than once",
                        self.chart
                    )));
                }
                found = Some(map.next_value::<Vec<Entry>>()?);
            } else {
                // Somebody else's chart. However it is shaped -- a scalar, a
                // mapping, a list of aliases repeating one node thousands of
                // times -- this walks the event stream and keeps none of it.
                // See the grandparent module's docs for why that is
                // load-bearing and not just tidy.
                map.next_value::<IgnoredAny>()?;
            }
        }

        Ok(found.unwrap_or_default())
    }
}
