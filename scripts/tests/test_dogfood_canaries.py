"""Synthetic tests for `dogfood.py canaries --phase pre|post`.

Every Docker, iptables, /proc, /sys and loopback call goes through a scripted
fake that fails the test on any unscripted call; nothing here touches a real
daemon, firewall or network. Container IDs, image IDs and credentials are
SYNTHETIC. The tests prove the canaries pass only on complete, expected
evidence and fail closed on wrong or missing evidence; they do not prove any
real host is isolated.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))

from test_disposable_dogfood import (  # noqa: E402
    FAKE_CONTAINER_IDS,
    FAKE_NETWORK_ID,
    GOOD_INFO,
    GOOD_PROFILE,
    FakeDocker,
    container_doc,
    dogfood,
    iptables_listing,
    network_doc,
    profile_toml,
    real_lock_json,
)

SYNTHETIC_ADMIN_PASSWORD = "synthetic-bootstrap-admin-password-not-real"
GATEWAY = "10.213.7.1"
LISTENERS = [("0.0.0.0", 22), ("127.0.0.1", 18780), ("127.0.0.1", 18781), ("192.168.50.10", 6443), ("::", 443)]
PROBE_ID = "8" * 64


def full_ruleset(profile: dogfood.Profile, **variant) -> str:
    listings = iptables_listing(profile, **variant)
    return "".join(listings.values()) + "-P FORWARD DROP\n-N DOCKER\n-A FORWARD -j DOCKER-USER\n"


def outcomes_for(targets, **overrides) -> str:
    results = {t.id: ("status:200" if t.expect == "answers" else "timeout") for t in targets}
    results.update(overrides)
    return json.dumps({"results": results}) + "\n"


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class Canaries(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name) / "profile"
        self.dir.mkdir()
        (self.dir / "profile.toml").write_text(profile_toml(GOOD_PROFILE), encoding="utf-8")
        self.profile = dogfood.parse_profile(dict(GOOD_PROFILE))
        self.docker = FakeDocker()
        self.ruleset = full_ruleset(self.profile)
        self.listeners = list(LISTENERS)
        self.bridge_present = False
        self.loopback = {f"{self.profile.console_origin}/healthz": (200, "ok", {}),
                         f"{self.profile.issuer}/.well-known/openid-configuration": (200, self.discovery(), {})}
        self.http_calls: list[str] = []

        def ruleset():
            if isinstance(self.ruleset, BaseException):
                raise self.ruleset
            return self.ruleset

        def listeners():
            if isinstance(self.listeners, BaseException):
                raise self.listeners
            return self.listeners

        def http_json(url, profile, *rest, **kw):
            self.http_calls.append(url)
            answer = self.loopback[url]
            if isinstance(answer, BaseException):
                raise answer
            return answer

        patches = [
            mock.patch.object(dogfood, "run_docker", self.docker),
            mock.patch.object(dogfood, "worktree_head", lambda root: dogfood.PINNED_COMMIT),
            mock.patch.object(dogfood, "read_iptables_ruleset", ruleset),
            mock.patch.object(dogfood, "read_host_listeners", listeners),
            mock.patch.object(dogfood, "bridge_interface_present", lambda: self.bridge_present),
            mock.patch.object(dogfood, "http_json", http_json),
            mock.patch.object(dogfood, "utc_stamp", lambda: "20261007T000000Z"),
        ]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

    def discovery(self) -> dict:
        return {
            "issuer": self.profile.issuer,
            "authorization_endpoint": dogfood.derive_endpoints(self.profile.issuer, self.profile.reachable_at)["authorization"],
            "code_challenge_methods_supported": ["S256"],
        }

    def run_main(self, argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = dogfood.main(["--profile-dir", str(self.dir), *argv])
        return code, out.getvalue(), err.getvalue()

    def activate_on_disk(self):
        out = self.dir / ".out"
        out.mkdir()
        (out / dogfood.LOCK_NAME).write_text(real_lock_json())
        lock = dogfood.parse_lock(json.loads(real_lock_json()), self.profile)
        rendered = dogfood.render(self.profile, lock)
        dogfood.write_rendered(dogfood.Paths(self.dir), rendered)
        receipt = {
            "project_name": self.profile.project_name,
            "daemon": dict(dogfood.EXPECTED_DAEMON),
            "compose_sha256": dogfood.sha256_text(rendered.compose_json),
            "containers": {s: {"id": FAKE_CONTAINER_IDS[s], "name": dogfood.CONTAINER_NAMES[s], "image": lock.image_for(s)} for s in dogfood.CONTAINER_NAMES},
            "network": {"id": FAKE_NETWORK_ID, "name": dogfood.NETWORK_NAME},
        }
        (out / dogfood.RECEIPT_NAME).write_text(json.dumps(receipt))
        (out / ".ephemeral").mkdir()
        (out / ".ephemeral" / "keycloak.env").write_text(f"KC_BOOTSTRAP_ADMIN_USERNAME=bootstrap-synthetic\nKC_BOOTSTRAP_ADMIN_PASSWORD={SYNTHETIC_ADMIN_PASSWORD}\n")

    def script_activation(self, probe_output=None, network=None, probe_digests=None, probe_exit=None, lingering=None, state_running=True):
        project = self.profile.project_name
        attached = {FAKE_CONTAINER_IDS[s]: {"Name": n, "IPv4Address": f"10.213.7.{i + 2}/24", "MacAddress": "02:42:00:00:00:0" + str(i)}
                    for i, (s, n) in enumerate(dogfood.CONTAINER_NAMES.items())}
        net = network if network is not None else network_doc(project, attached)
        if network is None:
            net["IPAM"]["Config"][0]["Gateway"] = GATEWAY
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.on(lambda a: a[:2] == ["image", "inspect"], json.dumps(probe_digests if probe_digests is not None else [dogfood.PROBE_REPO_DIGEST]))
        documents = {FAKE_CONTAINER_IDS[s]: dict(container_doc(s, project), State={"Running": state_running}) for s in dogfood.CONTAINER_NAMES}
        documents[FAKE_NETWORK_ID] = net
        documents[dogfood.CANARY_PROBE_NAME] = lingering
        self.docker.inspect_responses(documents)

        def run(args):
            if probe_exit is not None:
                raise subprocess.CalledProcessError(probe_exit, "docker")
            if probe_output is not None:
                return probe_output
            targets = dogfood.canary_pre_targets(self.profile, GATEWAY, self.listeners)
            return outcomes_for(targets)

        self.docker.on(lambda a: a[0] == "run", run)
        self.docker.on(lambda a: a[:2] == ["rm", "--force"], "")

    def receipts(self) -> list[dict]:
        directory = self.dir / dogfood.CANARY_DIR_NAME
        return [json.loads(p.read_text()) for p in sorted(directory.glob("canary-*.json"))] if directory.exists() else []

    def probe_runs(self):
        return self.docker.calls_matching("run")

    # -- pre: pass ---------------------------------------------------------

    def test_pre_passes_on_complete_expected_evidence_and_writes_a_receipt(self):
        self.activate_on_disk()
        self.script_activation()
        code, out, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
        self.assertEqual(code, 0, err)
        self.assertIn("canaries pre: PASS", out)
        [receipt] = self.receipts()
        self.assertEqual(receipt["result"], "pass")
        self.assertEqual(receipt["findings"], [])
        self.assertEqual(receipt["evidence"]["iptables_S"], self.ruleset)
        network = receipt["evidence"]["network_inspect"]
        self.assertEqual(network["Name"], dogfood.NETWORK_NAME)
        self.assertTrue(network["Internal"])
        self.assertNotIn("MacAddress", json.dumps(network))
        self.assertEqual(receipt["evidence"]["gateway"], GATEWAY)
        ids = {row["id"]: row for row in receipt["evidence"]["targets"]}
        self.assertEqual(ids["control:console"]["verdict"], "pass")
        for port in (22, 443, 6443, 8200, self.profile.console_port, self.profile.oidc_port):
            self.assertEqual(ids[f"gateway:{port}"]["outcome"], "timeout")
        self.assertIn("host:192.168.50.10:6443", ids)
        self.assertIn("public:1.1.1.1:53", ids)
        self.assertIn("public:url", ids)
        self.assertNotIn("host:127.0.0.1:18780", ids, "loopback listeners are probed through the gateway, not as their own address")
        path = next((self.dir / dogfood.CANARY_DIR_NAME).glob("canary-pre-*.json"))
        self.assertEqual(stat.S_IMODE(os.stat(path).st_mode), 0o600)
        self.assertNotIn(SYNTHETIC_ADMIN_PASSWORD, path.read_text())

        [run] = self.probe_runs()
        for flag in ("--rm", "--read-only", "--pull", "never", "--network", dogfood.NETWORK_NAME, "--cap-drop", "ALL",
                     "no-new-privileges:true", "--user", "1000:1000", "--memory", "--pids-limit", dogfood.PROBE_IMAGE):
            self.assertIn(flag, run)
        self.assertIn(f"{dogfood.COMPOSE_PROJECT_LABEL}={self.profile.project_name}", run)
        self.assertNotIn("--privileged", run)
        self.assertNotIn("host", run[run.index("--network") + 1])
        self.assertEqual({c[0] for c in self.docker.calls}, {"info", "container", "network", "image", "run"})
        self.assertTrue(all(c[1] == "inspect" for c in self.docker.calls if c[0] in ("container", "network", "image")), "only reads besides the one probe")
        self.assertEqual(self.http_calls, [f"{self.profile.console_origin}/healthz", f"{self.profile.issuer}/.well-known/openid-configuration"])

    def test_pre_redacts_a_credential_that_reaches_the_evidence(self):
        self.activate_on_disk()
        self.ruleset = full_ruleset(self.profile).replace("-P FORWARD DROP", f"-P FORWARD DROP\n-N X{SYNTHETIC_ADMIN_PASSWORD}")
        self.script_activation()
        code, _, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
        self.assertEqual(code, 0, err)
        text = next((self.dir / dogfood.CANARY_DIR_NAME).glob("canary-pre-*.json")).read_text()
        self.assertNotIn(SYNTHETIC_ADMIN_PASSWORD, text)
        self.assertIn("[redacted]", text)

    # -- pre: fail ---------------------------------------------------------

    def test_pre_fails_when_anything_that_must_be_blocked_answers(self):
        targets = dogfood.canary_pre_targets(self.profile, GATEWAY, LISTENERS)
        cases = {
            "host answered with a reset": {"gateway:22": "refused"},
            "host port connected": {"gateway:8200": "connected"},
            "specific host address connected": {"host:192.168.50.10:6443": "connected"},
            "public address connected": {"public:1.1.1.1:53": "connected"},
            "public url answered": {"public:url": "status:200"},
            "tls error means the far end was reached": {"public:url": "error:CERT_HAS_EXPIRED"},
            "positive control silent": {"control:keycloak": "timeout"},
            "positive control wrong status": {"control:console": "status:502"},
        }
        for name, overrides in cases.items():
            with self.subTest(name=name):
                self.setUp()
                self.activate_on_disk()
                self.script_activation(probe_output=outcomes_for(targets, **overrides))
                code, _, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
                self.assertEqual(code, 1)
                self.assertIn(next(iter(overrides)), err)
                self.assertEqual(self.receipts()[-1]["result"], "fail")

    def test_pre_fails_without_starting_a_probe_when_a_precondition_fails(self):
        cases = {
            "gate absent": lambda: setattr(self, "ruleset", full_ruleset(self.profile, drop_rule="INPUT")),
            "rule above the gate": lambda: setattr(self, "ruleset", full_ruleset(self.profile, above="DOCKER-USER")),
            "console loopback down": lambda: self.loopback.__setitem__(f"{self.profile.console_origin}/healthz", (502, None, {})),
            "keycloak loopback refused": lambda: self.loopback.__setitem__(
                f"{self.profile.issuer}/.well-known/openid-configuration", ConnectionRefusedError()),
        }
        for name, breaks in cases.items():
            with self.subTest(name=name):
                self.setUp()
                self.activate_on_disk()
                self.script_activation()
                breaks()
                code, _, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
                self.assertEqual(code, 1, name)
                self.assertEqual(self.probe_runs(), [], f"{name}: no probe may start")
                self.assertIn("probe not started", err)

    def test_pre_refuses_a_foreign_container_with_the_probe_name(self):
        self.activate_on_disk()
        foreign = {"Id": "7" * 64, "Name": f"/{dogfood.CANARY_PROBE_NAME}", "Config": {"Image": "busybox", "Labels": {}}}
        self.script_activation(lingering=foreign)
        code, _, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
        self.assertEqual(code, 1)
        self.assertEqual(self.probe_runs(), [])
        self.assertEqual(self.docker.calls_matching("rm"), [], "a container this tool did not start is never removed")

    def test_pre_requires_yes(self):
        code, _, err = self.run_main(["canaries", "--phase", "pre"])
        self.assertEqual(code, 2)
        self.assertEqual(self.docker.calls, [])
        self.assertEqual(self.receipts(), [])

    # -- pre: missing evidence -----------------------------------------------

    def test_pre_fails_closed_on_missing_evidence(self):
        targets = dogfood.canary_pre_targets(self.profile, GATEWAY, LISTENERS)
        partial = json.loads(outcomes_for(targets))
        del partial["results"]["public:url"]
        no_gateway = network_doc(self.profile.project_name)
        cases = {
            "iptables unreadable": dict(setup=lambda: setattr(self, "ruleset", dogfood.ProfileError("could not read the iptables ruleset: permission denied"))),
            "listeners unreadable": dict(setup=lambda: setattr(self, "listeners", dogfood.ProfileError("/proc/net/tcp is missing"))),
            "network has no gateway": dict(network=no_gateway),
            "probe image absent": dict(probe_digests=[]),
            "probe output empty": dict(probe_output=""),
            "probe output not json": dict(probe_output="Segmentation fault\n"),
            "a target without an outcome": dict(probe_output=json.dumps(partial)),
            "an unrequested target": dict(probe_output=outcomes_for(targets, **{"gateway:1": "timeout"})),
            "probe container failed": dict(probe_exit=125),
        }
        for name, case in cases.items():
            with self.subTest(name=name):
                self.setUp()
                self.activate_on_disk()
                self.script_activation(**{k: v for k, v in case.items() if k != "setup"})
                if "setup" in case:
                    case["setup"]()
                code, _, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
                self.assertEqual(code, 1, name)
                [receipt] = self.receipts()
                self.assertEqual(receipt["result"], "fail")
                self.assertTrue(receipt["findings"])

    def test_pre_without_an_activation_has_nothing_to_test(self):
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        code, _, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
        self.assertEqual(code, 1)
        self.assertIn("no real pins.lock.json", err)
        self.assertEqual(self.probe_runs(), [])

    def test_pre_refuses_the_wrong_daemon_before_anything_else(self):
        self.activate_on_disk()
        self.docker.on(lambda a: a[0] == "info", json.dumps(dict(dogfood.EXPECTED_DAEMON, ID="someone-else")))
        code, _, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
        self.assertEqual(code, 1)
        self.assertEqual([c[0] for c in self.docker.calls], ["info"])

    def test_a_stopped_recorded_container_fails(self):
        self.activate_on_disk()
        self.script_activation(state_running=False)
        code, _, err = self.run_main(["canaries", "--phase", "pre", "--yes"])
        self.assertEqual(code, 1)
        self.assertIn("not running", err)

    # -- post ----------------------------------------------------------------

    def script_post(self, labelled=None, names="", networks="bridge\nhost\nnone\n"):
        labelled = labelled or {}
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.on(lambda a: a[:2] == ["ps", "--all"] and "--filter" in a, labelled.get("containers", ""))
        self.docker.on(lambda a: a[:2] == ["network", "ls"] and "--filter" in a, labelled.get("networks", ""))
        self.docker.on(lambda a: a[:2] == ["volume", "ls"], labelled.get("volumes", ""))
        self.docker.on(lambda a: a[:2] == ["ps", "--all"], names)
        self.docker.on(lambda a: a[:2] == ["network", "ls"], networks)

    def clean_ruleset(self) -> str:
        return "-P INPUT ACCEPT\n-P FORWARD DROP\n-N DOCKER-USER\n-A DOCKER-USER -j RETURN\n"

    def test_post_passes_when_nothing_owned_remains(self):
        self.ruleset = self.clean_ruleset()
        self.script_post(names="unrelated-app\n")
        code, out, err = self.run_main(["canaries", "--phase", "post"])
        self.assertEqual(code, 0, err)
        [receipt] = self.receipts()
        self.assertEqual(receipt["result"], "pass")
        self.assertEqual(receipt["evidence"]["iptables_S"], self.ruleset)
        flat = " ".join(" ".join(c) for c in self.docker.calls)
        for forbidden in ("run", "rm", "create", "prune"):
            self.assertNotIn(f" {forbidden} ", f" {flat} ", "post is read-only")

    def test_post_fails_on_any_remaining_owned_resource(self):
        cases = {
            "labelled container": dict(post=dict(labelled={"containers": "a" * 64})),
            "labelled network": dict(post=dict(labelled={"networks": FAKE_NETWORK_ID})),
            "labelled volume": dict(post=dict(labelled={"volumes": "fabric-dogfood-data"})),
            "lingering probe by name": dict(post=dict(names=f"{dogfood.CANARY_PROBE_NAME}\n")),
            "container by name": dict(post=dict(names="fabric-dogfood-keycloak\n")),
            "network by name": dict(post=dict(networks=f"bridge\n{dogfood.NETWORK_NAME}\n")),
            "gate rules left": dict(ruleset=True),
            "bridge interface left": dict(bridge=True),
            "local state left": dict(out=True),
        }
        for name, case in cases.items():
            with self.subTest(name=name):
                self.setUp()
                self.ruleset = full_ruleset(self.profile) if case.get("ruleset") else self.clean_ruleset()
                self.bridge_present = bool(case.get("bridge"))
                if case.get("out"):
                    (self.dir / ".out").mkdir()
                self.script_post(**case.get("post", {}))
                code, _, err = self.run_main(["canaries", "--phase", "post"])
                self.assertEqual(code, 1, name)
                self.assertEqual(self.receipts()[-1]["result"], "fail")

    def test_post_fails_closed_when_iptables_cannot_be_read(self):
        self.ruleset = dogfood.ProfileError("could not read the iptables ruleset: permission denied")
        self.script_post()
        code, _, err = self.run_main(["canaries", "--phase", "post"])
        self.assertEqual(code, 1)
        self.assertIn("permission denied", err)


class CanaryPureFunctions(unittest.TestCase):
    def setUp(self):
        self.profile = dogfood.parse_profile(dict(GOOD_PROFILE))

    def test_proc_net_tcp_listeners_are_parsed_in_host_byte_order(self):
        v4 = (
            "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n"
            "   0: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 1\n"
            "   1: 0100007F:4964 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 2\n"
            "   2: 0A32A8C0:1923 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 3\n"
            "   3: 0A32A8C0:E1F2 0132A8C0:0016 01 00000000:00000000 00:00000000 00000000     0        0 4\n"
        )
        self.assertEqual(dogfood.parse_proc_net_tcp(v4, ipv6=False), [("0.0.0.0", 22), ("127.0.0.1", 18788), ("192.168.50.10", 6435)])
        v6 = (
            "  sl  local_address                         remote_address                        st\n"
            "   0: 00000000000000000000000000000000:01BB 00000000000000000000000000000000:0000 0A\n"
            "   1: 00000000000000000000000001000000:0050 00000000000000000000000000000000:0000 0A\n"
        )
        self.assertEqual(dogfood.parse_proc_net_tcp(v6, ipv6=True), [("::", 443), ("::1", 80)])
        with self.assertRaises(dogfood.ProfileError):
            dogfood.parse_proc_net_tcp("header\n 0: ZZZZ:0016 0:0 0A\n", ipv6=False)

    def test_targets_cover_protected_observed_and_profile_ports_on_the_gateway(self):
        targets = dogfood.canary_pre_targets(self.profile, GATEWAY, [("0.0.0.0", 5000), ("10.213.7.1", 53)])
        ids = [t.id for t in targets]
        self.assertEqual(ids[:2], ["control:console", "control:keycloak"])
        for port in (*self.profile.protected_host_ports, 5000, 53, self.profile.console_port, self.profile.oidc_port):
            self.assertIn(f"gateway:{port}", ids)
        self.assertNotIn("host:10.213.7.1:53", ids, "addresses inside the sandbox subnet are the gateway itself")
        self.assertTrue(all(t.expect == "blocked" for t in targets[2:]))
        with self.assertRaises(dogfood.ProfileError):
            dogfood.canary_pre_targets(self.profile, GATEWAY, [("0.0.0.0", p) for p in range(1024, 1024 + dogfood.CANARY_MAX_TARGETS)])

    def test_the_gateway_must_be_reported_inside_the_profile_subnet(self):
        doc = network_doc(self.profile.project_name)
        self.assertIsNone(dogfood.network_gateway(doc, self.profile))
        doc["IPAM"]["Config"][0]["Gateway"] = "172.17.0.1"
        self.assertIsNone(dogfood.network_gateway(doc, self.profile))
        doc["IPAM"]["Config"][0]["Gateway"] = GATEWAY
        self.assertEqual(dogfood.network_gateway(doc, self.profile), GATEWAY)

    def test_gate_absence_is_judged_on_the_bridge_and_subnet_tokens(self):
        self.assertTrue(dogfood.evaluate_gate_absent(full_ruleset(self.profile), self.profile))
        self.assertEqual(dogfood.evaluate_gate_absent("-A FORWARD -i fabric-dogfood01 -j DROP\n-A INPUT -s 10.213.7.0/25 -j DROP\n", self.profile), [])

    def test_protected_host_ports_are_validated(self):
        for bad in ([], [0], [22, 22], "22", [True], [70000], list(range(1, dogfood.MAX_PROTECTED_HOST_PORTS + 2))):
            with self.subTest(bad=bad):
                with self.assertRaises(dogfood.ProfileError) as refused:
                    dogfood.parse_profile(dict(GOOD_PROFILE, protected_host_ports=bad))
                self.assertIn("protected_host_ports", str(refused.exception))

    def test_probe_image_is_the_pinned_console_builder_base(self):
        dockerfile = (dogfood.REPO_ROOT / "apps/control-plane-ui/Dockerfile").read_text(encoding="utf-8")
        self.assertIn(f"FROM {dogfood.PROBE_IMAGE} AS builder", dockerfile.splitlines())


if __name__ == "__main__":
    unittest.main()
