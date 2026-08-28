# File-size policy

- **Status:** Accepted
- **Applies to:** production `.rs` files under `crates/*/src/`.
- **Enforced by:** [`scripts/check_file_sizes.py`](../../scripts/check_file_sizes.py),
  run in CI (see [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml)).

## Why a line-count rule at all

This codebase follows the "one concept per file, lots of small files"
convention: a file holds one struct or enum together with its `impl` blocks,
one trait, one handler, or one closely-related set of pure functions (see the
Rust codebase conventions and the README's "Conventions" section). Line count
is a coarse proxy for that, but it is a genuinely useful one — a file that
has drifted past a few hundred lines has almost always drifted past "one
concept" too, and length is the one property a script can check without
understanding the code.

The thresholds below are guidance backed by a hard CI gate at the top end,
not a target to hit by mechanically splitting files. A file that is short
because it does one thing is the goal; a file that is short because a
cohesive type got fragmented across three modules to satisfy a line count is
a worse outcome than leaving it long, and reviewers should say so.

## Thresholds

| Lines | Status |
|---|---|
| ≤ 80 | Normal. No comment needed. |
| 81–120 | Acceptable, provided the file is cohesive — everything in it is genuinely one concept and splitting it would separate things that belong together. |
| 121–150 | Needs a clear reason, stated in review or in a comment at the top of the file. This band is for "this is legitimately one thing and it is a bit long," not "I didn't get around to splitting it." |
| > 150 | A design smell. The CI check in `scripts/check_file_sizes.py` fails the build unless the file is listed in that script's `EXEMPTIONS`, with the reason recorded next to the entry. |

**Tests may be larger.** The thresholds apply to production code. A file's
own inline `#[cfg(test)] mod tests { ... }` block, and any file that is
itself a test file, do not count toward these limits — see "What counts as
production code" below. Thorough test coverage is not something this policy
wants to discourage by making long test modules expensive.

## What "a clear reason" looks like

Valid reasons to be in the 121–150 band, or to hold an exemption above 150:

- **A cohesive wire-format type.** A struct that mirrors an external
  protocol's shape (for example the hand-written NDC wire types in
  `fabric-connector-ndc`, per
  [ADR 0001](../decisions/0001-ndc-as-connector-boundary.md)), together with
  its `Serialize`/`Deserialize` derives or manual impls, is one concept even
  when the field list is long. Splitting the struct from its impls, or
  splitting a request type from its matching response type, fights the type
  rather than clarifying it.
- **A small enum plus its trivial, tightly related impls** — `Display`,
  `From`, a handful of one-line predicate methods — where every impl exists
  only because the enum does, and none of them would be reused or tested
  independently of it.

Not valid reasons:

- "It would take effort to split." Effort is sequencing, not correctness —
  the rustdoc conventions in this workspace hold the same line.
- "The file has multiple structs but they're all related to the same
  feature." Related is not the same as one concept — if two of the types
  could reasonably have their own file with its own focused rustdoc, they
  should.
- Hitting the limit because a function grew a long match arm or a large
  validation routine. That is usually a sign the function itself should be
  decomposed, independent of which file it lives in.

## What counts as "production lines"

`scripts/check_file_sizes.py` scans every `*.rs` file under `crates/` and
excludes:

- any file inside a directory literally named `tests` — this workspace's
  per-crate integration tests (`crates/<crate>/tests/*.rs`);
- any file named `*_tests.rs` — the sibling-module convention this codebase
  uses to pull a large inline test module out of the type it covers (for
  example `execution_target.rs` / `execution_target_tests.rs`).

For every remaining file, if the file ends in a single inline
`#[cfg(test)] mod tests { ... }` block, that block's lines are subtracted
before the thresholds are applied — a file with 100 lines of production code
and 80 lines of inline tests is measured as 100, not 180. A `#[cfg(test)]`
item that is *not* the file's final module (for example a scattered
`#[cfg(test)] use` or helper) is not treated specially and counts as normal
file content, since it is not the "tests pulled out of the line count"
convention this rule is accommodating.

## Exemptions

`scripts/check_file_sizes.py` keeps a small, in-file `EXEMPTIONS` mapping of
`path -> reason`. Adding an entry is the only way a file over 150 production
lines passes CI, and it is deliberately visible in the script itself rather
than in a config file elsewhere, so a `git blame` on the script shows who
exempted what and when. Keep the list short: an exemption is for the
"cohesive wire-format type" case above, not a place to park files that
should be split but haven't been yet.
