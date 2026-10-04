#!/usr/bin/env python3
"""Hosted-runner trial of the disposable dogfood profile. TEST ONLY; never LucentRoot.

Two modes, one script, one gate:

  DEFAULT (build only). Runs nowhere but under GitHub Actions and refuses the
  LucentRoot daemon by ID and by name before anything else. Then it builds the
  control plane and console from `git archive <pinned commit>`, derives
  Keycloak from the pinned digest (dogfood.py's own functions), saves the three
  images as compressed archives, renders and validates the profile with the
  real image IDs, writes a redacted manifest, and finishes. It applies no
  iptables rule, creates no network, starts no container, bootstraps no realm,
  and creates no credential. The runtime stage functions below are not called
  at all; the offline tests in scripts/tests/test_ci_smoke.py prove that with
  scripted seams.

  --runtime. The existing runtime path, kept for a later, separately approved
  workflow_dispatch: the five printed iptables rules (runner only), compose up,
  realm bootstrap with credentials that live only in memory and one 0600 file
  under RUNNER_TEMP, a real authorization-code + PKCE sign-in through the real
  console driven by Playwright, the catalogue as that operator, connectivity
  probes, and a `finally` that removes exactly what it created by recorded ID
  and exact rule. The host-identity expectation in dogfood.py is pointed, in
  memory, at the runner's own daemon so every existing verifier runs
  unchanged; that is a test composition of a host assertion, not a bypass of
  any authentication. No fabricated token exists anywhere.

Either way, credentials, HTTP bodies, container logs and subprocess stderr
never reach the job log or the artifact directory. Failures are reported by
assertion name or exception class only. The one exception: in build-only
mode, where no credential has been generated, a bounded, redacted tail of a
failing build command's stderr is printed to the job log (never written under
the artifact directory) so a compiler or Docker build failure has a cause.

    python3 examples/disposable-dogfood/ci_smoke.py --artifact-dir "$RUNNER_TEMP/dogfood-artifacts"
    python3 examples/disposable-dogfood/ci_smoke.py --artifact-dir "$RUNNER_TEMP/dogfood-artifacts" --runtime
"""

from __future__ import annotations

import argparse
import gzip
import ipaddress
import json
import os
import re
import secrets
import shutil
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from typing import Any, Callable

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import dogfood  # noqa: E402

LUCENTROOT_ID = "47e4864c-619b-4190-8ef5-65d1feceb155"
LUCENTROOT_NAME = "lucentroot"
SUDO = "/usr/bin/sudo"
PEER_NETWORK = "fabric-dogfood-ci-peer"
PEER_NAME = "fabric-dogfood-ci-peer"
PROBE_NAME = "fabric-dogfood-ci-probe"
PEER_PORT = 9000
NODE_BASE_PATTERN = re.compile(r"^FROM (node:22-bookworm-slim@sha256:[0-9a-f]{64}) AS builder$", re.M)
# TCP connect (or, with PROBE_PATH, an HTTP GET) with a 5 s budget: 0 reached, 1 refused/unreachable, 2 timed out.
PROBE_JS = (
    "const h=process.env.PROBE_HOST,p=+process.env.PROBE_PORT,path=process.env.PROBE_PATH;"
    "const done=c=>{process.exit(c)};setTimeout(()=>done(2),5000).unref();"
    "if(path){require('http').get({host:h,port:p,path,timeout:5000},r=>{r.resume();done(0)}).on('error',()=>done(1));}"
    "else{const s=require('net').connect({host:h,port:p});s.setTimeout(5000);"
    "s.on('connect',()=>{s.destroy();done(0)});s.on('timeout',()=>done(2));s.on('error',()=>done(1));}"
)
PROBE_NEGATIVE_EXITS = (1, 2)  # the only exit codes that mean "the network blocked it"; anything else is a broken probe
LISTEN_JS = "require('net').createServer(s=>s.end()).listen(+process.env.LISTEN_PORT,'0.0.0.0')"
# The one environment entry the pinned control-plane image itself carries (`ENV FABRIC_CP_CONFIG=...` in its Dockerfile).
# It is the mount path of the read-only config; it overrides nothing the profile authored.
CP_IMAGE_CONFIG_ENV = f"FABRIC_CP_CONFIG={dogfood.CP_CONFIG_PATH}"
# After code redemption the control plane may still be on its previous JWKS read; a 401 on the first
# authenticated request is the only status treated as temporary, for at most ATTEMPTS * DELAY seconds.
TOKEN_ACCEPTANCE_ATTEMPTS = 10
TOKEN_ACCEPTANCE_DELAY = 3.0  # 10 * 3.0 = 30 s bound
# Build-only mode has no generated credential, so compiler/Docker build stderr may be kept for diagnosis. Tail only,
# job log only (never the artifact directory), redacted, and never under --runtime. `main` sets this per mode.
CAPTURE_BUILD_STDERR = False
BUILD_STDERR_TAIL_BYTES = 4000
build_stderr_tails: list[str] = []

# Every function that applies a host rule, starts a container or network, bootstraps the realm, creates a
# credential, drives a browser or probes connectivity. `main` calls these only under --runtime, and the
# offline tests replace each of them with a seam that fails the test if the default mode reaches it.
RUNTIME_STAGES = ("iptables_with_sudo", "activate", "bootstrap_and_ready", "sign_in_through_console", "exercise_catalogue", "connectivity")
# Docker subcommands the default mode must never issue. The tests assert over the scripted call log.
RUNTIME_DOCKER_SUBCOMMANDS = ("run", "create", "start", "restart", "network", "compose", "logs", "exec", "rm", "login", "push")


class SmokeError(Exception):
    """An assertion failed. Carries the assertion's name only; the detail lives (redacted) in the manifest."""

    def __init__(self, assertion: str) -> None:
        super().__init__(assertion)
        self.assertion = assertion


class Trial:
    """Assertion ledger, secret register, and LIFO cleanup list for one run."""

    def __init__(self) -> None:
        self.results: list[dict[str, str]] = []
        self.notes: list[str] = []
        self.cleanups: list[tuple[str, Callable[[], Any]]] = []
        self._secrets: list[str] = []
        self.receipt: dict[str, Any] | None = None
        self.stage = "start"
        self.manifest: dict[str, Any] = {"source_commit": dogfood.PINNED_COMMIT}

    def check(self, name: str, ok: bool, detail: str = "") -> None:
        self.results.append({"assertion": name, "outcome": "pass" if ok else "fail", "detail": self.redact(detail)})
        print(f"[{'ok' if ok else 'FAIL'}] {name}{(': ' + self.redact(detail)) if detail and not ok else ''}")
        if not ok:
            raise SmokeError(name)

    def secret(self, value: str) -> None:
        self._secrets.append(value)

    def redact(self, text: str) -> str:
        for value in self._secrets:
            text = text.replace(value, "[redacted]")
        return text

    def defer(self, label: str, action: Callable[[], Any]) -> None:
        self.cleanups.append((label, action))

    def run_cleanups(self) -> int:
        """Runs every deferred cleanup, newest first. Returns how many FAILED; a failed cleanup fails the trial."""
        failed = 0
        while self.cleanups:
            label, action = self.cleanups.pop()
            try:
                outcome = action()
                self.notes.append(f"cleanup: {label}: {self.redact(str(outcome if outcome is not None else 'done'))}")
            except Exception as error:  # noqa: BLE001 - every cleanup must get its turn
                failed += 1
                self.notes.append(f"cleanup FAILED: {label}: {type(error).__name__}")
        return failed


# ---------------------------------------------------------------------------
# Host seams: fixed argv, no shell, nothing inherited, stderr never surfaced
# ---------------------------------------------------------------------------


def _truncated_failure(binary: str, args: list[str], code: int) -> subprocess.CalledProcessError:
    """Same exception type dogfood's callers already handle, with the argv cut to the subcommand word.

    No rule text, no container ID, no body: the job log and the manifest see
    `docker compose exited 1`, never what the daemon printed.
    """
    return subprocess.CalledProcessError(code, [Path(binary).name, *(args[:1])])


def quiet_run_bound(binary: str, args: list[str], capture: bool) -> bytes:
    """`dogfood.run_bound` with stderr discarded. Installed by `main` so inherited stderr can never bypass redaction.

    Under build-only mode (`CAPTURE_BUILD_STDERR`), a failing command's stderr tail is kept in memory for `main`
    to print, redacted, to the job log. The raised exception still carries no stderr and a truncated argv.
    """
    keep = CAPTURE_BUILD_STDERR
    try:
        result = subprocess.run(
            [binary, *args], check=True, env=dogfood.docker_environment(), shell=False,
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE if capture else subprocess.DEVNULL,
            stderr=subprocess.PIPE if keep else subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError as error:
        if keep and error.stderr:
            tail = error.stderr[-BUILD_STDERR_TAIL_BYTES:].decode("utf-8", "replace")
            build_stderr_tails.append(f"--- {Path(binary).name} {' '.join(args[:1])} exited {error.returncode}; last {len(tail)} chars of stderr ---\n{tail}")
        raise _truncated_failure(binary, args, error.returncode) from None
    return result.stdout if capture else b""


def host_run(argv: list[str]) -> str:
    try:
        result = subprocess.run(
            argv, check=True, shell=False, env={"PATH": dogfood.SAFE_PATH, "LANG": "C.UTF-8"},
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
    except subprocess.CalledProcessError as error:
        raise _truncated_failure(argv[0], argv[1:], error.returncode) from None
    return result.stdout.decode("utf-8", "replace")


def runner_only_daemon(trial: Trial) -> None:
    """GITHUB_ACTIONS must be true; the daemon must not be LucentRoot; then dogfood's expectation is pointed at it."""
    trial.check("GITHUB_ACTIONS is true", os.environ.get("GITHUB_ACTIONS") == "true")
    trial.check("RUNNER_TEMP is set", bool(os.environ.get("RUNNER_TEMP")))
    info = json.loads(dogfood.run_docker(["info", "--format", "{{json .}}"], capture=True))
    observed = {key: info.get(key) for key in dogfood.EXPECTED_DAEMON}
    trial.check("daemon identity is readable", all(isinstance(v, str) and v for v in observed.values()), str(observed))
    trial.check("daemon is not LucentRoot", observed["ID"] != LUCENTROOT_ID and observed["Name"] != LUCENTROOT_NAME, str(observed))
    dogfood.EXPECTED_DAEMON.update(observed)  # in place: every dogfood verifier now expects this ephemeral runner
    trial.manifest["runner_daemon"] = dogfood.verify_daemon()


def iptables_with_sudo(trial: Trial, profile: dogfood.Profile) -> None:
    """Applies the exact printed gate, registers its exact inverse for cleanup, and makes the verifier accept it."""
    iptables = dogfood.locate(dogfood.IPTABLES_BIN_CANDIDATES, "iptables")

    def read_chain(chain: str) -> str:
        try:
            return host_run([SUDO, "-n", iptables, "-S", chain])
        except (subprocess.CalledProcessError, OSError) as error:
            raise dogfood.ProfileError(f"could not read iptables chain {chain}: {error}") from error

    dogfood.read_iptables = read_chain  # runner has no privilege without sudo; same read-only listing
    chains = list(dogfood.expected_iptables_rules(profile))
    before = dogfood.evaluate_iptables({c: read_chain(c) for c in chains}, profile)
    trial.check("gate is absent before this trial applies it", bool(before))
    apply = [line.split() for line in dogfood.enforcement_apply_commands(profile) if line.startswith("iptables -I ")]
    trial.check("the printed gate is exactly five rules", len(apply) == 5)
    applied: list[list[str]] = []

    def remove_applied() -> str:
        # `iptables -D <chain> <rule>` for each applied `iptables -I <chain> <n> <rule>`, newest first: the
        # same inverse `enforcement_remove_commands` prints, limited to what this run actually inserted.
        for parts in reversed(applied):
            host_run([SUDO, "-n", iptables, "-D", parts[2], *parts[4:]])
        return f"removed {len(applied)} rules"

    trial.defer("remove the applied iptables rules (exact inverse, no flush)", remove_applied)
    for parts in apply:
        host_run([SUDO, "-n", iptables, *parts[1:]])
        applied.append(parts)
    dogfood.preflight_enforcement(profile)  # the existing verifier, unchanged
    trial.check("existing verifier accepts the applied gate", True)
    trial.manifest["iptables"] = {c: read_chain(c) for c in chains}


# ---------------------------------------------------------------------------
# Build, render, save (the whole of the default mode)
# ---------------------------------------------------------------------------


def build_images(trial: Trial, profile: dogfood.Profile, work: Path) -> tuple[dogfood.Lock, str]:
    source = work / "src"
    dogfood.extract_pinned_source(source)  # `git archive` from the checkout; the only use of the working tree
    dogfood.REPO_ROOT = source  # the console-header comparison now reads the pinned nginx.conf, not the branch's
    match = NODE_BASE_PATTERN.search((source / "apps/control-plane-ui/Dockerfile").read_text(encoding="utf-8"))
    trial.check("probe helper base is the pinned node:22 digest from the console Dockerfile", match is not None)
    node_image = match.group(1)  # type: ignore[union-attr]  # recorded now; pulled only by the runtime path
    cp_id = dogfood.docker_build(source, source / "Dockerfile", "control-plane-api")
    console_id = dogfood.docker_build(source, source / "apps/control-plane-ui/Dockerfile", "console")
    dogfood.run_docker(["pull", profile.keycloak_image])
    digests = json.loads(dogfood.run_docker(["image", "inspect", "--format", "{{json .RepoDigests}}", profile.keycloak_image], capture=True))
    trial.check("pulled Keycloak reports the pinned digest", profile.keycloak_image in digests)
    kc_context = work / "keycloak"
    kc_context.mkdir(mode=0o700)
    for name, text in dogfood.keycloak_derived_context(profile).items():
        trial.check(f"derived context file {name} is allowlisted", name in dogfood.KC_CONTEXT_FILES)
        dogfood.write_regular_file(kc_context / name, text, 0o644, exclusive=True)
    kc_id = dogfood.docker_build(kc_context, kc_context / "Dockerfile", None)
    lock = dogfood.Lock(dogfood.PINNED_COMMIT, cp_id, console_id, profile.keycloak_image, kc_id, dict(dogfood.EXPECTED_DAEMON), False)
    trial.manifest["images"] = {s: lock.image_for(s) for s in dogfood.CONTAINER_NAMES}
    trial.manifest["keycloak_base_image"] = profile.keycloak_image
    trial.manifest["probe_helper_image"] = node_image
    return lock, node_image


def save_images(lock: dogfood.Lock, artifact_dir: Path) -> None:
    """Image archives only: `docker save <id>` then gzip. No container, no volume, no credential is in an image."""
    for service in dogfood.CONTAINER_NAMES:
        raw = artifact_dir / f"{service}.tar"
        dogfood.run_docker(["save", "--output", str(raw), lock.image_for(service)])
        with open(raw, "rb") as source, gzip.open(artifact_dir / f"{service}-image.tar.gz", "wb", compresslevel=6) as target:
            shutil.copyfileobj(source, target)
        raw.unlink()


def render_profile(trial: Trial, profile: dogfood.Profile, lock: dogfood.Lock, work: Path) -> dogfood.Paths:
    profile_dir = work / "profile"
    profile_dir.mkdir(mode=0o700)
    dogfood.write_regular_file(profile_dir / "profile.toml", (HERE / "profile.toml").read_text(encoding="utf-8"), 0o644, exclusive=True)
    paths = dogfood.Paths(profile_dir)
    rendered = dogfood.render(profile, lock)
    findings = dogfood.validate_rendered(rendered, profile, lock)
    trial.check("rendered artefacts validate with genuine image IDs", not findings, "; ".join(findings))
    dogfood.write_rendered(paths, rendered)
    trial.check("written artefacts show no drift", not dogfood.drift_findings(paths, rendered))
    return paths


def build_only(trial: Trial, artifact_dir: Path, work: Path) -> tuple[dogfood.Profile, dogfood.Lock, str, dogfood.Paths]:
    """Everything the default mode does after the daemon check. Nothing here starts, binds, applies or signs in."""
    trial.stage = "build"
    profile = dogfood.load_profile(HERE / "profile.toml")
    lock, node_image = build_images(trial, profile, work)
    trial.stage = "save"
    save_images(lock, artifact_dir)
    trial.stage = "render"
    paths = render_profile(trial, profile, lock, work)
    return profile, lock, node_image, paths


# ---------------------------------------------------------------------------
# Runtime path (--runtime only): activate, bootstrap, readiness
# ---------------------------------------------------------------------------


def runtime_posture_findings(document: dict[str, Any], service: str) -> list[str]:
    """What the daemon says is actually running: read-only root, no capabilities, no socket, no shared service."""
    host, config, findings = document.get("HostConfig") or {}, document.get("Config") or {}, []
    if not host.get("ReadonlyRootfs"):
        findings.append("root filesystem is writable")
    if host.get("Privileged") or host.get("CapAdd"):
        findings.append("privileged or capabilities added")
    if host.get("CapDrop") != ["ALL"]:
        findings.append(f"cap_drop is {host.get('CapDrop')}")
    if "no-new-privileges:true" not in (host.get("SecurityOpt") or []):
        findings.append("no-new-privileges missing")
    if host.get("NetworkMode") != dogfood.NETWORK_NAME:
        findings.append(f"network mode {host.get('NetworkMode')}")
    for mount in document.get("Mounts") or []:
        location = f"{mount.get('Source', '')}->{mount.get('Destination', '')}"
        if "docker.sock" in location or mount.get("Type") not in ("bind", "tmpfs"):
            findings.append(f"mount refused: {mount.get('Type')} {location}")
        elif mount.get("Type") == "bind" and (mount.get("RW") or not str(mount.get("Source", "")).endswith((dogfood.CP_CONFIG_NAME, dogfood.CONSOLE_CONF_NAME))):
            findings.append(f"bind mount refused: {location}")
    if set(host.get("Tmpfs") or {}) != set(dogfood.TMPFS_SPECS[service]):
        findings.append(f"tmpfs set differs: {sorted(host.get('Tmpfs') or {})}")
    for entry in config.get("Env") or []:  # keys only are ever reported; values never leave this function
        key = entry.split("=", 1)[0]
        if service == dogfood.SERVICE_CP and entry == CP_IMAGE_CONFIG_ENV:
            continue  # the pinned control-plane Dockerfile bakes exactly this ENV; the same key with any other path is still refused
        if key.startswith(dogfood.FORBIDDEN_ENV_PREFIXES + ("VAULT_", "BAO_", "OPENBAO_", "GIT_", "GITHUB_")):
            findings.append(f"environment key {key}")
        if any(word in entry.lower() for word in ("openbao", "vault", "gitea", "gitlab", "github.com")):
            findings.append(f"environment key {key} names a shared service")
    return findings


def activate(trial: Trial, profile: dogfood.Profile, lock: dogfood.Lock, paths: dogfood.Paths) -> tuple[dict[str, dict[str, Any]], dict[str, Any], str, str]:
    collisions = dogfood.collision_findings(profile)
    trial.check("no pre-existing container or network with this profile's names", not collisions, "; ".join(collisions))
    admin_user, admin_password = f"bootstrap-{secrets.token_hex(3)}", secrets.token_urlsafe(32)
    trial.secret(admin_password)
    paths.ephemeral.mkdir(mode=0o700)
    dogfood.write_regular_file(paths.ephemeral_env, f"KC_BOOTSTRAP_ADMIN_USERNAME={admin_user}\nKC_BOOTSTRAP_ADMIN_PASSWORD={admin_password}\n", 0o600, exclusive=True)
    trial.defer("delete the ephemeral bootstrap credential file", lambda: paths.ephemeral_env.unlink(missing_ok=True))

    def remove_resources() -> list[str]:
        if trial.receipt is not None:
            return dogfood.remove_recorded(trial.receipt, profile, lock)  # exact recorded IDs, re-verified
        return dogfood.remove_owned_by_name(profile, lock)  # partial start: only provably owned

    trial.defer("remove the trial's containers and network by recorded ID", remove_resources)
    dogfood.run_docker([*dogfood.compose_args(paths, profile), "up", "--detach", "--no-build", "--pull", "never"])
    observed: dict[str, dict[str, Any]] = {}
    findings: list[str] = []
    for service, name in dogfood.CONTAINER_NAMES.items():
        document = dogfood.docker_inspect("container", name)
        findings += dogfood.evaluate_owned_container(document, service, profile, lock)
        if document is not None:
            observed[service] = document
            findings += [f"{service}: {f}" for f in runtime_posture_findings(document, service)]
    network = dogfood.docker_inspect("network", dogfood.NETWORK_NAME)
    findings += dogfood.evaluate_owned_network(network, profile)
    trial.check("started containers and network are this profile's own, with the declared runtime posture", not findings, "; ".join(findings))
    trial.receipt = {
        "project_name": profile.project_name,
        "daemon": dict(dogfood.EXPECTED_DAEMON),
        "compose_sha256": dogfood.sha256_text((paths.out / dogfood.COMPOSE_NAME).read_text(encoding="utf-8")),
        "containers": {s: {"id": d["Id"], "name": dogfood.CONTAINER_NAMES[s], "image": d["Image"]} for s, d in observed.items()},
        "network": {"id": network["Id"], "name": dogfood.NETWORK_NAME},  # type: ignore[index]
    }
    trial.check("receipt parses as a real activation", dogfood.parse_receipt(dict(trial.receipt), profile, lock, (paths.out / dogfood.COMPOSE_NAME).read_text(encoding="utf-8")) == trial.receipt)
    return observed, network, admin_user, admin_password  # type: ignore[return-value]


def bootstrap_and_ready(trial: Trial, profile: dogfood.Profile, lock: dogfood.Lock, observed: dict[str, dict[str, Any]], admin_user: str, admin_password: str) -> tuple[str, str]:
    port_findings = dogfood.evaluate_published_port(observed[dogfood.SERVICE_KEYCLOAK], dogfood.KEYCLOAK_PORT, profile.bind_address, profile.oidc_port)
    trial.check("owned Keycloak publishes 8080 on the loopback port only", not port_findings, "; ".join(port_findings))
    base = profile.keycloak_public_base
    dogfood.wait_for("Keycloak master realm (read-only root, seeded quarkus tmpfs)", lambda: True if dogfood.http_json(f"{base}/realms/master", profile)[0] == 200 else None, attempts=100, delay=3.0)
    trial.check("derived Keycloak started under a read-only root", True)
    operator, password = dogfood.bootstrap_realm(profile, admin_user, admin_password)
    trial.secret(password)

    # The control plane re-reads the realm's signing keys every jwks_refresh_seconds (pinned to 5 by the
    # profile and its validator). Once the realm's key set is served, one refresh interval is enough for the
    # control plane's next read to see it. No restart, no shortcut: the keys are read from the issuer as always.
    dogfood.wait_for("the bootstrapped realm serves its signing keys", lambda: True if dogfood.http_json(f"{base}/realms/{profile.realm}/protocol/openid-connect/certs", profile)[0] == 200 else None, attempts=20, delay=1.0)
    time.sleep(dogfood.JWKS_REFRESH_SECONDS + 1)
    trial.notes.append(f"waited one jwks_refresh_seconds interval ({dogfood.JWKS_REFRESH_SECONDS}s) after realm bootstrap; no container was restarted")

    origin = profile.console_origin
    dogfood.wait_for("console /healthz and /api/session through the proxy", lambda: True if dogfood.http_json(f"{origin}/healthz", profile)[0] == 200 and dogfood.http_json(f"{origin}/api/session", profile)[0] == 200 else None, attempts=40, delay=3.0)
    _, session, _ = dogfood.http_json(f"{origin}/api/session", profile)
    findings = dogfood.evaluate_session_config(session, profile)
    trial.check("session config sends the browser to the published issuer with the console client and redirect", not findings, "; ".join(findings))
    status, discovery, _ = dogfood.http_json(f"{profile.issuer}/.well-known/openid-configuration", profile)
    findings = dogfood.evaluate_discovery(discovery, profile) if status == 200 else [f"discovery answered {status}"]
    trial.check("issuer discovery carries the configured issuer and S256", not findings, "; ".join(findings))
    for route in ("/api/catalogue", "/api/operator"):
        status, _, _ = dogfood.http_json(f"{origin}{route}", profile)
        trial.check(f"anonymous GET {route} answers 401", status == 401, f"status {status}")
    return operator, password


# ---------------------------------------------------------------------------
# Real OIDC through the real console, then the catalogue as that operator
# ---------------------------------------------------------------------------


def sign_in_through_console(trial: Trial, profile: dogfood.Profile, operator: str, password: str) -> str:
    """Drives the real sign-in. Reports by named assertion only: no URL (an auth code travels in one), no Playwright message."""
    from playwright.sync_api import Error as PlaywrightError  # pinned, installed only in the runner's temporary venv
    from playwright.sync_api import sync_playwright

    origin = profile.console_origin
    authorization = dogfood.derive_endpoints(profile.issuer, profile.reachable_at)["authorization"]
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        try:
            page = browser.new_context().new_page()
            page.set_default_timeout(60_000)
            page.goto(f"{origin}/")
            # The console first tries a silent sign-in (prompt=none); Keycloak sends it back with login_required and
            # the button appears. A navigation can land mid-click, so the click is retried a few times.
            button = page.get_by_role("button", name="Sign in")
            clicked = False
            for _ in range(3):
                try:
                    button.wait_for(state="visible")
                    button.click()
                    clicked = True
                    break
                except PlaywrightError:
                    continue
            trial.check("the console's Sign in button was clicked", clicked)
            try:
                page.wait_for_url(lambda url: url.startswith(authorization))
            except PlaywrightError:
                trial.check("browser was sent to the issuer's authorization endpoint", False)
            auth_url = page.url  # inspected in memory only; never printed, never stored
            trial.check("browser was sent to the issuer's authorization endpoint with PKCE S256 and the console client",
                        "code_challenge_method=S256" in auth_url and f"client_id={dogfood.CONSOLE_CLIENT_ID}" in auth_url and "response_type=code" in auth_url)
            page.fill("#username", operator)
            page.fill("#password", password)
            is_redeem = lambda r: r.url == f"{origin}/api/session" and r.request.method == "POST"  # noqa: E731
            is_catalogue = lambda r: r.url == f"{origin}/api/catalogue" and r.request.method == "GET"  # noqa: E731
            try:
                with page.expect_response(is_catalogue) as loaded, page.expect_response(is_redeem) as redeemed:
                    page.click("#kc-login")
            except PlaywrightError:
                trial.check("console redeemed the code and loaded the catalogue within the timeout", False)
            trial.check("console redeemed the authorization code at POST /api/session", redeemed.value.status == 200, f"status {redeemed.value.status}")
            token = redeemed.value.json().get("access_token")
            trial.check("redemption returned an access token", isinstance(token, str) and bool(token))
            trial.secret(token)
            trial.check("browser returned to the console origin", page.url.startswith(f"{origin}/"))
            first = loaded.value.status
            if first == 200:
                trial.check("signed-in console read the catalogue through the proxy with that token", True)
            else:
                # Only a 401 is temporary: the control plane may not have re-read the realm's keys yet. The same real
                # token is presented again, for a bounded time; any other status is a failure, not a retry.
                trial.check("first authenticated catalogue read answered 200 or a temporary 401", first == 401, f"status {first}")
                verdict, last = poll_token_acceptance(lambda: dogfood.http_json(f"{origin}/api/operator", profile, headers={"Authorization": f"Bearer {token}"})[0])
                trial.check("GET /api/operator accepted the same token within the JWKS refresh bound", verdict == "accepted", f"{verdict} with status {last}")
                # The browser's own retry: reload the real console and let its normal silent sign-in run again. The
                # catalogue read that follows must be a real 200 from the console with whatever token it holds.
                try:
                    with page.expect_response(is_catalogue) as reloaded:
                        page.reload()
                    again = reloaded.value.status
                except PlaywrightError:
                    again = None
                trial.check("after reload the console read the catalogue through the proxy", again == 200, f"status {again}")
        finally:
            browser.close()
    return token


def poll_token_acceptance(fetch_status: Callable[[], int], sleep: Callable[[float], None] = time.sleep,
                          attempts: int = TOKEN_ACCEPTANCE_ATTEMPTS, delay: float = TOKEN_ACCEPTANCE_DELAY) -> tuple[str, int]:
    """Presents one already-issued token until it is accepted, for at most attempts * delay seconds.

    Returns ("accepted", 200) on a 200; ("rejected", status) at once on anything other than 200 or 401;
    ("timed out", 401) when every attempt was 401. Nothing here mints, alters or substitutes a credential.
    """
    last = 401
    for attempt in range(attempts):
        last = fetch_status()
        if last == 200:
            return "accepted", last
        if last != 401:
            return "rejected", last
        if attempt + 1 < attempts:
            sleep(delay)
    return "timed out", last


def exercise_catalogue(trial: Trial, profile: dogfood.Profile, token: str) -> None:
    origin = profile.console_origin
    auth = {"Authorization": f"Bearer {token}"}
    # Acceptance of the token was already established (or waited for, bounded) inside sign_in_through_console.
    status, who, _ = dogfood.http_json(f"{origin}/api/operator", profile, headers=auth)
    trial.check("GET /api/operator answers 200 for the signed-in operator", status == 200 and isinstance(who, dict) and isinstance(who.get("subject"), str), f"status {status}")
    status, stored, _ = dogfood.http_json(f"{origin}/api/catalogue", profile, headers=auth)
    trial.check("operator can read the catalogue", status == 200 and isinstance(stored, dict), f"status {status}")
    revision = stored.get("revision")

    def command(body: dict[str, Any], revision: str | None) -> dict[str, Any]:
        condition = {"If-None-Match": "*"} if revision is None else {"If-Match": f'"{revision}"'}
        status, result, _ = dogfood.http_json(f"{origin}/api/catalogue", profile, json.dumps(body).encode(), {**auth, "Content-Type": "application/json", **condition}, "POST")
        trial.check(f"catalogue command {body['action']} answers 200", status == 200 and isinstance(result, dict), f"status {status}")
        return result

    app_id = f"trial-{secrets.token_hex(2)}"
    stored = command({"action": "createApplication", "id": app_id, "name": "Dogfood trial"}, revision)
    app = next((a for a in stored["catalogue"]["applications"] if a.get("id") == app_id), None)
    trial.check("created application appears as a draft", app is not None and isinstance(app.get("draft"), dict))
    draft = dict(app["draft"])  # type: ignore[index]
    draft["description"] = "edited by the disposable dogfood trial"
    stored = command({"action": "saveApplication", "id": app_id, "definition": draft}, stored["revision"])
    status, again, _ = dogfood.http_json(f"{origin}/api/catalogue", profile, headers=auth)
    app = next((a for a in again["catalogue"]["applications"] if a.get("id") == app_id), None)
    trial.check("edited draft persisted on the control plane's state tmpfs", status == 200 and app is not None and app["draft"].get("description") == draft["description"])
    trial.check("nothing was published: no release exists", all(a.get("releases") == [] for a in again["catalogue"]["applications"]))
    status, clients, _ = dogfood.http_json(f"{origin}/api/clients", profile, headers=auth)
    trial.check("nothing was assigned: no client exists", status == 200 and isinstance(clients, dict) and clients.get("clients") == [], f"status {status}")


# ---------------------------------------------------------------------------
# Connectivity probes: positive first, then negatives with positive controls
# ---------------------------------------------------------------------------


def probe(network: str, host: str, port: int, node_image: str, path: str | None = None) -> bool:
    """A transient, read-only, unprivileged node container on `network`.

    True when the connect (or GET) succeeds; False only when the probe itself
    reports refused/unreachable (1) or timed out (2). Any other failure (the
    daemon refusing to start the container, a missing image, exit 125, a
    crashed node) is a broken probe, not a blocked network, and raises.
    """
    argv = ["run", "--rm", "--name", PROBE_NAME, "--network", network, "--pull", "never", "--read-only", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges:true", "--user", "1000:1000", "--env", f"PROBE_HOST={host}", "--env", f"PROBE_PORT={port}"]
    if path:
        argv += ["--env", f"PROBE_PATH={path}"]
    try:
        dogfood.run_docker([*argv, node_image, "node", "-e", PROBE_JS], capture=True)
        return True
    except subprocess.CalledProcessError as error:
        if error.returncode in PROBE_NEGATIVE_EXITS:
            return False
        raise SmokeError(f"probe container did not run (docker exit {error.returncode}); this is not evidence of a blocked network") from None


def connectivity(trial: Trial, profile: dogfood.Profile, network: dict[str, Any], node_image: str) -> None:
    def remove_lingering_probe() -> str:
        document = dogfood.docker_inspect("container", PROBE_NAME)
        if document is not None and (document.get("Config") or {}).get("Image") == node_image:
            dogfood.run_docker(["rm", "--force", document["Id"]])
            return f"removed {document['Id'][:12]}"
        return "none lingering"

    trial.defer("remove a lingering probe container", remove_lingering_probe)
    trial.check("positive: probe on the trial network reaches console /healthz", probe(dogfood.NETWORK_NAME, dogfood.SERVICE_CONSOLE, dogfood.CONSOLE_PORT, node_image, "/healthz"))
    trial.check("positive: probe on the trial network reaches cp /api/session", probe(dogfood.NETWORK_NAME, dogfood.SERVICE_CP, dogfood.CP_PORT, node_image, "/api/session"))

    ipam = (network.get("IPAM") or {}).get("Config") or [{}]
    gateway = ipam[0].get("Gateway") or str(ipaddress.ip_network(profile.subnet)[1])
    listener = socket.socket()
    listener.bind((gateway, 0))
    listener.listen(4)
    trial.defer("close the synthetic host listener on the bridge gateway", listener.close)
    host_port = listener.getsockname()[1]
    with socket.create_connection((gateway, host_port), timeout=5):
        pass
    trial.check("control: the host reaches its own synthetic listener on the bridge gateway", True)
    trial.check("negative: trial network cannot reach the synthetic host listener on the bridge gateway", not probe(dogfood.NETWORK_NAME, gateway, host_port, node_image))

    peer_network_id = dogfood.run_docker(["network", "create", "--driver", "bridge", PEER_NETWORK], capture=True)
    trial.defer("remove the temporary peer network by ID", lambda: dogfood.run_docker(["network", "rm", peer_network_id]))
    peer_id = dogfood.run_docker(["run", "--detach", "--name", PEER_NAME, "--network", PEER_NETWORK, "--pull", "never", "--read-only", "--cap-drop", "ALL",
                                  "--security-opt", "no-new-privileges:true", "--user", "1000:1000", "--env", f"LISTEN_PORT={PEER_PORT}", node_image, "node", "-e", LISTEN_JS], capture=True)
    trial.defer("remove the synthetic peer container by ID", lambda: dogfood.run_docker(["rm", "--force", peer_id]))
    peer = dogfood.docker_inspect("container", peer_id) or {}
    peer_ip = ((peer.get("NetworkSettings") or {}).get("Networks") or {}).get(PEER_NETWORK, {}).get("IPAddress")
    trial.check("synthetic peer has an address on its own bridge", bool(peer_ip))
    trial.check("control: a probe on the peer's bridge reaches the peer", probe(PEER_NETWORK, peer_ip, PEER_PORT, node_image))
    trial.check("negative: trial network cannot reach the synthetic peer on another bridge", not probe(dogfood.NETWORK_NAME, peer_ip, PEER_PORT, node_image))
    trial.check("negative: trial network cannot reach a public address (1.1.1.1:53)", not probe(dogfood.NETWORK_NAME, "1.1.1.1", 53, node_image))
    trial.notes.append("scope: DNS resolution from the trial network was not probed; no real shared service was contacted; the negatives were observed with the gate applied and Docker's own internal-network and bridge-isolation rules in place -- the trial did not isolate which of those blocked each probe")

    inspected = dogfood.docker_inspect("network", dogfood.NETWORK_NAME)
    findings = dogfood.evaluate_network_inspect(inspected, profile) if inspected else ["network missing"]
    trial.check("after probes: network is internal, IPv6 off, fixed bridge and subnet, only the three containers attached", not findings, "; ".join(findings))
    trial.manifest["network"] = {k: inspected.get(k) for k in ("Name", "Internal", "EnableIPv6", "Options")} | {"attached": sorted(c.get("Name") for c in (inspected.get("Containers") or {}).values())}  # type: ignore[union-attr]


def runtime_trial(trial: Trial, profile: dogfood.Profile, lock: dogfood.Lock, node_image: str, paths: dogfood.Paths) -> None:
    """The --runtime path only. Looked up by name so the offline tests can replace every stage with a tripwire."""
    stages = {name: globals()[name] for name in RUNTIME_STAGES}
    trial.stage = "pull probe helper"
    dogfood.run_docker(["pull", node_image])  # runner egress, before any trial network exists
    trial.stage = "iptables"
    stages["iptables_with_sudo"](trial, profile)
    trial.stage = "activate"
    observed, network, admin_user, admin_password = stages["activate"](trial, profile, lock, paths)
    trial.stage = "bootstrap"
    operator, password = stages["bootstrap_and_ready"](trial, profile, lock, observed, admin_user, admin_password)
    trial.stage = "sign-in"
    token = stages["sign_in_through_console"](trial, profile, operator, password)
    trial.stage = "catalogue"
    stages["exercise_catalogue"](trial, profile, token)
    trial.stage = "connectivity"
    stages["connectivity"](trial, profile, network, node_image)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--artifact-dir", required=True, help="where image archives and the redacted manifest go; nothing else is written there")
    parser.add_argument("--runtime", action="store_true", help="ALSO apply the gate, start containers, bootstrap, sign in and probe (separately approved runs only)")
    args = parser.parse_args(argv)
    artifact_dir = Path(args.artifact_dir).resolve()
    artifact_dir.mkdir(parents=True, exist_ok=True)
    dogfood.run_bound = quiet_run_bound  # every docker/git call from here on: stderr discarded, argv truncated on failure
    global CAPTURE_BUILD_STDERR
    CAPTURE_BUILD_STDERR = not args.runtime  # build-only has no generated credential: keep build diagnostics; runtime stays strict
    build_stderr_tails.clear()
    trial = Trial()
    trial.manifest["mode"] = "runtime" if args.runtime else "build-only"
    outcome, work = 1, None
    try:
        trial.stage = "daemon"
        runner_only_daemon(trial)
        work = Path(tempfile.mkdtemp(prefix="dogfood-ci-", dir=os.environ["RUNNER_TEMP"]))
        profile, lock, node_image, paths = build_only(trial, artifact_dir, work)
        if args.runtime:
            runtime_trial(trial, profile, lock, node_image, paths)
        outcome = 0
    except SmokeError as error:
        trial.manifest["failure"] = f"assertion failed: {error.assertion}"
    except Exception as error:  # noqa: BLE001 - class name and stage only: no message, URL, body or Playwright text ever reaches the log
        trial.manifest["failure"] = f"{type(error).__name__} during {trial.stage}"
    finally:
        failed_cleanups = trial.run_cleanups()
        if failed_cleanups:
            outcome = 1
            trial.manifest["cleanup_failures"] = failed_cleanups
        if work is not None:
            dogfood._remove_private_dir(work)  # noqa: SLF001 - our own mkdtemp under RUNNER_TEMP
        trial.manifest.update({"outcome": "pass" if outcome == 0 else "fail", "assertions": trial.results, "notes": [trial.redact(n) for n in trial.notes]})
        manifest = trial.redact(json.dumps(trial.manifest, indent=2, sort_keys=True)) + "\n"
        dogfood.write_regular_file(artifact_dir / "result-manifest.json", manifest, 0o644, exclusive=False)
        print(manifest)
        if "failure" in trial.manifest:
            print(f"trial failed: {trial.manifest['failure']}", file=sys.stderr)
            if not args.runtime and build_stderr_tails:  # job log only; never written under the artifact directory
                print("build diagnostics (bounded stderr tail, redacted):", file=sys.stderr)
                for tail in build_stderr_tails:
                    print(trial.redact(tail), file=sys.stderr)
        CAPTURE_BUILD_STDERR = False
    return outcome


if __name__ == "__main__":
    sys.exit(main())
