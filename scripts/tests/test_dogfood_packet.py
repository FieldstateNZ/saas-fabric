"""Golden-output tests for `dogfood.py packet`, the offline activation packet.

The packet is rendered from the shipped profile.toml and a SYNTHETIC but
well-formed lock (image IDs no daemon produced). Any change to what an
authorizer would be shown -- commit, image IDs, ports, gate, inverse,
ceilings, canary plan, cleanup -- changes the bytes and fails here until
golden/dogfood_packet.md is regenerated and reviewed.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import contextlib
import hashlib
import io
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

from test_disposable_dogfood import FakeDocker, dogfood, real_lock_json  # noqa: E402

GOLDEN = Path(__file__).resolve().parent / "golden" / "dogfood_packet.md"
SHIPPED_PROFILE = dogfood.HERE / "profile.toml"


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class ActivationPacket(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name) / "profile"
        self.dir.mkdir()
        (self.dir / "profile.toml").write_text(SHIPPED_PROFILE.read_text(encoding="utf-8"), encoding="utf-8")
        self.docker = FakeDocker()
        for patch in (
            mock.patch.object(dogfood, "run_docker", self.docker),
            mock.patch.object(dogfood, "worktree_head", lambda root: dogfood.PINNED_COMMIT),
        ):
            patch.start()
            self.addCleanup(patch.stop)

    def run_main(self, argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = dogfood.main(["--profile-dir", str(self.dir), *argv])
        return code, out.getvalue(), err.getvalue()

    def prepare(self, lock_json: str) -> Path:
        out = self.dir / ".out"
        out.mkdir()
        (out / dogfood.LOCK_NAME).write_text(lock_json)
        code, _, err = self.run_main(["prepare"])
        self.assertEqual(code, 0, err)
        return out

    def test_packet_matches_the_golden_output(self):
        self.prepare(real_lock_json())
        code, out, err = self.run_main(["packet"])
        self.assertEqual(code, 0, err)
        self.assertEqual(out, GOLDEN.read_text(encoding="utf-8"))
        self.assertEqual(self.docker.calls, [], "the packet is offline: no Docker call")
        self.assertEqual(self.run_main(["packet"])[1], out, "deterministic")

    def test_packet_fingerprint_covers_every_line_above_it(self):
        self.prepare(real_lock_json())
        out = self.run_main(["packet"])[1]
        body, _, tail = out.rpartition("packet-sha256: ")
        self.assertEqual(tail.strip(), hashlib.sha256(body.encode()).hexdigest())

    def test_packet_names_what_the_authorizer_approves(self):
        self.prepare(real_lock_json())
        out = self.run_main(["packet"])[1]
        profile = dogfood.load_profile(self.dir / "profile.toml")
        lock = dogfood.parse_lock(json.loads(real_lock_json()), profile)
        for value in (
            dogfood.PINNED_COMMIT, lock.cp_image_id, lock.console_image_id, lock.keycloak_image_id, profile.keycloak_image,
            dogfood.PROBE_IMAGE, dogfood.ssh_forward_options(profile), dogfood.ssh_forward_command(profile),
            *dogfood.enforcement_apply_commands(profile), *dogfood.enforcement_remove_commands(profile),
            "NOT WorkSpec, runtime or tenant acceptance", "Off-host backup is not a gate",
            "canaries --phase pre --yes", "canaries --phase post", "reset --yes",
        ):
            self.assertIn(value, out)
        for name, text in dogfood.render(profile, lock).files().items():
            self.assertIn(f"- {name}: {dogfood.sha256_text(text)}", out)

    def test_packet_is_refused_while_check_fails(self):
        out = self.prepare(real_lock_json())
        compose = out / dogfood.COMPOSE_NAME
        compose.write_text(compose.read_text().replace('"512m"', '"8g"'))
        code, text, err = self.run_main(["packet"])
        self.assertEqual(code, 1)
        self.assertEqual(text, "", "nothing an authorizer could mistake for a packet")
        self.assertIn("packet refused: `check` fails", err)

    def test_packet_is_refused_without_a_lock_for_synthetic_pins_and_after_activation(self):
        code, text, err = self.run_main(["packet"])
        self.assertEqual((code, text), (1, ""))
        self.assertIn("packet refused", err)

        code, _, err = self.run_main(["prepare", "--synthetic-pins"])
        self.assertEqual(code, 0, err)
        code, text, err = self.run_main(["packet"])
        self.assertEqual((code, text), (1, ""))
        self.assertIn("synthetic image IDs", err)

    def test_packet_is_refused_once_an_activation_exists(self):
        out = self.prepare(real_lock_json())
        (out / dogfood.RECEIPT_NAME).write_text("{}")
        code, text, err = self.run_main(["packet"])
        self.assertEqual((code, text), (1, ""))
        self.assertIn("activation already exists", err)


if __name__ == "__main__":
    unittest.main()
