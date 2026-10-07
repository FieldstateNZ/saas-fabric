"""Synthetic tests for examples/disposable-dogfood/dogfood.py.

Standard library only. Every test drives the real generator, validator and
command flow over rendered artefacts, tampered copies of them, or scripted
fakes of the Docker / iptables / terminal seams -- never a string search over
the tool's own source and never a real daemon. Image IDs, container IDs and
credentials used here are SYNTHETIC and labelled as such.

Nothing here proves runtime behaviour: not that containers start, not that
the firewall gate isolates anything. It proves the tool refuses when the
evidence it demands is missing or wrong.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import contextlib
import copy
import http.server
import io
import json
import os
import socket
import stat
import subprocess
import sys
import tempfile
import termios
import threading
import time
import unittest
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO_ROOT / "examples" / "disposable-dogfood"))

import dogfood  # noqa: E402

# The pinned public digest (recorded in profile.toml). Nothing in these tests contacts it.
PINNED_KEYCLOAK = "quay.io/keycloak/keycloak@sha256:9d1f1b2b7261ff53c66cb1092dfcdc34a5fb77e81f9e6a6e75b8b6a795de8067"

GOOD_PROFILE = {
    "project_name": "fabric-dogfood-disposable",
    "bind_address": "127.0.0.1",
    "console_port": 18780,
    "oidc_port": 18781,
    "realm": "fabric-dogfood",
    "subnet": "10.213.7.0/24",
    "keycloak_image": PINNED_KEYCLOAK,
    "source_commit": dogfood.PINNED_COMMIT,
    "protected_host_ports": [22, 443, 8200],
}

# SYNTHETIC "real" lock: well-formed, non-synthetic-looking IDs that no daemon has ever produced.
FAKE_IDS = {"cp": "sha256:" + "a1" * 32, "console": "sha256:" + "b2" * 32, "keycloak": "sha256:" + "c3" * 32}
FAKE_CONTAINER_IDS = {"cp": "1" * 64, "console": "2" * 64, "keycloak": "3" * 64}
FAKE_NETWORK_ID = "4" * 64
FOREIGN_CONTAINER_ID = "9" * 64
GOOD_INFO = json.dumps(dict(dogfood.EXPECTED_DAEMON, OSType="linux"))
SYNTHETIC_OPERATOR_PASSWORD = "synthetic-operator-password-not-real"


def profile_toml(values: dict) -> str:
    return "".join(f"{key} = {json.dumps(value)}\n" for key, value in values.items())


def real_lock_json() -> str:
    return json.dumps(
        {
            "source_commit": dogfood.PINNED_COMMIT,
            "cp_image_id": FAKE_IDS["cp"],
            "console_image_id": FAKE_IDS["console"],
            "keycloak_base_image": PINNED_KEYCLOAK,
            "keycloak_image_id": FAKE_IDS["keycloak"],
            "daemon": dict(dogfood.EXPECTED_DAEMON),
            "synthetic": False,
        }
    )


def iptables_listing(profile: dogfood.Profile, drop_rule: str | None = None, reorder: str | None = None, above: str | None = None) -> dict[str, str]:
    """`iptables -S` text for both chains: the gate first, other rules below, ctstate in the other order."""
    listings = {}
    for chain, rules in dogfood.expected_iptables_rules(profile).items():
        rules = [r.replace("RELATED,ESTABLISHED", "ESTABLISHED,RELATED") for r in rules]
        if drop_rule and chain == drop_rule:
            rules = [r for r in rules if "ESTABLISHED" not in r]
        if reorder and chain == reorder:
            rules = rules[1:] + rules[:1]
        head = f"-P {chain} ACCEPT" if chain == "INPUT" else f"-N {chain}"
        trailing = "-A INPUT -i lo -j ACCEPT" if chain == "INPUT" else f"-A {chain} -j RETURN"
        lines = [head]
        if above and chain == above:
            lines.append(trailing)
        lines += [f"-A {chain} {r}" for r in rules]
        lines.append(trailing)
        listings[chain] = "\n".join(lines) + "\n"
    return listings


def container_doc(service: str, project: str, image: str | None = None, name: str | None = None, cid: str | None = None, ports: dict | None = None) -> dict:
    return {
        "Id": cid or FAKE_CONTAINER_IDS[service],
        "Name": "/" + (name or dogfood.CONTAINER_NAMES[service]),
        "Image": image or FAKE_IDS[service],
        "Config": {"Labels": {dogfood.COMPOSE_PROJECT_LABEL: project}},
        "NetworkSettings": {"Ports": ports or {}},
    }


def network_doc(project: str, containers: dict | None = None, subnet: str = "10.213.7.0/24") -> dict:
    return {
        "Id": FAKE_NETWORK_ID,
        "Name": dogfood.NETWORK_NAME,
        "Internal": True,
        "EnableIPv6": False,
        "Options": {"com.docker.network.bridge.name": dogfood.BRIDGE_NAME},
        "IPAM": {"Config": [{"Subnet": subnet}]},
        "Labels": {dogfood.COMPOSE_PROJECT_LABEL: project},
        "Containers": containers or {},
    }


class FakeDocker:
    """Scripted stand-in for dogfood.run_docker. Unscripted calls fail the test."""

    def __init__(self):
        self.calls: list[list[str]] = []
        self.handlers: list[tuple] = []

    def on(self, matcher, response):
        self.handlers.append((matcher, response))
        return self

    def __call__(self, args, capture=False):
        self.calls.append(list(args))
        for matcher, response in self.handlers:
            if matcher(args):
                if isinstance(response, BaseException):
                    raise response
                return response(args) if callable(response) else response
        raise AssertionError(f"unscripted docker call: {args}")

    def inspect_responses(self, documents: dict[str, dict | None]):
        """`<kind> inspect --format {{json .}} <ref>` answers from `documents`; a None raises like a missing ref."""

        def respond(args):
            ref = args[-1]
            document = documents.get(ref)
            if document is None:
                raise subprocess.CalledProcessError(1, "docker")
            return json.dumps(document)

        return self.on(lambda a: len(a) >= 2 and a[1] == "inspect", respond)

    def calls_matching(self, *prefix):
        return [c for c in self.calls if c[: len(prefix)] == list(prefix)]


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class ProfileInputs(unittest.TestCase):
    def test_good_profile_loads_and_derives_aligned_urls(self):
        profile = dogfood.parse_profile(dict(GOOD_PROFILE))
        self.assertEqual(profile.issuer, "http://127.0.0.1:18781/realms/fabric-dogfood")
        self.assertEqual(profile.redirect_uri, "http://127.0.0.1:18780/")
        self.assertEqual(profile.reachable_at, "http://keycloak:8080/realms/fabric-dogfood")

    def test_shipped_profile_pins_keycloak_by_the_recorded_digest(self):
        profile = dogfood.load_profile(dogfood.HERE / "profile.toml")
        self.assertEqual(profile.keycloak_image, PINNED_KEYCLOAK)

    def test_hostile_inputs_are_refused(self):
        cases = {
            "bind_address": "0.0.0.0",
            "realm": "master",
            "console_port": 80,
            "oidc_port": 18780,
            "subnet": "8.8.8.0/24",
            "keycloak_image": "quay.io/keycloak/keycloak:26.7.2",
            "source_commit": "0" * 40,
            "project_name": "../etc",
        }
        for key, bad in cases.items():
            with self.subTest(key=key):
                with self.assertRaises(dogfood.ProfileError) as refused:
                    dogfood.parse_profile(dict(GOOD_PROFILE, **{key: bad}))
                self.assertIn(key, str(refused.exception))

    def test_foreign_registry_digest_and_unknown_keys_are_refused(self):
        with self.assertRaises(dogfood.ProfileError):
            dogfood.parse_profile(dict(GOOD_PROFILE, keycloak_image="evil.example/keycloak@sha256:" + "ab" * 32))
        with self.assertRaises(dogfood.ProfileError) as refused:
            dogfood.parse_profile(dict(GOOD_PROFILE, extra_mount="/var/run/docker.sock"))
        self.assertIn("unknown keys", str(refused.exception))


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class RenderedArtefacts(unittest.TestCase):
    def setUp(self):
        self.profile = dogfood.parse_profile(dict(GOOD_PROFILE))
        self.lock = dogfood.synthetic_lock(self.profile)
        self.rendered = dogfood.render(self.profile, self.lock)

    def test_rendered_profile_validates_cleanly(self):
        self.assertEqual(dogfood.validate_rendered(self.rendered, self.profile, self.lock), [])

    def test_endpoint_derivation_matches_the_adapter_rule(self):
        endpoints = dogfood.derive_endpoints(self.profile.issuer, self.profile.reachable_at)
        self.assertEqual(endpoints["authorization"], "http://127.0.0.1:18781/realms/fabric-dogfood/protocol/openid-connect/auth")
        self.assertEqual(endpoints["token"], "http://keycloak:8080/realms/fabric-dogfood/protocol/openid-connect/token")
        self.assertEqual(endpoints["jwks"], "http://keycloak:8080/realms/fabric-dogfood/protocol/openid-connect/certs")
        self.assertEqual(dogfood.derive_endpoints("http://x/realms/r/", "")["token"], "http://x/realms/r/protocol/openid-connect/token")

    def test_tampered_control_plane_toml_is_refused(self):
        base = self.rendered.control_plane_toml
        tampers = {
            "shared keycloak reconciliation": base.replace('[identity_provider]\nmode = "in_memory"', '[identity_provider]\nmode = "keycloak"\nbase_url = "http://keycloak.shared:8080"\nadmin_realm = "master"\nclient_id = "x"\naudience = "y"'),
            "openbao": base.replace('[secret_store]\nmode = "in_memory"', '[secret_store]\nmode = "open_bao"\naddress = "http://openbao:8200"\nrole = "r"'),
            "backend to a shared host": base.replace(self.profile.reachable_at, "http://10.0.0.5:8080/realms/fabric-dogfood"),
            "issuer elsewhere": base.replace(self.profile.issuer, "https://sso.example.com/realms/fabric-dogfood"),
            "platform management": base + '\n[platform_management]\nenvironment = "lucentroot"\n',
            "git repository": base.replace('mode = "local_directory"', 'mode = "git"').replace(f'path = "{dogfood.CP_STATE_PATH}"', 'owner = "o"'),
            "unknown key": base + '\nlisten_backdoor = "0.0.0.0:9"\n',
            "redirect without slash": base.replace(self.profile.redirect_uri, self.profile.redirect_uri.rstrip("/")),
            "state outside tmpfs": base.replace(dogfood.CP_STATE_PATH, "/host/state"),
            "interpolation": base.replace('request_timeout_seconds = 30', 'request_timeout_seconds = 30\npublic_base_url = "${PUBLIC}"'),
            "jwks refresh back to the production default": base.replace(f"jwks_refresh_seconds = {dogfood.JWKS_REFRESH_SECONDS}", "jwks_refresh_seconds = 300"),
            "jwks refresh line removed": base.replace(f"jwks_refresh_seconds = {dogfood.JWKS_REFRESH_SECONDS}\n", ""),
            "leeway widened": base.replace(f"jwks_refresh_seconds = {dogfood.JWKS_REFRESH_SECONDS}", f"jwks_refresh_seconds = {dogfood.JWKS_REFRESH_SECONDS}\nleeway_seconds = 3600"),
        }
        self.assertIn(f"\njwks_refresh_seconds = {dogfood.JWKS_REFRESH_SECONDS}\n", base)
        self.assertEqual(dogfood.JWKS_REFRESH_SECONDS, 5)
        for name, text in tampers.items():
            with self.subTest(name=name):
                self.assertNotEqual(text, base, "tamper did not apply")
                self.assertTrue(dogfood.validate_control_plane_toml(text, self.profile), f"{name} passed validation")

    def test_tampered_console_conf_is_refused(self):
        base = self.rendered.console_conf
        tampers = {
            "proxy to another host": base.replace(dogfood.CP_UPSTREAM, "http://openbao.shared:8200"),
            "dropped csp": "\n".join(line for line in base.splitlines() if "Content-Security-Policy" not in line),
            "second api location": base + "\nlocation /api/x { proxy_pass http://cp:8081; }\n",
            "history fallback": base.replace("try_files $uri $uri/ =404;", "try_files $uri /index.html;"),
        }
        for name, text in tampers.items():
            with self.subTest(name=name):
                self.assertTrue(dogfood.validate_console_conf(text, self.profile), f"{name} passed validation")

    def test_tampered_compose_is_refused(self):
        base = json.loads(self.rendered.compose_json)

        def tamper(mutate):
            doc = copy.deepcopy(base)
            mutate(doc)
            return doc

        cp, console, kc = dogfood.SERVICE_CP, dogfood.SERVICE_CONSOLE, dogfood.SERVICE_KEYCLOAK
        cases = {
            "docker socket": tamper(lambda d: d["services"][cp]["volumes"].append({"type": "bind", "source": "/var/run/docker.sock", "target": "/var/run/docker.sock", "read_only": True})),
            "host path bind": tamper(lambda d: d["services"][cp]["volumes"].append({"type": "bind", "source": "/etc/fabric-shared", "target": "/etc/fabric", "read_only": True})),
            "named volume": tamper(lambda d: d["services"][kc]["volumes"].append({"type": "volume", "source": "kcdata", "target": "/opt/keycloak/data"})),
            "tmpfs smuggled under volumes": tamper(lambda d: d["services"][kc]["volumes"].append({"type": "tmpfs", "target": "/opt/keycloak/providers", "tmpfs": {"size": 1024}})),
            "network not internal": tamper(lambda d: d["networks"][dogfood.NETWORK_NAME].__setitem__("internal", False)),
            "published on all interfaces": tamper(lambda d: d["services"][console]["ports"][0].__setitem__("host_ip", "0.0.0.0")),
            "cp published": tamper(lambda d: d["services"][cp].__setitem__("ports", [{"target": 8081, "published": "8081", "host_ip": "127.0.0.1", "protocol": "tcp"}])),
            "fabric env override": tamper(lambda d: d["services"][cp]["environment"].__setitem__("FABRIC_CP_SETTING_SECRET_STORE__MODE", "open_bao")),
            "env list inherits host": tamper(lambda d: d["services"][cp].__setitem__("environment", ["FABRIC_CP_SETTING_LISTEN"])),
            "privileged": tamper(lambda d: d["services"][kc].__setitem__("privileged", True)),
            "cap add": tamper(lambda d: d["services"][kc].__setitem__("cap_add", ["NET_ADMIN"])),
            "host networking": tamper(lambda d: d["services"][cp].__setitem__("network_mode", "host")),
            "entrypoint override": tamper(lambda d: d["services"][kc].__setitem__("entrypoint", ["/bin/bash"])),
            "writable root": tamper(lambda d: d["services"][console].__setitem__("read_only", False)),
            "root user": tamper(lambda d: d["services"][cp].__setitem__("user", "0:0")),
            "keycloak by base digest instead of derived id": tamper(lambda d: d["services"][kc].__setitem__("image", PINNED_KEYCLOAK)),
            "image by tag": tamper(lambda d: d["services"][kc].__setitem__("image", "quay.io/keycloak/keycloak:26.7.2")),
            "image swapped": tamper(lambda d: d["services"][cp].__setitem__("image", "sha256:" + "f" * 64)),
            "pull always": tamper(lambda d: d["services"][cp].__setitem__("pull_policy", "always")),
            "no pids limit": tamper(lambda d: d["services"][cp].pop("pids_limit")),
            "memory above the ceiling": tamper(lambda d: d["services"][kc].__setitem__("mem_limit", "4g")),
            "cpus above the ceiling": tamper(lambda d: d["services"][console].__setitem__("cpus", 4.0)),
            "pids above the ceiling": tamper(lambda d: d["services"][cp].__setitem__("pids_limit", 100000)),
            "extra network": tamper(lambda d: d["services"][cp].__setitem__("networks", [dogfood.NETWORK_NAME, "bridge"])),
            "ipv6": tamper(lambda d: d["networks"][dogfood.NETWORK_NAME].__setitem__("enable_ipv6", True)),
            "other subnet": tamper(lambda d: d["networks"][dogfood.NETWORK_NAME]["ipam"]["config"].__setitem__(0, {"subnet": "172.17.0.0/16"})),
            "kc hostname drift": tamper(lambda d: d["services"][kc]["environment"].__setitem__("KC_HOSTNAME", "http://sso.shared.example")),
            "env_file on cp": tamper(lambda d: d["services"][cp].__setitem__("env_file", [{"path": "../../.env", "required": True}])),
            "named volumes section": tamper(lambda d: d.__setitem__("volumes", {"shared": {}})),
            "extra service": tamper(lambda d: d["services"].__setitem__("openbao", dict(d["services"][cp]))),
        }
        for name, doc in cases.items():
            with self.subTest(name=name):
                findings = dogfood.validate_compose(doc, self.profile, self.lock, json.dumps(doc))
                self.assertTrue(findings, f"{name} passed validation")
        with self.subTest(name="interpolation in text"):
            self.assertTrue(dogfood.validate_compose(base, self.profile, self.lock, self.rendered.compose_json.replace("start-dev", "${CMD}")))

    def test_tmpfs_mounts_declare_size_mode_and_owner_per_service(self):
        base = json.loads(self.rendered.compose_json)
        kc = base["services"][dogfood.SERVICE_KEYCLOAK]["tmpfs"]
        cp = base["services"][dogfood.SERVICE_CP]["tmpfs"]
        self.assertIn(f"{dogfood.KC_QUARKUS_PATH}:size={256 * dogfood.MIB},mode=0700,uid=1000,gid=1000,nosuid,nodev", kc)
        self.assertIn(f"{dogfood.CP_STATE_PATH}:size={16 * dogfood.MIB},mode=0700,uid=65532,gid=65532,nosuid,nodev,noexec", cp)
        for name in base["services"]:
            self.assertEqual(dogfood.validate_tmpfs(name, base["services"][name]["tmpfs"]), [])

        def mutate(service, replace_from, replace_to):
            entries = [e.replace(replace_from, replace_to) for e in base["services"][service]["tmpfs"]]
            self.assertNotEqual(entries, base["services"][service]["tmpfs"], "tamper did not apply")
            return dogfood.validate_tmpfs(service, entries)

        self.assertTrue(mutate(dogfood.SERVICE_KEYCLOAK, "mode=0700", "mode=1777"), "world-writable tmpfs passed")
        self.assertTrue(mutate(dogfood.SERVICE_CP, "uid=65532", "uid=0"), "root-owned tmpfs passed")
        self.assertTrue(mutate(dogfood.SERVICE_CP, ",noexec", ""), "exec tmpfs on the control plane passed")
        self.assertTrue(mutate(dogfood.SERVICE_KEYCLOAK, f"size={256 * dogfood.MIB}", f"size={10 * 1024 ** 3}"), "uncapped tmpfs passed")
        self.assertTrue(dogfood.validate_tmpfs(dogfood.SERVICE_KEYCLOAK, kc[1:]), "missing writable path passed")
        self.assertTrue(dogfood.validate_tmpfs(dogfood.SERVICE_KEYCLOAK, kc + ["/opt/keycloak/providers:size=1,mode=0700,uid=1000,gid=1000,nosuid,nodev"]), "extra writable path passed")

    def test_keycloak_derived_context_is_allowlisted_pinned_and_secret_free(self):
        context = dogfood.keycloak_derived_context(self.profile)
        self.assertEqual(set(context), set(dogfood.KC_CONTEXT_FILES))
        dockerfile = context["Dockerfile"]
        froms = [line for line in dockerfile.splitlines() if line.startswith("FROM ")]
        self.assertEqual(froms[0], f"FROM {PINNED_KEYCLOAK} AS base")
        self.assertTrue(all(PINNED_KEYCLOAK in f or f == "FROM base" for f in froms))
        self.assertFalse(any(line.startswith("RUN ") for line in dockerfile.splitlines()), "no RUN step: no package install, no shell at build")
        self.assertIn(f"COPY --from=base --chown=1000:0 {dogfood.KC_QUARKUS_PATH} {dogfood.KC_QUARKUS_SEED_PATH}", dockerfile)
        self.assertIn("USER 1000", dockerfile)
        entrypoint = context["kc-entrypoint.sh"]
        self.assertIn('exec /opt/keycloak/bin/kc.sh "$@"', entrypoint)
        self.assertIn("is not empty", entrypoint)
        # seed is 1000:0, the tmpfs and the process are 1000:1000: preserving the foreign group would make cp fail
        self.assertIn('cp -a --no-preserve=ownership "$seed/." "$live/"', entrypoint)
        for secret_marker in ("KC_BOOTSTRAP", "PASSWORD", "curl", "wget", "${"):
            self.assertNotIn(secret_marker, dockerfile + entrypoint)

    def test_lock_records_the_derived_keycloak_separately_from_the_base_digest(self):
        good = json.loads(dogfood.lock_to_json(self.lock))
        self.assertTrue(dogfood.parse_lock(dict(good), self.profile).synthetic)
        real = json.loads(real_lock_json())
        lock = dogfood.parse_lock(dict(real), self.profile)
        self.assertFalse(lock.synthetic)
        self.assertEqual(lock.keycloak_base_image, PINNED_KEYCLOAK)
        self.assertEqual(lock.image_for(dogfood.SERVICE_KEYCLOAK), FAKE_IDS["keycloak"])
        compose = dogfood.compose_document(self.profile, lock)
        self.assertEqual(compose["services"][dogfood.SERVICE_KEYCLOAK]["image"], FAKE_IDS["keycloak"])
        refusals = {
            "claims real ids it does not have": dict(good, synthetic=False),
            "base digest drift": dict(real, keycloak_base_image="quay.io/keycloak/keycloak@sha256:" + "cd" * 32),
            "tag as image id": dict(real, cp_image_id="saas-fabric-control-plane:latest"),
            "derived keycloak by digest": dict(real, keycloak_image_id=PINNED_KEYCLOAK),
            "other daemon": dict(real, daemon=dict(dogfood.EXPECTED_DAEMON, ID="00000000-0000-0000-0000-000000000000")),
            "missing daemon": {k: v for k, v in real.items() if k != "daemon"},
        }
        for name, data in refusals.items():
            with self.subTest(name=name):
                with self.assertRaises(dogfood.ProfileError):
                    dogfood.parse_lock(data, self.profile)


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class EnforcementGate(unittest.TestCase):
    def setUp(self):
        self.profile = dogfood.parse_profile(dict(GOOD_PROFILE))

    def test_rules_accept_established_before_dropping_and_are_scoped_to_this_sandbox(self):
        for chain, rules in dogfood.expected_iptables_rules(self.profile).items():
            self.assertIn("ESTABLISHED", rules[0], f"{chain}: first rule must accept return traffic")
            self.assertTrue(all("DROP" in r for r in rules[1:]), chain)
            for rule in rules:
                self.assertTrue(dogfood.BRIDGE_NAME in rule or "10.213.7.0/24" in rule, rule)

    def test_printed_apply_and_remove_are_exact_inverses_with_no_flush(self):
        text = dogfood.enforcement_text(self.profile)
        apply = [l for l in text.splitlines() if l.startswith("iptables -I ")]
        remove = [l for l in text.splitlines() if l.startswith("iptables -D ")]
        self.assertEqual(len(apply), 5)
        self.assertEqual(len(remove), len(apply))
        applied = {(l.split()[2], tuple(l.split()[4:])) for l in apply}  # (CHAIN, RULE) — drop "iptables -I CHAIN N"
        removed = {(l.split()[2], tuple(l.split()[3:])) for l in remove}  # (CHAIN, RULE) — drop "iptables -D CHAIN"
        self.assertEqual(applied, removed)
        for forbidden in (" -F", " -X", " -P ", "sysctl", "nft "):
            self.assertNotIn(forbidden, text)
        self.assertIn("None has been run", text)

    def test_preflight_evaluation_demands_every_rule_in_order(self):
        good = iptables_listing(self.profile)
        self.assertEqual(dogfood.evaluate_iptables(good, self.profile), [])
        self.assertTrue(dogfood.evaluate_iptables(iptables_listing(self.profile, drop_rule="DOCKER-USER"), self.profile))
        self.assertTrue(dogfood.evaluate_iptables(iptables_listing(self.profile, reorder="INPUT"), self.profile))
        self.assertTrue(dogfood.evaluate_iptables(iptables_listing(self.profile, above="DOCKER-USER"), self.profile), "a RETURN above the gate passed")
        self.assertTrue(dogfood.evaluate_iptables({"INPUT": good["INPUT"]}, self.profile), "missing chain listing passed")
        self.assertTrue(dogfood.evaluate_iptables(dict(good, **{"DOCKER-USER": "-A FORWARD -j ACCEPT\n"}), self.profile), "absent chain passed")
        other = dogfood.parse_profile(dict(GOOD_PROFILE, subnet="10.213.8.0/24"))
        self.assertTrue(dogfood.evaluate_iptables(good, other), "rules for another subnet passed")

    def test_unreadable_iptables_fails_closed(self):
        with mock.patch.object(dogfood, "read_iptables", side_effect=dogfood.ProfileError("could not read iptables chain INPUT")):
            with self.assertRaises(dogfood.ProfileError):
                dogfood.preflight_enforcement(self.profile)


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class DockerSeam(unittest.TestCase):
    def test_docker_environment_inherits_nothing_and_binds_the_socket(self):
        hostile = {
            "PATH": "/attacker/bin:/usr/bin", "HOME": "/home/operator", "DOCKER_HOST": "tcp://10.0.0.9:2375",
            "DOCKER_CONTEXT": "remote", "DOCKER_CONFIG": "/home/operator/.docker", "DOCKER_TLS_VERIFY": "1",
            "DOCKER_CERT_PATH": "/home/operator/.docker", "http_proxy": "http://127.0.0.1:1", "COMPOSE_FILE": "/elsewhere.yaml",
            "FABRIC_CP_SETTING_LISTEN": "0.0.0.0:1", "FABRIC_SECRET_X": "s",
        }
        with mock.patch.dict(os.environ, hostile, clear=True):
            env = dogfood.docker_environment(hostile)
        self.assertEqual(env["DOCKER_HOST"], "unix:///var/run/docker.sock")
        self.assertEqual(env["PATH"], dogfood.SAFE_PATH)
        for key in ("DOCKER_CONTEXT", "DOCKER_TLS_VERIFY", "DOCKER_CERT_PATH", "http_proxy", "COMPOSE_FILE"):
            self.assertNotIn(key, env)
        self.assertFalse(any(k.startswith("FABRIC_") for k in env))
        private = Path(env["DOCKER_CONFIG"])
        self.assertEqual(env["HOME"], str(private))
        self.assertNotEqual(str(private), "/home/operator")
        self.assertTrue(private.is_dir() and not private.is_symlink())
        self.assertEqual(list(private.iterdir()), [], "the isolated Docker config directory must be empty: no credentials, no contexts")
        self.assertEqual(stat.S_IMODE(private.stat().st_mode), 0o700)

    def test_daemon_identity_is_verified_before_anything_else(self):
        self.assertEqual(dogfood.evaluate_daemon(json.loads(GOOD_INFO)), [])
        self.assertTrue(dogfood.evaluate_daemon(dict(dogfood.EXPECTED_DAEMON, Name="riley")))
        self.assertTrue(dogfood.evaluate_daemon(dict(dogfood.EXPECTED_DAEMON, ServerVersion="28.0.0")))
        other = FakeDocker().on(lambda a: a[0] == "info", json.dumps(dict(dogfood.EXPECTED_DAEMON, ID="other")))
        with mock.patch.object(dogfood, "run_docker", other):
            with self.assertRaises(dogfood.ProfileError) as refused:
                dogfood.verify_daemon()
        self.assertIn("wrong Docker daemon", str(refused.exception))

    def test_bound_binaries_and_no_shell(self):
        with mock.patch.object(dogfood, "locate", return_value="/usr/bin/docker"):
            with mock.patch.object(dogfood.subprocess, "run") as run:
                run.return_value = mock.Mock(stdout=b"ok\n")
                dogfood.run_docker(["version"], capture=True)
        argv = run.call_args.args[0]
        self.assertEqual(argv, ["/usr/bin/docker", "version"])
        self.assertIs(run.call_args.kwargs["shell"], False)
        self.assertEqual(run.call_args.kwargs["env"]["DOCKER_HOST"], dogfood.DOCKER_HOST)
        with self.assertRaises(dogfood.ProfileError):
            dogfood.locate(("/nonexistent/docker",), "docker")


class LoopbackHttp(unittest.TestCase):
    """Two real loopback servers: an allowlisted one that redirects, and a disallowed target that must never be hit."""

    class Counting(http.server.BaseHTTPRequestHandler):
        hits: list[tuple[str, str | None]] = []
        redirect_to: str | None = None

        def log_message(self, *_):  # silence
            pass

        def do_GET(self):  # noqa: N802
            type(self).hits.append((self.path, self.headers.get("Authorization")))
            if type(self).redirect_to:
                self.send_response(302)
                self.send_header("Location", type(self).redirect_to)
                self.end_headers()
                return
            body = b'{"ok": true}'
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    @staticmethod
    def free_port() -> int:
        with socket.socket() as s:
            s.bind(("127.0.0.1", 0))
            return s.getsockname()[1]

    def serve(self, name: str):
        handler = type(name, (self.Counting,), {"hits": [], "redirect_to": None})
        server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(server.server_close)  # cleanups run last-in first-out: shutdown, then close
        self.addCleanup(server.shutdown)
        return server, handler

    def test_redirects_proxies_and_foreign_ports_are_refused(self):
        allowed, allowed_handler = self.serve("Allowed")
        disallowed, disallowed_handler = self.serve("Disallowed")
        proxy_trap, proxy_handler = self.serve("ProxyTrap")
        allowed_handler.redirect_to = f"http://127.0.0.1:{disallowed.server_port}/stolen"
        profile = dogfood.parse_profile(dict(GOOD_PROFILE, console_port=allowed.server_port, oidc_port=self.free_port()))
        synthetic_bearer = "Bearer synthetic-not-a-credential"
        trap = {"http_proxy": f"http://127.0.0.1:{proxy_trap.server_port}", "HTTP_PROXY": f"http://127.0.0.1:{proxy_trap.server_port}", "no_proxy": ""}
        with mock.patch.dict(os.environ, trap):
            with self.assertRaises(dogfood.ProfileError) as refused:
                dogfood.http_json(f"{profile.console_origin}/api/session", profile, headers={"Authorization": synthetic_bearer})
            self.assertIn("redirect", str(refused.exception))
            with self.assertRaises(dogfood.ProfileError):
                dogfood.http_json(f"http://127.0.0.1:{disallowed.server_port}/direct", profile)
        self.assertEqual(len(allowed_handler.hits), 1)
        self.assertEqual(disallowed_handler.hits, [], "the disallowed target received a request (redirect followed or allowlist bypassed)")
        self.assertEqual(proxy_handler.hits, [], "an inherited proxy was honoured")
        self.assertNotIn(synthetic_bearer, str(refused.exception))

    def test_allowlisted_request_without_redirect_succeeds(self):
        allowed, handler = self.serve("Plain")
        profile = dogfood.parse_profile(dict(GOOD_PROFILE, console_port=allowed.server_port, oidc_port=self.free_port()))
        status, body, _ = dogfood.http_json(f"{profile.console_origin}/healthz", profile)
        self.assertEqual((status, body), (200, {"ok": True}))
        self.assertEqual(len(handler.hits), 1)

    def test_loopback_url_refuses_userinfo_other_hosts_schemes_and_fragments(self):
        profile = dogfood.parse_profile(dict(GOOD_PROFILE))
        for url in (
            "http://user:pw@127.0.0.1:18780/",
            "http://localhost:18780/",
            "http://127.0.0.2:18780/",
            "https://127.0.0.1:18780/",
            "http://127.0.0.1:18782/",
            "http://127.0.0.1:18780/#x",
            "http://127.0.0.1@evil.example:18780/",
        ):
            with self.subTest(url=url):
                with self.assertRaises(dogfood.ProfileError):
                    dogfood.loopback_url(url, profile)
        self.assertEqual(dogfood.loopback_url("http://127.0.0.1:18781/realms/master", profile), "http://127.0.0.1:18781/realms/master")


class FileSafety(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.addCleanup(self.tmp.cleanup)

    def test_credential_file_is_exclusive_0600_and_never_written_through_a_symlink(self):
        target = self.dir / "keycloak.env"
        dogfood.write_regular_file(target, "KC_BOOTSTRAP_ADMIN_USERNAME=synthetic\n", 0o600, exclusive=True)
        self.assertEqual(stat.S_IMODE(target.stat().st_mode), 0o600)
        with self.assertRaises(dogfood.ProfileError):
            dogfood.write_regular_file(target, "overwrite\n", 0o600, exclusive=True)
        victim = self.dir / "victim"
        victim.write_text("precious\n")
        link = self.dir / "link.env"
        link.symlink_to(victim)
        with self.assertRaises(dogfood.ProfileError):
            dogfood.write_regular_file(link, "clobber\n", 0o600, exclusive=True)
        with self.assertRaises(dogfood.ProfileError):
            dogfood.write_regular_file(link, "clobber\n", 0o644, exclusive=False)
        self.assertEqual(victim.read_text(), "precious\n")

    def test_receipt_parsing_refuses_forgeries(self):
        profile = dogfood.parse_profile(dict(GOOD_PROFILE))
        lock = dogfood.parse_lock(json.loads(real_lock_json()), profile)
        compose = dogfood.compose_to_json(dogfood.compose_document(profile, lock))
        good = {
            "project_name": profile.project_name,
            "daemon": dict(dogfood.EXPECTED_DAEMON),
            "compose_sha256": dogfood.sha256_text(compose),
            "containers": {s: {"id": FAKE_CONTAINER_IDS[s], "name": dogfood.CONTAINER_NAMES[s], "image": FAKE_IDS[s]} for s in dogfood.CONTAINER_NAMES},
            "network": {"id": FAKE_NETWORK_ID, "name": dogfood.NETWORK_NAME},
        }
        self.assertEqual(dogfood.parse_receipt(copy.deepcopy(good), profile, lock, compose), good)

        def forged(mutate):
            data = copy.deepcopy(good)
            mutate(data)
            return data

        cases = {
            "short id": forged(lambda d: d["containers"]["cp"].__setitem__("id", "abc123")),
            "other name": forged(lambda d: d["containers"]["cp"].__setitem__("name", "shared-openbao")),
            "other image": forged(lambda d: d["containers"]["cp"].__setitem__("image", "sha256:" + "f" * 64)),
            "extra container": forged(lambda d: d["containers"].__setitem__("extra", {"id": FOREIGN_CONTAINER_ID, "name": "x", "image": FAKE_IDS["cp"]})),
            "other network name": forged(lambda d: d["network"].__setitem__("name", "bridge")),
            "other daemon": forged(lambda d: d["daemon"].__setitem__("ID", "other")),
            "other project": forged(lambda d: d.__setitem__("project_name", "someone-elses-project")),
            "tampered compose": forged(lambda d: d.__setitem__("compose_sha256", "0" * 64)),
        }
        for name, data in cases.items():
            with self.subTest(name=name):
                with self.assertRaises(dogfood.ProfileError):
                    dogfood.parse_receipt(data, profile, lock, compose)

    def test_ownership_evaluators_reject_foreign_resources(self):
        profile = dogfood.parse_profile(dict(GOOD_PROFILE))
        lock = dogfood.parse_lock(json.loads(real_lock_json()), profile)
        self.assertEqual(dogfood.evaluate_owned_container(container_doc("cp", profile.project_name), "cp", profile, lock), [])
        self.assertTrue(dogfood.evaluate_owned_container(container_doc("cp", "other-project"), "cp", profile, lock))
        self.assertTrue(dogfood.evaluate_owned_container(container_doc("cp", profile.project_name, image="sha256:" + "f" * 64), "cp", profile, lock))
        self.assertTrue(dogfood.evaluate_owned_container(container_doc("cp", profile.project_name, name="fabric-dogfood-keycloak"), "cp", profile, lock))
        self.assertTrue(dogfood.evaluate_owned_container(None, "cp", profile, lock))
        self.assertEqual(dogfood.evaluate_owned_network(network_doc(profile.project_name), profile), [])
        self.assertTrue(dogfood.evaluate_owned_network(network_doc("other-project"), profile))
        ports = {"8080/tcp": [{"HostIp": "127.0.0.1", "HostPort": "18781"}]}
        self.assertEqual(dogfood.evaluate_published_port(container_doc("keycloak", profile.project_name, ports=ports), 8080, "127.0.0.1", 18781), [])
        self.assertTrue(dogfood.evaluate_published_port(container_doc("keycloak", profile.project_name, ports={"8080/tcp": [{"HostIp": "0.0.0.0", "HostPort": "18781"}]}), 8080, "127.0.0.1", 18781))
        self.assertTrue(dogfood.evaluate_published_port(container_doc("keycloak", profile.project_name, ports=dict(ports, **{"9000/tcp": [{"HostIp": "127.0.0.1", "HostPort": "9000"}]})), 8080, "127.0.0.1", 18781))
        self.assertTrue(dogfood.evaluate_published_port(container_doc("keycloak", profile.project_name, ports={}), 8080, "127.0.0.1", 18781))


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class CommandLine(unittest.TestCase):
    """Command flows over a temporary profile directory with every external seam faked."""

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.dir = Path(self.tmp.name) / "profile"
        self.dir.mkdir()
        (self.dir / "profile.toml").write_text(profile_toml(GOOD_PROFILE), encoding="utf-8")
        self.profile = dogfood.parse_profile(dict(GOOD_PROFILE))
        self.docker = FakeDocker()  # unscripted: any call fails the test
        patches = [
            mock.patch.object(dogfood, "run_docker", self.docker),
            mock.patch.object(dogfood, "worktree_head", lambda root: dogfood.PINNED_COMMIT),  # environmental; pinned for determinism
            mock.patch.object(dogfood, "read_iptables", lambda chain: iptables_listing(self.profile)[chain]),
            mock.patch.object(dogfood, "human_console", lambda: io.StringIO()),
            # The terminal seam: a human "types" a SYNTHETIC password; no /dev/tty is opened under test.
            mock.patch.object(dogfood, "prompt_operator_password", lambda console: SYNTHETIC_OPERATOR_PASSWORD),
        ]
        for patch in patches:
            patch.start()
            self.addCleanup(patch.stop)

    def run_main(self, argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = dogfood.main(["--profile-dir", str(self.dir), *argv])
        return code, out.getvalue(), err.getvalue()

    def prepare_real(self):
        """A profile directory as `build` would leave it, with SYNTHETIC-but-well-formed IDs."""
        out = self.dir / ".out"
        out.mkdir()
        (out / dogfood.LOCK_NAME).write_text(real_lock_json())
        code, _, err = self.run_main(["prepare"])
        self.assertEqual(code, 0, err)
        return out

    def write_receipt(self, out: Path, **overrides):
        lock = dogfood.parse_lock(json.loads(real_lock_json()), self.profile)
        receipt = {
            "project_name": self.profile.project_name,
            "daemon": dict(dogfood.EXPECTED_DAEMON),
            "compose_sha256": dogfood.sha256_text((out / dogfood.COMPOSE_NAME).read_text()),
            "containers": {s: {"id": FAKE_CONTAINER_IDS[s], "name": dogfood.CONTAINER_NAMES[s], "image": lock.image_for(s)} for s in dogfood.CONTAINER_NAMES},
            "network": {"id": FAKE_NETWORK_ID, "name": dogfood.NETWORK_NAME},
        }
        receipt.update(overrides)
        (out / dogfood.RECEIPT_NAME).write_text(json.dumps(receipt))
        ephemeral = out / ".ephemeral"
        ephemeral.mkdir()
        (ephemeral / "keycloak.env").write_text("KC_BOOTSTRAP_ADMIN_USERNAME=synthetic\n")
        return receipt

    def activate_argv(self, digest: str | None = None) -> list[str]:
        """`activate` with the approved packet's digest for whatever lock is on disk (a placeholder when there is none)."""
        if digest is None:
            digest = "0" * 64
            try:
                lock = dogfood.parse_lock(json.loads((self.dir / ".out" / dogfood.LOCK_NAME).read_text()), self.profile)
                if not lock.synthetic:
                    digest = dogfood.packet_digest(self.profile, lock)
            except (OSError, ValueError, dogfood.ProfileError):
                pass
        return ["activate", "--yes", "--packet-sha256", digest]

    # -- no-Docker paths -------------------------------------------------

    def test_default_invocation_is_prepare_and_check_only(self):
        code, out, err = self.run_main([])
        self.assertEqual(code, 1)
        self.assertIn("blocked: no pins.lock.json", err)
        self.assertEqual(self.docker.calls, [])
        self.assertFalse((self.dir / ".out").exists())

    def test_synthetic_prepare_writes_validated_files_and_check_detects_drift(self):
        code, out, err = self.run_main(["prepare", "--synthetic-pins"])
        self.assertEqual(code, 0, err)
        self.assertEqual(self.docker.calls, [])
        out_dir = self.dir / ".out"
        for name in dogfood.RENDERED_NAMES:
            self.assertTrue((out_dir / name).is_file(), name)
        compose = json.loads((out_dir / dogfood.COMPOSE_NAME).read_text())
        self.assertNotIn("${", (out_dir / dogfood.COMPOSE_NAME).read_text())
        self.assertEqual(compose["networks"][dogfood.NETWORK_NAME]["internal"], True)
        self.assertNotIn("ports", compose["services"][dogfood.SERVICE_CP])
        toml_path = out_dir / dogfood.CP_CONFIG_NAME
        toml_path.write_text(toml_path.read_text().replace('mode = "in_memory"', 'mode = "keycloak"', 1))
        code, out, err = self.run_main(["check"])
        self.assertEqual(code, 1)
        self.assertIn("differs from what profile.toml renders", err)

    def test_dotenv_presence_fails_check(self):
        self.run_main(["prepare", "--synthetic-pins"])
        (self.dir / ".env").write_text("FABRIC_CP_SETTING_LISTEN=0.0.0.0:1\n")
        code, out, err = self.run_main(["check"])
        self.assertEqual(code, 1)
        self.assertIn(".env", err)

    def test_unenforced_option_no_longer_exists(self):
        self.prepare_real()
        for flag in ("accepted-unenforced", "applied"):
            with self.subTest(flag=flag), contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    dogfood.main(["--profile-dir", str(self.dir), "activate", "--yes", "--network-enforcement", flag])
        self.assertEqual(self.docker.calls, [])

    def test_activate_refuses_without_yes_and_refuses_synthetic_pins(self):
        self.run_main(["prepare", "--synthetic-pins"])
        self.assertEqual(self.run_main(["activate"])[0], 2)
        code, out, err = self.run_main(["activate", "--yes"])
        self.assertEqual(code, 1)
        self.assertIn("synthetic", err)
        self.assertEqual(self.docker.calls, [], "activate must refuse before touching Docker")
        self.assertFalse((self.dir / ".out" / ".ephemeral").exists(), "no credentials may be created on refusal")

    def test_build_and_reset_refuse_without_yes(self):
        self.assertEqual(self.run_main(["build"])[0], 2)
        self.assertEqual(self.run_main(["reset"])[0], 2)
        self.assertEqual(self.docker.calls, [])

    def test_prepare_refuses_while_an_activation_exists(self):
        out = self.prepare_real()
        self.write_receipt(out)
        code, _, err = self.run_main(["prepare"])
        self.assertEqual(code, 1)
        self.assertIn("reset first", err)

    def test_enforcement_prints_rules_scoped_to_this_bridge_only(self):
        code, out, err = self.run_main(["enforcement"])
        self.assertEqual(code, 0)
        self.assertNotIn("accepted-unenforced", out)
        for line in out.splitlines():
            if line.startswith("iptables"):
                self.assertTrue(dogfood.BRIDGE_NAME in line or "10.213.7.0/24" in line, line)
        self.assertEqual(self.docker.calls, [])

    def test_symlinked_out_directory_is_refused_everywhere(self):
        victim = Path(self.tmp.name) / "victim"
        victim.mkdir()
        (victim / "precious.txt").write_text("keep me\n")
        (self.dir / ".out").symlink_to(victim)
        for argv in (["prepare", "--synthetic-pins"], ["check"], ["reset", "--yes"], ["activate", "--yes"]):
            with self.subTest(argv=argv):
                code, _, err = self.run_main(argv)
                self.assertEqual(code, 1)
                self.assertIn("symlink", err)
        self.assertTrue((victim / "precious.txt").is_file())
        self.assertEqual(sorted(p.name for p in victim.iterdir()), ["precious.txt"])
        self.assertEqual(self.docker.calls, [])

    # -- activation ----------------------------------------------------------

    def test_activate_fails_closed_when_the_firewall_gate_is_absent(self):
        self.prepare_real()
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        with mock.patch.object(dogfood, "read_iptables", lambda chain: iptables_listing(self.profile, drop_rule="DOCKER-USER")[chain]):
            code, _, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("enforcement gate not in place", err)
        self.assertEqual([c[0] for c in self.docker.calls], ["info"], "only the read-only daemon check may run before the gate is verified")
        self.assertFalse((self.dir / ".out" / ".ephemeral").exists())

    def test_activate_refuses_without_the_approved_packet_digest_before_any_docker_call(self):
        self.prepare_real()
        for argv, expected in ((["activate", "--yes"], 2), (self.activate_argv("not-a-digest"), 2), (self.activate_argv("f" * 64), 1)):
            with self.subTest(argv=argv):
                code, _, err = self.run_main(argv)
                self.assertEqual(code, expected)
                self.assertIn("packet", err)
        self.assertEqual(self.docker.calls, [])
        self.assertFalse((self.dir / ".out" / ".ephemeral" / "keycloak.env").exists())

    def test_activate_refuses_a_packet_approved_for_other_artefacts(self):
        self.prepare_real()
        approved = self.activate_argv()
        lock_path = self.dir / ".out" / dogfood.LOCK_NAME
        lock_path.write_text(lock_path.read_text().replace("a1" * 32, "d4" * 32))
        self.assertEqual(self.run_main(["prepare"])[0], 0)
        code, _, err = self.run_main(approved)
        self.assertEqual(code, 1)
        self.assertIn("do not produce the approved packet", err)
        self.assertEqual(self.docker.calls, [])

    def test_activate_refuses_the_wrong_daemon(self):
        self.prepare_real()
        self.docker.on(lambda a: a[0] == "info", json.dumps(dict(dogfood.EXPECTED_DAEMON, Name="riley")))
        code, _, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("wrong Docker daemon", err)
        self.assertEqual(len(self.docker.calls), 1)

    def test_activate_aborts_on_existing_same_name_resources_instead_of_adopting(self):
        self.prepare_real()
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.on(lambda a: a[:2] == ["ps", "--all"] and "--format" in a, "someone-elses\nfabric-dogfood-keycloak\n")
        self.docker.on(lambda a: a[:2] == ["ps", "--all"] and "--filter" in a, "")
        self.docker.on(lambda a: a[:2] == ["network", "ls"], "bridge\nhost\nnone\n")
        code, _, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("fabric-dogfood-keycloak", err)
        self.assertEqual(self.docker.calls_matching("compose"), [], "compose up must not run on a collision")
        self.assertEqual(self.docker.calls_matching("rm"), [], "nothing pre-existing may be removed")
        self.assertFalse((self.dir / ".out" / ".ephemeral").exists(), "no credential may exist after a refused activation")

    def test_partial_startup_cleans_only_provably_owned_resources(self):
        self.prepare_real()
        project = self.profile.project_name
        foreign_keycloak = container_doc("keycloak", "another-project", cid=FOREIGN_CONTAINER_ID)
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.on(lambda a: a[:2] == ["ps", "--all"], "")
        self.docker.on(lambda a: a[:2] == ["network", "ls"], "bridge\n")
        self.docker.on(lambda a: a[0] == "compose" and "up" in a, subprocess.CalledProcessError(1, "docker"))
        self.docker.inspect_responses({
            dogfood.CONTAINER_NAMES["cp"]: container_doc("cp", project),
            dogfood.CONTAINER_NAMES["console"]: None,
            dogfood.CONTAINER_NAMES["keycloak"]: foreign_keycloak,
            dogfood.NETWORK_NAME: network_doc(project),
        })
        self.docker.on(lambda a: a[:2] == ["rm", "--force"] or a[:2] == ["network", "rm"], "")
        code, _, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("activation abandoned", err)
        up = self.docker.calls_matching("compose")
        self.assertEqual(len(up), 1)
        self.assertIn("--no-build", up[0])
        self.assertEqual(self.docker.calls_matching("rm"), [["rm", "--force", FAKE_CONTAINER_IDS["cp"]]])
        self.assertEqual(self.docker.calls_matching("network", "rm"), [["network", "rm", FAKE_NETWORK_ID]])
        self.assertNotIn(FOREIGN_CONTAINER_ID, str(self.docker.calls), "a container with our name but another project's label must be left alone")
        self.assertFalse((self.dir / ".out" / ".ephemeral" / "keycloak.env").exists(), "the bootstrap credential must not outlive a failed start")
        self.assertFalse((self.dir / ".out" / dogfood.RECEIPT_NAME).exists())

    def test_activate_does_not_bootstrap_until_the_owned_keycloak_publishes_the_port(self):
        self.prepare_real()
        project = self.profile.project_name
        wrong_port = {"8080/tcp": [{"HostIp": "0.0.0.0", "HostPort": "18781"}]}
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.on(lambda a: a[:2] == ["ps", "--all"], "")
        self.docker.on(lambda a: a[:2] == ["network", "ls"], "bridge\n")
        self.docker.on(lambda a: a[0] == "compose" and "up" in a, "")
        self.docker.inspect_responses({
            dogfood.CONTAINER_NAMES["cp"]: container_doc("cp", project),
            dogfood.CONTAINER_NAMES["console"]: container_doc("console", project),
            dogfood.CONTAINER_NAMES["keycloak"]: container_doc("keycloak", project, ports=wrong_port),
            dogfood.NETWORK_NAME: network_doc(project),
        })
        with mock.patch.object(dogfood, "http_json", side_effect=AssertionError("no HTTP request may be made before the port is verified")):
            code, _, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("refusing to bootstrap", err)
        receipt = json.loads((self.dir / ".out" / dogfood.RECEIPT_NAME).read_text())
        self.assertEqual(receipt["containers"]["cp"]["id"], FAKE_CONTAINER_IDS["cp"])
        self.assertEqual(receipt["network"]["id"], FAKE_NETWORK_ID)
        self.assertEqual(stat.S_IMODE((self.dir / ".out" / ".ephemeral" / "keycloak.env").stat().st_mode), 0o600)

    def test_activate_refuses_without_a_human_console(self):
        self.prepare_real()
        with mock.patch.object(dogfood, "human_console", lambda: None):
            code, _, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("/dev/tty", err)
        self.assertEqual(self.docker.calls, [])

    def test_activate_does_not_prompt_while_a_read_only_check_is_failing(self):
        self.prepare_real()
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        tripwire = mock.Mock(side_effect=AssertionError("the human must not be asked for a password before every read-only check has passed"))
        with mock.patch.object(dogfood, "read_iptables", lambda chain: iptables_listing(self.profile, drop_rule="DOCKER-USER")[chain]), \
                mock.patch.object(dogfood, "prompt_operator_password", tripwire):
            code, _, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("enforcement gate not in place", err)
        tripwire.assert_not_called()

    def test_activate_prompts_after_the_read_only_checks_and_a_rejected_password_mutates_nothing(self):
        self.prepare_real()
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.on(lambda a: a[:2] == ["ps", "--all"], "")
        self.docker.on(lambda a: a[:2] == ["network", "ls"], "bridge\n")
        prompt = mock.Mock(side_effect=dogfood.ProfileError("operator password rejected: the two entries differ"))
        with mock.patch.object(dogfood, "prompt_operator_password", prompt), \
                mock.patch.object(dogfood, "http_json", side_effect=AssertionError("no HTTP request may follow a rejected password")):
            code, out, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("the two entries differ", err)
        self.assertIn("nothing was started", err)
        prompt.assert_called_once()
        self.assertTrue(self.docker.calls_matching("ps"), "the prompt comes after the collision check, not before it")
        self.assertEqual(self.docker.calls_matching("compose"), [], "compose up must not run after a rejected password")
        self.assertEqual(self.docker.calls_matching("rm"), [])
        self.assertFalse((self.dir / ".out" / ".ephemeral").exists(), "no credential may be created after a rejected password")
        self.assertFalse((self.dir / ".out" / dogfood.RECEIPT_NAME).exists())
        self.assertNotIn(SYNTHETIC_OPERATOR_PASSWORD, out + err)

    def test_activate_refuses_when_the_terminal_vanishes_before_the_prompt(self):
        self.prepare_real()
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.on(lambda a: a[:2] == ["ps", "--all"], "")
        self.docker.on(lambda a: a[:2] == ["network", "ls"], "bridge\n")
        consoles = iter([io.StringIO(), None])  # present for the availability check, gone at the prompt
        with mock.patch.object(dogfood, "human_console", lambda: next(consoles)):
            code, _, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 1)
        self.assertIn("went away", err)
        self.assertEqual(self.docker.calls_matching("compose"), [])
        self.assertFalse((self.dir / ".out" / ".ephemeral").exists())

    def test_activate_sends_the_typed_password_to_keycloak_once_and_prints_only_the_username(self):
        self.prepare_real()
        project = self.profile.project_name
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.on(lambda a: a[:2] == ["ps", "--all"], "")
        self.docker.on(lambda a: a[:2] == ["network", "ls"], "bridge\n")
        self.docker.on(lambda a: a[0] == "compose" and "up" in a, "")
        self.docker.inspect_responses({
            dogfood.CONTAINER_NAMES["cp"]: container_doc("cp", project),
            dogfood.CONTAINER_NAMES["console"]: container_doc("console", project),
            dogfood.CONTAINER_NAMES["keycloak"]: container_doc("keycloak", project, ports={"8080/tcp": [{"HostIp": "127.0.0.1", "HostPort": "18781"}]}),
            dogfood.NETWORK_NAME: network_doc(project),
        })
        bootstrap = mock.Mock(return_value=("operator-abc123", SYNTHETIC_OPERATOR_PASSWORD))
        with mock.patch.object(dogfood, "bootstrap_realm", bootstrap), \
                mock.patch.object(dogfood, "http_json", return_value=(200, {}, {})), \
                mock.patch.object(dogfood, "cmd_status", return_value=[]), \
                mock.patch.object(dogfood, "evaluate_discovery", return_value=[]), \
                mock.patch.object(dogfood, "wait_for", lambda description, probe, **_: True):
            code, out, err = self.run_main(self.activate_argv())
        self.assertEqual(code, 0, err)
        bootstrap.assert_called_once()
        self.assertEqual(bootstrap.call_args.args[3], SYNTHETIC_OPERATOR_PASSWORD, "the typed password is what the operator is created with")
        self.assertIn("operator-abc123", out)
        self.assertNotIn(SYNTHETIC_OPERATOR_PASSWORD, out + err, "the password is never printed")
        for path in (self.dir / ".out").rglob("*"):
            if path.is_file():
                self.assertNotIn(SYNTHETIC_OPERATOR_PASSWORD, path.read_text(errors="replace"), f"the password is never written: {path}")

    # -- reset -----------------------------------------------------------------

    def test_reset_without_a_receipt_touches_no_docker_resource(self):
        out = self.prepare_real()
        code, text, err = self.run_main(["reset", "--yes"])
        self.assertEqual(code, 0, err)
        self.assertEqual(self.docker.calls, [])
        self.assertIn("no ownership receipt", text)
        self.assertFalse(out.exists())
        self.assertTrue((self.dir / "profile.toml").is_file())

    def test_reset_removes_exactly_the_recorded_ids_and_no_volumes(self):
        out = self.prepare_real()
        self.write_receipt(out)
        project = self.profile.project_name
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.inspect_responses({
            FAKE_CONTAINER_IDS["cp"]: container_doc("cp", project),
            FAKE_CONTAINER_IDS["console"]: None,  # already gone: skipped, not an error
            FAKE_CONTAINER_IDS["keycloak"]: container_doc("keycloak", project),
            FAKE_NETWORK_ID: network_doc(project),
        })
        self.docker.on(lambda a: a[:2] == ["rm", "--force"] or a[:2] == ["network", "rm"], "")
        code, text, err = self.run_main(["reset", "--yes"])
        self.assertEqual(code, 0, err)
        self.assertEqual(self.docker.calls_matching("rm"), [["rm", "--force", FAKE_CONTAINER_IDS["cp"]], ["rm", "--force", FAKE_CONTAINER_IDS["keycloak"]]])
        self.assertEqual(self.docker.calls_matching("network", "rm"), [["network", "rm", FAKE_NETWORK_ID]])
        flat = " ".join(" ".join(c) for c in self.docker.calls)
        for forbidden in ("compose", "down", "--volumes", "--remove-orphans", "prune", "volume"):
            self.assertNotIn(forbidden, flat)
        self.assertFalse(out.exists())

    def test_reset_refuses_a_forged_receipt_and_removes_nothing(self):
        out = self.prepare_real()
        self.write_receipt(out)
        self.docker.on(lambda a: a[0] == "info", GOOD_INFO)
        self.docker.inspect_responses({
            FAKE_CONTAINER_IDS["cp"]: container_doc("cp", self.profile.project_name, name="shared-openbao"),  # recorded ID now names a foreign container
            FAKE_CONTAINER_IDS["console"]: container_doc("console", self.profile.project_name),
            FAKE_CONTAINER_IDS["keycloak"]: container_doc("keycloak", "another-project"),
            FAKE_NETWORK_ID: network_doc(self.profile.project_name, containers={"x": {"Name": "shared-openbao"}}),
        })
        code, _, err = self.run_main(["reset", "--yes"])
        self.assertEqual(code, 1)
        self.assertIn("nothing removed", err)
        self.assertEqual(self.docker.calls_matching("rm"), [])
        self.assertEqual(self.docker.calls_matching("network", "rm"), [])
        self.assertTrue((out / dogfood.RECEIPT_NAME).is_file(), "local state is kept when ownership cannot be re-verified")

    def test_reset_refuses_a_tampered_compose_before_any_docker_call(self):
        out = self.prepare_real()
        self.write_receipt(out)
        compose = out / dogfood.COMPOSE_NAME
        compose.write_text(compose.read_text().replace('"internal": true', '"internal": false'))
        code, _, err = self.run_main(["reset", "--yes"])
        self.assertEqual(code, 1)
        self.assertIn("compose_sha256", err)
        self.assertEqual(self.docker.calls, [])

    def test_reset_refuses_a_receipt_with_an_unrecorded_shape(self):
        out = self.prepare_real()
        self.write_receipt(out, containers={"cp": {"id": FOREIGN_CONTAINER_ID, "name": "shared-openbao", "image": FAKE_IDS["cp"]}})
        code, _, err = self.run_main(["reset", "--yes"])
        self.assertEqual(code, 1)
        self.assertEqual(self.docker.calls, [])
        self.assertTrue(out.exists())

    def test_reset_never_deletes_through_a_symlinked_ephemeral_dir_or_file(self):
        out = self.prepare_real()
        victim = Path(self.tmp.name) / "victim.env"
        victim.write_text("precious\n")
        ephemeral = out / ".ephemeral"
        ephemeral.mkdir()
        (ephemeral / "keycloak.env").symlink_to(victim)
        code, _, err = self.run_main(["reset", "--yes"])
        self.assertEqual(code, 1)
        self.assertIn("symlink", err)
        self.assertEqual(victim.read_text(), "precious\n")
        self.assertEqual(self.docker.calls, [])


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class OperatorPasswordPrompt(unittest.TestCase):
    """The interactive seam: a human types a NEW synthetic-only operator password; the tool never echoes, prints or falls back."""

    def setUp(self):
        self.profile = dogfood.parse_profile(dict(GOOD_PROFILE))

    @staticmethod
    def scripted(*entries: str):
        """A stand-in for the echo-off terminal read: answers each prompt in turn and records the prompts it was shown."""
        answers = iter(entries)
        prompts: list[str] = []

        def read_secret(prompt: str) -> str:
            prompts.append(prompt)
            return next(answers)

        return read_secret, prompts

    def test_two_matching_entries_are_accepted_and_never_written_to_the_console(self):
        console = io.StringIO()
        read_secret, prompts = self.scripted(SYNTHETIC_OPERATOR_PASSWORD, SYNTHETIC_OPERATOR_PASSWORD)
        self.assertEqual(dogfood.prompt_operator_password(console, read_secret), SYNTHETIC_OPERATOR_PASSWORD)
        self.assertEqual(len(prompts), 2, "the password is confirmed by a second entry")
        self.assertNotIn(SYNTHETIC_OPERATOR_PASSWORD, console.getvalue())
        self.assertIn("NEW password", console.getvalue())

    def test_empty_short_and_mismatched_entries_are_refused_without_echoing_them(self):
        short = "short-synthetic"
        other = SYNTHETIC_OPERATOR_PASSWORD + "-other"
        cases = {
            "empty": (("", ""), "empty"),
            "short": ((short, short), "shorter than 16"),
            "mismatch": ((SYNTHETIC_OPERATOR_PASSWORD, other), "entries differ"),
        }
        for label, (entries, reason) in cases.items():
            with self.subTest(label=label):
                console = io.StringIO()
                read_secret, _ = self.scripted(*entries)
                with self.assertRaises(dogfood.ProfileError) as refused:
                    dogfood.prompt_operator_password(console, read_secret)
                self.assertIn(reason, str(refused.exception))
                for secret in (short, other, SYNTHETIC_OPERATOR_PASSWORD):
                    self.assertNotIn(secret, str(refused.exception))
                    self.assertNotIn(secret, console.getvalue())

    def pty_as_dev_tty(self):
        """A real pseudo-terminal standing in for /dev/tty: `os.open("/dev/tty")` hands out a fresh descriptor to its slave side."""
        master, slave = os.openpty()
        self.addCleanup(os.close, master)
        self.addCleanup(os.close, slave)
        os.set_blocking(master, False)
        real_open = os.open
        handed_out: list[int] = []

        def fake_open(path, flags, *args, **kwargs):
            if path != "/dev/tty":
                return real_open(path, flags, *args, **kwargs)
            handed_out.append(os.dup(slave))
            return handed_out[-1]

        patch = mock.patch.object(dogfood.os, "open", fake_open)
        patch.start()
        self.addCleanup(patch.stop)
        return master, slave, handed_out

    @staticmethod
    def type_once_echo_is_off(master: int, slave: int, data: bytes) -> threading.Thread:
        """Types `data` on the master side only after the slave has ECHO cleared.

        A pty echoes at input time, and the tool's TCSAFLUSH discards typed-ahead
        input, so typing before echo is off would both leak and be lost. After
        five seconds the data is typed anyway so a broken tool fails instead of hanging.
        """

        def typist():
            deadline = time.monotonic() + 5
            while termios.tcgetattr(slave)[3] & termios.ECHO and time.monotonic() < deadline:
                time.sleep(0.01)
            os.write(master, data)

        thread = threading.Thread(target=typist, daemon=True)
        thread.start()
        return thread

    def drain(self, master: int) -> bytes:
        try:
            return os.read(master, 4096)
        except BlockingIOError:
            return b""

    def test_the_secret_is_read_from_a_verified_terminal_with_echo_off_and_attributes_restored(self):
        master, slave, handed_out = self.pty_as_dev_tty()
        before = termios.tcgetattr(slave)
        self.assertTrue(before[3] & termios.ECHO, "the pty starts with echo on, so a leak would be visible on the master side")
        typist = self.type_once_echo_is_off(master, slave, SYNTHETIC_OPERATOR_PASSWORD.encode() + b"\n")
        self.assertEqual(dogfood.read_secret_from_terminal("operator password: "), SYNTHETIC_OPERATOR_PASSWORD)
        typist.join(timeout=5)
        shown = self.drain(master)
        self.assertIn(b"operator password: ", shown)
        self.assertNotIn(SYNTHETIC_OPERATOR_PASSWORD.encode(), shown, "the terminal echoed the secret")
        self.assertEqual(termios.tcgetattr(slave), before, "terminal attributes were not restored")
        self.assertEqual(len(handed_out), 1)
        with self.assertRaises(OSError):
            os.fstat(handed_out[0])  # the descriptor this read opened is closed again

    def test_end_of_input_at_the_prompt_is_a_refusal_not_an_empty_password_and_restores_the_terminal(self):
        master, slave, _ = self.pty_as_dev_tty()
        before = termios.tcgetattr(slave)
        typist = self.type_once_echo_is_off(master, slave, b"\x04")  # ^D at the start of a line: canonical-mode EOF, no bytes delivered
        with self.assertRaises(dogfood.ProfileError) as refused:
            dogfood.prompt_operator_password(io.StringIO())
        typist.join(timeout=5)
        self.assertIn("EOFError", str(refused.exception))
        self.assertIn("nothing was read from stdin", str(refused.exception))
        self.assertEqual(termios.tcgetattr(slave), before)

    def test_an_interrupt_during_the_read_restores_the_terminal_and_closes_the_descriptor(self):
        master, slave, handed_out = self.pty_as_dev_tty()
        before = termios.tcgetattr(slave)
        with mock.patch.object(dogfood.os, "read", mock.Mock(side_effect=KeyboardInterrupt)):
            with self.assertRaises(KeyboardInterrupt):
                dogfood.read_secret_from_terminal("operator password: ")
        self.assertEqual(termios.tcgetattr(slave), before, "echo must come back even when the read is interrupted")
        with self.assertRaises(OSError):
            os.fstat(handed_out[0])

    def test_an_unavailable_dev_tty_never_reads_a_usable_stdin(self):
        """The getpass hazard: with /dev/tty gone, a TTY stdin is a tempting fallback. It must not be touched."""
        master, slave = os.openpty()
        self.addCleanup(os.close, master)
        os.write(master, b"typed-on-a-stdin-terminal\n")
        stdin = os.fdopen(slave, "r")  # a real, readable terminal on sys.stdin
        self.addCleanup(stdin.close)
        real_open = os.open

        def no_dev_tty(path, flags, *args):
            if path == "/dev/tty":
                raise OSError("No such device or address")
            return real_open(path, flags, *args)

        console = io.StringIO()
        with mock.patch.object(dogfood.sys, "stdin", stdin), \
                mock.patch.object(dogfood.os, "open", no_dev_tty), \
                mock.patch.object(dogfood.os, "read", mock.Mock(side_effect=AssertionError("nothing may be read when /dev/tty is unavailable"))), \
                mock.patch.object(dogfood.termios, "tcsetattr", mock.Mock(side_effect=AssertionError("no terminal to configure"))):
            with self.assertRaises(dogfood.ProfileError) as refused:
                dogfood.prompt_operator_password(console)
        self.assertIn("never read from stdin", str(refused.exception))
        self.assertNotIn("typed-on-a-stdin-terminal", str(refused.exception) + console.getvalue())
        os.set_blocking(slave, False)
        self.assertEqual(os.read(slave, 4096), b"typed-on-a-stdin-terminal\n", "the stdin line was consumed")

    def test_a_dev_tty_that_is_not_a_terminal_is_refused_before_any_read(self):
        read_end, write_end = os.pipe()
        self.addCleanup(os.close, read_end)
        self.addCleanup(os.close, write_end)
        with mock.patch.object(dogfood.os, "open", lambda path, flags, *a, **k: os.dup(write_end)), \
                mock.patch.object(dogfood.os, "read", mock.Mock(side_effect=AssertionError("no read from a non-terminal"))):
            with self.assertRaises(dogfood.ProfileError) as refused:
                dogfood.read_secret_from_terminal("operator password: ")
        self.assertIn("not a terminal", str(refused.exception))

    def test_human_console_refuses_a_dev_tty_that_is_not_a_terminal(self):
        read_end, write_end = os.pipe()
        self.addCleanup(lambda: os.close(read_end))
        with mock.patch.object(dogfood.os, "open", lambda path, flags: write_end):
            self.assertIsNone(dogfood.human_console())
        with self.assertRaises(OSError):
            os.fstat(write_end)  # closed again by human_console, not leaked

    def test_human_console_is_none_when_dev_tty_cannot_be_opened(self):
        with mock.patch.object(dogfood.os, "open", mock.Mock(side_effect=OSError())):
            self.assertIsNone(dogfood.human_console())

    def fake_keycloak(self):
        """Answers bootstrap_realm's loopback calls and records every request body."""
        requests: list[tuple[str, Any]] = []

        def http_json(url, profile, body=None, headers=None, method="GET"):
            requests.append((url, json.loads(body) if body and method == "POST" and not url.endswith("/token") else None))
            if url.endswith("/protocol/openid-connect/token"):
                return 200, {"access_token": "synthetic-admin-token"}, {}
            if url.endswith("/users") and method == "POST":
                return 201, None, {"Location": f"{url}/0f0f0f0f-0000-4000-8000-000000000001"}
            if url.endswith(f"/roles/{dogfood.OPERATOR_ROLE}") and method != "POST":
                return 200, {"id": "role-id", "name": dogfood.OPERATOR_ROLE}, {}
            return 201, None, {}

        return http_json, requests

    def test_bootstrap_realm_sets_the_given_password_and_returns_it_unchanged(self):
        http_json, requests = self.fake_keycloak()
        with mock.patch.object(dogfood, "http_json", http_json):
            operator, password = dogfood.bootstrap_realm(self.profile, "bootstrap-synthetic", "synthetic-admin-password", SYNTHETIC_OPERATOR_PASSWORD)
        self.assertTrue(operator.startswith("operator-"))
        self.assertEqual(password, SYNTHETIC_OPERATOR_PASSWORD)
        created = [body for url, body in requests if url.endswith("/users") and body]
        self.assertEqual(len(created), 1)
        self.assertEqual(created[0]["credentials"], [{"type": "password", "value": SYNTHETIC_OPERATOR_PASSWORD, "temporary": False}])

    def test_bootstrap_realm_generates_a_password_when_none_is_given_for_the_noninteractive_path(self):
        http_json, requests = self.fake_keycloak()
        out, err = io.StringIO(), io.StringIO()
        with mock.patch.object(dogfood, "http_json", http_json), contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            operator, password = dogfood.bootstrap_realm(self.profile, "bootstrap-synthetic", "synthetic-admin-password")
        self.assertGreaterEqual(len(password), dogfood.MIN_OPERATOR_PASSWORD_LENGTH)
        created = [body for url, body in requests if url.endswith("/users") and body]
        self.assertEqual(created[0]["credentials"][0]["value"], password)
        self.assertEqual(out.getvalue() + err.getvalue(), "", "the CI path produces no output at all")


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class ReadinessEvidence(unittest.TestCase):
    def setUp(self):
        self.profile = dogfood.parse_profile(dict(GOOD_PROFILE))

    def test_discovery_must_carry_the_configured_issuer(self):
        good = {
            "issuer": self.profile.issuer,
            "authorization_endpoint": f"{self.profile.issuer}/protocol/openid-connect/auth",
            "code_challenge_methods_supported": ["S256", "plain"],
        }
        self.assertEqual(dogfood.evaluate_discovery(good, self.profile), [])
        self.assertTrue(dogfood.evaluate_discovery(dict(good, issuer="http://keycloak:8080/realms/fabric-dogfood"), self.profile))
        self.assertTrue(dogfood.evaluate_discovery(dict(good, code_challenge_methods_supported=["plain"]), self.profile))

    def test_session_config_must_send_the_browser_to_the_published_issuer(self):
        good = {
            "authorization_endpoint": f"{self.profile.issuer}/protocol/openid-connect/auth",
            "client_id": dogfood.CONSOLE_CLIENT_ID,
            "redirect_uri": self.profile.redirect_uri,
            "scope": "openid profile",
        }
        self.assertEqual(dogfood.evaluate_session_config(good, self.profile), [])
        self.assertTrue(dogfood.evaluate_session_config(dict(good, authorization_endpoint="http://keycloak:8080/realms/fabric-dogfood/protocol/openid-connect/auth"), self.profile))
        self.assertTrue(dogfood.evaluate_session_config(dict(good, redirect_uri="http://127.0.0.1:18780/callback"), self.profile))

    def test_network_inspect_evidence(self):
        good = [network_doc(self.profile.project_name, containers={"a": {"Name": "fabric-dogfood-cp"}, "b": {"Name": "fabric-dogfood-keycloak"}})]
        self.assertEqual(dogfood.evaluate_network_inspect(good, self.profile), [])
        bad = copy.deepcopy(good)
        bad[0]["Internal"] = False
        bad[0]["Containers"]["c"] = {"Name": "shared-openbao"}
        findings = dogfood.evaluate_network_inspect(bad, self.profile)
        self.assertTrue(any("not internal" in f for f in findings))
        self.assertTrue(any("shared-openbao" in f for f in findings))


if __name__ == "__main__":
    unittest.main()
