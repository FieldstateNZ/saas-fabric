//! Reading a header value a byte, a token or a quoted string at a time.

/// A position in a header value.
pub(super) struct Cursor<'a> {
    /// The value.
    text: &'a str,

    /// The byte offset read so far.
    at: usize,
}

impl<'a> Cursor<'a> {
    /// The start of `text`.
    pub(super) fn new(text: &'a str) -> Self {
        Self { text, at: 0 }
    }

    /// The next byte, unread.
    pub(super) fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    /// Skips every byte in `set`.
    pub(super) fn skip(&mut self, set: &[u8]) {
        while self.peek().is_some_and(|byte| set.contains(&byte)) {
            self.at += 1;
        }
    }

    /// A token: one or more `tchar`s.
    pub(super) fn token(&mut self) -> Option<&'a str> {
        let start = self.at;
        while self.peek().is_some_and(is_tchar) {
            self.at += 1;
        }
        (self.at > start).then(|| self.text.get(start..self.at)).flatten()
    }

    /// The next parameter's lower-cased name and its `=`, or `None` — with
    /// nothing consumed — when what follows is not a parameter.
    pub(super) fn parameter_name(&mut self) -> Option<String> {
        let mark = self.at;
        self.skip(b", \t");
        let name = self.token().map(str::to_ascii_lowercase);
        self.skip(b" \t");
        if name.is_some() && self.peek() == Some(b'=') {
            self.at += 1;
            return name;
        }
        self.at = mark;
        None
    }

    /// A quoted string, its escapes undone; `None` if it never closes.
    pub(super) fn quoted(&mut self) -> Option<String> {
        let mut value = String::new();
        let mut rest = self.text.get(self.at + 1..)?.char_indices();
        while let Some((offset, character)) = rest.next() {
            match character {
                '"' => {
                    self.at += 1 + offset + 1;
                    return Some(value);
                }
                '\\' => value.push(rest.next()?.1),
                other => value.push(other),
            }
        }
        None
    }
}

/// Whether `byte` may appear in a token (RFC 9110 section 5.6.2).
fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}
