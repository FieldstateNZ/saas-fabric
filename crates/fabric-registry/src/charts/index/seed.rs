//! Deserialising just the requested chart's raw entries out of a chart index
//! document, and nothing else in it.
//!
//! # Why a `DeserializeSeed`, and not a `Value` map
//!
//! A chart repository serves every chart it holds in one document, and the
//! obvious way to read only the one asked for -- deserialise `entries` as
//! `BTreeMap<String, serde_norway::Value>`, then look up the requested key --
//! is not safe. A `Value` still *materialises* every other chart's subtree in
//! full, and YAML aliases amplify that: a chart nobody asked for can repeat
//! one anchored node hundreds of thousands of times, and each repetition
//! becomes a fresh, independently allocated `Value` rather than a reference
//! to the first. A neighbour built that way, well inside the streamed body's
//! byte bound, allocates gigabytes reading a document that is itself a
//! couple of megabytes on the wire -- the byte bound this crate enforces
//! bounds what arrives over the wire, not what a `Value` tree costs to hold
//! once aliases have expanded it in memory.
//!
//! [`entries_of`] instead reads the document with this shape: this file
//! walks the top-level mapping looking for `entries`, and [`entries`] walks
//! `entries` itself looking for the requested chart's key, deserialising
//! *that* value into `Entry` and consuming every other value with
//! `serde::de::IgnoredAny`. Ignoring is a walk of the parse event stream
//! with nothing kept -- an alias is one event whether it is skipped once or
//! two hundred thousand times, and the anchor it points at is never
//! re-materialised for each occurrence -- so an unrelated chart's shape,
//! extreme or otherwise, costs this reader time proportional to the bytes it
//! read and no meaningful memory at all.

mod entries;

use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, Visitor};

use entries::Entries;
pub(super) use entries::Entry;

/// Reads `chart`'s raw entries out of a chart index document `body`, and
/// nothing else in it — see the module docs for why that matters.
///
/// # Errors
///
/// A `serde_norway` error if `body` is not a YAML mapping, if `entries` is
/// not itself a mapping, if the requested chart's key appears under
/// `entries` more than once, or if the requested chart's own entries do not
/// match `Entry`'s shape. A malformed entry under any *other* chart's name
/// never reaches any of these checks.
pub(super) fn entries_of(body: &str, chart: &str) -> serde_norway::Result<Vec<Entry>> {
    RequestedChart { chart }.deserialize(serde_norway::Deserializer::from_str(body))
}

/// Reads only `chart`'s releases out of a chart index document.
struct RequestedChart<'a> {
    /// The chart this reader was asked for.
    chart: &'a str,
}

impl<'de> DeserializeSeed<'de> for RequestedChart<'_> {
    type Value = Vec<Entry>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(TopLevel { chart: self.chart })
    }
}

/// Visits the document's top-level mapping, looking only for `entries`.
struct TopLevel<'a> {
    /// The chart this reader was asked for.
    chart: &'a str,
}

impl<'de> Visitor<'de> for TopLevel<'_> {
    type Value = Vec<Entry>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a chart index document")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut releases = None;

        while let Some(key) = map.next_key::<String>()? {
            if key == "entries" {
                releases = Some(map.next_value_seed(Entries { chart: self.chart })?);
            } else {
                // `apiVersion` and anything else this document carries
                // outside `entries` is of no interest, and is walked without
                // being turned into a value.
                map.next_value::<IgnoredAny>()?;
            }
        }

        // Missing `entries` is a chart repository with nothing published,
        // not a malformed one.
        Ok(releases.unwrap_or_default())
    }
}
