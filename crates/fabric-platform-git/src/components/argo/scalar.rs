//! One `key: value` line, split so everything but the value can be put back.

use super::lines::Line;
use super::{value, Refusal};

/// A line read as a mapping key and its value.
///
/// The five pieces concatenate back into the line they came from, exactly:
/// `lead + key + ":" + gap + value + suffix`. Rewriting replaces the value and
/// emits the other four unchanged, so indentation, the separation after the
/// colon, the author's quoting and any trailing comment all survive byte for
/// byte. There is no lossy fallback: a piece that cannot be cut exactly makes
/// the line not a key, rather than a line that comes back shorter.
pub(super) struct Scalar<'a> {
    /// Everything before the key: the indentation, and a `- ` when the key is
    /// the first one of a sequence entry.
    lead: &'a str,

    /// The key itself, matched whole — `targetRevisionOverride` is a different
    /// key from `targetRevision`, and a prefix test would confuse them.
    pub(super) key: &'a str,

    /// The separation between the colon and the value: spaces, tabs, or both.
    gap: &'a str,

    /// The value token, quotes included, or empty when there is none.
    value: &'a str,

    /// Whatever followed it: separation, a comment.
    suffix: &'a str,
}

impl<'a> Scalar<'a> {
    /// Reads `from` — a suffix of `line`'s content — as `key: value`.
    ///
    /// `None` means the line is not a mapping key at all: `key:value` without
    /// separation is one plain scalar in YAML, and reading it as a key is how a
    /// renderer starts editing text that means something else.
    pub(super) fn read(line: &Line<'a>, from: &'a str) -> Option<Self> {
        let lead = line.content.strip_suffix(from)?;
        let (key, after) = from.split_once(':')?;

        if !(after.is_empty() || after.starts_with([' ', '\t'])) {
            return None;
        }

        let tail = after.trim_start_matches([' ', '\t']);
        let (value, suffix) = value::split(tail)?;

        Some(Self {
            lead,
            key,
            gap: after.strip_suffix(tail)?,
            value,
            suffix,
        })
    }

    /// The value with one matching pair of quotes taken off, for comparison.
    pub(super) fn unquoted(&self) -> &'a str {
        ['"', '\'']
            .into_iter()
            .find_map(|quote| {
                self.value
                    .strip_prefix(quote)
                    .and_then(|inner| inner.strip_suffix(quote))
            })
            .unwrap_or(self.value)
    }

    /// Whether the key carries a value beside it on its own line. `spec:` and
    /// `sources:` are read for structure, not content, so a value beside either
    /// — `sources: [...]` — is a shape the walk must refuse, not step past.
    pub(super) const fn carries_a_value(&self) -> bool {
        !self.value.is_empty()
    }

    /// Whether the value is a block scalar header, so the lines below it are
    /// text rather than structure.
    pub(super) fn opens_a_block(&self) -> bool {
        self.value.starts_with(['|', '>'])
    }

    /// The line rewritten with `version` in place of its value.
    ///
    /// The author's quoting is kept, because it is theirs: dropping it changes
    /// a line that was only supposed to gain a version.
    ///
    /// # Errors
    ///
    /// A clause saying why, when the value that is there cannot be replaced
    /// without understanding more of YAML than a version bump should, or when
    /// `version` itself would not read back as the word it was written as.
    pub(super) fn rewrite(&self, version: &str) -> Result<String, Refusal> {
        let quote = match self.value.chars().next() {
            None => return Err("declares a targetRevision with no value".to_owned()),
            Some('"') => "\"",
            Some('\'') => "'",
            Some('|' | '>' | '{' | '[' | '&' | '*' | '!') => {
                return Err("declares a targetRevision that is a block scalar, a flow \
                            collection, an anchor, an alias or a tag, none of which is a \
                            version to move"
                    .to_owned())
            }
            Some(_) => "",
        };

        if !value::is_writable(version) {
            return Err(format!(
                "cannot be moved to '{version}', which is not a version a chart \
                 repository publishes and not one YAML would read back as written"
            ));
        }

        Ok(format!(
            "{}{}:{}{quote}{version}{quote}{}",
            self.lead, self.key, self.gap, self.suffix
        ))
    }
}
