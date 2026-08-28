//! Splitting and decoding a raw query string.

/// Splits a query string into decoded key/value pairs.
pub(super) fn parse_pairs(raw: &str) -> Vec<(String, String)> {
    raw.split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(key), percent_decode(value))
        })
        .collect()
}

/// Decodes `+` and `%XX` escapes.
///
/// Hand-written rather than pulled from a crate: the rules are short, and a
/// dependency for `%20` is not worth the supply chain.
///
/// A stray `%` that is not a valid escape is kept as written rather than
/// dropped, so a malformed value fails field validation instead of silently
/// changing meaning.
fn percent_decode(input: &str) -> String {
    let bytes = input.replace('+', " ").into_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        match bytes.get(index) {
            Some(b'%') => {
                if let Some(byte) = hex_escape(&bytes, index) {
                    decoded.push(byte);
                    index += 3;
                } else {
                    decoded.push(b'%');
                    index += 1;
                }
            }
            Some(byte) => {
                decoded.push(*byte);
                index += 1;
            }
            None => break,
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

/// Reads the two hex digits following a `%`, if they are there and valid.
fn hex_escape(bytes: &[u8], percent: usize) -> Option<u8> {
    let pair = bytes.get(percent + 1..percent + 3)?;

    u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_pairs_on_ampersands() {
        assert_eq!(
            parse_pairs("a=1&b=2"),
            [("a".to_owned(), "1".to_owned()), ("b".to_owned(), "2".to_owned())]
        );
    }

    #[test]
    fn a_key_with_no_value_reads_as_empty() {
        assert_eq!(parse_pairs("flag"), [("flag".to_owned(), String::new())]);
    }

    #[test]
    fn decodes_percent_escapes() {
        assert_eq!(parse_pairs("name=Alice%20Smith")[0].1, "Alice Smith");
    }

    #[test]
    fn decodes_a_plus_as_a_space() {
        assert_eq!(parse_pairs("name=Alice+Smith")[0].1, "Alice Smith");
    }

    #[test]
    fn an_invalid_escape_is_preserved_rather_than_dropped() {
        // It must reach field validation and be rejected there, not silently
        // become a different value.
        assert_eq!(parse_pairs("name=100%zz")[0].1, "100%zz");
    }

    #[test]
    fn a_trailing_percent_does_not_run_off_the_end() {
        assert_eq!(parse_pairs("name=abc%")[0].1, "abc%");
    }
}
