#!/usr/bin/env python3
"""Fails when a published document shape changes without raising the reader
floor (gap report G6; ADR 0018 §9, amended for G6).

A reader's document types deny unknown fields, so a document whose shape
changed is refused by every older reader. The publication record's
minimum_reader_contract tells those readers to keep their last complete
generation instead. This script holds a change to the shape to a raise of
that floor:

    crates/fabric-runtime-publication/published-shapes.json
        The shape this publisher writes, and the minimumReaderContract it
        was recorded at. tests/published_shapes.rs holds it to what the
        publisher actually writes; this script compares it with the same
        file at a base commit.

    python3 scripts/check_reader_floor.py --base REF

Refused:
    - a base that cannot be read, or whose source and compatibility.yaml
      disagree on the floor;
    - the lock's floor is not the source's MINIMUM_READER_CONTRACT, or
      compatibility.yaml disagrees with the source (release_compatibility);
    - the floor fell since REF;
    - the shape changed since REF and the floor did not rise.

A base whose tree provably has no lock (before this check existed) adopts
it, provided the floor does not fall from the base's source and
compatibility.yaml. Any base that cannot be read -- an unknown commit, an
unreadable tree, a missing blob -- is refused, never taken as absent.
Standard library and git only.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

import release_compatibility as rc

REPO_ROOT = Path(__file__).resolve().parent.parent
LOCK = "crates/fabric-runtime-publication/published-shapes.json"
REGENERATE = (
    "FABRIC_WRITE_PUBLISHED_SHAPES=1 cargo test -p fabric-runtime-publication --test published_shapes"
)


class Refused(Exception):
    """A change this check will not admit, with the reason."""


def parse_lock(text: str, where: str) -> tuple[int, dict[str, list[str]]]:
    """`(minimumReaderContract, shapes)` from a lock's text."""
    try:
        lock = json.loads(text)
    except json.JSONDecodeError as error:
        raise Refused(f"{where} is not JSON: {error}") from error
    floor = lock.get("minimumReaderContract") if isinstance(lock, dict) else None
    shapes = lock.get("shapes") if isinstance(lock, dict) else None
    if not isinstance(floor, int) or isinstance(floor, bool) or floor < 1:
        raise Refused(f"{where} has no positive minimumReaderContract")
    if not isinstance(shapes, dict) or not all(
        isinstance(lines, list) and all(isinstance(line, str) for line in lines) for lines in shapes.values()
    ):
        raise Refused(f"{where} has no shapes")
    return floor, shapes


def git(root: Path, *argv: str) -> subprocess.CompletedProcess:
    return subprocess.run(["git", "-C", str(root), *argv], capture_output=True, check=False)


def base_commit(base: str, root: Path) -> str:
    """The full commit `base` names, or Refused."""
    resolved = git(root, "rev-parse", "--verify", "--quiet", f"{base}^{{commit}}")
    if resolved.returncode != 0:
        raise Refused(f"the base {base!r} is not a commit in this repository")
    return resolved.stdout.decode().strip()


def read_at(commit: str, relative: str, root: Path) -> str | None:
    """`relative`'s text at `commit`; None only when the commit's tree
    provably has no such path.

    Absence is taken from `git ls-tree`, which reads only trees. Every other
    failure -- an unreadable tree, a missing or corrupt blob, a path that is
    not a file -- is Refused: a base that cannot be read is no evidence that
    the lock did not exist.
    """
    listed = git(root, "ls-tree", "-z", "--full-tree", commit, "--", relative)
    if listed.returncode != 0:
        raise Refused(f"cannot read the tree of {commit} for {relative}: {listed.stderr.decode().strip()}")
    entries = [entry for entry in listed.stdout.decode().split("\0") if entry]
    if not entries:
        return None
    if len(entries) != 1:
        raise Refused(f"{relative} at {commit} is listed {len(entries)} times")
    metadata, _, path = entries[0].partition("\t")
    mode, kind, blob = (metadata.split(" ") + ["", "", ""])[:3]
    if path != relative or kind != "blob" or not mode.startswith("100"):
        raise Refused(f"{relative} at {commit} is not a file ({entries[0]!r})")
    shown = git(root, "cat-file", "blob", blob)
    if shown.returncode != 0:
        raise Refused(f"{relative} at {commit} names blob {blob}, which cannot be read: "
                      f"{shown.stderr.decode().strip()}")
    try:
        return shown.stdout.decode("utf-8")
    except UnicodeDecodeError as error:
        raise Refused(f"{relative} at {commit} is not UTF-8") from error


def base_source_floor(commit: str, root: Path) -> int:
    """The minimum reader contract the base's source and compatibility.yaml
    state. Both must be readable and agree."""
    relative, declaration = rc.SOURCE_CONSTANTS["minimumReaderContract"]
    source = read_at(commit, relative, root)
    if source is None:
        raise Refused(f"{relative} does not exist at {commit}, so its reader floor is unknown")
    matches = declaration.findall(source)
    if len(matches) != 1:
        raise Refused(f"{relative} at {commit} declares MINIMUM_READER_CONTRACT {len(matches)} times, not once")
    text = read_at(commit, rc.COMPATIBILITY_FILE, root)
    if text is None:
        raise Refused(f"{rc.COMPATIBILITY_FILE} does not exist at {commit}, so its reader floor is unknown")
    try:
        stated = rc.parse_subset(text)["spec"]["publication"]["minimumReaderContract"]
    except (rc.Refused, KeyError, TypeError) as error:
        raise Refused(f"{rc.COMPATIBILITY_FILE} at {commit} states no minimumReaderContract") from error
    if stated != int(matches[0]):
        raise Refused(
            f"at {commit}, {rc.COMPATIBILITY_FILE} states reader floor {stated} and the source {matches[0]}"
        )
    return stated


def difference(before: dict[str, list[str]], after: dict[str, list[str]]) -> list[str]:
    lines = []
    for document in sorted(set(before) | set(after)):
        old = set(before.get(document, []))
        new = set(after.get(document, []))
        lines += [f"- {document} {line}" for line in sorted(old - new)]
        lines += [f"+ {document} {line}" for line in sorted(new - old)]
    return lines


def check(base: str, root: Path = REPO_ROOT) -> str:
    """What the check found, or Refused."""
    path = root / LOCK
    if not path.is_file():
        raise Refused(f"{LOCK} does not exist; write it with {REGENERATE}")
    floor, shapes = parse_lock(path.read_text(encoding="utf-8"), LOCK)
    try:
        contracts = rc.check_contracts(root)
    except rc.Refused as error:
        raise Refused(str(error)) from error
    if floor != contracts["minimumReaderContract"]:
        raise Refused(
            f"{LOCK} was recorded at reader contract {floor}, but the source's "
            f"MINIMUM_READER_CONTRACT is {contracts['minimumReaderContract']}; "
            f"regenerate it with {REGENERATE}"
        )

    commit = base_commit(base, root)
    source_floor = base_source_floor(commit, root)
    if floor < source_floor:
        raise Refused(f"the reader floor fell from {source_floor} to {floor} since {base}; a floor never falls")
    text = read_at(commit, LOCK, root)
    if text is None:
        return (
            f"{base} provably has no {LOCK}: the lock is adopted at reader contract {floor} "
            f"(the base's floor is {source_floor})"
        )
    base_floor, base_shapes = parse_lock(text, f"{LOCK} at {base}")
    if base_floor != source_floor:
        raise Refused(f"{LOCK} at {base} was recorded at {base_floor}, but that commit's floor is {source_floor}")
    changes = difference(base_shapes, shapes)
    if not changes:
        return f"the published shape is unchanged since {base}; reader floor {floor}"
    if floor == base_floor:
        raise Refused(
            f"the published shape changed since {base} without raising the reader floor "
            f"(still {floor}). Readers at {floor} deny unknown fields and refuse this shape. "
            "Raise MINIMUM_READER_CONTRACT (crates/fabric-runtime-publication/src/record.rs) and "
            "compatibility.yaml's minimumReaderContract, make sure READER_CONTRACT reaches it, and "
            f"regenerate the lock with {REGENERATE}.\n" + "\n".join(changes)
        )
    return (
        f"the published shape changed since {base} and the reader floor rose from {base_floor} to {floor}:\n"
        + "\n".join(changes)
    )


def main(argv: list[str] | None = None, root: Path = REPO_ROOT) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--base", required=True, help="the commit to compare the lock with")
    arguments = parser.parse_args(argv)
    try:
        print(check(arguments.base, root))
    except Refused as error:
        print(f"refused: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
