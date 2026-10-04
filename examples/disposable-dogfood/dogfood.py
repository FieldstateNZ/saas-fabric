#!/usr/bin/env python3
"""Disposable internal dogfood profile for the SaaS Fabric control plane.

One file: inputs are allowlisted and validated, every artefact is rendered
from a template, every rendered artefact is validated again as the thing
Docker will actually read, and the only commands that touch a Docker daemon
require `--yes`, verify the exact daemon they expect, and are never the default.

    python3 dogfood.py                 # prepare + check (no Docker calls)
    python3 dogfood.py check           # validate what is on disk
    python3 dogfood.py enforcement     # print (never apply) the per-sandbox firewall gate and its exact inverse
    python3 dogfood.py build --yes     # git-archive the pinned commit; docker build; pull Keycloak by digest; derive it
    python3 dogfood.py activate --yes  # fail closed unless the firewall gate is observed; prompt for the operator password; compose up; bootstrap
    python3 dogfood.py status          # loopback HTTP readiness evidence
    python3 dogfood.py reset --yes     # remove ONLY the containers and network recorded by activation

Python 3.11 or newer (tomllib). No third-party packages.

Intended execution host: the Docker daemon recorded in EXPECTED_DAEMON
(LucentRoot), through its local unix socket only, after separately approved
staging. This file has not been executed against any daemon by its author.
"""

from __future__ import annotations

import argparse
import atexit
import hashlib
import io
import ipaddress
import json
import os
import re
import secrets
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import termios
import time
import urllib.error
import urllib.parse
import urllib.request
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - reported at runtime instead
    tomllib = None  # type: ignore[assignment]

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent
TEMPLATES = HERE / "templates"

# The source commit this profile's build recipes are locked to. `build` reads
# the tree through `git archive <commit>` into a private temporary directory,
# never through the working tree, so modified or untracked files (including
# anything under .out/) can never enter a build context.
PINNED_COMMIT = "1f46788ecd5c59af1a4937aae0e51cf59b8f8081"

# The only Docker daemon this launcher may talk to, as observed READ-ONLY
# (`docker info`) before this file was written. Every Docker-touching command
# verifies all three fields first and refuses any other daemon.
DOCKER_HOST = "unix:///var/run/docker.sock"
EXPECTED_DAEMON = {"ID": "47e4864c-619b-4190-8ef5-65d1feceb155", "Name": "lucentroot", "ServerVersion": "29.5.3"}

# Bound command endpoints: fixed absolute candidates, first existing wins. No
# PATH lookup, no shell, ever.
DOCKER_BIN_CANDIDATES = ("/usr/bin/docker", "/usr/local/bin/docker")
IPTABLES_BIN_CANDIDATES = ("/usr/sbin/iptables", "/sbin/iptables")
GIT_BIN_CANDIDATES = ("/usr/bin/git", "/usr/local/bin/git")
SAFE_PATH = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"

OUT_DIR_NAME = ".out"
EPHEMERAL_DIR_NAME = ".ephemeral"
EPHEMERAL_ENV_NAME = "keycloak.env"
LOCK_NAME = "pins.lock.json"
RECEIPT_NAME = "ownership.json"
COMPOSE_NAME = "compose.json"
CP_CONFIG_NAME = "control-plane.toml"
CONSOLE_CONF_NAME = "console.nginx.conf"
RENDERED_NAMES = (CP_CONFIG_NAME, CONSOLE_CONF_NAME, COMPOSE_NAME, LOCK_NAME)
KC_CONTEXT_FILES = ("Dockerfile", "kc-entrypoint.sh")  # the ONLY files ever added to a build context

SERVICE_CP = "cp"
SERVICE_CONSOLE = "console"
SERVICE_KEYCLOAK = "keycloak"
CONTAINER_NAMES = {
    SERVICE_CP: "fabric-dogfood-cp",
    SERVICE_CONSOLE: "fabric-dogfood-console",
    SERVICE_KEYCLOAK: "fabric-dogfood-keycloak",
}
NETWORK_NAME = "fabric-dogfood-internal"
BRIDGE_NAME = "fabric-dogfood0"  # 15 characters: the Linux interface-name limit.
COMPOSE_PROJECT_LABEL = "com.docker.compose.project"

CP_PORT = 8081
CONSOLE_PORT = 8080
KEYCLOAK_PORT = 8080
CP_CONFIG_PATH = "/etc/fabric/control-plane.toml"
CP_STATE_PATH = "/var/lib/fabric/state"
CONSOLE_CONF_PATH = "/etc/nginx/conf.d/default.conf"
KC_QUARKUS_PATH = "/opt/keycloak/lib/quarkus"
KC_QUARKUS_SEED_PATH = "/opt/keycloak/lib/quarkus.seed"
KC_DATA_PATH = "/opt/keycloak/data"
KC_ENTRYPOINT_PATH = "/opt/keycloak/bin/dogfood-entrypoint.sh"
CONSOLE_CLIENT_ID = "saas-fabric-console"
OPERATOR_ROLE = "fabric-operator"
CP_UPSTREAM = f"http://{SERVICE_CP}:{CP_PORT}"

# Container users, from the images' own Dockerfiles: distroless `nonroot`,
# nginx-unprivileged's 101, Keycloak's 1000.
USERS = {SERVICE_CP: "65532:65532", SERVICE_CONSOLE: "101:101", SERVICE_KEYCLOAK: "1000:1000"}

MIB = 1024 * 1024
TMPFS_CAP_BYTES = 256 * MIB
# Every writable path, per service, with its byte cap. Nothing else is writable.
TMPFS_SPECS: dict[str, dict[str, int]] = {
    SERVICE_CP: {CP_STATE_PATH: 16 * MIB, "/tmp": 16 * MIB},
    SERVICE_CONSOLE: {"/tmp": 32 * MIB},
    SERVICE_KEYCLOAK: {KC_DATA_PATH: 64 * MIB, KC_QUARKUS_PATH: 256 * MIB, "/tmp": 64 * MIB},
}
# The JVM may extract native libraries into /tmp and map them executable; the
# other two images never execute anything from a writable path.
TMPFS_EXEC_ALLOWED = {SERVICE_CP: False, SERVICE_CONSOLE: False, SERVICE_KEYCLOAK: True}
LOGGING = {"driver": "json-file", "options": {"max-size": "5m", "max-file": "2"}}

ALLOWED_BIND_SOURCES = {f"./{CP_CONFIG_NAME}", f"./{CONSOLE_CONF_NAME}"}
FORBIDDEN_SERVICE_KEYS = {
    "privileged", "cap_add", "network_mode", "pid", "ipc", "uts", "userns_mode", "devices",
    "sysctls", "extra_hosts", "dns", "secrets", "configs", "volumes_from", "links", "build",
    "cgroup_parent", "device_cgroup_rules", "group_add", "init", "platform", "runtime", "shm_size",
    "storage_opt", "profiles", "external_links", "healthcheck", "entrypoint",
}
FORBIDDEN_ENV_PREFIXES = ("FABRIC_",)
KEYCLOAK_IMAGE_PATTERN = re.compile(r"^quay\.io/keycloak/keycloak@sha256:[0-9a-f]{64}$")
IMAGE_ID_PATTERN = re.compile(r"^sha256:[0-9a-f]{64}$")
SYNTHETIC_IMAGE_ID_PATTERN = re.compile(r"^sha256:0{63}[1-9]$")
CONTAINER_ID_PATTERN = re.compile(r"^[0-9a-f]{64}$")
COMMIT_PATTERN = re.compile(r"^[0-9a-f]{40}$")
REALM_PATTERN = re.compile(r"^[a-z][a-z0-9-]{2,31}$")
PROJECT_PATTERN = re.compile(r"^[a-z][a-z0-9-]{2,40}$")
RESERVED_REALMS = {"master"}

PROFILE_KEYS = {
    "project_name", "bind_address", "console_port", "oidc_port", "realm", "subnet", "keycloak_image",
    "source_commit",
}


class ProfileError(Exception):
    """The inputs are not acceptable; the message lists every reason."""


# ---------------------------------------------------------------------------
# Inputs
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class Profile:
    project_name: str
    bind_address: str
    console_port: int
    oidc_port: int
    realm: str
    subnet: str
    keycloak_image: str
    source_commit: str

    @property
    def console_origin(self) -> str:
        return f"http://{self.bind_address}:{self.console_port}"

    @property
    def redirect_uri(self) -> str:
        return f"{self.console_origin}/"

    @property
    def keycloak_public_base(self) -> str:
        return f"http://{self.bind_address}:{self.oidc_port}"

    @property
    def issuer(self) -> str:
        return f"{self.keycloak_public_base}/realms/{self.realm}"

    @property
    def reachable_at(self) -> str:
        return f"http://{SERVICE_KEYCLOAK}:{KEYCLOAK_PORT}/realms/{self.realm}"


def parse_profile(data: dict[str, Any]) -> Profile:
    """Validates the allowlisted inputs; every problem is reported at once."""
    problems: list[str] = []
    unknown = sorted(set(data) - PROFILE_KEYS)
    if unknown:
        problems.append(f"unknown keys: {', '.join(unknown)}")
    missing = sorted(PROFILE_KEYS - set(data))
    if missing:
        problems.append(f"missing keys: {', '.join(missing)}")
    if problems:
        raise ProfileError("; ".join(problems))

    def text(key: str) -> str:
        value = data[key]
        if not isinstance(value, str):
            problems.append(f"{key} must be a string")
            return ""
        return value.strip()

    def port(key: str) -> int:
        value = data[key]
        if isinstance(value, bool) or not isinstance(value, int) or not 1024 <= value <= 65535:
            problems.append(f"{key} must be an integer high port (1024-65535)")
            return 0
        return value

    project_name = text("project_name")
    if not PROJECT_PATTERN.match(project_name):
        problems.append("project_name must be lowercase letters, digits and hyphens")
    bind_address = text("bind_address")
    if bind_address != "127.0.0.1":
        problems.append("bind_address must be exactly 127.0.0.1; nothing is published elsewhere")
    console_port = port("console_port")
    oidc_port = port("oidc_port")
    if console_port and console_port == oidc_port:
        problems.append("console_port and oidc_port must differ")
    realm = text("realm")
    if not REALM_PATTERN.match(realm) or realm in RESERVED_REALMS:
        problems.append("realm must be a short lowercase name and never 'master'")
    subnet = text("subnet")
    try:
        network = ipaddress.ip_network(subnet, strict=True)
        if network.version != 4 or not network.is_private or network.prefixlen != 24:
            problems.append("subnet must be a private IPv4 /24")
    except ValueError:
        problems.append("subnet must be a CIDR such as 10.213.7.0/24")
    keycloak_image = text("keycloak_image")
    if not KEYCLOAK_IMAGE_PATTERN.match(keycloak_image):
        problems.append("keycloak_image must be quay.io/keycloak/keycloak@sha256:<64 hex>; tags are refused")
    source_commit = text("source_commit")
    if source_commit != PINNED_COMMIT:
        problems.append(f"source_commit must be the pinned commit {PINNED_COMMIT}")

    if problems:
        raise ProfileError("; ".join(problems))
    return Profile(project_name, bind_address, console_port, oidc_port, realm, subnet, keycloak_image, source_commit)


def load_profile(path: Path) -> Profile:
    if tomllib is None:
        raise ProfileError("Python 3.11 or newer is required (tomllib)")
    if path.is_symlink() or not path.is_file():
        raise ProfileError(f"{path} is not a regular file")
    try:
        data = tomllib.loads(path.read_text(encoding="utf-8"))
    except (tomllib.TOMLDecodeError, UnicodeDecodeError) as error:
        raise ProfileError(f"{path} does not parse: {error}") from error
    return parse_profile(data)


# ---------------------------------------------------------------------------
# Lock file: what `build` recorded
# ---------------------------------------------------------------------------


LOCK_KEYS = {"source_commit", "cp_image_id", "console_image_id", "keycloak_base_image", "keycloak_image_id", "daemon", "synthetic"}


@dataclass(frozen=True)
class Lock:
    source_commit: str
    cp_image_id: str
    console_image_id: str
    keycloak_base_image: str  # the public digest the derived image was built FROM
    keycloak_image_id: str  # the derived local image actually run
    daemon: dict[str, str]
    synthetic: bool

    def image_for(self, service: str) -> str:
        return {SERVICE_CP: self.cp_image_id, SERVICE_CONSOLE: self.console_image_id, SERVICE_KEYCLOAK: self.keycloak_image_id}[service]


def evaluate_daemon(info: Any) -> list[str]:
    """`docker info` output must name exactly the expected daemon."""
    if not isinstance(info, dict):
        return ["docker info did not return an object"]
    findings = []
    for key, expected in EXPECTED_DAEMON.items():
        if info.get(key) != expected:
            findings.append(f"daemon {key} is {info.get(key)!r}, expected {expected!r}")
    return findings


def parse_lock(data: dict[str, Any], profile: Profile) -> Lock:
    problems: list[str] = []
    if not isinstance(data, dict) or set(data) != LOCK_KEYS:
        raise ProfileError(f"lock keys must be exactly {sorted(LOCK_KEYS)}")
    if data["source_commit"] != PINNED_COMMIT:
        problems.append("lock source_commit is not the pinned commit")
    for key in ("cp_image_id", "console_image_id", "keycloak_image_id"):
        if not isinstance(data[key], str) or not IMAGE_ID_PATTERN.match(data[key]):
            problems.append(f"{key} must be an exact image ID sha256:<64 hex>")
    if data["keycloak_base_image"] != profile.keycloak_image:
        problems.append("lock keycloak_base_image differs from profile.toml; rebuild")
    if not isinstance(data["synthetic"], bool):
        problems.append("synthetic must be a boolean")
    if not isinstance(data["daemon"], dict) or set(data["daemon"]) != set(EXPECTED_DAEMON):
        problems.append("daemon must record ID, Name and ServerVersion")
    elif not data["synthetic"]:
        problems += evaluate_daemon(data["daemon"])
    if problems:
        raise ProfileError("; ".join(problems))
    synthetic = data["synthetic"]
    ids_synthetic = all(SYNTHETIC_IMAGE_ID_PATTERN.match(data[k]) for k in ("cp_image_id", "console_image_id", "keycloak_image_id"))
    if synthetic != ids_synthetic:
        raise ProfileError("synthetic flag disagrees with the image IDs recorded")
    return Lock(
        data["source_commit"], data["cp_image_id"], data["console_image_id"], data["keycloak_base_image"],
        data["keycloak_image_id"], dict(data["daemon"]), synthetic,
    )


def synthetic_lock(profile: Profile) -> Lock:
    """Obviously fake IDs for review and tests. `activate` refuses them."""
    fake = {"ID": "synthetic", "Name": "synthetic", "ServerVersion": "synthetic"}
    return Lock(PINNED_COMMIT, "sha256:" + "0" * 63 + "1", "sha256:" + "0" * 63 + "2", profile.keycloak_image, "sha256:" + "0" * 63 + "3", fake, True)


def lock_to_json(lock: Lock) -> str:
    return json.dumps(
        {
            "source_commit": lock.source_commit,
            "cp_image_id": lock.cp_image_id,
            "console_image_id": lock.console_image_id,
            "keycloak_base_image": lock.keycloak_base_image,
            "keycloak_image_id": lock.keycloak_image_id,
            "daemon": lock.daemon,
            "synthetic": lock.synthetic,
        },
        indent=2,
        sort_keys=True,
    ) + "\n"


# ---------------------------------------------------------------------------
# Rendering
# ---------------------------------------------------------------------------


def fill(template: str, values: dict[str, str]) -> str:
    """`@@NAME@@` substitution only. No `${}` anywhere, by construction."""
    out = template
    for name, value in values.items():
        out = out.replace(f"@@{name}@@", value)
    leftover = re.findall(r"@@[A-Z_]+@@", out)
    if leftover:
        raise ProfileError(f"template placeholders left unfilled: {leftover}")
    return out


def shipped_console_conf() -> str:
    return (REPO_ROOT / "apps" / "control-plane-ui" / "nginx.conf").read_text(encoding="utf-8")


def shipped_security_headers(conf: str) -> list[str]:
    """Every `add_header` line of the shipped console configuration."""
    return [line.strip() for line in conf.splitlines() if line.strip().startswith("add_header ")]


def render_control_plane_toml(profile: Profile) -> str:
    template = (TEMPLATES / "control-plane.toml.tmpl").read_text(encoding="utf-8")
    return fill(
        template,
        {
            "SOURCE_COMMIT": profile.source_commit,
            "CP_PORT": str(CP_PORT),
            "ISSUER": profile.issuer,
            "REACHABLE_AT": profile.reachable_at,
            "CLIENT_ID": CONSOLE_CLIENT_ID,
            "REQUIRED_ROLE": OPERATOR_ROLE,
            "REDIRECT_URI": profile.redirect_uri,
            "CP_STATE_PATH": CP_STATE_PATH,
            "JWKS_REFRESH_SECONDS": str(JWKS_REFRESH_SECONDS),
        },
    )


def render_console_conf(profile: Profile) -> str:
    template = (TEMPLATES / "console.nginx.conf.tmpl").read_text(encoding="utf-8")
    headers = "\n".join(f"    {line}" for line in shipped_security_headers(shipped_console_conf()))
    return fill(
        template,
        {"CONSOLE_PORT": str(CONSOLE_PORT), "SECURITY_HEADERS": headers, "CP_UPSTREAM": CP_UPSTREAM},
    )


def bind_ro(source: str, target: str) -> dict[str, Any]:
    return {"type": "bind", "source": source, "target": target, "read_only": True}


def tmpfs_entry(path: str, size: int, user: str, exec_allowed: bool) -> str:
    """One `tmpfs:` entry: explicit size, mode and owner so the non-root user can write it."""
    uid, gid = user.split(":")
    flags = "nosuid,nodev" if exec_allowed else "nosuid,nodev,noexec"
    return f"{path}:size={size},mode=0700,uid={uid},gid={gid},{flags}"


def parse_tmpfs_entry(entry: str) -> tuple[str, dict[str, str], set[str]]:
    path, _, options = entry.partition(":")
    values: dict[str, str] = {}
    flags: set[str] = set()
    for option in options.split(",") if options else []:
        key, eq, value = option.partition("=")
        if eq:
            values[key] = value
        else:
            flags.add(key)
    return path, values, flags


def compose_document(profile: Profile, lock: Lock) -> dict[str, Any]:
    """The compose file, as JSON (a YAML subset `docker compose -f` reads)."""

    def service(name: str, mem: str, cpus: float, pids: int) -> dict[str, Any]:
        return {
            "image": lock.image_for(name),
            "container_name": CONTAINER_NAMES[name],
            "user": USERS[name],
            "read_only": True,
            "cap_drop": ["ALL"],
            "security_opt": ["no-new-privileges:true"],
            "pull_policy": "never",
            "restart": "no",
            "mem_limit": mem,
            "cpus": cpus,
            "pids_limit": pids,
            "logging": LOGGING,
            "networks": [NETWORK_NAME],
            "tmpfs": [
                tmpfs_entry(path, size, USERS[name], TMPFS_EXEC_ALLOWED[name]) for path, size in TMPFS_SPECS[name].items()
            ],
        }

    cp = service(SERVICE_CP, "512m", 1.0, 256)
    cp["command"] = [CP_CONFIG_PATH]
    cp["environment"] = {"RUST_LOG": "info"}
    cp["volumes"] = [bind_ro(f"./{CP_CONFIG_NAME}", CP_CONFIG_PATH)]

    console = service(SERVICE_CONSOLE, "128m", 0.5, 64)
    console["environment"] = {}
    console["volumes"] = [bind_ro(f"./{CONSOLE_CONF_NAME}", CONSOLE_CONF_PATH)]
    console["ports"] = [
        {"target": CONSOLE_PORT, "published": str(profile.console_port), "host_ip": profile.bind_address, "protocol": "tcp"}
    ]
    console["depends_on"] = [SERVICE_CP]

    keycloak = service(SERVICE_KEYCLOAK, "1536m", 2.0, 512)
    keycloak["command"] = ["start-dev"]
    keycloak["env_file"] = [{"path": f"./{EPHEMERAL_DIR_NAME}/{EPHEMERAL_ENV_NAME}", "required": True}]
    keycloak["environment"] = {
        "KC_HOSTNAME": profile.keycloak_public_base,
        "KC_HTTP_ENABLED": "true",
        "KC_HEALTH_ENABLED": "true",
        "KC_DB": "dev-mem",
        "KC_LOG_LEVEL": "info",
    }
    keycloak["volumes"] = []
    keycloak["ports"] = [
        {"target": KEYCLOAK_PORT, "published": str(profile.oidc_port), "host_ip": profile.bind_address, "protocol": "tcp"}
    ]

    return {
        "name": profile.project_name,
        "services": {SERVICE_CP: cp, SERVICE_CONSOLE: console, SERVICE_KEYCLOAK: keycloak},
        "networks": {
            NETWORK_NAME: {
                "name": NETWORK_NAME,
                "driver": "bridge",
                "internal": True,
                "enable_ipv6": False,
                "driver_opts": {
                    "com.docker.network.bridge.name": BRIDGE_NAME,
                    "com.docker.network.bridge.enable_ip_masquerade": "false",
                },
                "ipam": {"driver": "default", "config": [{"subnet": profile.subnet}]},
            }
        },
    }


def compose_to_json(document: dict[str, Any]) -> str:
    return json.dumps(document, indent=2, sort_keys=True) + "\n"


def keycloak_derived_context(profile: Profile) -> dict[str, str]:
    """The two allowlisted, secret-free files of the derived Keycloak image.

    The public image's `start-dev` re-augments into /opt/keycloak/lib/quarkus
    at boot. Under a read-only root that path has to be a tmpfs, and a tmpfs
    hides the jars the image ships there. The derived image keeps an immutable
    copy of the shipped directory beside it; a narrow entrypoint copies that
    seed into the (empty, owned) tmpfs and then execs the stock `kc.sh`. No
    RUN step, no package install, no network during the build.
    """
    dockerfile = "\n".join(
        [
            "# GENERATED by dogfood.py for the disposable profile; base pinned by digest, never a tag.",
            f"FROM {profile.keycloak_image} AS base",
            "FROM base",
            f"COPY --from=base --chown=1000:0 {KC_QUARKUS_PATH} {KC_QUARKUS_SEED_PATH}",
            f"COPY --chown=1000:0 --chmod=0555 kc-entrypoint.sh {KC_ENTRYPOINT_PATH}",
            "USER 1000",
            f'ENTRYPOINT ["{KC_ENTRYPOINT_PATH}"]',
            'CMD ["start-dev"]',
            "",
        ]
    )
    entrypoint = "\n".join(
        [
            "#!/bin/bash",
            "# GENERATED by dogfood.py. Seeds the writable quarkus tmpfs from the immutable copy, then execs kc.sh.",
            "set -eu",
            f'seed="{KC_QUARKUS_SEED_PATH}"',
            f'live="{KC_QUARKUS_PATH}"',
            'if [ ! -d "$live" ] || [ ! -w "$live" ]; then',
            '  echo "refusing to start: $live is not a writable mount" >&2',
            "  exit 1",
            "fi",
            'if [ -n "$(ls -A "$live")" ]; then',
            '  echo "refusing to start: $live is not empty (expected a fresh tmpfs)" >&2',
            "  exit 1",
            "fi",
            # The seed is owned 1000:0; the tmpfs is 1000:1000 and we run as 1000:1000. Preserving the foreign
            # group would make cp fail (chown is not ours); the copy takes the tmpfs owner, which is what kc.sh needs.
            'cp -a --no-preserve=ownership "$seed/." "$live/"',
            'exec /opt/keycloak/bin/kc.sh "$@"',
            "",
        ]
    )
    return {"Dockerfile": dockerfile, "kc-entrypoint.sh": entrypoint}


@dataclass(frozen=True)
class Rendered:
    control_plane_toml: str
    console_conf: str
    compose_json: str
    lock_json: str

    def files(self) -> dict[str, str]:
        return {
            CP_CONFIG_NAME: self.control_plane_toml,
            CONSOLE_CONF_NAME: self.console_conf,
            COMPOSE_NAME: self.compose_json,
            LOCK_NAME: self.lock_json,
        }


def render(profile: Profile, lock: Lock) -> Rendered:
    return Rendered(
        render_control_plane_toml(profile),
        render_console_conf(profile),
        compose_to_json(compose_document(profile, lock)),
        lock_to_json(lock),
    )


# ---------------------------------------------------------------------------
# Validation of rendered artefacts (what Docker and the control plane read)
# ---------------------------------------------------------------------------


def derive_endpoints(issuer: str, reachable_at: str) -> dict[str, str]:
    """The same rule as crates/fabric-keycloak/src/operator/endpoints.rs."""
    issuer = issuer.strip().rstrip("/")
    reachable_at = reachable_at.strip().rstrip("/") or issuer
    return {
        "authorization": f"{issuer}/protocol/openid-connect/auth",
        "token": f"{reachable_at}/protocol/openid-connect/token",
        "jwks": f"{reachable_at}/protocol/openid-connect/certs",
    }


def host_of(url: str) -> str:
    return urllib.parse.urlsplit(url).netloc


# Mirrors `deny_unknown_fields` across the control-plane config structs at the
# pinned commit. A key not listed here is a finding, as it would be at startup.
TOML_SCHEMA: dict[str, Any] = {
    "listen": str,
    "request_timeout_seconds": int,
    "control_plane": {
        "public_base_url": str,
        "operator": {
            "mode": str, "issuer": str, "reachable_at": str, "client_id": str, "required_role": str,
            "redirect_uri": str, "leeway_seconds": int, "jwks_refresh_seconds": int,
        },
        "reconciliation": {"interval_seconds": int},
    },
    "desired_state": {"mode": str, "path": str},
    "identity_provider": {"mode": str},
    "secret_store": {"mode": str},
    "registries": {"http_timeout_seconds": int, "resolution_budget_seconds": int},
}
FORBIDDEN_TOML_TABLES = ("git_host", "platform_management")
GIT_HOST_DEFAULT_TIMEOUT = 10
# The control plane reads the realm's signing keys at startup and again every
# `jwks_refresh_seconds`. In this profile the realm is bootstrapped AFTER the
# control plane starts, so the production default (300) would refuse every
# bearer for up to five minutes. Five seconds bounds that window on a
# throwaway realm; it is a startup-ordering setting, not an auth change: the
# keys are still read from the issuer and every token is still verified.
JWKS_REFRESH_SECONDS = 5


def check_schema(value: Any, schema: Any, path: str, findings: list[str]) -> None:
    if isinstance(schema, dict):
        if not isinstance(value, dict):
            findings.append(f"{path}: expected a table")
            return
        for key, inner in value.items():
            if key not in schema:
                findings.append(f"{path}.{key}: unknown key (deny_unknown_fields would refuse it)")
                continue
            check_schema(inner, schema[key], f"{path}.{key}", findings)
    elif isinstance(value, bool) or not isinstance(value, schema):
        findings.append(f"{path}: expected {schema.__name__}")


def validate_control_plane_toml(text: str, profile: Profile) -> list[str]:
    findings: list[str] = []
    if tomllib is None:
        return ["Python 3.11 or newer is required (tomllib)"]
    try:
        config = tomllib.loads(text)
    except tomllib.TOMLDecodeError as error:
        return [f"control-plane.toml does not parse: {error}"]
    if "${" in text or "FABRIC_" in text.replace("FABRIC_CP_CONFIG", ""):
        findings.append("control-plane.toml must contain no interpolation or environment override")
    for table in FORBIDDEN_TOML_TABLES:
        if table in config:
            findings.append(f"[{table}] must be absent: no Git host, platform repository or deployment registry")
            config.pop(table)
    check_schema(config, TOML_SCHEMA, "config", findings)
    if findings:
        return findings

    if config.get("listen") != f"0.0.0.0:{CP_PORT}":
        findings.append(f"listen must be 0.0.0.0:{CP_PORT} (unpublished, internal network only)")
    for section, mode in (("desired_state", "local_directory"), ("identity_provider", "in_memory"), ("secret_store", "in_memory")):
        if config.get(section, {}).get("mode") != mode:
            findings.append(f"[{section}].mode must be {mode}")
    if config.get("desired_state", {}).get("path") != CP_STATE_PATH:
        findings.append(f"[desired_state].path must be the tmpfs {CP_STATE_PATH}")

    operator = config.get("control_plane", {}).get("operator", {})
    if operator.get("mode") != "oidc":
        findings.append("operator mode must be oidc")
    if operator.get("issuer") != profile.issuer:
        findings.append(f"issuer must be {profile.issuer}")
    if operator.get("reachable_at") != profile.reachable_at:
        findings.append(f"reachable_at must be {profile.reachable_at} (the internal service, never a shared host)")
    if operator.get("client_id") != CONSOLE_CLIENT_ID:
        findings.append(f"client_id must be {CONSOLE_CLIENT_ID}")
    if operator.get("required_role") != OPERATOR_ROLE:
        findings.append(f"required_role must be {OPERATOR_ROLE}")
    if operator.get("redirect_uri") != profile.redirect_uri:
        findings.append(f"redirect_uri must be {profile.redirect_uri} (console origin root with trailing slash)")
    if operator.get("jwks_refresh_seconds") != JWKS_REFRESH_SECONDS:
        findings.append(f"jwks_refresh_seconds must be exactly {JWKS_REFRESH_SECONDS} (the realm is bootstrapped after the control plane starts)")
    if "leeway_seconds" in operator:
        findings.append("leeway_seconds must be left at the control plane's default")
    if config.get("control_plane", {}).get("public_base_url"):
        findings.append("public_base_url must be absent: the Git connection flow is out of scope")

    endpoints = derive_endpoints(operator.get("issuer", ""), operator.get("reachable_at", ""))
    if host_of(endpoints["authorization"]) != f"{profile.bind_address}:{profile.oidc_port}":
        findings.append("the browser's authorization endpoint must be the loopback-published Keycloak port")
    for name in ("token", "jwks"):
        if host_of(endpoints[name]) != f"{SERVICE_KEYCLOAK}:{KEYCLOAK_PORT}":
            findings.append(f"the backend {name} endpoint must be the internal keycloak service")
    if not profile.redirect_uri.startswith(f"http://{profile.bind_address}:{profile.console_port}/"):
        findings.append("redirect_uri must return to the published console origin")

    budget = config.get("registries", {}).get("resolution_budget_seconds", 8)
    timeout = config.get("registries", {}).get("http_timeout_seconds", 10)
    request = config.get("request_timeout_seconds", 30)
    if budget == 0 or timeout == 0:
        findings.append("registries timeouts must be non-zero (startup refuses zero)")
    if not GIT_HOST_DEFAULT_TIMEOUT + budget + GIT_HOST_DEFAULT_TIMEOUT < request:
        findings.append("resolution budget does not fit between two Git calls inside one request")
    return findings


def validate_console_conf(text: str, profile: Profile) -> list[str]:
    findings: list[str] = []
    shipped = shipped_security_headers(shipped_console_conf())
    if not shipped:
        findings.append("the shipped nginx.conf has no add_header lines to compare against")
    present = [line.strip() for line in text.splitlines()]
    for header in shipped:
        if header not in present:
            findings.append(f"security header missing or altered: {header[:60]}")
    if f"listen {CONSOLE_PORT};" not in text:
        findings.append(f"console must listen on {CONSOLE_PORT}")
    if text.count("location /api/") != 1 or "proxy_pass $control_plane;" not in text:
        findings.append("exactly one /api/ location proxying through $control_plane is required")
    urls = set(re.findall(r"https?://[^\s;\"']+", text))
    allowed = {CP_UPSTREAM, "https://github.com"}
    for url in sorted(urls - allowed):
        findings.append(f"unexpected address in console configuration: {url}")
    if "resolver 127.0.0.11" not in text:
        findings.append("the proxy must resolve through Docker's embedded DNS only")
    if "try_files $uri $uri/ =404;" not in text:
        findings.append("the shipped no-history-fallback rule must be kept")
    if "${" in text:
        findings.append("console configuration must contain no interpolation")
    return findings


def validate_tmpfs(name: str, entries: Any) -> list[str]:
    where = f"services.{name}.tmpfs"
    if not isinstance(entries, list) or not all(isinstance(e, str) for e in entries):
        return [f"{where} must be a list of 'path:options' strings"]
    findings: list[str] = []
    uid, gid = USERS[name].split(":")
    seen: list[str] = []
    for entry in entries:
        path, values, flags = parse_tmpfs_entry(entry)
        seen.append(path)
        if path not in TMPFS_SPECS[name]:
            findings.append(f"{where}: {path} is not a declared writable path")
            continue
        try:
            size = int(values.get("size", ""))
        except ValueError:
            size = 0
        if size <= 0 or size > TMPFS_CAP_BYTES or size > TMPFS_SPECS[name][path]:
            findings.append(f"{where}: {path} size must be explicit and at most {TMPFS_SPECS[name][path]} bytes")
        if values.get("mode") != "0700":
            findings.append(f"{where}: {path} mode must be 0700 (owner only, never world-writable)")
        if values.get("uid") != uid or values.get("gid") != gid:
            findings.append(f"{where}: {path} must be owned by {USERS[name]}")
        if not {"nosuid", "nodev"} <= flags:
            findings.append(f"{where}: {path} must be nosuid,nodev")
        if not TMPFS_EXEC_ALLOWED[name] and "noexec" not in flags:
            findings.append(f"{where}: {path} must be noexec")
    for path in TMPFS_SPECS[name]:
        if seen.count(path) != 1:
            findings.append(f"{where}: {path} must appear exactly once")
    return findings


def validate_compose(document: Any, profile: Profile, lock: Lock, raw_text: str | None = None) -> list[str]:
    findings: list[str] = []
    if raw_text is not None and "${" in raw_text:
        findings.append("compose must contain no ${} interpolation")
    if not isinstance(document, dict) or set(document) != {"name", "services", "networks"}:
        return findings + ["compose top level must be exactly name, services, networks (no volumes/secrets/configs)"]
    if document["name"] != profile.project_name:
        findings.append("compose project name must match profile.project_name")

    services = document["services"]
    if set(services) != set(CONTAINER_NAMES):
        findings.append(f"services must be exactly {sorted(CONTAINER_NAMES)}")
        return findings

    for name, svc in services.items():
        where = f"services.{name}"
        forbidden = sorted(FORBIDDEN_SERVICE_KEYS & set(svc))
        if forbidden:
            findings.append(f"{where}: forbidden keys {forbidden}")
        if svc.get("image") != lock.image_for(name):
            findings.append(f"{where}.image must be the recorded local image ID")
        if not IMAGE_ID_PATTERN.match(str(svc.get("image", ""))):
            findings.append(f"{where}.image must be an exact local image ID (pull_policy never; nothing resolves a name at run time)")
        if svc.get("container_name") != CONTAINER_NAMES[name]:
            findings.append(f"{where}.container_name must be {CONTAINER_NAMES[name]}")
        if svc.get("user") != USERS[name]:
            findings.append(f"{where}.user must be {USERS[name]} (never root)")
        if svc.get("read_only") is not True:
            findings.append(f"{where}.read_only must be true")
        if svc.get("cap_drop") != ["ALL"]:
            findings.append(f"{where}.cap_drop must be [ALL]")
        if "no-new-privileges:true" not in svc.get("security_opt", []):
            findings.append(f"{where}.security_opt must include no-new-privileges:true")
        if svc.get("pull_policy") != "never":
            findings.append(f"{where}.pull_policy must be never")
        if svc.get("restart") != "no":
            findings.append(f"{where}.restart must be no")
        if not isinstance(svc.get("pids_limit"), int) or svc["pids_limit"] <= 0:
            findings.append(f"{where}.pids_limit must be a positive integer")
        if not svc.get("mem_limit") or not svc.get("cpus"):
            findings.append(f"{where}: mem_limit and cpus are required")
        if svc.get("logging") != LOGGING:
            findings.append(f"{where}.logging must cap log size")
        if svc.get("networks") != [NETWORK_NAME]:
            findings.append(f"{where}.networks must be exactly [{NETWORK_NAME}]")
        findings += validate_tmpfs(name, svc.get("tmpfs"))

        env = svc.get("environment")
        if not isinstance(env, dict):
            findings.append(f"{where}.environment must be a mapping (list form can inherit host variables)")
        else:
            for key, value in env.items():
                if key.startswith(FORBIDDEN_ENV_PREFIXES):
                    findings.append(f"{where}.environment.{key}: FABRIC_* overrides are refused")
                if not isinstance(value, str) or "${" in value:
                    findings.append(f"{where}.environment.{key}: must be a literal string")
        env_files = svc.get("env_file", [])
        if name == SERVICE_KEYCLOAK:
            if env_files != [{"path": f"./{EPHEMERAL_DIR_NAME}/{EPHEMERAL_ENV_NAME}", "required": True}]:
                findings.append(f"{where}.env_file must be exactly the required ephemeral keycloak.env")
        elif env_files:
            findings.append(f"{where}.env_file is only allowed on keycloak")

        for volume in svc.get("volumes", []):
            if not isinstance(volume, dict) or volume.get("type") != "bind":
                findings.append(f"{where}: only allowlisted read-only bind mounts may appear under volumes (tmpfs goes under `tmpfs`)")
                continue
            source = str(volume.get("source", ""))
            if source not in ALLOWED_BIND_SOURCES or volume.get("read_only") is not True:
                findings.append(f"{where}: bind mount {source!r} is not an allowlisted read-only config file")
            if "docker.sock" in source or source.startswith("/"):
                findings.append(f"{where}: host path or socket mount refused: {source}")

        ports = svc.get("ports", [])
        if name == SERVICE_CP and ports:
            findings.append("the control plane must not publish a port")
        expected_port = {SERVICE_CONSOLE: profile.console_port, SERVICE_KEYCLOAK: profile.oidc_port}.get(name)
        if expected_port is not None:
            if len(ports) != 1:
                findings.append(f"{where}: exactly one published port expected")
            for port in ports:
                if port.get("host_ip") != "127.0.0.1":
                    findings.append(f"{where}: ports must bind 127.0.0.1 only")
                if str(port.get("published")) != str(expected_port):
                    findings.append(f"{where}: published port must be {expected_port}")
                if port.get("protocol") != "tcp":
                    findings.append(f"{where}: port protocol must be tcp")
        if name == SERVICE_KEYCLOAK:
            if isinstance(env, dict) and env.get("KC_HOSTNAME") != profile.keycloak_public_base:
                findings.append("KC_HOSTNAME must equal the issuer base so minted tokens carry the configured issuer")
            if svc.get("command") != ["start-dev"]:
                findings.append("keycloak command must be start-dev")
        if name == SERVICE_CP and svc.get("command") != [CP_CONFIG_PATH]:
            findings.append(f"cp command must be [{CP_CONFIG_PATH}]")

    networks = document["networks"]
    if set(networks) != {NETWORK_NAME}:
        findings.append(f"networks must be exactly {NETWORK_NAME}")
    else:
        net = networks[NETWORK_NAME]
        if net.get("internal") is not True:
            findings.append("the network must be internal")
        if net.get("enable_ipv6") is not False:
            findings.append("IPv6 must be disabled on the network")
        if net.get("driver") != "bridge" or net.get("name") != NETWORK_NAME:
            findings.append("the network must be a bridge with the fixed name")
        opts = net.get("driver_opts", {})
        if opts.get("com.docker.network.bridge.name") != BRIDGE_NAME:
            findings.append(f"the bridge interface must be named {BRIDGE_NAME} so the enforcement gate can target it")
        if opts.get("com.docker.network.bridge.enable_ip_masquerade") != "false":
            findings.append("masquerade must be disabled")
        subnets = [c.get("subnet") for c in net.get("ipam", {}).get("config", [])]
        if subnets != [profile.subnet]:
            findings.append(f"ipam subnet must be exactly {profile.subnet}")
    return findings


def validate_rendered(rendered: Rendered, profile: Profile, lock: Lock) -> list[str]:
    findings = validate_control_plane_toml(rendered.control_plane_toml, profile)
    findings += validate_console_conf(rendered.console_conf, profile)
    try:
        document = json.loads(rendered.compose_json)
    except json.JSONDecodeError as error:
        return findings + [f"compose.json does not parse: {error}"]
    findings += validate_compose(document, profile, lock, rendered.compose_json)
    return findings


# ---------------------------------------------------------------------------
# Readiness and ownership evidence (pure functions over observed documents)
# ---------------------------------------------------------------------------


def evaluate_discovery(document: Any, profile: Profile) -> list[str]:
    """Checks a realm's openid-configuration as fetched from the loopback port."""
    findings: list[str] = []
    if not isinstance(document, dict):
        return ["discovery document is not an object"]
    if document.get("issuer") != profile.issuer:
        findings.append(f"issuer {document.get('issuer')!r} != configured {profile.issuer!r}")
    expected_auth = derive_endpoints(profile.issuer, profile.reachable_at)["authorization"]
    if document.get("authorization_endpoint") != expected_auth:
        findings.append("authorization_endpoint is not the loopback-published one the console will be sent to")
    if "S256" not in document.get("code_challenge_methods_supported", []):
        findings.append("realm does not advertise PKCE S256")
    return findings


def evaluate_session_config(document: Any, profile: Profile) -> list[str]:
    """Checks GET /api/session as answered through the console's proxy."""
    findings: list[str] = []
    if not isinstance(document, dict):
        return ["session config is not an object"]
    expected_auth = derive_endpoints(profile.issuer, profile.reachable_at)["authorization"]
    if document.get("authorization_endpoint") != expected_auth:
        findings.append("control plane sends the browser somewhere other than the published issuer")
    if document.get("client_id") != CONSOLE_CLIENT_ID:
        findings.append("client_id differs from the console client")
    if document.get("redirect_uri") != profile.redirect_uri:
        findings.append("redirect_uri differs from the console origin root")
    if document.get("scope") != "openid profile":
        findings.append("scope is not the console's 'openid profile'")
    return findings


def evaluate_network_inspect(document: Any, profile: Profile) -> list[str]:
    """Checks `docker network inspect <name>` output after activation."""
    findings: list[str] = []
    if isinstance(document, list):
        if len(document) != 1:
            return ["network inspect must describe exactly one network"]
        document = document[0]
    if not isinstance(document, dict):
        return ["network inspect output is not an object"]
    if document.get("Name") != NETWORK_NAME:
        findings.append("network name differs")
    if document.get("Internal") is not True:
        findings.append("network is not internal")
    if document.get("EnableIPv6") is not False:
        findings.append("IPv6 is enabled")
    if document.get("Options", {}).get("com.docker.network.bridge.name") != BRIDGE_NAME:
        findings.append("bridge interface name differs; the enforcement gate would target the wrong interface")
    subnets = [c.get("Subnet") for c in document.get("IPAM", {}).get("Config", [])]
    if subnets != [profile.subnet]:
        findings.append(f"subnet {subnets} != {profile.subnet}")
    containers = {c.get("Name") for c in document.get("Containers", {}).values()}
    unexpected = containers - set(CONTAINER_NAMES.values())
    if unexpected:
        findings.append(f"unexpected containers attached: {sorted(unexpected)}")
    return findings


def evaluate_owned_container(document: Any, service: str, profile: Profile, lock: Lock) -> list[str]:
    """A `docker container inspect` object is ours only if name, project label and image all match."""
    if not isinstance(document, dict):
        return [f"{service}: inspect output is not an object"]
    findings = []
    if not CONTAINER_ID_PATTERN.match(str(document.get("Id", ""))):
        findings.append(f"{service}: no full container ID")
    if document.get("Name") != f"/{CONTAINER_NAMES[service]}":
        findings.append(f"{service}: name is {document.get('Name')!r}, not /{CONTAINER_NAMES[service]}")
    labels = (document.get("Config") or {}).get("Labels") or {}
    if labels.get(COMPOSE_PROJECT_LABEL) != profile.project_name:
        findings.append(f"{service}: compose project label is not {profile.project_name}")
    if document.get("Image") != lock.image_for(service):
        findings.append(f"{service}: image {document.get('Image')!r} is not the locked image ID")
    return findings


def evaluate_owned_network(document: Any, profile: Profile) -> list[str]:
    if not isinstance(document, dict):
        return ["network: inspect output is not an object"]
    findings = []
    if not CONTAINER_ID_PATTERN.match(str(document.get("Id", ""))):
        findings.append("network: no full network ID")
    if document.get("Name") != NETWORK_NAME:
        findings.append(f"network: name is {document.get('Name')!r}")
    if (document.get("Labels") or {}).get(COMPOSE_PROJECT_LABEL) != profile.project_name:
        findings.append(f"network: compose project label is not {profile.project_name}")
    return findings


def evaluate_published_port(document: Any, container_port: int, host_ip: str, host_port: int) -> list[str]:
    """The inspected container must publish exactly container_port on host_ip:host_port and nothing else."""
    if not isinstance(document, dict):
        return ["inspect output is not an object"]
    ports = ((document.get("NetworkSettings") or {}).get("Ports")) or {}
    bindings = {key: value for key, value in ports.items() if value}
    expected = {f"{container_port}/tcp": [{"HostIp": host_ip, "HostPort": str(host_port)}]}
    if bindings != expected:
        return [f"published ports are {bindings!r}, expected {expected!r}"]
    return []


# ---------------------------------------------------------------------------
# Host-side enforcement gate: printed, verified read-only, never applied here
# ---------------------------------------------------------------------------


def expected_iptables_rules(profile: Profile) -> dict[str, list[str]]:
    """Rules per chain, top to bottom, as `iptables -S` prints them after `-A CHAIN `.

    `internal: true` removes the default route and masquerade. It does NOT
    stop a container reaching the host's own addresses (the bridge gateway,
    or a shared Keycloak/OpenBao published on 0.0.0.0) or anything routed
    locally. These rules close that for this profile's bridge and subnet
    only. Return traffic for connections the host opened to the published
    ports is accepted (ESTABLISHED,RELATED) before anything is dropped, in
    both chains; new connections out of the sandbox are dropped.
    """
    net = str(ipaddress.ip_network(profile.subnet))
    established = f"-i {BRIDGE_NAME} -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT"
    return {
        "INPUT": [established, f"-i {BRIDGE_NAME} -j DROP"],
        "DOCKER-USER": [established, f"-s {net} ! -d {net} -j DROP", f"-i {BRIDGE_NAME} ! -o {BRIDGE_NAME} -j DROP"],
    }


def enforcement_apply_commands(profile: Profile) -> list[str]:
    commands = []
    for chain, rules in expected_iptables_rules(profile).items():
        for index, rule in enumerate(rules, start=1):
            commands.append(f"iptables -I {chain} {index} {rule}")
    return commands


def enforcement_remove_commands(profile: Profile) -> list[str]:
    """The exact inverse of apply: one `-D` per rule, nothing flushed."""
    commands = []
    for chain, rules in expected_iptables_rules(profile).items():
        for rule in reversed(rules):
            commands.append(f"iptables -D {chain} {rule}")
    return commands


def enforcement_text(profile: Profile) -> str:
    lines = [
        f"# Per-sandbox enforcement for bridge {BRIDGE_NAME} / {profile.subnet}.",
        "# This tool never applies or removes these. They are a separately approved host change.",
        "# `activate` reads `iptables -S INPUT` and `iptables -S DOCKER-USER` and refuses to start unless",
        "# every rule below is present, in this order, with the ESTABLISHED accept above the drops.",
        "#",
        "# APPLY (insert positions keep this top-to-bottom order; DOCKER-USER precedes Docker's own FORWARD rules):",
        *enforcement_apply_commands(profile),
        "#",
        "# REMOVE on reset (exact inverse, one rule at a time; nothing flushed, no chain deleted, no policy changed):",
        *enforcement_remove_commands(profile),
        "#",
        "# Host->container traffic to the two published 127.0.0.1 ports is unaffected: the reply direction",
        "# is ESTABLISHED and accepted above; the request direction never matches -i " + BRIDGE_NAME + ".",
        "# Evidence: `iptables -S INPUT`, `iptables -S DOCKER-USER`, `docker network inspect " + NETWORK_NAME + "`.",
        "# These rules being present is a precondition, not proof. Runtime negative canaries (a probe from inside",
        "# the sandbox that FAILS to reach a host-local port and FAILS to reach the Internet) must pass before",
        "# anyone uses the console interactively. None has been run.",
    ]
    return "\n".join(lines)


def normalize_rule(line: str) -> list[str]:
    tokens = line.split()
    for index, token in enumerate(tokens):
        if token == "--ctstate" and index + 1 < len(tokens):
            tokens[index + 1] = ",".join(sorted(tokens[index + 1].split(",")))
    return tokens


def evaluate_iptables(listings: dict[str, str], profile: Profile) -> list[str]:
    """Pure check of `iptables -S <chain>` output for every expected chain."""
    findings: list[str] = []
    for chain, rules in expected_iptables_rules(profile).items():
        listing = listings.get(chain)
        if listing is None:
            findings.append(f"{chain}: listing missing")
            continue
        lines = [normalize_rule(l) for l in listing.splitlines() if l.strip()]
        if not any(l[:2] in (["-N", chain], ["-P", chain]) for l in lines):
            findings.append(f"{chain}: chain does not exist")
            continue
        present = [l[2:] for l in lines if l[:2] == ["-A", chain]]
        positions = []
        for rule in rules:
            wanted = normalize_rule(rule)
            hits = [i for i, candidate in enumerate(present) if candidate == wanted]
            if len(hits) != 1:
                findings.append(f"{chain}: rule not present exactly once: {rule}")
                continue
            positions.append(hits[0])
        if len(positions) == len(rules) and positions != list(range(len(rules))):
            findings.append(
                f"{chain}: the gate must be the first {len(rules)} rules in this exact order (positions seen {positions}); "
                "a rule above or between them could accept or return traffic before the drops"
            )
    return findings


# ---------------------------------------------------------------------------
# Filesystem state
# ---------------------------------------------------------------------------


def worktree_head(repo_root: Path) -> str | None:
    """The checked-out commit, read without invoking git. None if unreadable."""
    try:
        git = repo_root / ".git"
        if git.is_file():
            gitdir = Path(git.read_text(encoding="utf-8").split("gitdir:", 1)[1].strip())
            if not gitdir.is_absolute():
                gitdir = (repo_root / gitdir).resolve()
        elif git.is_dir():
            gitdir = git
        else:
            return None
        head = (gitdir / "HEAD").read_text(encoding="utf-8").strip()
        if not head.startswith("ref: "):
            return head if COMMIT_PATTERN.match(head) else None
        ref = head[5:]
        common = gitdir
        commondir = gitdir / "commondir"
        if commondir.is_file():
            common = (gitdir / commondir.read_text(encoding="utf-8").strip()).resolve()
        for base in (gitdir, common):
            candidate = base / ref
            if candidate.is_file():
                value = candidate.read_text(encoding="utf-8").strip()
                return value if COMMIT_PATTERN.match(value) else None
        packed = common / "packed-refs"
        if packed.is_file():
            for line in packed.read_text(encoding="utf-8").splitlines():
                parts = line.split()
                if len(parts) == 2 and parts[1] == ref and COMMIT_PATTERN.match(parts[0]):
                    return parts[0]
        return None
    except (OSError, IndexError, UnicodeDecodeError):
        return None


def real_child_dir(parent: Path, name: str) -> Path:
    """`parent/name`, required to be absent or a real (non-symlink) directory. Never followed."""
    path = parent / name
    try:
        info = os.lstat(path)
    except FileNotFoundError:
        return path
    if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):
        raise ProfileError(f"{path} must be a real directory, not a symlink or file")
    return path


def refuse_symlink(path: Path) -> None:
    try:
        if stat.S_ISLNK(os.lstat(path).st_mode):
            raise ProfileError(f"{path} is a symlink; refusing to read, write or delete through it")
    except FileNotFoundError:
        pass


def write_regular_file(path: Path, text: str, mode: int, exclusive: bool) -> None:
    """Creates/overwrites `path` without following a symlink; `exclusive` refuses an existing file."""
    refuse_symlink(path)
    flags = os.O_WRONLY | os.O_CREAT | os.O_NOFOLLOW | (os.O_EXCL if exclusive else os.O_TRUNC)
    try:
        fd = os.open(path, flags, mode)
    except FileExistsError:
        raise ProfileError(f"{path} already exists; refusing to overwrite") from None
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        handle.write(text)
    os.chmod(path, mode)


def dotenv_findings(*directories: Path) -> list[str]:
    return [f"{d / '.env'} exists; a .env could feed compose interpolation and is refused" for d in directories if (d / ".env").exists()]


@dataclass
class Paths:
    profile_dir: Path

    @property
    def profile_file(self) -> Path:
        return self.profile_dir / "profile.toml"

    @property
    def out(self) -> Path:
        return real_child_dir(self.profile_dir, OUT_DIR_NAME)

    @property
    def ephemeral(self) -> Path:
        return real_child_dir(self.out, EPHEMERAL_DIR_NAME)

    @property
    def ephemeral_env(self) -> Path:
        return self.ephemeral / EPHEMERAL_ENV_NAME

    @property
    def receipt(self) -> Path:
        return self.out / RECEIPT_NAME


def read_lock(paths: Paths, profile: Profile) -> Lock | None:
    lock_path = paths.out / LOCK_NAME
    refuse_symlink(lock_path)
    if not lock_path.is_file():
        return None
    try:
        return parse_lock(json.loads(lock_path.read_text(encoding="utf-8")), profile)
    except (json.JSONDecodeError, ProfileError) as error:
        raise ProfileError(f"{lock_path}: {error}") from error


def write_rendered(paths: Paths, rendered: Rendered) -> None:
    out = paths.out
    out.mkdir(mode=0o755, exist_ok=True)
    for name, text in rendered.files().items():
        write_regular_file(out / name, text, 0o644, exclusive=False)


def drift_findings(paths: Paths, rendered: Rendered) -> list[str]:
    """What is on disk must be byte-identical to what the inputs render to."""
    findings = []
    for name, text in rendered.files().items():
        target = paths.out / name
        if target.is_symlink():
            findings.append(f"{target} is a symlink; refused")
        elif not target.is_file():
            findings.append(f"{target} is missing; run prepare")
        elif target.read_text(encoding="utf-8") != text:
            findings.append(f"{target} differs from what profile.toml renders; edits are refused -- run prepare")
    return findings


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


# ---------------------------------------------------------------------------
# Ownership receipt: what activation created and reset may remove
# ---------------------------------------------------------------------------

RECEIPT_KEYS = {"project_name", "daemon", "compose_sha256", "containers", "network"}


def parse_receipt(data: Any, profile: Profile, lock: Lock, compose_text: str) -> dict[str, Any]:
    if not isinstance(data, dict) or set(data) != RECEIPT_KEYS:
        raise ProfileError(f"receipt keys must be exactly {sorted(RECEIPT_KEYS)}")
    problems = []
    if data["project_name"] != profile.project_name:
        problems.append("receipt project_name differs from profile")
    problems += evaluate_daemon(data["daemon"])
    if data["compose_sha256"] != sha256_text(compose_text):
        problems.append("receipt compose_sha256 differs from compose.json on disk (tampered or re-rendered); nothing is removed")
    containers = data["containers"]
    if not isinstance(containers, dict) or set(containers) != set(CONTAINER_NAMES):
        problems.append(f"receipt containers must be exactly {sorted(CONTAINER_NAMES)}")
    else:
        for service, entry in containers.items():
            if not isinstance(entry, dict) or set(entry) != {"id", "name", "image"}:
                problems.append(f"receipt {service} must record id, name and image")
                continue
            if not CONTAINER_ID_PATTERN.match(str(entry["id"])):
                problems.append(f"receipt {service} id is not a full 64-hex ID")
            if entry["name"] != CONTAINER_NAMES[service]:
                problems.append(f"receipt {service} name is not {CONTAINER_NAMES[service]}")
            if entry["image"] != lock.image_for(service):
                problems.append(f"receipt {service} image is not the locked image ID")
    network = data["network"]
    if not isinstance(network, dict) or set(network) != {"id", "name"} or not CONTAINER_ID_PATTERN.match(str(network["id"])) or network["name"] != NETWORK_NAME:
        problems.append("receipt network must record a full ID and the fixed name")
    if problems:
        raise ProfileError("; ".join(problems))
    return data


def read_receipt(paths: Paths, profile: Profile, lock: Lock) -> dict[str, Any] | None:
    refuse_symlink(paths.receipt)
    if not paths.receipt.is_file():
        return None
    compose_path = paths.out / COMPOSE_NAME
    refuse_symlink(compose_path)
    if not compose_path.is_file():
        raise ProfileError("ownership receipt exists but compose.json is missing; nothing is removed")
    try:
        data = json.loads(paths.receipt.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise ProfileError(f"ownership receipt does not parse: {error}") from error
    return parse_receipt(data, profile, lock, compose_path.read_text(encoding="utf-8"))


# ---------------------------------------------------------------------------
# Process seams: Docker, iptables, git. Fixed binaries, clean environment, no shell.
# ---------------------------------------------------------------------------

_ISOLATED_DIR: Path | None = None


def isolated_dir() -> Path:
    """A private, empty directory used as HOME and DOCKER_CONFIG so nothing of the operator's is read."""
    global _ISOLATED_DIR
    if _ISOLATED_DIR is None:
        _ISOLATED_DIR = Path(tempfile.mkdtemp(prefix="dogfood-isolated-"))
        atexit.register(_remove_private_dir, _ISOLATED_DIR)
    return _ISOLATED_DIR


def _remove_private_dir(path: Path) -> None:
    if path.is_dir() and not path.is_symlink() and path.name.startswith("dogfood-"):
        shutil.rmtree(path, ignore_errors=True)


def locate(candidates: tuple[str, ...], what: str) -> str:
    for candidate in candidates:
        if os.path.isfile(candidate) and os.access(candidate, os.X_OK):
            return candidate
    raise ProfileError(f"{what} not found at any bound location {list(candidates)}")


def docker_environment(source: dict[str, str] | None = None) -> dict[str, str]:
    """The complete environment for every Docker/git/iptables call. `source` is deliberately ignored.

    Nothing is inherited: not HOME, not DOCKER_HOST/CONTEXT/CONFIG/TLS, not
    proxies, not FABRIC_* or COMPOSE_*. The host is explicit; the config
    directory is a private empty one, so no operator credential or context
    can be read.
    """
    private = str(isolated_dir())
    return {
        "PATH": SAFE_PATH,
        "HOME": private,
        "LANG": "C.UTF-8",
        "DOCKER_HOST": DOCKER_HOST,
        "DOCKER_CONFIG": private,
        "DOCKER_CLI_HINTS": "false",
        "GIT_CONFIG_NOSYSTEM": "1",
        "GIT_CONFIG_GLOBAL": "/dev/null",
    }


def run_bound(binary: str, args: list[str], capture: bool) -> bytes:
    result = subprocess.run(
        [binary, *args], check=True, env=docker_environment(), shell=False,
        stdin=subprocess.DEVNULL, stdout=subprocess.PIPE if capture else None,
    )
    return result.stdout if capture else b""


def run_docker(args: list[str], capture: bool = False) -> str:
    """The single seam through which Docker is ever invoked."""
    return run_bound(locate(DOCKER_BIN_CANDIDATES, "docker"), args, capture).decode("utf-8", "replace").strip()


def read_iptables(chain: str) -> str:
    """Read-only listing of one chain. Any failure (missing binary, no privilege) raises."""
    try:
        return run_bound(locate(IPTABLES_BIN_CANDIDATES, "iptables"), ["-S", chain], capture=True).decode("utf-8", "replace")
    except (subprocess.CalledProcessError, OSError) as error:
        raise ProfileError(f"could not read iptables chain {chain}: {error}") from error


def run_git(args: list[str]) -> bytes:
    return run_bound(locate(GIT_BIN_CANDIDATES, "git"), ["-C", str(REPO_ROOT), *args], capture=True)


def verify_daemon() -> dict[str, str]:
    """Refuses any daemon other than the recorded one. Called before every Docker-touching command."""
    try:
        info = json.loads(run_docker(["info", "--format", "{{json .}}"], capture=True))
    except (subprocess.CalledProcessError, json.JSONDecodeError, OSError) as error:
        raise ProfileError(f"could not read docker info over {DOCKER_HOST}: {error}") from error
    findings = evaluate_daemon(info)
    if findings:
        raise ProfileError("wrong Docker daemon: " + "; ".join(findings))
    return {key: info[key] for key in EXPECTED_DAEMON}


def docker_inspect(kind: str, reference: str) -> dict[str, Any] | None:
    """`docker <kind> inspect` as one object, or None when the daemon does not know the reference."""
    try:
        text = run_docker([kind, "inspect", "--format", "{{json .}}", reference], capture=True)
    except subprocess.CalledProcessError:
        return None
    try:
        document = json.loads(text)
    except json.JSONDecodeError:
        return None
    return document if isinstance(document, dict) else None


def preflight_enforcement(profile: Profile) -> None:
    listings = {chain: read_iptables(chain) for chain in expected_iptables_rules(profile)}
    findings = evaluate_iptables(listings, profile)
    if findings:
        raise ProfileError("host enforcement gate not in place (see `enforcement`): " + "; ".join(findings))


def collision_findings(profile: Profile) -> list[str]:
    """Any existing resource with our names, labels or network is a refusal, never adopted."""
    findings = []
    names = set(run_docker(["ps", "--all", "--format", "{{.Names}}"], capture=True).split())
    taken = sorted(names & set(CONTAINER_NAMES.values()))
    if taken:
        findings.append(f"containers already exist with this profile's names: {taken}")
    labelled = run_docker(["ps", "--all", "--quiet", "--filter", f"label={COMPOSE_PROJECT_LABEL}={profile.project_name}"], capture=True).split()
    if labelled:
        findings.append(f"containers already carry project label {profile.project_name}: {labelled}")
    networks = set(run_docker(["network", "ls", "--format", "{{.Name}}"], capture=True).split())
    if NETWORK_NAME in networks:
        findings.append(f"network {NETWORK_NAME} already exists")
    return findings


def compose_args(paths: Paths, profile: Profile) -> list[str]:
    return ["compose", "--project-name", profile.project_name, "--project-directory", str(paths.out), "-f", str(paths.out / COMPOSE_NAME)]


# ---------------------------------------------------------------------------
# Commands: prepare, check, enforcement (no Docker)
# ---------------------------------------------------------------------------


def cmd_prepare(paths: Paths, synthetic_pins: bool) -> int:
    profile = load_profile(paths.profile_file)
    if paths.ephemeral_env.exists() or paths.receipt.exists():
        print("refusing to re-render while an activation (credentials or ownership receipt) exists; run reset first", file=sys.stderr)
        return 1
    lock = read_lock(paths, profile)
    if synthetic_pins:
        if lock is not None and not lock.synthetic:
            print("refusing --synthetic-pins: a real lock recorded by build exists; reset first", file=sys.stderr)
            return 1
        lock = synthetic_lock(profile)
        print("using SYNTHETIC image IDs (review only; activate will refuse them)")
    elif lock is None:
        print(
            "blocked: no pins.lock.json. Run `build --yes` (approved, uses Docker) to record the image IDs, "
            "or `prepare --synthetic-pins` to render for review only.",
            file=sys.stderr,
        )
        return 1
    rendered = render(profile, lock)
    findings = validate_rendered(rendered, profile, lock)
    if findings:
        print("rendered artefacts failed validation; nothing written:", file=sys.stderr)
        for finding in findings:
            print(f"  - {finding}", file=sys.stderr)
        return 1
    write_rendered(paths, rendered)
    print(f"rendered {', '.join(rendered.files())} into {paths.out}")
    return cmd_check(paths)


def cmd_check(paths: Paths) -> int:
    findings: list[str] = []
    try:
        profile = load_profile(paths.profile_file)
        out = paths.out
        paths.ephemeral  # noqa: B018 - raises if .ephemeral is a symlink or a file
    except ProfileError as error:
        print(f"refused: {error}", file=sys.stderr)
        return 1
    findings += dotenv_findings(paths.profile_dir, out)
    head = worktree_head(REPO_ROOT)
    if head is None:
        findings.append("could not read the worktree HEAD; confirm `git rev-parse HEAD` equals the pin by hand")
    elif head != PINNED_COMMIT:
        findings.append(f"worktree HEAD {head} is not the pinned commit {PINNED_COMMIT} (build reads the pin via git archive, but the console header comparison reads this tree)")
    if not (REPO_ROOT / "apps/control-plane-ui/nginx.conf").is_file():
        findings.append("apps/control-plane-ui/nginx.conf is missing; security headers cannot be compared")
    try:
        lock = read_lock(paths, profile)
    except ProfileError as error:
        lock = None
        findings.append(str(error))
    if lock is None:
        findings.append("no valid pins.lock.json; compose cannot be rendered or checked")
    else:
        rendered = render(profile, lock)
        findings += drift_findings(paths, rendered)
        findings += validate_rendered(rendered, profile, lock)
        state = "SYNTHETIC (review only)" if lock.synthetic else f"recorded by build on daemon {lock.daemon.get('Name')}"
        print(f"pins: {state}; keycloak base {profile.keycloak_image}; derived keycloak {lock.keycloak_image_id}")
    print(f"ephemeral credentials: {'present' if paths.ephemeral_env.exists() else 'absent (none created)'}")
    print(f"ownership receipt: {'present' if paths.receipt.exists() else 'absent (nothing activated)'}")
    print(f"issuer {profile.issuer}; redirect {profile.redirect_uri}; backend {profile.reachable_at}")
    if findings:
        print("check FAILED:", file=sys.stderr)
        for finding in findings:
            print(f"  - {finding}", file=sys.stderr)
        return 1
    print("check OK: inputs, rendered configuration, compose and nginx all validate; runtime behaviour unverified")
    return 0


# ---------------------------------------------------------------------------
# build: git archive of the pin, docker build, pull by digest, derive Keycloak
# ---------------------------------------------------------------------------


def extract_pinned_source(destination: Path) -> None:
    """`git archive <pin>` into `destination`: tracked content of that commit only."""
    verified = run_git(["rev-parse", "--verify", f"{PINNED_COMMIT}^{{commit}}"]).decode().strip()
    if verified != PINNED_COMMIT:
        raise ProfileError(f"git does not resolve the pinned commit: {verified!r}")
    archive = run_git(["archive", "--format=tar", PINNED_COMMIT])
    destination.mkdir(mode=0o700)
    with tarfile.open(fileobj=io.BytesIO(archive), mode="r:") as tar:
        for member in tar.getmembers():
            name = member.name
            if name.startswith("/") or ".." in Path(name).parts or not (member.isfile() or member.isdir()):
                raise ProfileError(f"refusing archive member {name!r} (only plain files and directories are extracted)")
        if hasattr(tarfile, "data_filter"):
            tar.extractall(destination, filter="data")
        else:  # pragma: no cover - Python < 3.11.4
            tar.extractall(destination)


def docker_build(context: Path, dockerfile: Path, target: str | None) -> str:
    args = ["build", "--quiet", "--pull=false", "--file", str(dockerfile)]
    if target:
        args += ["--target", target]
    image_id = run_docker([*args, str(context)], capture=True)
    if not IMAGE_ID_PATTERN.match(image_id):
        raise ProfileError(f"docker build did not return an image ID: {image_id!r}")
    return image_id


def cmd_build(paths: Paths, yes: bool) -> int:
    if not yes:
        print("build uses Docker and needs --yes", file=sys.stderr)
        return 2
    profile = load_profile(paths.profile_file)
    if paths.receipt.exists() or paths.ephemeral_env.exists():
        print("an activation exists; reset before rebuilding", file=sys.stderr)
        return 1
    daemon = verify_daemon()
    work = Path(tempfile.mkdtemp(prefix="dogfood-build-"))
    try:
        source = work / "src"
        extract_pinned_source(source)
        pinned_conf = (source / "apps/control-plane-ui/nginx.conf").read_text(encoding="utf-8")
        if pinned_conf != shipped_console_conf():
            print("apps/control-plane-ui/nginx.conf in this working tree differs from the pinned commit; the rendered console configuration would not match the image. Refusing.", file=sys.stderr)
            return 1
        print(f"building control plane and console from `git archive {PINNED_COMMIT}` (private context, no working-tree files)")
        cp_id = docker_build(source, source / "Dockerfile", "control-plane-api")
        console_id = docker_build(source, source / "apps/control-plane-ui/Dockerfile", "console")

        print(f"pulling {profile.keycloak_image} (the one approved outbound fetch, by digest)")
        run_docker(["pull", profile.keycloak_image])
        digests = run_docker(["image", "inspect", "--format", "{{json .RepoDigests}}", profile.keycloak_image], capture=True)
        if profile.keycloak_image not in json.loads(digests):
            print("pulled image does not report the pinned digest", file=sys.stderr)
            return 1
        kc_context = work / "keycloak"
        kc_context.mkdir(mode=0o700)
        for name, text in keycloak_derived_context(profile).items():
            if name not in KC_CONTEXT_FILES:
                raise ProfileError(f"{name} is not an allowlisted build-context file")
            write_regular_file(kc_context / name, text, 0o644, exclusive=True)
        kc_id = docker_build(kc_context, kc_context / "Dockerfile", None)
    finally:
        _remove_private_dir(work)

    lock = Lock(PINNED_COMMIT, cp_id, console_id, profile.keycloak_image, kc_id, daemon, False)
    paths.out.mkdir(mode=0o755, exist_ok=True)
    write_regular_file(paths.out / LOCK_NAME, lock_to_json(lock), 0o644, exclusive=False)
    print(f"recorded {paths.out / LOCK_NAME}: cp {cp_id}, console {console_id}, derived keycloak {kc_id} (from {profile.keycloak_image})")
    return cmd_prepare(paths, synthetic_pins=False)


# ---------------------------------------------------------------------------
# Loopback HTTP: fixed ports, no redirects, no proxies, no userinfo
# ---------------------------------------------------------------------------


class _RefuseRedirects(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):  # type: ignore[override]
        raise ProfileError(f"redirect ({code}) refused: this tool never follows a Location header")


_OPENER = urllib.request.build_opener(urllib.request.ProxyHandler({}), _RefuseRedirects())


def loopback_url(url: str, profile: Profile) -> str:
    parts = urllib.parse.urlsplit(url)
    allowed_netlocs = {f"{profile.bind_address}:{profile.console_port}", f"{profile.bind_address}:{profile.oidc_port}"}
    if (
        parts.scheme != "http"
        or parts.netloc not in allowed_netlocs
        or parts.username is not None
        or parts.password is not None
        or parts.fragment
    ):
        raise ProfileError(f"refusing to contact {parts.scheme}://{parts.netloc}: only http://127.0.0.1 on the two profile ports, without credentials, is allowed")
    return url


def http_json(url: str, profile: Profile, data: bytes | None = None, headers: dict[str, str] | None = None, method: str | None = None) -> tuple[int, Any, dict[str, str]]:
    request = urllib.request.Request(loopback_url(url, profile), data=data, headers=headers or {}, method=method)
    try:
        with _OPENER.open(request, timeout=10) as response:  # noqa: S310 - loopback only, checked above
            body = response.read()
            status = response.status
            response_headers = dict(response.headers)
    except urllib.error.HTTPError as error:
        body = error.read()
        status = error.code
        response_headers = dict(error.headers)
    if 300 <= status < 400:
        raise ProfileError(f"redirect ({status}) refused: this tool never follows a Location header")
    try:
        parsed = json.loads(body) if body else None
    except json.JSONDecodeError:
        parsed = body.decode("utf-8", "replace")
    return status, parsed, response_headers


def wait_for(description: str, probe: Callable[[], Any], attempts: int = 60, delay: float = 3.0) -> Any:
    for _ in range(attempts):
        try:
            value = probe()
            if value is not None:
                return value
        except (urllib.error.URLError, OSError, ProfileError):
            pass
        time.sleep(delay)
    raise ProfileError(f"timed out waiting for {description}")


MIN_OPERATOR_PASSWORD_LENGTH = 16


def human_console() -> Any:
    """The controlling terminal, verified to be a terminal, for the prompt text that precedes the operator password. None if there is none.

    Opens `/dev/tty` read-write without becoming its controlling process and
    refuses anything that is not a terminal (a redirected or piped
    `/dev/tty` cannot disable echo). Nothing secret is ever written to it.
    """
    try:
        fd = os.open("/dev/tty", os.O_RDWR | os.O_NOCTTY)
    except OSError:
        return None
    if not os.isatty(fd):
        os.close(fd)
        return None
    return os.fdopen(fd, "w", encoding="utf-8")


def read_secret_from_terminal(prompt: str) -> str:
    """One echo-disabled line from `/dev/tty`, opened and verified here. Never stdin, never echoed.

    `getpass` is not used: when `/dev/tty` cannot be opened it silently reads
    `sys.stdin` instead (a TTY stdin gets no `GetPassWarning`), so a warning
    filter cannot guarantee the no-stdin rule. This function opens `/dev/tty`
    itself, requires `isatty`, clears ECHO with termios on that descriptor,
    reads from that descriptor only, and restores the attributes in `finally`
    whatever happens. If the terminal is unavailable nothing is read from
    anywhere. Linux only, like the rest of this launcher.
    """
    try:
        fd = os.open("/dev/tty", os.O_RDWR | os.O_NOCTTY)
    except OSError:
        raise ProfileError("no controlling terminal (/dev/tty): the operator password is never read from stdin") from None
    try:
        if not os.isatty(fd):
            raise ProfileError("/dev/tty is not a terminal: the operator password is never read from stdin")
        saved = termios.tcgetattr(fd)
        quiet = list(saved)
        quiet[3] &= ~termios.ECHO
        termios.tcsetattr(fd, termios.TCSAFLUSH, quiet)
        try:
            os.write(fd, prompt.encode("utf-8"))
            line = b""
            while not line.endswith(b"\n"):
                chunk = os.read(fd, 1024)
                if not chunk:
                    raise EOFError
                line += chunk
        finally:
            try:
                termios.tcsetattr(fd, termios.TCSAFLUSH, saved)
                os.write(fd, b"\n")  # the newline the terminal did not echo; nothing secret
            except (OSError, termios.error):
                pass  # the terminal is gone; the read already failed and the fd is closed below
        return line.decode("utf-8", errors="strict")[:-1]
    finally:
        os.close(fd)


def prompt_operator_password(console: Any, read_secret: Callable[[str], str] = read_secret_from_terminal) -> str:
    """Asks the human, twice, for a NEW password for the synthetic dogfood operator.

    Rejects an empty entry, one shorter than MIN_OPERATOR_PASSWORD_LENGTH, a
    mismatch between the two entries, or a terminal whose echo cannot be
    disabled. The password is returned to the caller for the one Keycloak
    request that sets it; it is never printed, logged or written to disk by
    this tool. Error messages name the reason only, never the input.
    """
    console.write(
        "Choose a NEW password for the synthetic dogfood operator: one not used anywhere else,\n"
        f"at least {MIN_OPERATOR_PASSWORD_LENGTH} characters, typed twice with echo off. It is set on the throwaway realm only,\n"
        "never shown again and never stored by this tool; it is gone on reset.\n"
    )
    console.flush()
    try:
        first = read_secret("operator password: ")
        second = read_secret("operator password (again): ")
    except (EOFError, OSError, UnicodeDecodeError, termios.error) as error:
        raise ProfileError(f"operator password not read ({error.__class__.__name__}): the controlling terminal could not be read with echo off; nothing was read from stdin") from None
    if not first:
        raise ProfileError("operator password rejected: empty")
    if len(first) < MIN_OPERATOR_PASSWORD_LENGTH:
        raise ProfileError(f"operator password rejected: shorter than {MIN_OPERATOR_PASSWORD_LENGTH} characters")
    if first != second:
        raise ProfileError("operator password rejected: the two entries differ")
    return first


def bootstrap_realm(profile: Profile, admin_user: str, admin_password: str, operator_password: str | None = None) -> tuple[str, str]:
    """Creates the throwaway realm, console client, operator role and one operator.

    Returns `(operator_username, operator_password)`. When `operator_password`
    is None (the noninteractive CI path) a random one is generated and
    returned so the caller can use it without any human or any output;
    otherwise the given password (interactive activation, typed by the human)
    is set and returned unchanged. Everything created here lives in the
    Keycloak container's tmpfs and is gone on reset. Errors carry status
    codes only, never bodies or secrets.
    """
    base = profile.keycloak_public_base
    form = urllib.parse.urlencode({"grant_type": "password", "client_id": "admin-cli", "username": admin_user, "password": admin_password}).encode()
    status, token, _ = http_json(f"{base}/realms/master/protocol/openid-connect/token", profile, form, {"Content-Type": "application/x-www-form-urlencoded"}, "POST")
    if status != 200 or not isinstance(token, dict) or not isinstance(token.get("access_token"), str):
        raise ProfileError(f"bootstrap admin token refused ({status})")
    auth = {"Authorization": f"Bearer {token['access_token']}", "Content-Type": "application/json"}

    def post(path: str, body: Any) -> dict[str, str]:
        status, _, headers = http_json(f"{base}/admin/realms{path}", profile, json.dumps(body).encode(), auth, "POST")
        if status not in (201, 204):
            raise ProfileError(f"POST {path} answered {status}")
        return headers

    post("", {"realm": profile.realm, "enabled": True, "displayName": "Fabric disposable dogfood"})
    post(f"/{profile.realm}/roles", {"name": OPERATOR_ROLE, "description": "Disposable dogfood operator"})
    post(
        f"/{profile.realm}/clients",
        {
            "clientId": CONSOLE_CLIENT_ID,
            "protocol": "openid-connect",
            "publicClient": True,
            "standardFlowEnabled": True,
            "directAccessGrantsEnabled": False,
            "implicitFlowEnabled": False,
            "serviceAccountsEnabled": False,
            "redirectUris": [profile.redirect_uri],
            "webOrigins": [profile.console_origin],
            "attributes": {"pkce.code.challenge.method": "S256", "post.logout.redirect.uris": "+"},
        },
    )
    operator = f"operator-{secrets.token_hex(3)}"
    password = secrets.token_urlsafe(24) if operator_password is None else operator_password
    headers = post(
        f"/{profile.realm}/users",
        {"username": operator, "enabled": True, "credentials": [{"type": "password", "value": password, "temporary": False}]},
    )
    location = headers.get("Location") or headers.get("location") or ""
    user_id = location.rstrip("/").rsplit("/", 1)[-1]
    if not re.match(r"^[0-9a-f-]{36}$", user_id):
        raise ProfileError("could not read the created user's id from the Location header")
    status, role, _ = http_json(f"{base}/admin/realms/{profile.realm}/roles/{OPERATOR_ROLE}", profile, None, auth)
    if status != 200 or not isinstance(role, dict):
        raise ProfileError("could not read back the operator role")
    post(f"/{profile.realm}/users/{user_id}/role-mappings/realm", [role])
    return operator, password


# ---------------------------------------------------------------------------
# activate / status / reset
# ---------------------------------------------------------------------------


def remove_owned_by_name(profile: Profile, lock: Lock) -> list[str]:
    """After a failed start: remove only containers/network that are provably ours. Returns what was removed."""
    removed = []
    for service, name in CONTAINER_NAMES.items():
        document = docker_inspect("container", name)
        if document is not None and not evaluate_owned_container(document, service, profile, lock):
            run_docker(["rm", "--force", document["Id"]])
            removed.append(f"container {name} {document['Id'][:12]}")
    network = docker_inspect("network", NETWORK_NAME)
    if network is not None and not evaluate_owned_network(network, profile) and not network.get("Containers"):
        run_docker(["network", "rm", network["Id"]])
        removed.append(f"network {NETWORK_NAME} {network['Id'][:12]}")
    return removed


def remove_recorded(receipt: dict[str, Any], profile: Profile, lock: Lock) -> list[str]:
    """Reset's only Docker writes: `rm --force <recorded id>` and `network rm <recorded id>`, each re-verified first."""
    plan: list[tuple[str, str, str]] = []
    problems: list[str] = []
    for service, entry in receipt["containers"].items():
        document = docker_inspect("container", entry["id"])
        if document is None:
            continue
        findings = evaluate_owned_container(document, service, profile, lock)
        if document.get("Id") != entry["id"]:
            findings.append(f"{service}: daemon returned a different ID than recorded")
        if findings:
            problems += findings
        else:
            plan.append(("rm", entry["id"], f"container {entry['name']}"))
    network = docker_inspect("network", receipt["network"]["id"])
    if network is not None:
        findings = evaluate_owned_network(network, profile)
        if network.get("Id") != receipt["network"]["id"]:
            findings.append("network: daemon returned a different ID than recorded")
        attached = {c.get("Name") for c in (network.get("Containers") or {}).values()}
        foreign = attached - set(CONTAINER_NAMES.values())
        if foreign:
            findings.append(f"network: foreign containers attached {sorted(foreign)}; not removing")
        if findings:
            problems += findings
        else:
            plan.append(("network", network["Id"], f"network {NETWORK_NAME}"))
    if problems:
        raise ProfileError("ownership could not be re-verified; nothing removed: " + "; ".join(problems))
    removed = []
    for kind, identifier, label in plan:
        run_docker(["rm", "--force", identifier] if kind == "rm" else ["network", "rm", identifier])
        removed.append(f"{label} {identifier[:12]}")
    return removed


def delete_local_state(paths: Paths) -> list[str]:
    """Deletes exactly the files this tool writes, never through a symlink, never a tree."""
    out = paths.out
    if not out.exists():
        return []
    ephemeral = paths.ephemeral
    candidates = [ephemeral / EPHEMERAL_ENV_NAME, out / RECEIPT_NAME, *(out / name for name in RENDERED_NAMES)]
    for candidate in candidates:
        refuse_symlink(candidate)
    leftovers = []
    for candidate in candidates:
        if candidate.exists():
            candidate.unlink()
    for directory in (ephemeral, out):
        if directory.exists():
            try:
                directory.rmdir()
            except OSError:
                leftovers.append(f"{directory} is not empty; unexpected files were left in place")
    return leftovers


def cmd_activate(paths: Paths, yes: bool) -> int:
    if not yes:
        print("activate starts containers and creates ephemeral credentials; it needs --yes", file=sys.stderr)
        return 2
    console = human_console()
    if console is None:
        print("refusing: no controlling terminal (/dev/tty); the operator password is typed by a human with echo off, never read from stdin or logged", file=sys.stderr)
        return 1
    console.close()  # availability proven; reopened for the prompt once every read-only check has passed
    if cmd_check(paths) != 0:
        return 1
    profile = load_profile(paths.profile_file)
    lock = read_lock(paths, profile)
    if lock is None or lock.synthetic:
        print("refusing to activate: the lock holds synthetic image IDs; run build --yes", file=sys.stderr)
        return 1
    if paths.ephemeral_env.exists() or paths.receipt.exists():
        print("an activation already exists (credentials or ownership receipt); reset first", file=sys.stderr)
        return 1

    daemon = verify_daemon()
    preflight_enforcement(profile)
    collisions = collision_findings(profile)
    if collisions:
        print("refusing to activate: existing resources would be adopted, never acceptable:", file=sys.stderr)
        for finding in collisions:
            print(f"  - {finding}", file=sys.stderr)
        return 1

    # Every read-only check has passed; nothing has been created yet. The human
    # types the operator password now, so a refusal above never asks for it and
    # a rejected entry below leaves nothing to clean up. It stays in memory only.
    console = human_console()
    if console is None:
        print("refusing: the controlling terminal went away before the operator password could be read; nothing was started", file=sys.stderr)
        return 1
    try:
        with console:
            operator_password = prompt_operator_password(console)
    except ProfileError as error:
        print(f"refusing to activate: {error}; nothing was started", file=sys.stderr)
        return 1

    admin_user = f"bootstrap-{secrets.token_hex(3)}"
    admin_password = secrets.token_urlsafe(32)
    paths.ephemeral.mkdir(mode=0o700, exist_ok=True)
    os.chmod(paths.ephemeral, 0o700)
    write_regular_file(paths.ephemeral_env, f"KC_BOOTSTRAP_ADMIN_USERNAME={admin_user}\nKC_BOOTSTRAP_ADMIN_PASSWORD={admin_password}\n", 0o600, exclusive=True)

    def abandon(reason: str) -> int:
        removed = remove_owned_by_name(profile, lock)
        paths.ephemeral_env.unlink(missing_ok=True)
        print(f"activation abandoned: {reason}. Removed only provably owned resources: {removed or 'none'}", file=sys.stderr)
        return 1

    try:
        run_docker([*compose_args(paths, profile), "up", "--detach", "--no-build", "--pull", "never"])
    except subprocess.CalledProcessError as error:
        return abandon(f"compose up failed ({error.returncode})")

    observed: dict[str, dict[str, Any]] = {}
    findings: list[str] = []
    for service, name in CONTAINER_NAMES.items():
        document = docker_inspect("container", name)
        findings += evaluate_owned_container(document, service, profile, lock)
        if document is not None:
            observed[service] = document
    network = docker_inspect("network", NETWORK_NAME)
    findings += evaluate_owned_network(network, profile)
    findings += evaluate_network_inspect(network, profile) if network else []
    if findings:
        return abandon("started resources did not verify as this profile's own: " + "; ".join(findings))
    receipt = {
        "project_name": profile.project_name,
        "daemon": daemon,
        "compose_sha256": sha256_text((paths.out / COMPOSE_NAME).read_text(encoding="utf-8")),
        "containers": {service: {"id": doc["Id"], "name": CONTAINER_NAMES[service], "image": doc["Image"]} for service, doc in observed.items()},
        "network": {"id": network["Id"], "name": NETWORK_NAME},  # type: ignore[index]
    }
    write_regular_file(paths.receipt, json.dumps(receipt, indent=2, sort_keys=True) + "\n", 0o600, exclusive=True)

    # Bootstrap only once the loopback port is proven to belong to the owned Keycloak container.
    port_findings = evaluate_published_port(observed[SERVICE_KEYCLOAK], KEYCLOAK_PORT, profile.bind_address, profile.oidc_port)
    if port_findings:
        print("refusing to bootstrap: " + "; ".join(port_findings), file=sys.stderr)
        return 1

    def master_realm_ready() -> bool | None:
        status, _, _ = http_json(f"{profile.keycloak_public_base}/realms/master", profile)
        return True if status == 200 else None

    wait_for("Keycloak master realm on the loopback port", master_realm_ready)
    operator, _ = bootstrap_realm(profile, admin_user, admin_password, operator_password)
    del operator_password
    _, discovery, _ = http_json(f"{profile.issuer}/.well-known/openid-configuration", profile)
    findings = evaluate_discovery(discovery, profile)
    findings += cmd_status(paths, quiet=True)
    print(f"\nactivated. Sign in at {profile.console_origin} as operator username: {operator} with the password you typed (ephemeral; gone on reset).")
    print("Before interactive use: run the negative connectivity canaries in README.md; none has been run by this tool.")
    if findings:
        print("readiness findings:", file=sys.stderr)
        for finding in findings:
            print(f"  - {finding}", file=sys.stderr)
        return 1
    return 0


def cmd_status(paths: Paths, quiet: bool = False) -> list[str]:
    profile = load_profile(paths.profile_file)
    findings: list[str] = []
    try:
        status, _, _ = http_json(f"{profile.console_origin}/healthz", profile)
        if status != 200:
            findings.append(f"console /healthz answered {status}")
        status, session, _ = http_json(f"{profile.console_origin}/api/session", profile)
        if status != 200:
            findings.append(f"/api/session through the console proxy answered {status}")
        else:
            findings += evaluate_session_config(session, profile)
        status, discovery, _ = http_json(f"{profile.issuer}/.well-known/openid-configuration", profile)
        if status != 200:
            findings.append(f"issuer discovery answered {status}")
        else:
            findings += evaluate_discovery(discovery, profile)
    except (urllib.error.URLError, OSError, ProfileError) as error:
        findings.append(f"not reachable: {error}")
    try:
        verify_daemon()
        inspected = docker_inspect("network", NETWORK_NAME)
        findings += evaluate_network_inspect(inspected, profile) if inspected else [f"network {NETWORK_NAME} not present"]
    except ProfileError as error:
        findings.append(str(error))
    if not quiet:
        print("ready" if not findings else "NOT ready")
        for finding in findings:
            print(f"  - {finding}")
    return findings


def cmd_reset(paths: Paths, yes: bool) -> int:
    if not yes:
        print("reset removes the recorded containers and network and deletes rendered state; it needs --yes", file=sys.stderr)
        return 2
    profile = load_profile(paths.profile_file)
    out = paths.out
    paths.ephemeral  # noqa: B018 - raises on a symlinked .ephemeral
    removed: list[str] = []
    if out.exists():
        lock = read_lock(paths, profile)
        receipt = read_receipt(paths, profile, lock) if lock is not None else None
        if paths.receipt.exists() and (lock is None or receipt is None):
            print("an ownership receipt exists but cannot be validated; nothing is removed", file=sys.stderr)
            return 1
        if receipt is not None:
            verify_daemon()
            removed = remove_recorded(receipt, profile, lock)  # type: ignore[arg-type]
        else:
            print("no ownership receipt: no Docker resource is touched. If containers named "
                  f"{sorted(CONTAINER_NAMES.values())} exist, inspect them by hand before removing anything.")
    leftovers = delete_local_state(paths)
    print(f"reset: removed {removed or 'no Docker resources'}; local state deleted. Images are kept. No volume was ever created or removed.")
    print("Remember to remove the host firewall rules with the REMOVE lines from `enforcement`; this tool does not.")
    for leftover in leftovers:
        print(f"  - {leftover}", file=sys.stderr)
    return 1 if leftovers else 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--profile-dir", default=str(HERE), help="directory holding profile.toml (default: this directory)")
    sub = parser.add_subparsers(dest="command")
    prepare = sub.add_parser("prepare", help="render and check (default)")
    prepare.add_argument("--synthetic-pins", action="store_true", help="render with obviously fake image IDs for review")
    sub.add_parser("check", help="validate inputs and what is on disk; no Docker")
    build = sub.add_parser("build", help="git-archive the pin, docker build, pull Keycloak by digest, derive it")
    build.add_argument("--yes", action="store_true")
    activate = sub.add_parser("activate", help="verify daemon + firewall gate, compose up, bootstrap the throwaway realm")
    activate.add_argument("--yes", action="store_true")
    sub.add_parser("status", help="loopback readiness evidence")
    reset = sub.add_parser("reset", help="remove only the recorded containers and network; delete rendered and ephemeral state")
    reset.add_argument("--yes", action="store_true")
    sub.add_parser("enforcement", help="print the host firewall gate and its exact inverse (never applied here)")
    args = parser.parse_args(argv)

    paths = Paths(Path(args.profile_dir).resolve())
    try:
        if args.command in (None, "prepare"):
            return cmd_prepare(paths, synthetic_pins=getattr(args, "synthetic_pins", False))
        if args.command == "check":
            return cmd_check(paths)
        if args.command == "build":
            return cmd_build(paths, args.yes)
        if args.command == "activate":
            return cmd_activate(paths, args.yes)
        if args.command == "status":
            return 0 if not cmd_status(paths) else 1
        if args.command == "reset":
            return cmd_reset(paths, args.yes)
        if args.command == "enforcement":
            print(enforcement_text(load_profile(paths.profile_file)))
            return 0
    except ProfileError as error:
        print(f"refused: {error}", file=sys.stderr)
        return 1
    except subprocess.CalledProcessError as error:
        print(f"command failed with status {error.returncode}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    sys.exit(main())
