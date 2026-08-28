#!/usr/bin/env python3
"""Enforce that one Rust version is named in one place.

`docs/architecture/toolchain-policy.md` says the version appears in
`rust-toolchain.toml` and nowhere else. A container build is where that
quietly stops being true: a `Dockerfile` has to name a builder image, an image
tag carries a version, and nothing connects the two.

The consequence is the failure the pin exists to prevent, one layer down. CI
would check the workspace with the pinned compiler and then ship an artifact
built by a different one — and the difference would surface as a bug in
production that no gate reproduces, because no gate used that compiler.

So the `RUST_VERSION` default in every Dockerfile must equal the pinned
channel. Both are edited by hand; this is what makes editing one of them a
build failure rather than a divergence.

Usage:
    python3 scripts/check_toolchain_pin.py

Exit status is 0 when they agree, 1 otherwise.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
TOOLCHAIN_FILE = REPO_ROOT / "rust-toolchain.toml"

# `channel = "1.98.0"`, ignoring comments and whitespace.
CHANNEL = re.compile(r'^\s*channel\s*=\s*"([^"]+)"', re.MULTILINE)

# `ARG RUST_VERSION=1.98.0`. A Dockerfile without the argument is not checked:
# the console's image builds no Rust and has no business naming a compiler.
ARG_RUST_VERSION = re.compile(r"^ARG\s+RUST_VERSION=(\S+)", re.MULTILINE)


def pinned_channel() -> str | None:
    """The channel `rust-toolchain.toml` pins."""
    if not TOOLCHAIN_FILE.is_file():
        return None

    match = CHANNEL.search(TOOLCHAIN_FILE.read_text(encoding="utf-8"))

    return match.group(1) if match else None


def dockerfiles() -> list[Path]:
    """Every Dockerfile in the repository, excluding dependency directories."""
    return sorted(
        path
        for path in REPO_ROOT.rglob("Dockerfile*")
        if "node_modules" not in path.parts and "target" not in path.parts
    )


def main() -> int:
    channel = pinned_channel()

    if channel is None:
        print(
            "error: rust-toolchain.toml does not pin a channel; "
            "see docs/architecture/toolchain-policy.md",
            file=sys.stderr,
        )
        return 1

    print(f"rust-toolchain.toml pins {channel}")

    failures = []
    checked = 0

    for path in dockerfiles():
        declared = ARG_RUST_VERSION.search(path.read_text(encoding="utf-8"))

        if declared is None:
            continue

        checked += 1
        relative = path.relative_to(REPO_ROOT)

        if declared.group(1) == channel:
            print(f"          ok  {relative}")
        else:
            failures.append(
                f"  {relative} builds with {declared.group(1)}, "
                f"but the workspace is pinned to {channel}\n"
                "    why it matters: the released binary would be compiled by a "
                "compiler no gate ran, so a lint or a miscompilation caught in "
                "CI says nothing about the artifact."
            )

    if failures:
        print(f"\n{len(failures)} Dockerfile(s) disagree with the pin:", file=sys.stderr)
        for failure in failures:
            print(failure, file=sys.stderr)
        return 1

    if checked == 0:
        # Not an error: a repository may legitimately have no Rust image. Said
        # out loud so a silently-renamed argument is not read as a pass.
        print("\nOK: no Dockerfile declares RUST_VERSION.")
        return 0

    print(f"\nOK: {checked} Dockerfile(s) build with the pinned toolchain.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
