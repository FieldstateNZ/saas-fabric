"""Offline tests for scripts/release_compatibility.py.

Standard library only. Each test builds a throwaway repository root holding a
compatibility.yaml and the three source files whose constants it is compared
with, so a refusal is proved against a known disagreement rather than against
whatever the real tree happens to say. One test reads the real tree, which is
what the release itself runs.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import contextlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import release_compatibility as rc  # noqa: E402

RUNTIME = "ghcr.io/fieldstatenz/saas-fabric"
COMMIT = "0123456789abcdef0123456789abcdef01234567"
DIGEST_A = "sha256:" + "a" * 64
DIGEST_B = "sha256:" + "b" * 64
DIGEST_C = "sha256:" + "c" * 64

COMPATIBILITY = f"""# A comment, as the real file opens with.
apiVersion: fabric.fieldstate.nz/v1
kind: Compatibility
spec:
  component: saas-fabric
  repository: {RUNTIME}
  minimumRollableVersion: {{floor}}
  publication:
    publisherContract: {{publisher}}
    minimumReaderContract: {{reader}}
"""


def write_tree(
    root: Path,
    *,
    floor: str = "0.3.0-preview.16",
    publisher: int = 3,
    reader: int = 1,
    compiled_floor: str | None = None,
    compiled_publisher: int | None = None,
    compiled_reader: int | None = None,
) -> None:
    (root / "compatibility.yaml").write_text(
        COMPATIBILITY.format(floor=floor, publisher=publisher, reader=reader), encoding="utf-8"
    )
    sources = {
        "crates/fabric-component/src/compatibility.rs": (
            f'pub const MINIMUM_ROLLABLE_VERSION: &str = "{compiled_floor or floor}";\n'
            f'pub const RUNTIME_REPOSITORY: &str = "{RUNTIME}";\n'
        ),
        "crates/fabric-runtime-publication/src/manifest.rs": (
            f"pub const CONTRACT_VERSION: u32 = {compiled_publisher or publisher};\n"
        ),
        "crates/fabric-runtime-publication/src/record.rs": (
            f"pub const MINIMUM_READER_CONTRACT: u32 = {compiled_reader or reader};\n"
        ),
    }
    for relative, text in sources.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8")


class TreeCase(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)

    def run_cli(self, *argv: str) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            status = rc.main(list(argv), root=self.root)
        return status, out.getvalue(), err.getvalue()


class Precedence(unittest.TestCase):
    def test_semver_precedence(self) -> None:
        ordered = [
            "0.2.9",
            "0.3.0-preview.9",
            "0.3.0-preview.15",
            "0.3.0-preview.16",
            "0.3.0-preview.16.1",
            "0.3.0-preview.100",
            "0.3.0-rc.1",
            "0.3.0",
            "0.3.1-preview.1",
            "1.0.0",
        ]
        for lower, higher in zip(ordered, ordered[1:]):
            with self.subTest(lower=lower, higher=higher):
                self.assertTrue(rc.precedes(lower, higher))
                self.assertFalse(rc.precedes(higher, lower))
        self.assertFalse(rc.precedes("0.3.0-preview.16", "0.3.0-preview.16"))

    def test_numeric_identifiers_precede_alphanumeric(self) -> None:
        self.assertTrue(rc.precedes("0.3.0-1", "0.3.0-alpha"))

    def test_a_version_release_yml_would_refuse_is_refused(self) -> None:
        for text in ("v0.3.0", "0.3", "0.03.0", "0.3.0-preview.016", "0.3.0+build", "0.3.0-", "x" * 129):
            with self.subTest(text=text), self.assertRaises(rc.Refused):
                rc.parse_version(text)


class Floor(TreeCase):
    def setUp(self) -> None:
        super().setUp()
        write_tree(self.root)

    def test_the_floor_and_above_are_admitted(self) -> None:
        for version in ("0.3.0-preview.16", "0.3.0-preview.17", "0.3.0", "0.4.0-preview.1"):
            with self.subTest(version=version):
                status, out, _ = self.run_cli("floor", version)
                self.assertEqual(status, 0)
                self.assertIn("at or above the floor 0.3.0-preview.16", out)

    def test_below_the_floor_is_refused(self) -> None:
        for version in ("0.3.0-preview.15", "0.3.0-preview.9", "0.2.0", "0.3.0-dev"):
            with self.subTest(version=version):
                status, _, err = self.run_cli("floor", version)
                self.assertEqual(status, 1)
                self.assertIn("::error::", err)

    def test_an_unpublishable_version_is_refused(self) -> None:
        status, _, err = self.run_cli("floor", "v0.3.0-preview.16")
        self.assertEqual(status, 1)
        self.assertIn("not a version", err)

    def test_show_prints_the_floor_and_repository(self) -> None:
        self.assertEqual(self.run_cli("show", "minimumRollableVersion")[1], "0.3.0-preview.16\n")
        self.assertEqual(self.run_cli("show", "repository")[1], f"{RUNTIME}\n")


class Contracts(TreeCase):
    def test_agreement_prints_the_contracts(self) -> None:
        write_tree(self.root)
        status, out, _ = self.run_cli("contracts")
        self.assertEqual(status, 0)
        self.assertEqual(
            json.loads(out),
            {
                "minimumRollableVersion": "0.3.0-preview.16",
                "repository": RUNTIME,
                "publisherContract": 3,
                "minimumReaderContract": 1,
            },
        )

    def test_every_disagreement_is_refused(self) -> None:
        cases = {
            "publisherContract": {"compiled_publisher": 2},
            "minimumReaderContract": {"compiled_reader": 2},
            "minimumRollableVersion": {"compiled_floor": "0.3.0-preview.15"},
        }
        for key, override in cases.items():
            with self.subTest(key=key):
                write_tree(self.root, **override)
                status, _, err = self.run_cli("contracts")
                self.assertEqual(status, 1)
                self.assertIn(key, err)

    def test_a_missing_or_duplicated_declaration_is_refused(self) -> None:
        write_tree(self.root)
        manifest = self.root / "crates/fabric-runtime-publication/src/manifest.rs"
        manifest.write_text("pub const CONTRACT_VERSION: u32 = 3;\n" * 2, encoding="utf-8")
        self.assertIn("2 times", self.run_cli("contracts")[2])
        manifest.write_text("pub const RENAMED: u32 = 3;\n", encoding="utf-8")
        self.assertIn("0 times", self.run_cli("contracts")[2])
        manifest.unlink()
        self.assertIn("does not exist", self.run_cli("contracts")[2])

    def test_the_real_tree_agrees(self) -> None:
        contracts = rc.check_contracts(REPO_ROOT)
        self.assertGreaterEqual(contracts["publisherContract"], 3)
        rc.check_floor(contracts["minimumRollableVersion"], REPO_ROOT)


class BinaryContracts(TreeCase):
    VERSION = "0.3.0-preview.16"

    def setUp(self) -> None:
        super().setUp()
        write_tree(self.root)
        self.runtime = {"binary": "fabric-api", "readerContract": 1, "version": "0.3.0"}
        self.control_plane = {
            "binary": "fabric-control-plane-api",
            "minimumReaderContract": 1,
            "minimumRollableVersion": "0.3.0-preview.16",
            "publisherContract": 3,
            "version": "0.3.0",
        }

    def check(self, role: str, report: object, version: str | None = None) -> tuple[int, str, str]:
        reported = self.root / "reported.json"
        reported.write_text(report if isinstance(report, str) else json.dumps(report), encoding="utf-8")
        return self.run_cli(
            "binary-contracts", "--role", role, "--version", version or self.VERSION, "--reported", str(reported)
        )

    def assert_refused(self, role: str, report: object, reason: str, version: str | None = None) -> None:
        status, out, err = self.check(role, report, version)
        self.assertEqual(status, 1, out)
        self.assertIn("::error::", err)
        self.assertIn(reason, err)

    def test_binaries_that_agree_with_the_file_are_admitted(self) -> None:
        for role, report in (("runtime", self.runtime), ("controlPlane", self.control_plane)):
            with self.subTest(role=role):
                status, out, _ = self.check(role, report)
                self.assertEqual(status, 0)
                self.assertIn(report["binary"], out)

    def test_a_dev_build_and_a_stable_release_report_the_same_core(self) -> None:
        for version in ("0.3.0-dev", "0.3.0"):
            with self.subTest(version=version):
                self.assertEqual(self.check("controlPlane", self.control_plane, version)[0], 0)

    def test_a_publisher_contract_the_file_does_not_state_is_refused(self) -> None:
        for key, value in (
            ("publisherContract", 2),
            ("minimumReaderContract", 2),
            ("minimumRollableVersion", "0.3.0-preview.15"),
        ):
            with self.subTest(key=key):
                self.assert_refused("controlPlane", {**self.control_plane, key: value}, key)

    def test_a_reader_below_the_minimum_the_file_states_is_refused(self) -> None:
        write_tree(self.root, reader=2)
        self.assert_refused("runtime", self.runtime, "below the minimumReaderContract 2")

    def test_a_reader_above_the_minimum_is_admitted(self) -> None:
        self.assertEqual(self.check("runtime", {**self.runtime, "readerContract": 2})[0], 0)

    def test_a_binary_reporting_the_wrong_shape_is_refused(self) -> None:
        missing = {key: value for key, value in self.control_plane.items() if key != "publisherContract"}
        self.assert_refused("controlPlane", missing, "reported")
        self.assert_refused("controlPlane", {**self.control_plane, "extra": 1}, "reported")
        self.assert_refused("runtime", self.control_plane, "reported")
        self.assert_refused("controlPlane", [], "JSON object")
        self.assert_refused("controlPlane", "not json", "did not print JSON")
        self.assert_refused("controlPlane", "", "did not print JSON")

    def test_a_contract_that_is_not_a_positive_integer_is_refused(self) -> None:
        for value in (True, "3", 0, 3.0, None):
            with self.subTest(value=value):
                self.assert_refused("controlPlane", {**self.control_plane, "publisherContract": value}, "positive integer")

    def test_another_binary_or_version_is_refused(self) -> None:
        self.assert_refused("runtime", {**self.runtime, "binary": "fabric-control-plane-api"}, "not fabric-api")
        self.assert_refused("runtime", {**self.runtime, "version": "0.2.0"}, "version core")
        self.assert_refused("runtime", self.runtime, "version core", version="0.4.0-preview.1")

    def test_an_image_with_no_reporting_binary_is_refused(self) -> None:
        self.assert_refused("console", self.runtime, "no binary")

    def test_an_unreadable_report_is_refused(self) -> None:
        status, _, err = self.run_cli(
            "binary-contracts", "--role", "runtime", "--version", self.VERSION, "--reported", str(self.root / "none")
        )
        self.assertEqual(status, 1)
        self.assertIn("could not be read", err)


class Parser(unittest.TestCase):
    def test_the_subset_is_parsed(self) -> None:
        parsed = rc.parse_subset("a: 1\nb:\n  c: 'x y'\n  d:\n    e: f\ng: h\n")
        self.assertEqual(parsed, {"a": 1, "b": {"c": "x y", "d": {"e": "f"}}, "g": "h"})

    def test_anything_outside_the_subset_is_refused(self) -> None:
        for text in (
            "a: [1, 2]\n",
            "a: {b: 1}\n",
            "a: &anchor 1\n",
            "a: 1 # trailing comment\n",
            "a:\n - 1\n",
            "a:\n   b: 1\n",
            "a: 1\n  b: 2\n",
            "a: 1\na: 2\n",
            "a:\tb\n",
            "a: |\n  text\n",
            "a: 'unterminated\n",
        ):
            with self.subTest(text=text), self.assertRaises(rc.Refused):
                rc.parse_subset(text)

    def test_the_real_file_parses(self) -> None:
        spec = rc.load_compatibility(REPO_ROOT)
        self.assertEqual(spec["repository"], RUNTIME)


class Provenance(TreeCase):
    def setUp(self) -> None:
        super().setUp()
        write_tree(self.root)
        self.out = self.root / "release-provenance.json"

    def render(self, *images: str, version: str = "0.3.0-preview.16", commit: str = COMMIT) -> tuple[int, str]:
        argv = ["provenance", "--version", version, "--commit", commit,
                "--source", "https://github.com/FieldstateNZ/saas-fabric", "--out", str(self.out)]
        for image in images:
            argv += ["--image", image]
        status, _, err = self.run_cli(*argv)
        return status, err

    def all_images(self) -> list[str]:
        return [
            f"runtime={RUNTIME}@{DIGEST_A}",
            f"controlPlane={RUNTIME}-control-plane@{DIGEST_B}",
            f"console={RUNTIME}-control-plane-ui@{DIGEST_C}",
        ]

    def test_records_the_commit_every_image_and_the_contracts(self) -> None:
        status, err = self.render(*self.all_images())
        self.assertEqual(status, 0, err)
        spec = json.loads(self.out.read_text())["spec"]
        self.assertEqual(spec["version"], "0.3.0-preview.16")
        self.assertEqual(spec["source"]["commit"], COMMIT)
        self.assertEqual(
            [(image["role"], image["digest"]) for image in spec["images"]],
            [("console", DIGEST_C), ("controlPlane", DIGEST_B), ("runtime", DIGEST_A)],
        )
        self.assertEqual(spec["contracts"]["publisherContract"], 3)
        self.assertEqual(spec["contracts"]["minimumReaderContract"], 1)
        self.assertEqual(spec["compatibility"]["digest"], rc.compatibility_digest(self.root))

    def test_rendering_is_deterministic(self) -> None:
        self.render(*self.all_images())
        first = self.out.read_bytes()
        self.render(*reversed(self.all_images()))
        self.assertEqual(self.out.read_bytes(), first)

    def test_refuses_what_it_cannot_record_truthfully(self) -> None:
        good = self.all_images()
        cases = {
            "below the floor": dict(images=good, version="0.3.0-preview.15"),
            "short commit": dict(images=good, commit="0123456"),
            "no images": dict(images=[]),
            "bad digest": dict(images=[f"runtime={RUNTIME}@sha256:abc"]),
            "tag not digest": dict(images=[f"runtime={RUNTIME}@latest"]),
            "duplicate role": dict(images=[good[0], f"runtime={RUNTIME}@{DIGEST_B}"]),
            "no runtime image": dict(images=good[1:]),
            "malformed": dict(images=["runtime"]),
        }
        for name, case in cases.items():
            with self.subTest(name=name):
                if self.out.exists():
                    self.out.unlink()
                status, err = self.render(
                    *case["images"],
                    version=case.get("version", "0.3.0-preview.16"),
                    commit=case.get("commit", COMMIT),
                )
                self.assertEqual(status, 1)
                self.assertIn("::error::", err)
                self.assertFalse(self.out.exists())

    def test_refuses_when_the_contracts_disagree(self) -> None:
        write_tree(self.root, compiled_publisher=2)
        status, err = self.render(*self.all_images())
        self.assertEqual(status, 1)
        self.assertIn("publisherContract", err)


SOURCE = "https://github.com/FieldstateNZ/saas-fabric"
ROLES = {
    "runtime": (RUNTIME, DIGEST_A),
    "controlPlane": (f"{RUNTIME}-control-plane", DIGEST_B),
    "console": (f"{RUNTIME}-control-plane-ui", DIGEST_C),
}


class FloorArtifact(TreeCase):
    """A floor proof as the workflow assembles it: the pulled release bundle
    and component descriptor, and each role's resolved tag and labels."""

    def setUp(self) -> None:
        super().setUp()
        write_tree(self.root)
        self.proof = self.root / "floor"
        self.roles = self.root / "roles.txt"
        self.roles.write_text("".join(f"{role} {repo}\n" for role, (repo, _) in ROLES.items()))
        (self.proof / "release").mkdir(parents=True)
        (self.proof / "descriptor").mkdir()
        (self.proof / "images").mkdir()
        (self.proof / "release" / "compatibility.yaml").write_bytes(
            (self.root / "compatibility.yaml").read_bytes()
        )
        rendered = rc.render_provenance(
            "0.3.0-preview.16", COMMIT, SOURCE,
            [f"{role}={repo}@{digest}" for role, (repo, digest) in ROLES.items()], self.root,
        )
        self.provenance = json.loads(rendered)
        self.descriptor = {
            "apiVersion": "fabric.fieldstate.nz/v1",
            "kind": "Component",
            "spec": {
                "name": "saas-fabric",
                "version": "0.3.0-preview.16",
                "images": {role: {"repository": repo, "digest": digest} for role, (repo, digest) in ROLES.items()},
            },
        }
        self.observed = {
            role: {
                "digest": digest,
                "labels": {
                    "org.opencontainers.image.source": SOURCE,
                    "org.opencontainers.image.revision": COMMIT,
                    "org.opencontainers.image.version": "0.3.0-preview.16",
                },
            }
            for role, (_, digest) in ROLES.items()
        }

    def check(self, source: str = SOURCE) -> tuple[int, str]:
        (self.proof / "release" / "release-provenance.json").write_text(json.dumps(self.provenance))
        if self.descriptor is not None:
            (self.proof / "descriptor" / "component.json").write_text(json.dumps(self.descriptor))
        for role, observed in self.observed.items():
            (self.proof / "images" / f"{role}.json").write_text(json.dumps(observed))
        status, _, err = self.run_cli(
            "floor-artifact", "--proof", str(self.proof), "--source", source, "--roles", str(self.roles)
        )
        return status, err

    def assert_refused(self, fragment: str, source: str = SOURCE) -> None:
        status, err = self.check(source)
        self.assertEqual(status, 1, "the floor proof was admitted")
        self.assertIn("::error::", err)
        self.assertIn(fragment, err)

    def images(self) -> dict:
        return {image["role"]: image for image in self.provenance["spec"]["images"]}

    def test_a_complete_floor_proof_is_admitted(self) -> None:
        status, err = self.check()
        self.assertEqual(status, 0, err)

    # The publisher

    def test_a_missing_publisher_is_refused(self) -> None:
        spec = self.provenance["spec"]
        spec["images"] = [image for image in spec["images"] if image["role"] == "runtime"]
        self.assert_refused("does not name the publisher, controlPlane")

    def test_a_missing_console_is_refused(self) -> None:
        spec = self.provenance["spec"]
        spec["images"] = [image for image in spec["images"] if image["role"] != "console"]
        self.assert_refused("does not name console")

    def test_a_publisher_at_a_foreign_repository_is_refused(self) -> None:
        self.images()["controlPlane"]["repository"] = "ghcr.io/elsewhere/saas-fabric-control-plane"
        self.assert_refused("names controlPlane at")

    def test_a_publisher_by_an_invalid_digest_is_refused(self) -> None:
        self.images()["controlPlane"]["digest"] = "sha256:abc"
        self.assert_refused("not a sha256 digest")

    def test_a_publisher_whose_tag_resolves_elsewhere_is_refused(self) -> None:
        self.observed["controlPlane"]["digest"] = "sha256:" + "d" * 64
        self.assert_refused("not the recorded")

    def test_a_publisher_built_from_another_commit_is_refused(self) -> None:
        self.observed["controlPlane"]["labels"]["org.opencontainers.image.revision"] = "f" * 40
        self.assert_refused("image.revision")

    def test_a_publisher_labelled_as_another_version_is_refused(self) -> None:
        self.observed["controlPlane"]["labels"]["org.opencontainers.image.version"] = "0.3.0-preview.17"
        self.assert_refused("image.version")

    def test_an_unpublished_role_is_refused(self) -> None:
        self.provenance["spec"]["images"].append(
            {"role": "extra", "repository": f"{RUNTIME}-extra", "digest": DIGEST_A}
        )
        self.assert_refused("which this release does not publish")

    def test_roles_without_a_publisher_are_refused(self) -> None:
        self.roles.write_text(f"runtime {RUNTIME}\n")
        self.assert_refused("no controlPlane role")

    # The source

    def test_a_malformed_commit_is_refused(self) -> None:
        for commit in ("0123456", "G" * 40, None, COMMIT.upper()):
            with self.subTest(commit=commit):
                self.provenance["spec"]["source"]["commit"] = commit
                self.assert_refused("not a full commit SHA")

    def test_a_foreign_source_is_refused(self) -> None:
        self.provenance["spec"]["source"]["repository"] = "https://github.com/someone/fork"
        self.assert_refused("names the source")

    def test_an_image_labelled_with_a_foreign_source_is_refused(self) -> None:
        self.observed["runtime"]["labels"]["org.opencontainers.image.source"] = "https://github.com/someone/fork"
        self.assert_refused("image.source")

    def test_a_malformed_expected_source_is_refused(self) -> None:
        self.assert_refused("not a source repository URL", source="FieldstateNZ/saas-fabric")

    def test_another_component_or_version_is_refused(self) -> None:
        self.provenance["spec"]["component"] = "other"
        self.assert_refused("component 'other'")
        self.provenance["spec"]["component"] = "saas-fabric"
        self.provenance["spec"]["version"] = "0.3.0-preview.17"
        self.assert_refused("provenance is for")

    # The compatibility file

    def test_a_floor_file_that_is_not_the_recorded_one_is_refused(self) -> None:
        path = self.proof / "release" / "compatibility.yaml"
        path.write_text(path.read_text() + "# edited\n")
        self.assert_refused("not the one its provenance recorded")

    def test_a_missing_floor_file_is_refused(self) -> None:
        (self.proof / "release" / "compatibility.yaml").unlink()
        self.assert_refused("carries no compatibility.yaml")

    def test_recorded_contracts_the_floor_file_does_not_state_are_refused(self) -> None:
        self.provenance["spec"]["contracts"]["publisherContract"] = 4
        self.assert_refused("does not state the contracts")

    def test_a_floor_built_below_this_sources_publisher_contract_must_advance(self) -> None:
        write_tree(self.root, publisher=4)
        self.assert_refused("advance minimumRollableVersion")

    def test_a_floor_with_a_different_reader_contract_is_refused(self) -> None:
        write_tree(self.root, reader=2)
        self.assert_refused("minimumReaderContract")

    def test_the_floor_file_is_compared_with_its_own_hash_not_todays(self) -> None:
        current = self.root / "compatibility.yaml"
        current.write_text("# a later comment\n" + current.read_text())
        status, err = self.check()
        self.assertEqual(status, 0, err)

    # Completion

    def test_an_interrupted_floor_without_a_descriptor_is_refused(self) -> None:
        self.descriptor = None
        self.assert_refused("component descriptor could not be read")

    def test_a_descriptor_naming_other_images_is_refused(self) -> None:
        self.descriptor["spec"]["images"]["controlPlane"]["digest"] = "sha256:" + "d" * 64
        self.assert_refused("name different images")

    def test_a_descriptor_for_another_version_is_refused(self) -> None:
        self.descriptor["spec"]["version"] = "0.3.0-preview.17"
        self.assert_refused("another component or version")

    def test_a_missing_image_observation_is_refused(self) -> None:
        del self.observed["console"]
        self.assert_refused("console image could not be read")

    def test_missing_or_foreign_provenance_is_refused(self) -> None:
        for document in ([], {}, {"apiVersion": "x", "kind": "y"}):
            with self.subTest(document=document):
                self.provenance = document
                status, err = self.check()
                self.assertEqual(status, 1)
                self.assertIn("::error::", err)


if __name__ == "__main__":
    unittest.main()
