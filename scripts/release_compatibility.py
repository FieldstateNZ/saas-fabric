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

    floor-artifact --proof DIR --source URL --roles ROLES
        Refuses a release above the floor unless the floor's complete published
        proof binds it: its provenance, its own compatibility.yaml, its
        component descriptor and what each of its image tags resolves to,
        all agreeing on the component, the version, this repository's source,
        one full commit and every role's repository and digest, and recording
        the contracts this source states. A version string is not proof the
        floor carries a format: if the floor was cut without that support,
        without provenance, or never completed, the floor must advance.

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
SOURCE = re.compile(r"^https://[A-Za-z0-9.-]+/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")


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
    return load_compatibility_file(root / COMPATIBILITY_FILE)


def load_compatibility_file(path: Path) -> dict:
    """The spec of a compatibility.yaml at `path`, with its shape checked."""
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
    return file_digest(root / COMPATIBILITY_FILE)


def file_digest(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


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


# The role whose image is the publisher: publisherContract is a capability of
# the control plane's binary, so a floor proof without that image, bound by
# digest, proves nothing about it.
PUBLISHER_ROLE = "controlPlane"

# The OCI labels every image leg stamps (release.yml, "Build").
LABEL_SOURCE = "org.opencontainers.image.source"
LABEL_REVISION = "org.opencontainers.image.revision"
LABEL_VERSION = "org.opencontainers.image.version"

FLOOR_PROVENANCE = "release/release-provenance.json"
FLOOR_COMPATIBILITY = "release/" + COMPATIBILITY_FILE
FLOOR_DESCRIPTOR = "descriptor/component.json"
FLOOR_IMAGES = "images"


def read_roles(path: Path) -> dict[str, str]:
    """`role repository` lines, as `descriptor images component.yaml` prints
    them: the roles a release publishes, and where."""
    roles: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) != 2 or not ROLE.match(parts[0]) or not REPOSITORY.match(parts[1]):
            raise Refused(f"{line!r} is not `role repository`")
        if parts[0] in roles:
            raise Refused(f"the role {parts[0]} is listed twice")
        roles[parts[0]] = parts[1]
    if PUBLISHER_ROLE not in roles:
        raise Refused(f"no {PUBLISHER_ROLE} role is published, so no publisher can be bound")
    return roles


def _read_json(proof: Path, relative: str, what: str) -> dict:
    try:
        document = json.loads((proof / relative).read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise Refused(f"the floor's {what} could not be read: {error}") from error
    if not isinstance(document, dict):
        raise Refused(f"the floor's {what} is not a JSON object")
    return document


def check_floor_artifact(proof: Path, source: str, roles: dict[str, str], root: Path = REPO_ROOT) -> None:
    """Refuses unless everything in `proof` binds the floor release to one
    commit of `source`, publishing every role in `roles`, built at the
    contracts this source states, and completed."""
    current = load_compatibility(root)
    floor = current["minimumRollableVersion"]
    component = current["component"]

    def refuse(reason: str) -> Refused:
        return Refused(
            f"the floor {floor}: {reason}; cut the floor with this workflow "
            "or advance minimumRollableVersion"
        )

    check_source(source)

    # The provenance: the component, the version, this repository, one commit.
    provenance = _read_json(proof, FLOOR_PROVENANCE, "release provenance")
    if provenance.get("apiVersion") != API_VERSION or provenance.get("kind") != PROVENANCE_KIND:
        raise refuse(f"it carries no {PROVENANCE_KIND}")
    body = provenance.get("spec")
    if not isinstance(body, dict):
        raise refuse("its provenance has no spec")
    if body.get("component") != component:
        raise refuse(f"its provenance is for component {body.get('component')!r}, not {component}")
    if body.get("version") != floor:
        raise refuse(f"its provenance is for {body.get('version')!r}")
    recorded_source = body.get("source") if isinstance(body.get("source"), dict) else {}
    if recorded_source.get("repository") != source:
        raise refuse(f"its provenance names the source {recorded_source.get('repository')!r}, not {source}")
    commit = recorded_source.get("commit")
    if not isinstance(commit, str) or not COMMIT.match(commit):
        raise refuse(f"its provenance names {commit!r}, not a full commit SHA")

    # Every role, once, at the repository this release publishes it to, by a
    # well-formed digest.
    images = body.get("images")
    if not isinstance(images, list) or not all(isinstance(image, dict) for image in images):
        raise refuse("its provenance lists no images")
    recorded: dict[str, dict] = {}
    for image in images:
        role, repository, digest = image.get("role"), image.get("repository"), image.get("digest")
        if role in recorded:
            raise refuse(f"its provenance names the role {role!r} twice")
        if not isinstance(digest, str) or not DIGEST.match(digest):
            raise refuse(f"its provenance names {role!r} by {digest!r}, not a sha256 digest")
        recorded[role] = {"repository": repository, "digest": digest}
    for role in sorted(set(roles) | set(recorded), key=lambda name: (name != PUBLISHER_ROLE, name)):
        if role not in recorded:
            what = "the publisher, " if role == PUBLISHER_ROLE else ""
            raise refuse(f"its provenance does not name {what}{role}")
        if role not in roles:
            raise refuse(f"its provenance names {role!r}, which this release does not publish")
        if recorded[role]["repository"] != roles[role]:
            raise refuse(f"its provenance names {role} at {recorded[role]['repository']!r}, not {roles[role]}")

    # The floor's own compatibility.yaml, by the digest its provenance
    # recorded, stating the contracts its provenance recorded.
    floor_file = proof / FLOOR_COMPATIBILITY
    if not floor_file.is_file():
        raise refuse(f"it carries no {COMPATIBILITY_FILE}")
    stated_file = body.get("compatibility") if isinstance(body.get("compatibility"), dict) else {}
    if file_digest(floor_file) != stated_file.get("digest"):
        raise refuse(f"its {COMPATIBILITY_FILE} is not the one its provenance recorded")
    floor_spec = load_compatibility_file(floor_file)
    contracts = body.get("contracts") if isinstance(body.get("contracts"), dict) else {}
    if stated(floor_spec) != contracts:
        raise refuse(f"its {COMPATIBILITY_FILE} does not state the contracts its provenance recorded")
    if floor_spec["component"] != component or floor_spec["repository"] != current["repository"]:
        raise refuse(f"its {COMPATIBILITY_FILE} is for another component or repository")
    if precedes(floor, floor_spec["minimumRollableVersion"]):
        raise refuse(f"its own floor {floor_spec['minimumRollableVersion']} is above itself")

    # Built at what this source states. Contracts only rise, so a floor that
    # records less cannot read what this source writes.
    for key in ("publisherContract", "minimumReaderContract"):
        wanted = current["publication"][key]
        if contracts.get(key) != wanted:
            raise refuse(f"it was built with {key} {contracts.get(key)!r}, but this source states {wanted}")

    # Completed: the component descriptor is attached last, so its presence,
    # naming the same images, is what separates a finished floor from an
    # interrupted one.
    descriptor = _read_json(proof, FLOOR_DESCRIPTOR, "component descriptor")
    described = descriptor.get("spec") if isinstance(descriptor.get("spec"), dict) else {}
    if descriptor.get("apiVersion") != API_VERSION or descriptor.get("kind") != "Component":
        raise refuse("its component descriptor is not a Component")
    if described.get("name") != component or described.get("version") != floor:
        raise refuse("its component descriptor is for another component or version")
    if described.get("images") != recorded:
        raise refuse("its component descriptor and provenance name different images")

    # And the registry agrees: each role's floor tag resolves to the recorded
    # digest, and each image says it was built from this source, at this
    # commit, as this version.
    for role, image in sorted(recorded.items()):
        observed = _read_json(proof, f"{FLOOR_IMAGES}/{role}.json", f"{role} image")
        if observed.get("digest") != image["digest"]:
            raise refuse(f"{roles[role]}:{floor} is {observed.get('digest')!r}, not the recorded {image['digest']}")
        labels = observed.get("labels") if isinstance(observed.get("labels"), dict) else {}
        for label, wanted in ((LABEL_SOURCE, source), (LABEL_REVISION, commit), (LABEL_VERSION, floor)):
            if labels.get(label) != wanted:
                raise refuse(f"the {role} image's {label} is {labels.get(label)!r}, not {wanted}")


# ---------------------------------------------------------------------------
# Provenance
# ---------------------------------------------------------------------------


def check_source(source: str) -> None:
    if not SOURCE.match(source):
        raise Refused(f"{source!r} is not a source repository URL")


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
    check_source(source)
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
    artifact.add_argument("--proof", required=True)
    artifact.add_argument("--source", required=True)
    artifact.add_argument("--roles", required=True)
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
            roles = read_roles(Path(arguments.roles))
            check_floor_artifact(Path(arguments.proof), arguments.source, roles, root)
            print("the floor's complete published proof carries this source's contracts")
        else:
            print(load_compatibility(root)[arguments.key])
    except Refused as refusal:
        print(f"::error::{refusal}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
