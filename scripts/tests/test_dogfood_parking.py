"""The disposable dogfood profile is parked (D01-9), and the parking holds.

D01-9 retired `/api/session` and the loopback workbench only once this
profile had moved to the gateway flow or been explicitly parked. It was
parked. These tests pin what that means, so neither half can drift:

- the profile is still pinned to a source commit that serves the
  `/api/session` route its sign-in stage drives, and the working tree no
  longer serves it -- so re-pinning would break the profile, not revive it;
- the runbook and every activation packet say the profile is parked;
- no release, image or hosting workflow refers to the profile, and its own
  workflow has no trigger beyond its branch and a manual dispatch.

Reads the pinned commit from git history, so CI checks out with full depth.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import re
import subprocess
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
PROFILE_DIR = REPO_ROOT / "examples" / "disposable-dogfood"
sys.path.insert(0, str(PROFILE_DIR))

import dogfood  # noqa: E402

ROUTES = "crates/fabric-control-plane/src/routes.rs"
SESSION_HANDLER = "crates/fabric-control-plane/src/handlers/session.rs"
SESSION_ROUTE = re.compile(r'"/session"')
WORKFLOWS = REPO_ROOT / ".github" / "workflows"
DOGFOOD_WORKFLOW = WORKFLOWS / "disposable-dogfood.yml"


def at_pinned_commit(path: str) -> str:
    """The file as the pinned commit holds it; fails, never skips, if history is missing."""
    result = subprocess.run(
        ["git", "-C", str(REPO_ROOT), "show", f"{dogfood.PINNED_COMMIT}:{path}"],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(
            f"cannot read {path} at the pinned commit {dogfood.PINNED_COMMIT}; "
            "check out with full history (fetch-depth: 0)"
        )
    return result.stdout


class ThePinIsWhatKeepsTheProfileCoherent(unittest.TestCase):
    def test_the_pinned_commit_still_serves_the_session_route_the_profile_drives(self):
        self.assertRegex(at_pinned_commit(ROUTES), SESSION_ROUTE)
        self.assertIn("redeem_session", at_pinned_commit(SESSION_HANDLER))

    def test_the_working_tree_no_longer_serves_it(self):
        self.assertNotRegex((REPO_ROOT / ROUTES).read_text(encoding="utf-8"), SESSION_ROUTE)
        self.assertFalse((REPO_ROOT / SESSION_HANDLER).exists())

    def test_the_profile_drives_the_session_route(self):
        # If this ever fails, the profile no longer depends on the retired
        # route, and the parking (and this file) should be revisited.
        smoke = (PROFILE_DIR / "ci_smoke.py").read_text(encoding="utf-8")
        self.assertIn("/api/session", smoke)
        self.assertIn("sign_in_through_console", smoke)

    @unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
    def test_the_shipped_profile_is_pinned_to_that_commit(self):
        profile = dogfood.load_profile(PROFILE_DIR / "profile.toml")
        self.assertEqual(profile.source_commit, dogfood.PINNED_COMMIT)


class TheParkingIsStated(unittest.TestCase):
    def test_the_runbook_says_parked_unsupported_and_not_deployed(self):
        readme = (PROFILE_DIR / "README.md").read_text(encoding="utf-8")
        match = re.search(r"^## Parked \(D01-9\)\n(.*?)^## ", readme, re.M | re.S)
        self.assertIsNotNone(match, "README has no 'Parked (D01-9)' section ahead of the next one")
        section = match.group(1)
        self.assertIn("documented, unsupported, and not part of any\ndeploy", section)
        self.assertIn("/api/session", section)

    def test_every_activation_packet_says_so(self):
        golden = (Path(__file__).resolve().parent / "golden" / "dogfood_packet.md").read_text(encoding="utf-8")
        self.assertIn(dogfood.PARKED, golden.splitlines()[:5])


class TheProfileIsNotPartOfAnyDeploy(unittest.TestCase):
    def test_no_other_workflow_refers_to_the_profile(self):
        for workflow in sorted(WORKFLOWS.glob("*.yml")):
            if workflow == DOGFOOD_WORKFLOW:
                continue
            self.assertNotIn("disposable-dogfood", workflow.read_text(encoding="utf-8"), workflow.name)

    def test_no_image_build_refers_to_the_profile(self):
        for dockerfile in (REPO_ROOT / "Dockerfile", REPO_ROOT / "apps" / "control-plane-ui" / "Dockerfile"):
            self.assertNotIn("disposable-dogfood", dockerfile.read_text(encoding="utf-8"), str(dockerfile))

    def test_its_own_workflow_runs_only_on_its_branch_or_by_hand(self):
        text = DOGFOOD_WORKFLOW.read_text(encoding="utf-8")
        triggers = text[text.index("\non:\n") : text.index("\npermissions:")]
        self.assertIn("branches: [codex/disposable-dogfood]", triggers)
        self.assertIn("workflow_dispatch:", triggers)
        for forbidden in ("pull_request", "schedule", "tags", "release", "main"):
            self.assertNotIn(forbidden, triggers, forbidden)


if __name__ == "__main__":
    unittest.main()
