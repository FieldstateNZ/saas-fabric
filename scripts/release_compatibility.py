#!/usr/bin/env python3
"""The release's side of compatibility.yaml (D01-21, #124).

compatibility.yaml states the rollback floor and the publication contracts
the binaries are built with. crates/fabric-component/tests/
this_repositorys_compatibility.rs holds the file to the compiled constants on
every pull request. This script holds a *release* to the file:

    floor VERSION
        Refuses a version that precedes minimumRollableVersion. A release
        below the floor would publish something Platform Management will
        never roll back to, and which misreads state the source writes.

    contracts
        Refuses a source whose contract constants disagree with the file, and
        prints them. The release runs it on the commit it builds from.

    provenance --version V --commit SHA --source URL --image ROLE=REPO@DIGEST...
        Renders the release's provenance: the source commit, every published
        image by digest, the digest of compatibility.yaml, and the contracts
        and floor it states. The bytes are fixed by its inputs, so a re-run
        renders the same document.

    floor-artifact PROVENANCE --subject DIGEST
        Refuses a release above the floor unless the floor's own published
        provenance shows it was built at the contracts this source states. A
        version string is not proof the floor carries a format: if the floor
        was cut without that support, or without provenance at all, the floor
        must advance.

    show KEY
        Prints minimumRollableVersion or repository, for the workflow.

Standard library only, so it runs on a bare runner. compatibility.yaml is
read by a deliberately strict subset parser that refuses anything outside the
plain block mappings the file uses, rather than guessing.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
COMPATIBILITY_FILE = "compatibility.yaml"

API_VERSION = "fabric.fieldstate.nz/v1"
KIND = "Compatibility"
PROVENANCE_KIND = "ReleaseProvenance"

# Where each compiled constant is defined, and the declaration that defines it.
# Exactly one declaration per file is accepted: a moved or renamed constant is
# a failure to read, never a silent default.
SOURCE_CONSTANTS = {
    "minimumRollableVersion": (
        "crates/fabric-component/src/compatibility.rs",
        re.compile(r'^pub const MINIMUM_ROLLABLE_VERSION: &str = "([^"]+)";$', re.MULTILINE),
    ),
    "repository": (
        "crates/fabric-component/src/compatibility.rs",
        re.compile(r'^pub const RUNTIME_REPOSITORY: &str = "([^"]+)";$', re.MULTILINE),
    ),
    "publisherContract": (
        "crates/fabric-runtime-publication/src/manifest.rs",
        re.compile(r"^pub const CONTRACT_VERSION: u32 = ([0-9]+);$", re.MULTILINE),
    ),
    "minimumReaderContract": (
        "crates/fabric-runtime-publication/src/record.rs",
        re.compile(r"^pub const MINIMUM_READER_CONTRACT: u32 = ([0-9]+);$", re.MULTILINE),
    ),
}

# The grammar release.yml admits for a tag: SemVer without build metadata,
# no leading zeros in numeric identifiers, at most 128 characters.
_NUMBER = r"(?:0|[1-9][0-9]*)"
_IDENTIFIER = r"(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)"
VERSION = re.compile(
    rf"^({_NUMBER})\.({_NUMBER})\.({_NUMBER})(?:-({_IDENTIFIER}(?:\.{_IDENTIFIER})*))?$"
)

DIGEST = re.compile(r"^sha256:[0-9a-f]{64}$")
COMMIT = re.compile(r"^[0-9a-f]{40}$")
ROLE = re.compile(r"^[a-z][A-Za-z0-9]*$")
REPOSITORY = re.compile(r"^[a-z0-9.-]+(?::[0-9]+)?(?:/[a-z0-9._-]+)+$")


class Refused(Exception):
    """A release this script will not admit, with the reason."""


# ---------------------------------------------------------------------------
# Versions
# ---------------------------------------------------------------------------


def parse_version(text: str) -> tuple[tuple[int, int, int], tuple[str, ...]]:
    """`(core, prerelease identifiers)`, or Refused if `text` is not a version
    release.yml could publish."""
    match = VERSION.match(text) if len(text) <= 128 else None
    if match is None:
        raise Refused(f"{text!r} is not a version this repository can publish")
    core = (int(match.group(1)), int(match.group(2)), int(match.group(3)))
    pre = tuple(match.group(4).split(".")) if match.group(4) else ()
    return core, pre


def _identifier_key(identifier: str) -> tuple[int, int, str]:
    # SemVer 11.4: numeric identifiers sort numerically and before
    # alphanumeric ones, which sort in ASCII order.
    if identifier.isdigit():
        return (0, int(identifier), "")
    return (1, 0, identifier)


def precedes(left: str, right: str) -> bool:
    """Whether `left` has lower SemVer precedence than `right`."""
    left_core, left_pre = parse_version(left)
    right_core, right_pre = parse_version(right)
    if left_core != right_core:
        return left_core < right_core
    # A prerelease precedes its release.
    if not left_pre or not right_pre:
        return bool(left_pre) and not right_pre
    return [_identifier_key(i) for i in left_pre] < [_identifier_key(i) for i in right_pre]


# ---------------------------------------------------------------------------
# compatibility.yaml
# ---------------------------------------------------------------------------


def _scalar(text: str, where: str) -> str | int:
    if text[:1] in ("'", '"'):
        if len(text) < 2 or text[-1] != text[0] or text[0] in text[1:-1]:
            raise Refused(f"{where}: unsupported quoting in {text!r}")
        return text[1:-1]
    if any(character in text for character in "{}[]&*!|>%@`#"):
        raise Refused(f"{where}: unsupported YAML in {text!r}")
    return int(text) if re.fullmatch(r"0|[1-9][0-9]*", text) else text


def parse_subset(text: str) -> dict:
    """Plain block mappings of plain or simply quoted scalars, indented by two
    spaces, with whole-line comments. Anything else is refused."""
    root: dict = {}
    # Each open mapping, with the indentation its keys must have.
    stack: list[tuple[int, dict]] = [(0, root)]
    for number, line in enumerate(text.splitlines(), start=1):
        where = f"{COMPATIBILITY_FILE}:{number}"
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if "\t" in line:
            raise Refused(f"{where}: tabs are not supported")
        indent = len(line) - len(line.lstrip(" "))
        while len(stack) > 1 and stack[-1][0] > indent:
            stack.pop()
        if stack[-1][0] != indent:
            raise Refused(f"{where}: unexpected indentation")
        match = re.fullmatch(r"([A-Za-z][A-Za-z0-9]*):(?: (.*))?", stripped)
        if match is None:
            raise Refused(f"{where}: expected `key: value` or `key:`, found {stripped!r}")
        key, value = match.group(1), match.group(2)
        mapping = stack[-1][1]
        if key in mapping:
            raise Refused(f"{where}: duplicate key {key!r}")
        if value is None or value.strip() == "":
            child: dict = {}
            mapping[key] = child
            stack.append((indent + 2, child))
        else:
            mapping[key] = _scalar(value.strip(), where)
    return root


def load_compatibility(root: Path = REPO_ROOT) -> dict:
    """compatibility.yaml's spec, with its shape checked."""
    path = root / COMPATIBILITY_FILE
    if not path.is_file():
        raise Refused(f"{COMPATIBILITY_FILE} does not exist")
    document = parse_subset(path.read_text(encoding="utf-8"))
    if document.get("apiVersion") != API_VERSION or document.get("kind") != KIND:
        raise Refused(f"{COMPATIBILITY_FILE} is not a {API_VERSION} {KIND}")
    spec = document.get("spec")
    publication = spec.get("publication") if isinstance(spec, dict) else None
    if not isinstance(publication, dict):
        raise Refused(f"{COMPATIBILITY_FILE} has no spec.publication")
    for key in ("component", "repository", "minimumRollableVersion"):
        if not isinstance(spec.get(key), str):
            raise Refused(f"{COMPATIBILITY_FILE} has no string spec.{key}")
    for key in ("publisherContract", "minimumReaderContract"):
        if not isinstance(publication.get(key), int) or publication[key] < 1:
            raise Refused(f"{COMPATIBILITY_FILE} has no positive spec.publication.{key}")
    parse_version(spec["minimumRollableVersion"])
    return spec


def compatibility_digest(root: Path = REPO_ROOT) -> str:
    return "sha256:" + hashlib.sha256((root / COMPATIBILITY_FILE).read_bytes()).hexdigest()


def stated(spec: dict) -> dict:
    """What compatibility.yaml states, in the shape the source is compared in."""
    return {
        "minimumRollableVersion": spec["minimumRollableVersion"],
        "repository": spec["repository"],
        "publisherContract": spec["publication"]["publisherContract"],
        "minimumReaderContract": spec["publication"]["minimumReaderContract"],
    }


# ---------------------------------------------------------------------------
# The source's constants
# ---------------------------------------------------------------------------


def source_constants(root: Path = REPO_ROOT) -> dict:
    """The constants the binaries are compiled with, read from their
    declarations."""
    found: dict = {}
    for key, (relative, declaration) in SOURCE_CONSTANTS.items():
        path = root / relative
        if not path.is_file():
            raise Refused(f"{relative} does not exist, so {key} cannot be read")
        matches = declaration.findall(path.read_text(encoding="utf-8"))
        if len(matches) != 1:
            raise Refused(f"{relative} declares {key} {len(matches)} times, not once")
        value = matches[0]
        found[key] = int(value) if key.endswith("Contract") else value
    return found


def check_contracts(root: Path = REPO_ROOT) -> dict:
    """The contracts, once the source and compatibility.yaml agree on them."""
    spec = load_compatibility(root)
    wanted = stated(spec)
    compiled = source_constants(root)
    disagreements = [
        f"{key}: compatibility.yaml states {wanted[key]!r}, the source is built with {compiled[key]!r}"
        for key in wanted
        if wanted[key] != compiled[key]
    ]
    if disagreements:
        raise Refused("; ".join(disagreements))
    return wanted


# ---------------------------------------------------------------------------
# The floor
# ---------------------------------------------------------------------------


def check_floor(version: str, root: Path = REPO_ROOT) -> str:
    """The floor, once `version` is at or above it."""
    floor = load_compatibility(root)["minimumRollableVersion"]
    if precedes(version, floor):
        raise Refused(
            f"{version} precedes the rollback floor {floor} in {COMPATIBILITY_FILE}; "
            "a release below the floor misreads state this source writes"
        )
    return floor


def check_floor_artifact(provenance: dict, subject: str, root: Path = REPO_ROOT) -> None:
    """Refuses unless `provenance` is the floor release's own, names `subject`
    as its runtime image, and records the contracts this source states."""
    spec = load_compatibility(root)
    floor = spec["minimumRollableVersion"]
    advance = "; cut the floor with this workflow or advance minimumRollableVersion"
    if provenance.get("apiVersion") != API_VERSION or provenance.get("kind") != PROVENANCE_KIND:
        raise Refused(f"the floor {floor} carries no {PROVENANCE_KIND}{advance}")
    body = provenance.get("spec") or {}
    if body.get("version") != floor:
        raise Refused(f"the floor's provenance is for {body.get('version')!r}, not {floor}{advance}")
    runtime = [
        image for image in body.get("images", []) if image.get("repository") == spec["repository"]
    ]
    if len(runtime) != 1 or runtime[0].get("digest") != subject:
        raise Refused(f"the floor's provenance does not name {spec['repository']}@{subject}{advance}")
    recorded = body.get("contracts") or {}
    wanted = stated(spec)
    for key in ("publisherContract", "minimumReaderContract"):
        if recorded.get(key) != wanted[key]:
            raise Refused(
                f"the floor {floor} was built with {key} {recorded.get(key)!r}, "
                f"but this source states {wanted[key]}{advance}"
            )


# ---------------------------------------------------------------------------
# Provenance
# ---------------------------------------------------------------------------


def parse_image(argument: str) -> dict:
    match = re.fullmatch(r"([^=]+)=([^@]+)@(.+)", argument)
    if match is None:
        raise Refused(f"--image {argument!r} is not ROLE=REPOSITORY@DIGEST")
    role, repository, digest = match.groups()
    if not ROLE.match(role):
        raise Refused(f"{role!r} is not an image role")
    if not REPOSITORY.match(repository):
        raise Refused(f"{repository!r} is not a repository")
    if not DIGEST.match(digest):
        raise Refused(f"{digest!r} is not a sha256 digest")
    return {"role": role, "repository": repository, "digest": digest}


def render_provenance(
    version: str, commit: str, source: str, images: list[str], root: Path = REPO_ROOT
) -> str:
    """The provenance document, as the exact bytes the release attaches."""
    contracts = check_contracts(root)
    check_floor(version, root)
    if not COMMIT.match(commit):
        raise Refused(f"{commit!r} is not a full commit SHA")
    if not images:
        raise Refused("a release publishes at least one image")
    parsed = sorted((parse_image(image) for image in images), key=lambda image: image["role"])
    roles = [image["role"] for image in parsed]
    if len(set(roles)) != len(roles):
        raise Refused(f"an image role is named twice in {roles}")
    if contracts["repository"] not in {image["repository"] for image in parsed}:
        raise Refused(f"no published image is {contracts['repository']}, the floor's repository")
    spec = load_compatibility(root)
    document = {
        "apiVersion": API_VERSION,
        "kind": PROVENANCE_KIND,
        "spec": {
            "component": spec["component"],
            "version": version,
            "source": {"repository": source, "commit": commit},
            "images": parsed,
            "compatibility": {"file": COMPATIBILITY_FILE, "digest": compatibility_digest(root)},
            "contracts": contracts,
        },
    }
    return json.dumps(document, indent=2, sort_keys=True) + "\n"


# ---------------------------------------------------------------------------
# The command line
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None, root: Path = REPO_ROOT) -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    commands = parser.add_subparsers(dest="command", required=True)
    floor = commands.add_parser("floor")
    floor.add_argument("version")
    commands.add_parser("contracts")
    provenance = commands.add_parser("provenance")
    provenance.add_argument("--version", required=True)
    provenance.add_argument("--commit", required=True)
    provenance.add_argument("--source", required=True)
    provenance.add_argument("--image", action="append", default=[])
    provenance.add_argument("--out", required=True)
    artifact = commands.add_parser("floor-artifact")
    artifact.add_argument("provenance")
    artifact.add_argument("--subject", required=True)
    show = commands.add_parser("show")
    show.add_argument("key", choices=["minimumRollableVersion", "repository"])
    arguments = parser.parse_args(argv)

    try:
        if arguments.command == "floor":
            print(f"{arguments.version} is at or above the floor {check_floor(arguments.version, root)}")
        elif arguments.command == "contracts":
            print(json.dumps(check_contracts(root), sort_keys=True))
        elif arguments.command == "provenance":
            rendered = render_provenance(
                arguments.version, arguments.commit, arguments.source, arguments.image, root
            )
            Path(arguments.out).write_text(rendered, encoding="utf-8")
        elif arguments.command == "floor-artifact":
            try:
                document = json.loads(Path(arguments.provenance).read_text(encoding="utf-8"))
            except (OSError, ValueError) as error:
                raise Refused(f"the floor's provenance could not be read: {error}") from error
            check_floor_artifact(document if isinstance(document, dict) else {}, arguments.subject, root)
            print("the floor's published provenance carries this source's contracts")
        else:
            print(load_compatibility(root)[arguments.key])
    except Refused as refusal:
        print(f"::error::{refusal}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
