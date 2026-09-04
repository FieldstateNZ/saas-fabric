//! Where in an Argo Application the walk currently is.

use super::lines::Line;
use super::scalar::Scalar;
use super::Refusal;

/// What a line is, to the walk.
pub(super) enum Role {
    /// Not part of `spec.sources`.
    Elsewhere,
    /// The `- ` that opens one of its entries.
    Opens,
    /// Somewhere inside the list, below whatever entry is open.
    Within,
}

/// The walk's position in the document.
///
/// # Only one `sources:` is the sources list
///
/// Argo puts a chart source in the `sources:` that is a **direct key of the
/// document's top-level `spec:`**. Both halves of that matter. A `sources:`
/// under `spec.template` is a pod template's business; one nested inside a
/// source's own `helm:` block is a values file listing its inputs. Either can
/// name the same chart, so a renderer entering whichever `sources:` it met
/// first would edit one of them — or count it as a second match and refuse a
/// file that was never ambiguous. So this tracks two indents rather than two
/// key names: the column `spec`'s own keys sit at, and the column its sources'
/// entries sit at. A key deeper than the first is nested, not direct.
pub(super) enum Position {
    /// Not inside a top-level `spec:`.
    Outside,
    /// Inside one. `children` is the column its own keys sit at, learned from
    /// the first of them.
    Spec { children: Option<usize> },
    /// Inside its `sources:`.
    Sources {
        /// The column `sources:` itself sits at, which is where the list ends.
        children: usize,
        /// The column each entry's `- ` sits at, learned from the first. YAML
        /// lets a sequence sit at its key's own indent or deeper; both are the
        /// same document.
        entries: Option<usize>,
    },
}

impl Position {
    /// Follows the walk into and out of `spec.sources`, and says what the line
    /// is once there.
    ///
    /// # Errors
    ///
    /// A clause saying why, when `spec:` or `sources:` is a shape it cannot
    /// step into.
    pub(super) fn observe(&mut self, line: &Line<'_>) -> Result<Role, Refusal> {
        if line.starts_a_document() {
            *self = Self::Outside;
            return Ok(Role::Elsewhere);
        }

        match self {
            Self::Outside => {
                if line.indent() == 0 && opens(line, "spec")? {
                    *self = Self::Spec { children: None };
                }
                Ok(Role::Elsewhere)
            }
            Self::Spec { children } => {
                // A key back at the document's own level has left `spec`.
                if line.indent() == 0 {
                    *self = Self::Outside;
                    return self.observe(line);
                }
                let keys = *children.get_or_insert(line.indent());
                if line.indent() == keys && opens(line, "sources")? {
                    *self = Self::Sources {
                        children: keys,
                        entries: None,
                    };
                }
                Ok(Role::Elsewhere)
            }
            Self::Sources { children, entries } => {
                let keys = *children;
                // Anything at or above the list's own key that is not one of
                // its entries has ended it.
                if line.indent() < keys || (line.indent() == keys && !line.opens_entry()) {
                    *self = Self::Spec { children: Some(keys) };
                    return self.observe(line);
                }
                if line.opens_entry() && line.indent() == *entries.get_or_insert(line.indent()) {
                    return Ok(Role::Opens);
                }
                Ok(Role::Within)
            }
        }
    }
}

/// Whether this line is the key `name`, opening a block for the walk to enter.
///
/// # Errors
///
/// A value on the same line — `sources: [...]`, `spec: {…}` — is flow style,
/// which has no indentation to walk. Stepping past it would leave the sources
/// it holds unread, and the file looking like it named none, so it is refused.
fn opens(line: &Line<'_>, name: &str) -> Result<bool, Refusal> {
    let Some(key) = Scalar::read(line, line.rest).filter(|key| key.key == name) else {
        return Ok(false);
    };

    if key.carries_a_value() {
        return Err(format!(
            "writes '{name}' and its contents on one line, and only the indented form \
             can be read without guessing at where each source ends"
        ));
    }

    Ok(true)
}
