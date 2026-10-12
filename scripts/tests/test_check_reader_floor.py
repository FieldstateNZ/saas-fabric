"""Offline tests for scripts/check_reader_floor.py.

Standard library and git only. Each test builds a throwaway repository with
a base commit and a working tree, so a refusal is proved against a known
change rather than against whatever the real history says. One test runs
against this repository's own HEAD, which is what CI does.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import contextlib
import io
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "scripts"))

import check_reader_floor as crf  # noqa: E402

SHAPES = {
    "tenants.json": ["$: array", "$[].tenant: string"],
    "data-sources.json": ["$: array", "$[].placement: one of \"shared\""],
}

COMPATIBILITY = """apiVersion: fabric.fieldstate.nz/v1
kind: Compatibility
spec:
  component: saas-fabric
  repository: ghcr.io/fieldstatenz/saas-fabric
  minimumRollableVersion: 0.3.0-preview.16
  publication:
    publisherContract: 3
    minimumReaderContract: {reader}
"""


def git(root: Path, *argv: str) -> str:
    return subprocess.run(
        ["git", "-C", str(root), *argv], capture_output=True, text=True, check=True
    ).stdout.strip()


class Repository(unittest.TestCase):
    def setUp(self) -> None:
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        git(self.root, "init", "--quiet")
        git(self.root, "config", "user.email", "test@example.invalid")
        git(self.root, "config", "user.name", "test")

    def write(self, *, reader: int = 1, lock_reader: int | None = None, shapes: dict | None = None,
              lock: bool = True) -> None:
        (self.root / "compatibility.yaml").write_text(COMPATIBILITY.format(reader=reader), encoding="utf-8")
        sources = {
            "crates/fabric-component/src/compatibility.rs": (
                'pub const MINIMUM_ROLLABLE_VERSION: &str = "0.3.0-preview.16";\n'
                'pub const RUNTIME_REPOSITORY: &str = "ghcr.io/fieldstatenz/saas-fabric";\n'
            ),
            "crates/fabric-runtime-publication/src/manifest.rs": "pub const CONTRACT_VERSION: u32 = 3;\n",
            "crates/fabric-runtime-publication/src/record.rs": (
                f"pub const MINIMUM_READER_CONTRACT: u32 = {reader};\n"
            ),
        }
        for relative, text in sources.items():
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text, encoding="utf-8")
        path = self.root / crf.LOCK
        if lock:
            path.write_text(
                json.dumps({
                    "minimumReaderContract": reader if lock_reader is None else lock_reader,
                    "shapes": SHAPES if shapes is None else shapes,
                }),
                encoding="utf-8",
            )
        elif path.exists():
            path.unlink()

    def commit(self) -> str:
        git(self.root, "add", "--all")
        git(self.root, "commit", "--quiet", "--allow-empty", "-m", "base")
        return git(self.root, "rev-parse", "HEAD")

    def run_cli(self, *argv: str) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            status = crf.main(list(argv), root=self.root)
        return status, out.getvalue(), err.getvalue()


def changed_shape() -> dict:
    shapes = json.loads(json.dumps(SHAPES))
    shapes["data-sources.json"].append("$[].placement: one of \"archive\"")
    return shapes


class ShapeAndFloor(Repository):
    def test_an_unchanged_shape_passes(self) -> None:
        self.write()
        base = self.commit()
        status, out, err = self.run_cli("--base", base)
        self.assertEqual(status, 0, err)
        self.assertIn("unchanged", out)

    def test_a_shape_change_without_a_floor_raise_is_refused(self) -> None:
        self.write()
        base = self.commit()
        self.write(shapes=changed_shape())
        status, _, err = self.run_cli("--base", base)
        self.assertEqual(status, 1)
        self.assertIn("without raising the reader floor", err)
        self.assertIn('+ data-sources.json $[].placement: one of "archive"', err)

    def test_a_removed_line_is_a_shape_change_too(self) -> None:
        self.write()
        base = self.commit()
        self.write(shapes={"tenants.json": ["$: array"], "data-sources.json": SHAPES["data-sources.json"]})
        status, _, err = self.run_cli("--base", base)
        self.assertEqual(status, 1)
        self.assertIn("- tenants.json $[].tenant: string", err)

    def test_a_shape_change_with_a_floor_raise_passes(self) -> None:
        self.write()
        base = self.commit()
        self.write(reader=2, shapes=changed_shape())
        status, out, err = self.run_cli("--base", base)
        self.assertEqual(status, 0, err)
        self.assertIn("rose from 1 to 2", out)

    def test_a_floor_raise_without_a_shape_change_passes(self) -> None:
        self.write()
        base = self.commit()
        self.write(reader=2)
        status, _, err = self.run_cli("--base", base)
        self.assertEqual(status, 0, err)

    def test_a_falling_floor_is_refused(self) -> None:
        self.write(reader=2)
        base = self.commit()
        self.write(reader=1)
        status, _, err = self.run_cli("--base", base)
        self.assertEqual(status, 1)
        self.assertIn("fell from 2 to 1", err)

    def test_a_lock_raised_alone_is_refused(self) -> None:
        self.write()
        base = self.commit()
        self.write(lock_reader=2, shapes=changed_shape())
        status, _, err = self.run_cli("--base", base)
        self.assertEqual(status, 1)
        self.assertIn("recorded at reader contract 2", err)

    def test_a_constant_raised_without_compatibility_yaml_is_refused(self) -> None:
        self.write()
        base = self.commit()
        self.write(reader=2, shapes=changed_shape())
        (self.root / "compatibility.yaml").write_text(COMPATIBILITY.format(reader=1), encoding="utf-8")
        status, _, err = self.run_cli("--base", base)
        self.assertEqual(status, 1)
        self.assertIn("minimumReaderContract", err)

    def test_a_base_without_a_lock_has_nothing_to_compare(self) -> None:
        self.write(lock=False)
        base = self.commit()
        self.write()
        status, out, err = self.run_cli("--base", base)
        self.assertEqual(status, 0, err)
        self.assertIn("nothing to compare", out)

    def test_a_missing_lock_is_refused(self) -> None:
        self.write()
        base = self.commit()
        self.write(lock=False)
        status, _, err = self.run_cli("--base", base)
        self.assertEqual(status, 1)
        self.assertIn("does not exist", err)

    def test_an_unknown_base_is_refused(self) -> None:
        self.write()
        self.commit()
        status, _, err = self.run_cli("--base", "0" * 40)
        self.assertEqual(status, 1)
        self.assertIn("is not a commit", err)

    def test_a_malformed_lock_is_refused(self) -> None:
        self.write()
        base = self.commit()
        (self.root / crf.LOCK).write_text('{"minimumReaderContract": true, "shapes": {}}', encoding="utf-8")
        status, _, err = self.run_cli("--base", base)
        self.assertEqual(status, 1)
        self.assertIn("no positive minimumReaderContract", err)


class ThisRepository(unittest.TestCase):
    def test_this_repositorys_lock_passes_against_its_own_head(self) -> None:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            status = crf.main(["--base", "HEAD"])
        self.assertEqual(status, 0, err.getvalue())


if __name__ == "__main__":
    unittest.main()
