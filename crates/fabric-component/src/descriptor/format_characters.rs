//! Unicode format characters (general category `Cf`), refused in a
//! component descriptor's declared text.
//!
//! # Why only here
//!
//! A bidirectional override or a zero-width joiner in a title makes it
//! display as something it is not -- a component could present itself as
//! another written in the other direction. The component descriptor is text
//! from outside, from whoever published an image, so its parser refuses them
//! (ADR 0026 section 2). The shared field validators do not: they also check
//! authored content and every frozen copy on each catalogue read, and
//! tightening them would make an already stored catalogue unreadable.
//!
//! The ranges are `Cf` as of Unicode 16, written out because no dependency
//! carries character categories and the standard library has no query for
//! one. Within v1 the list is fixed: adding a code point would refuse a
//! document an earlier build accepted, which is a tightening, and so a new
//! component descriptor version rather than an edit here.
use crate::errors::{invalid, ContractError};

/// Every `Cf` code point, as inclusive ranges.
const FORMAT_CHARACTERS: [(u32, u32); 21] = [
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

/// Whether `character` is a Unicode format character.
#[must_use]
pub(crate) fn is_format_character(character: char) -> bool {
    let code = u32::from(character);
    FORMAT_CHARACTERS
        .iter()
        .any(|(first, last)| (*first..=*last).contains(&code))
}

/// Refuses `value` if it holds a format character, naming it by `label`.
pub(crate) fn refuse_format_characters(value: &str, label: &str) -> Result<(), ContractError> {
    if value.chars().any(is_format_character) {
        return Err(invalid(format!(
            "{label} must not contain Unicode format characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ranges_are_ordered_and_disjoint() {
        for pair in FORMAT_CHARACTERS.windows(2) {
            if let [(first_start, first_end), (second_start, _)] = pair {
                assert!(first_start <= first_end && first_end < second_start);
            }
        }
    }

    #[test]
    fn direction_and_zero_width_characters_are_format_characters() {
        for character in [
            '\u{202E}',
            '\u{2066}',
            '\u{200B}',
            '\u{200D}',
            '\u{FEFF}',
            '\u{00AD}',
            '\u{E0041}',
        ] {
            assert!(is_format_character(character), "{:X}", u32::from(character));
        }
    }

    #[test]
    fn ordinary_text_is_not() {
        for character in ['a', 'é', 'ā', '中', ' ', '-', '\u{2065}', '\u{1F600}'] {
            assert!(!is_format_character(character), "{:X}", u32::from(character));
        }
    }
}
