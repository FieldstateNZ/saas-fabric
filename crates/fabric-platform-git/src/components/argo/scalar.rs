//! One `key: value` line, split so everything but the value can be put back.

use super::lines::Line;
use super::{value, Refusal};

/// A line read as a mapping key and its value.
///
/// The five pieces concatenate back into the line they came from, exactly:
/// `lead + key + ":" + gap + value + suffix`. That invariant is the whole
/// point — rewriting means replacing the value and emitting the other four
/// unchanged, so the indentation, the spacing the author chose after the
/// colon, their quoting and any trailing comment all survive a version bump
/// byte for byte.
pub(super) struct Scalar<'a> {
    /// Everything before the key: the indentation, and a `- ` when the key is
    /// the first one of a sequence entry.
    lead: &'a str,

    /// The key itself, matched whole — `targetRevisionOverride` is a different
    /// key from `targetRevision`, and a prefix test would confuse them.
    pub(super) key: &'a str,

    /// The whitespace between the colon and the value.
    gap: &'a str,

    /// The value token, quotes included, or empty when there is none.
    value: &'a str,

    /// Whatever followed it: trailing spaces, a comment.
    suffix: &'a str,
}

impl<'a> Scalar<'a> {
    /// Reads `from` — a suffix of `line`'s content — as `key: value`.
    ///
    /// `None` means the line is not a mapping key at all. `key:value` without
    /// the space is one plain scalar in YAML rather than a key, and reading it
    /// as one is how a renderer starts editing text that means something else.
    pub(super) fn read(line: &Line<'a>, from: &'a str) -> Option<Self> {
        let lead = line.content.strip_suffix(from)?;
        let (key, after) = from.split_once(':')?;

        if !(after.is_empty() || after.starts_with(' ')) {
            return None;
        }

        let tail = after.trim_start_matches(' ');
        let (value, suffix) = value::split(tail);

        Some(Self {
            lead,
            key,
            gap: after.get(..after.len() - tail.len()).unwrap_or_default(),
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

    /// Whether the key carries a value beside it on its own line.
    ///
    /// `spec:` and `sources:` are read for their structure, not their content,
    /// so a value beside either — `sources: [...]` — is a shape the walk
    /// cannot enter and must refuse rather than step past.
    pub(super) const fn carries_a_value(&self) -> bool {
        !self.value.is_empty()
    }

    /// The line rewritten with `version` in place of its value.
    ///
    /// The author's quoting is kept, because the quoting is theirs: dropping
    /// it changes a line that was only supposed to gain a version.
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

        if !value::is_plain(version) {
            return Err(format!(
                "cannot be moved to '{version}', which YAML would not read back as the \
                 one word it was written as"
            ));
        }

        let Self {
            lead,
            key,
            gap,
            suffix,
            ..
        } = *self;
        Ok(format!("{lead}{key}:{gap}{quote}{version}{quote}{suffix}"))
    }
}
