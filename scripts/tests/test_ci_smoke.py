"""Offline tests for examples/disposable-dogfood/ci_smoke.py.

Standard library only; no daemon, no network, no browser. Every external seam
(docker, sudo/iptables, HTTP, Playwright) is a scripted fake or a tripwire
that fails the test if reached. What is proved here:

  * the DEFAULT mode (no --runtime) builds, saves, renders and finishes, and
    cannot reach any runtime, security or credential function: no iptables,
    no network, no container, no realm bootstrap, no credential file;
  * LucentRoot is refused by ID and by name even with GITHUB_ACTIONS=true, in
    both modes, before any build;
  * --runtime is the only way into the runtime path, and it is gated there;
  * a probe's docker-level failure is not counted as a blocked network;
  * a failed cleanup fails the trial;
  * failures surface by assertion name or exception class only, with
    registered secrets redacted and subprocess stderr never surfaced.

Image IDs, daemon identities and secrets here are SYNTHETIC and say so.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[2]
EXAMPLE = REPO_ROOT / "examples" / "disposable-dogfood"
sys.path.insert(0, str(EXAMPLE))

import ci_smoke  # noqa: E402
import dogfood  # noqa: E402

PINNED_KEYCLOAK = "quay.io/keycloak/keycloak@sha256:9d1f1b2b7261ff53c66cb1092dfcdc34a5fb77e81f9e6a6e75b8b6a795de8067"
NODE_DIGEST = "node:22-bookworm-slim@sha256:" + "ab" * 32  # SYNTHETIC
RUNNER_INFO = {"ID": "runner-synthetic-0000", "Name": "fv-az000-synthetic", "ServerVersion": "28.0.0-synthetic"}
LUCENTROOT_INFO = dict(dogfood.EXPECTED_DAEMON)  # the real LucentRoot identity: the thing that must be refused
FAKE_IMAGE_IDS = ["sha256:" + "a1" * 32, "sha256:" + "b2" * 32, "sha256:" + "c3" * 32]  # SYNTHETIC


class Tripwire(Exception):
    """Raised by any seam the mode under test must not reach."""


def tripwire(name):
    def trip(*_args, **_kwargs):
        raise Tripwire(f"{name} was reached")

    return trip


class FakeDocker:
    """Scripted `dogfood.run_docker`; an unscripted call raises, which the script records as a failure."""

    def __init__(self, info: dict):
        self.calls: list[list[str]] = []
        self.info = info
        self.built = 0

    def __call__(self, args, capture=False):
        self.calls.append(list(args))
        head = args[0]
        if head == "info":
            return json.dumps(dict(self.info, OSType="linux"))
        if head == "build":
            image_id = FAKE_IMAGE_IDS[self.built]
            self.built += 1
            return image_id
        if head == "pull" and args[1] in (PINNED_KEYCLOAK, NODE_DIGEST):
            return ""
        if head == "image" and args[1] == "inspect":
            return json.dumps([PINNED_KEYCLOAK])
        if head == "save":
            Path(args[2]).write_bytes(b"synthetic image archive " + args[3].encode())
            return ""
        raise Tripwire(f"unscripted docker call: {args[:2]}")

    def subcommands(self) -> set[str]:
        return {c[0] for c in self.calls}


def fake_extract(destination: Path) -> None:
    """Stands in for `git archive`: the two Dockerfiles the script reads and the real shipped nginx.conf."""
    destination.mkdir(mode=0o700)
    (destination / "Dockerfile").write_text("FROM scratch AS control-plane-api\n")
    ui = destination / "apps" / "control-plane-ui"
    ui.mkdir(parents=True)
    (ui / "Dockerfile").write_text(f"FROM {NODE_DIGEST} AS builder\nFROM scratch AS console\n")
    shutil.copyfile(REPO_ROOT / "apps" / "control-plane-ui" / "nginx.conf", ui / "nginx.conf")


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class SmokeHarness(unittest.TestCase):
    """Every test: GITHUB_ACTIONS=true, a runner-shaped daemon unless overridden, every runtime seam a tripwire."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.runner_temp = Path(self.tmp.name) / "runner-temp"
        self.runner_temp.mkdir()
        self.artifacts = Path(self.tmp.name) / "artifacts"
        self.docker = FakeDocker(RUNNER_INFO)
        self.stages = {name: mock.MagicMock(side_effect=tripwire(name)) for name in ci_smoke.RUNTIME_STAGES}
        patches = [
            mock.patch.dict(os.environ, {"GITHUB_ACTIONS": "true", "RUNNER_TEMP": str(self.runner_temp)}),
            mock.patch.object(dogfood, "run_docker", self.docker),
            mock.patch.object(dogfood, "run_bound", dogfood.run_bound),  # main swaps it in; restore afterwards
            mock.patch.object(dogfood, "EXPECTED_DAEMON", dict(dogfood.EXPECTED_DAEMON)),  # the script mutates it in place
            mock.patch.object(dogfood, "REPO_ROOT", dogfood.REPO_ROOT),  # the script repoints it at the extracted source
            mock.patch.object(dogfood, "extract_pinned_source", fake_extract),
            mock.patch.object(dogfood, "worktree_head", lambda root: dogfood.PINNED_COMMIT),
            # Security, network and credential seams: reaching any of them is a test failure.
            mock.patch.object(ci_smoke, "host_run", tripwire("host_run (sudo/iptables)")),
            mock.patch.object(dogfood, "read_iptables", tripwire("read_iptables")),
            mock.patch.object(dogfood, "preflight_enforcement", tripwire("preflight_enforcement")),
            mock.patch.object(dogfood, "bootstrap_realm", tripwire("bootstrap_realm")),
            mock.patch.object(dogfood, "http_json", tripwire("http_json")),
            mock.patch.object(dogfood, "collision_findings", tripwire("collision_findings")),
            mock.patch.object(ci_smoke.time, "sleep", tripwire("time.sleep")),
            *[mock.patch.object(ci_smoke, name, stage) for name, stage in self.stages.items()],
        ]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

    def run_main(self, *extra):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = ci_smoke.main(["--artifact-dir", str(self.artifacts), *extra])
        manifest = json.loads((self.artifacts / "result-manifest.json").read_text())
        return code, manifest, out.getvalue() + err.getvalue()

    def assert_no_runtime_reached(self):
        for name, stage in self.stages.items():
            stage.assert_not_called()
        self.assertFalse(self.docker.subcommands() & set(ci_smoke.RUNTIME_DOCKER_SUBCOMMANDS), self.docker.calls)
        self.assertNotIn(["pull", NODE_DIGEST], self.docker.calls, "the probe helper is pulled only on the runtime path")
        self.assertEqual([p for p in self.runner_temp.rglob(".ephemeral")], [], "no credential directory was created")
        self.assertEqual([p for p in Path(self.tmp.name).rglob("keycloak.env")], [], "no credential file was created")

    # -- default mode: build only ------------------------------------------

    def test_default_builds_saves_renders_and_finishes_without_any_runtime_call(self):
        code, manifest, _ = self.run_main()
        self.assertEqual(code, 0, manifest)
        self.assertEqual(manifest["mode"], "build-only")
        self.assertEqual(manifest["outcome"], "pass")
        self.assertNotIn("failure", manifest)
        self.assert_no_runtime_reached()
        self.assertEqual(self.docker.subcommands(), {"info", "build", "pull", "image", "save"})
        self.assertEqual(self.docker.calls[0][0], "info", "the daemon is checked before anything else")
        self.assertEqual(sorted(p.name for p in self.artifacts.iterdir()),
                         ["console-image.tar.gz", "cp-image.tar.gz", "keycloak-image.tar.gz", "result-manifest.json"])
        self.assertEqual(manifest["images"], dict(zip(("cp", "console", "keycloak"), FAKE_IMAGE_IDS)))
        self.assertEqual(manifest["runner_daemon"], RUNNER_INFO)
        self.assertEqual([r["outcome"] for r in manifest["assertions"]].count("fail"), 0)
        self.assertIn("rendered artefacts validate with genuine image IDs", [r["assertion"] for r in manifest["assertions"]])
        self.assertEqual(list(self.runner_temp.iterdir()), [], "the private work directory is removed")
        self.assertEqual(manifest.get("cleanup_failures", 0), 0)

    def test_default_refuses_without_github_actions_before_any_docker_call(self):
        with mock.patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}):
            code, manifest, _ = self.run_main()
        self.assertEqual(code, 1)
        self.assertEqual(manifest["failure"], "assertion failed: GITHUB_ACTIONS is true")
        self.assertEqual(self.docker.calls, [])
        self.assert_no_runtime_reached()

    # -- LucentRoot is refused in both modes, even under GITHUB_ACTIONS ----

    def test_lucentroot_is_refused_by_id_and_by_name_before_any_build(self):
        for label, info in (
            ("real identity", LUCENTROOT_INFO),
            ("id only", dict(RUNNER_INFO, ID=ci_smoke.LUCENTROOT_ID)),
            ("name only", dict(RUNNER_INFO, Name=ci_smoke.LUCENTROOT_NAME)),
        ):
            for flags in ((), ("--runtime",)):
                with self.subTest(label=label, flags=flags):
                    self.docker.calls.clear()
                    self.docker.info = info
                    code, manifest, _ = self.run_main(*flags)
                    self.assertEqual(code, 1)
                    self.assertEqual(manifest["failure"], "assertion failed: daemon is not LucentRoot")
                    self.assertEqual([c[0] for c in self.docker.calls], ["info"], "nothing after the identity check")
                    self.assertNotIn("images", manifest)
                    self.assert_no_runtime_reached()
                    self.assertEqual(dogfood.EXPECTED_DAEMON, LUCENTROOT_INFO, "the expectation is never repointed at a refused daemon")

    # -- --runtime is the only door, and it opens after the build -----------

    def test_runtime_flag_is_the_only_way_into_the_runtime_path(self):
        code, manifest, _ = self.run_main("--runtime")
        self.assertEqual(code, 1)
        self.assertEqual(manifest["mode"], "runtime")
        self.assertEqual(manifest["failure"], "Tripwire during iptables")
        self.assertIn(["pull", NODE_DIGEST], self.docker.calls, "the probe helper is pulled first, as runner egress")
        self.stages["iptables_with_sudo"].assert_called_once()
        for name in ci_smoke.RUNTIME_STAGES[1:]:
            self.stages[name].assert_not_called()
        self.assertIn("images", manifest, "the build completed before the runtime gate")
        self.assertTrue((self.artifacts / "cp-image.tar.gz").exists())

    # -- failure reporting: names and classes only, secrets redacted -------

    def test_unexpected_exception_reports_class_and_stage_only(self):
        poison = "token=SYNTHETIC-SECRET-7f3a http://127.0.0.1:18781/auth?code=SYNTHETIC-CODE"
        with mock.patch.object(ci_smoke, "build_only", side_effect=RuntimeError(poison)):
            code, manifest, printed = self.run_main()
        self.assertEqual(code, 1)
        self.assertEqual(manifest["failure"], "RuntimeError during daemon")
        text = json.dumps(manifest) + printed
        self.assertNotIn("SYNTHETIC-SECRET", text)
        self.assertNotIn("code=", text)

    def test_failed_cleanup_fails_the_trial_even_after_a_passing_run(self):
        def build_then_leave_a_failing_cleanup(trial, _artifact_dir, _work):
            trial.defer("synthetic cleanup that fails", tripwire("cleanup"))
            return None, None, None, None

        with mock.patch.object(ci_smoke, "build_only", build_then_leave_a_failing_cleanup):
            code, manifest, _ = self.run_main()
        self.assertEqual(code, 1)
        self.assertEqual(manifest["outcome"], "fail")
        self.assertEqual(manifest["cleanup_failures"], 1)
        self.assertTrue(any(n.startswith("cleanup FAILED: synthetic cleanup that fails: Tripwire") for n in manifest["notes"]))


class TrialLedger(unittest.TestCase):
    def test_check_failure_carries_the_name_only_and_redacts_detail(self):
        trial = ci_smoke.Trial()
        trial.secret("SYNTHETIC-PASSWORD")
        with contextlib.redirect_stdout(io.StringIO()) as out, self.assertRaises(ci_smoke.SmokeError) as raised:
            trial.check("status check", False, "refused with SYNTHETIC-PASSWORD")
        self.assertEqual(raised.exception.assertion, "status check")
        self.assertEqual(str(raised.exception), "status check")
        self.assertEqual(trial.results[-1]["detail"], "refused with [redacted]")
        self.assertNotIn("SYNTHETIC-PASSWORD", out.getvalue())

    def test_run_cleanups_is_lifo_counts_failures_and_names_only_the_exception_class(self):
        trial, order = ci_smoke.Trial(), []
        trial.defer("first", lambda: order.append("first"))
        trial.defer("second", tripwire("second SYNTHETIC-DETAIL"))
        trial.defer("third", lambda: order.append("third"))
        self.assertEqual(trial.run_cleanups(), 1)
        self.assertEqual(order, ["third", "first"])
        self.assertEqual(trial.cleanups, [])
        self.assertIn("cleanup FAILED: second: Tripwire", trial.notes)
        self.assertNotIn("SYNTHETIC-DETAIL", "\n".join(trial.notes))


class ProbeExitCodes(unittest.TestCase):
    def probe_with(self, outcome):
        with mock.patch.object(dogfood, "run_docker", outcome if callable(outcome) else mock.Mock(side_effect=outcome)):
            return ci_smoke.probe("net", "host", 80, NODE_DIGEST)

    def test_only_refused_and_timed_out_count_as_blocked(self):
        self.assertTrue(self.probe_with(lambda *a, **k: ""))
        self.assertFalse(self.probe_with(subprocess.CalledProcessError(1, ["docker", "run"])))
        self.assertFalse(self.probe_with(subprocess.CalledProcessError(2, ["docker", "run"])))

    def test_docker_level_failure_is_not_evidence_of_a_blocked_network(self):
        for code in (125, 126, 127, 137, 3):
            with self.subTest(exit=code), self.assertRaises(ci_smoke.SmokeError):
                self.probe_with(subprocess.CalledProcessError(code, ["docker", "run"]))


class RuntimePosture(unittest.TestCase):
    """The daemon-reported environment: only the control-plane image's own config-path ENV is tolerated."""

    @staticmethod
    def document(service: str, env: list[str]) -> dict:
        return {
            "HostConfig": {"ReadonlyRootfs": True, "CapDrop": ["ALL"], "SecurityOpt": ["no-new-privileges:true"],
                           "NetworkMode": dogfood.NETWORK_NAME, "Tmpfs": {p: "" for p in dogfood.TMPFS_SPECS[service]}},
            "Config": {"Env": env},
            "Mounts": [],
        }

    def test_exact_pinned_cp_config_env_is_allowed_on_cp_only(self):
        allowed = f"FABRIC_CP_CONFIG={dogfood.CP_CONFIG_PATH}"
        self.assertEqual(ci_smoke.CP_IMAGE_CONFIG_ENV, allowed)
        self.assertEqual(ci_smoke.runtime_posture_findings(self.document(dogfood.SERVICE_CP, ["PATH=/usr/bin", allowed]), dogfood.SERVICE_CP), [])
        for service in (dogfood.SERVICE_CONSOLE, dogfood.SERVICE_KEYCLOAK):
            with self.subTest(service=service):
                self.assertEqual(ci_smoke.runtime_posture_findings(self.document(service, [allowed]), service), ["environment key FABRIC_CP_CONFIG"])

    def test_other_fabric_entries_are_still_refused_on_cp(self):
        for entry in ("FABRIC_CP_CONFIG=/tmp/other.toml", "FABRIC_CP_CONFIG=", "FABRIC_CP_CONFIG", "FABRIC_LISTEN=0.0.0.0:1", "FABRIC_=x"):
            with self.subTest(entry=entry):
                findings = ci_smoke.runtime_posture_findings(self.document(dogfood.SERVICE_CP, [entry]), dogfood.SERVICE_CP)
                self.assertEqual(findings, [f"environment key {entry.split('=', 1)[0]}"])
                self.assertNotIn("/tmp/other.toml", "".join(findings), "values never leave the function")


class TokenAcceptancePolling(unittest.TestCase):
    """Only an initial 401 is temporary; the same token is re-presented for a bounded time; nothing else is retried."""

    def poll(self, statuses):
        it, slept = iter(statuses), []
        verdict = ci_smoke.poll_token_acceptance(lambda: next(it), sleep=slept.append)
        return verdict, slept

    def test_bound_is_at_most_thirty_seconds(self):
        self.assertLessEqual(ci_smoke.TOKEN_ACCEPTANCE_ATTEMPTS * ci_smoke.TOKEN_ACCEPTANCE_DELAY, 30.0)

    def test_immediate_200_is_accepted_without_sleeping(self):
        self.assertEqual(self.poll([200]), (("accepted", 200), []))

    def test_401s_are_retried_then_accepted(self):
        verdict, slept = self.poll([401, 401, 200])
        self.assertEqual(verdict, ("accepted", 200))
        self.assertEqual(slept, [ci_smoke.TOKEN_ACCEPTANCE_DELAY] * 2)

    def test_any_other_status_is_rejected_at_once(self):
        for status in (403, 404, 500, 502, 204):
            with self.subTest(status=status):
                verdict, slept = self.poll([401, status, 200])
                self.assertEqual(verdict, ("rejected", status))
                self.assertEqual(len(slept), 1)

    def test_persistent_401_times_out_within_the_bound(self):
        verdict, slept = self.poll([401] * ci_smoke.TOKEN_ACCEPTANCE_ATTEMPTS)
        self.assertEqual(verdict, ("timed out", 401))
        self.assertEqual(len(slept), ci_smoke.TOKEN_ACCEPTANCE_ATTEMPTS - 1)
        self.assertLessEqual(sum(slept), 30.0)


class QuietSubprocess(unittest.TestCase):
    def test_build_only_keeps_a_bounded_stderr_tail_in_memory_but_not_on_the_exception(self):
        with mock.patch.object(ci_smoke, "CAPTURE_BUILD_STDERR", True), mock.patch.object(ci_smoke, "build_stderr_tails", []):
            with self.assertRaises(subprocess.CalledProcessError) as raised:
                ci_smoke.quiet_run_bound(sys.executable, ["-c", "import sys; sys.stderr.write('x'*5000 + 'SYNTHETIC-CAUSE'); sys.exit(7)"], True)
            self.assertIsNone(raised.exception.stderr)
            self.assertEqual(raised.exception.cmd[1:], ["-c"])
            self.assertEqual(len(ci_smoke.build_stderr_tails), 1)
            tail = ci_smoke.build_stderr_tails[0]
            self.assertIn("SYNTHETIC-CAUSE", tail)
            self.assertIn("exited 7", tail)
            self.assertLess(len(tail), ci_smoke.BUILD_STDERR_TAIL_BYTES + 200, "the tail is bounded")

    def test_quiet_run_bound_discards_stderr_and_truncates_argv_on_failure(self):
        with self.assertRaises(subprocess.CalledProcessError) as raised:
            ci_smoke.quiet_run_bound(sys.executable, ["-c", "import sys; sys.stderr.write('SYNTHETIC-LEAK'); sys.exit(7)"], True)
        error = raised.exception
        self.assertEqual(error.returncode, 7)
        self.assertEqual(error.cmd[1:], ["-c"], "only the subcommand word survives")
        self.assertNotIn("SYNTHETIC-LEAK", str(error))
        self.assertIsNone(error.stderr)
        self.assertEqual(ci_smoke.quiet_run_bound(sys.executable, ["-c", "print('out')"], True), b"out\n")

    def test_host_run_truncates_argv_on_failure(self):
        with self.assertRaises(subprocess.CalledProcessError) as raised:
            ci_smoke.host_run([sys.executable, "-c", "import sys; sys.stderr.write('SYNTHETIC-LEAK'); sys.exit(3)"])
        self.assertEqual(raised.exception.returncode, 3)
        self.assertEqual(raised.exception.cmd[1:], ["-c"])
        self.assertNotIn("SYNTHETIC-LEAK", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
