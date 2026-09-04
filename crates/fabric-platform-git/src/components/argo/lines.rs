//! A line of an Argo Application, kept exactly as it was read.

use super::Refusal;

/// A line of the document, with the bytes that ended it.
///
/// # Why the terminator travels with the line
///
/// A version bump has to produce a diff that says "one version moved" and
/// nothing else, because that is the diff an operator can review in a glance.
/// [`str::lines`] throws away which terminator each line had and whether the
/// file ended with one at all, so writing the lines back with a `\n` apiece
/// quietly converts a CRLF file to LF and adds a final newline its author left
/// off — a whole file rewritten, attached to a routine bump, in a repository
/// where every commit is desired state. So each line keeps its own ending, which
/// [`str::split_inclusive`] leaves attached to the line it ended.
pub(super) struct Line<'a> {
    /// The line without its terminator.
    pub(super) content: &'a str,

    /// The leading spaces, exactly as written.
    spaces: &'a str,

    /// Everything after them.
    pub(super) rest: &'a str,

    /// The bytes that ended the line: `"\n"`, `"\r\n"`, or nothing at all on a
    /// final line whose author left the newline off.
    pub(super) terminator: &'a str,
}

impl<'a> Line<'a> {
    /// Splits a document into lines.
    ///
    /// # Errors
    ///
    /// A clause saying why, if a line's indentation cannot be told from its
    /// content. Refusing beats guessing: a fallback here would hand back a line
    /// shorter than the one that went in.
    pub(super) fn split(text: &'a str) -> Result<Vec<Self>, Refusal> {
        text.split_inclusive('\n').map(Self::read).collect()
    }

    /// How deep the line sits, in spaces. Depth is the only measurement this
    /// renderer takes of a line's shape, because every question it asks — is
    /// this list `spec.sources`, is this key the entry's own — is about depth.
    pub(super) const fn indent(&self) -> usize {
        self.spaces.len()
    }

    /// Whether the line is empty or only whitespace.
    pub(super) fn is_blank(&self) -> bool {
        self.rest.trim_start().is_empty()
    }

    /// Whether the line says anything structural. Blank lines and whole-line
    /// comments are stepped over rather than allowed to set an indent, which is
    /// also why neither is ever measured — or refused for how it is indented.
    pub(super) fn is_significant(&self) -> bool {
        let bare = self.rest.trim_start();
        !bare.is_empty() && !bare.starts_with('#')
    }

    /// Whether a tab stands in this line's indentation.
    ///
    /// Only where indentation is what the tab is doing. A tab *within* a line —
    /// separating a comment, or inside a block scalar's text — is content that
    /// YAML and this renderer both leave alone.
    pub(super) fn is_tab_indented(&self) -> bool {
        self.rest.starts_with('\t')
    }

    /// Whether this is a `---` document marker, which starts a fresh document
    /// with a `spec:` of its own.
    pub(super) fn starts_a_document(&self) -> bool {
        self.spaces.is_empty() && self.rest.starts_with("---")
    }

    /// Whether the line opens a sequence entry.
    pub(super) fn opens_entry(&self) -> bool {
        self.rest == "-" || self.rest.starts_with("- ")
    }

    /// The first key written on a `- ` line, with the column it starts at.
    ///
    /// `- repoURL: …` puts a source's first key on the same line as the dash
    /// that opens it, and that column is where the rest of its keys sit.
    pub(super) fn after_dash(&self) -> Option<(usize, &'a str)> {
        let after = self.rest.strip_prefix('-')?;
        let text = after.trim_start_matches(' ');
        (!text.is_empty()).then(|| (self.content.len() - text.len(), text))
    }

    /// Reads one line, terminator and all.
    fn read(raw: &'a str) -> Result<Self, Refusal> {
        let (content, terminator) = Self::unterminate(raw);
        let rest = content.trim_start_matches(' ');
        let spaces = content
            .strip_suffix(rest)
            .ok_or_else(|| "has a line whose indentation could not be told from its content".to_owned())?;

        Ok(Self {
            content,
            spaces,
            rest,
            terminator,
        })
    }

    /// Separates a line from the bytes that ended it.
    fn unterminate(raw: &str) -> (&str, &str) {
        let Some(body) = raw.strip_suffix('\n') else {
            return (raw, "");
        };

        body.strip_suffix('\r')
            .map_or((body, "\n"), |body| (body, "\r\n"))
    }
}
