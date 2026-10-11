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


class FloorArtifact(TreeCase):
    def setUp(self) -> None:
        super().setUp()
        write_tree(self.root)
        self.path = self.root / "floor.json"

    def floor_provenance(self, **contracts: int) -> dict:
        out = self.root / "rendered.json"
        rc.main(
            ["provenance", "--version", "0.3.0-preview.16", "--commit", COMMIT,
             "--source", "https://github.com/FieldstateNZ/saas-fabric", "--out", str(out),
             "--image", f"runtime={RUNTIME}@{DIGEST_A}"],
            root=self.root,
        )
        document = json.loads(out.read_text())
        document["spec"]["contracts"].update(contracts)
        return document

    def check(self, document: object, subject: str = DIGEST_A) -> tuple[int, str]:
        self.path.write_text(json.dumps(document) if document is not None else "not json")
        status, _, err = self.run_cli("floor-artifact", str(self.path), "--subject", subject)
        return status, err

    def test_a_floor_built_at_these_contracts_is_admitted(self) -> None:
        status, err = self.check(self.floor_provenance())
        self.assertEqual(status, 0, err)

    def test_a_floor_without_the_publisher_contract_must_advance(self) -> None:
        status, err = self.check(self.floor_provenance(publisherContract=2))
        self.assertEqual(status, 1)
        self.assertIn("advance minimumRollableVersion", err)

    def test_a_floor_with_a_different_reader_contract_is_refused(self) -> None:
        self.assertEqual(self.check(self.floor_provenance(minimumReaderContract=2))[0], 1)

    def test_provenance_of_another_version_or_image_is_refused(self) -> None:
        other = self.floor_provenance()
        other["spec"]["version"] = "0.3.0-preview.17"
        self.assertEqual(self.check(other)[0], 1)
        self.assertEqual(self.check(self.floor_provenance(), subject=DIGEST_B)[0], 1)

    def test_missing_or_foreign_provenance_is_refused(self) -> None:
        for document in (None, [], {}, {"apiVersion": "x", "kind": "y"}):
            with self.subTest(document=document):
                status, err = self.check(document)
                self.assertEqual(status, 1)
                self.assertIn("::error::", err)


if __name__ == "__main__":
    unittest.main()
