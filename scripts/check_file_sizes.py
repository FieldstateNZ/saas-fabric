#!/usr/bin/env python3
"""Enforce the SaaS Fabric file-size policy on production Rust source.

See docs/architecture/file-size-policy.md for the thresholds and the
reasoning behind them. In short:

  <= 80 lines   normal
  81-120 lines  acceptable with cohesion
  121-150 lines needs a clear reason
  > 150 lines   a design smell -- fails this check unless exempted below

Scope: every `*.rs` file under `crates/`, EXCLUDING:
  - any file inside a directory literally named `tests` (integration tests
    live in `crates/<crate>/tests/`)
  - sibling unit-test files named `*_tests.rs` (this codebase's convention
    for pulling large inline test modules out of the type they cover)

For files that keep their tests inline instead, a single `#[cfg(test)]`
module at the very end of the file is subtracted from the line count before
the thresholds are applied, so test code never counts against the
production-code budget.

Usage:
    python3 scripts/check_file_sizes.py

Exit status is 0 when every production file is at or under the 150-line
hard limit (or is listed in EXEMPTIONS), 1 otherwise.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

WARN_THRESHOLD = 120
FAIL_THRESHOLD = 150

REPO_ROOT = Path(__file__).resolve().parent.parent
SCAN_ROOT = REPO_ROOT / "crates"

# Files intentionally left over the 150-line hard limit, with the reason
# recorded inline. Keep this list short and specific: per
# docs/architecture/file-size-policy.md, the only valid reasons are things
# like a genuinely cohesive wire-format type plus its trivial impls, where
# splitting would fragment one concept across files for no readability gain.
# Do NOT add an entry just because a file is inconvenient to shrink.
EXEMPTIONS: dict[str, str] = {
    "crates/fabric-fga-auth/src/cache.rs":
        "one security-critical decision -- which key may verify a token, and "
        "when trust must be refreshed -- whose branches only make sense read "
        "together. It was split once to satisfy the arithmetic, and the "
        "result was a per-issuer lock map in a file of its own, away from the "
        "rule it exists to protect. The two windows it turns on are genuinely "
        "separate policy and did move out, to windows.rs; what remains is one "
        "thing.",
    # "crates/fabric-connector-ndc/src/wire/query_request.rs":
    #     "one cohesive NDC wire-format type plus its (de)serialisation impls; "
    #     "splitting fragments a single wire shape across files",
}

# `#[cfg(test)]` on its own line, optionally indented.
_TEST_MOD_ATTR = re.compile(r"^\s*#\[cfg\(test\)\]\s*$")
# The module declaration that (by convention) immediately follows it.
_TEST_MOD_DECL = re.compile(r"^\s*(pub(\(\w+\))?\s+)?mod\s+\w+\s*\{\s*$")


def is_excluded(path: Path) -> bool:
    """True for integration-test files and `*_tests.rs` sibling modules."""
    rel_parts = path.relative_to(REPO_ROOT).parts
    if "tests" in rel_parts[:-1]:
        return True
    return path.name.endswith("_tests.rs")


def production_line_count(lines: list[str]) -> int:
    """Total lines, minus a trailing inline `#[cfg(test)]` module if present.

    Only a test module that runs to the end of the file is subtracted --
    that is the house convention (see the rust-codebase skill: "Inline
    `#[cfg(test)] mod tests` at the bottom of the file"). We look for the
    last non-blank line being a closing brace, then scan backward for the
    `#[cfg(test)]` attribute that starts the block. Anything else in the
    file (including scattered `#[cfg(test)]` items that are not the final
    module) is left alone and counts as production code.
    """
    total = len(lines)
    end = total
    while end > 0 and lines[end - 1].strip() == "":
        end -= 1
    if end == 0 or lines[end - 1].strip() != "}":
        return total

    for i in range(end - 1, -1, -1):
        if not _TEST_MOD_ATTR.match(lines[i]):
            continue
        j = i + 1
        while j < total and lines[j].strip() == "":
            j += 1
        if j < total and _TEST_MOD_DECL.match(lines[j]):
            return i
    return total


def main() -> int:
    if not SCAN_ROOT.is_dir():
        print(f"error: scan root {SCAN_ROOT} does not exist", file=sys.stderr)
        return 2

    rs_files = sorted(SCAN_ROOT.rglob("*.rs"))
    over_warn: list[tuple[str, int]] = []
    over_fail: list[tuple[str, int]] = []

    for path in rs_files:
        if is_excluded(path):
            continue
        lines = path.read_text(encoding="utf-8").splitlines()
        count = production_line_count(lines)
        rel = str(path.relative_to(REPO_ROOT))

        if count > WARN_THRESHOLD:
            over_warn.append((rel, count))
        if count > FAIL_THRESHOLD and rel not in EXEMPTIONS:
            over_fail.append((rel, count))

    if over_warn:
        print(f"Files over the {WARN_THRESHOLD}-line advisory threshold:")
        for rel, count in sorted(over_warn, key=lambda item: -item[1]):
            exempt = " (EXEMPT)" if rel in EXEMPTIONS else ""
            print(f"  {count:5d}  {rel}{exempt}")
    else:
        print(f"No production files over the {WARN_THRESHOLD}-line advisory threshold.")

    if over_fail:
        print(
            f"\nFAIL: files over the {FAIL_THRESHOLD}-line hard limit "
            "(see docs/architecture/file-size-policy.md):"
        )
        for rel, count in sorted(over_fail, key=lambda item: -item[1]):
            print(f"  {count:5d}  {rel}")
        return 1

    # Says what is actually true. "No file exceeds the limit" would be a lie
    # the moment anything is exempted, and a success line nobody trusts is a
    # success line nobody reads.
    noted = f" ({len(EXEMPTIONS)} exempted, with reasons)" if EXEMPTIONS else ""
    print(f"\nOK: no unexplained production file exceeds {FAIL_THRESHOLD} lines{noted}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
