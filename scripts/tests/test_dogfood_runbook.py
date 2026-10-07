"""The disposable-dogfood runbook must state what #111 requires, with the values the tool actually uses.

Reads examples/disposable-dogfood/README.md and checks its required sections
and its resource table against profile.toml and the constants in dogfood.py,
so a changed port, digest, ceiling or cleanup in either place without the
other fails here.

    python3 -m unittest discover -s scripts/tests -v
"""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
PROFILE_DIR = REPO_ROOT / "examples" / "disposable-dogfood"
sys.path.insert(0, str(PROFILE_DIR))

import dogfood  # noqa: E402

README = (PROFILE_DIR / "README.md").read_text(encoding="utf-8")

REQUIRED_SECTIONS = (
    "What this profile is not",
    "Prerequisites (resolved here, changed only by the authorized activation)",
    "Restricted SSH forwarding",
    "Host firewall gate",
    "Protected shared services",
    "Resources, ports, digests and owned cleanup",
    "Network restriction: what is and is not proven",
)


def section(title: str) -> str:
    match = re.search(rf"^(#+) {re.escape(title)}\n", README, re.M)
    if match is None:
        raise AssertionError(f"README has no section {title!r}")
    body = README[match.end():]
    end = re.search(rf"^#{{1,{len(match.group(1))}}} ", body, re.M)
    return body[: end.start()] if end else body


def table_rows(text: str) -> dict[str, list[str]]:
    rows: dict[str, list[str]] = {}
    for line in text.splitlines():
        if not line.startswith("|") or set(line) <= set("|- "):
            continue
        cells = [cell.strip() for cell in line.strip().strip("|").split("|")]
        rows[cells[0]] = cells[1:]
    return rows


def mib(size: int) -> str:
    return f"{size // dogfood.MIB} MiB"


@unittest.skipIf(dogfood.tomllib is None, "Python 3.11+ (tomllib) required")
class Runbook(unittest.TestCase):
    def setUp(self):
        self.profile = dogfood.load_profile(PROFILE_DIR / "profile.toml")

    def test_every_required_section_is_present(self):
        headings = re.findall(r"^#+ (.+)$", README, re.M)
        for title in REQUIRED_SECTIONS:
            self.assertIn(title, headings)

    def test_scope_statement_is_explicit(self):
        scope = section("What this profile is not")
        self.assertIn("**This profile is NOT WorkSpec acceptance, NOT runtime acceptance and NOT\ntenant acceptance.**", scope)
        self.assertIn("**Off-host backup is not a gate for this trial.**", scope)
        self.assertIn("**Activation needs separate authorization.**", scope)

    def test_ssh_forwarding_text_matches_the_generated_lines(self):
        forwarding = section("Restricted SSH forwarding")
        options = dogfood.ssh_forward_options(self.profile)
        self.assertTrue(options.startswith(
            f'restrict,port-forwarding,permitopen="127.0.0.1:{self.profile.console_port}",permitopen="127.0.0.1:{self.profile.oidc_port}"'
        ))
        self.assertIn(f"{options} ssh-ed25519 ", forwarding)
        self.assertIn(dogfood.ssh_forward_command(self.profile), forwarding)
        for port in (self.profile.console_port, self.profile.oidc_port):
            self.assertIn(f"-L 127.0.0.1:{port}:127.0.0.1:{port}", dogfood.ssh_forward_command(self.profile), "local and remote ports must be identical")
        self.assertIn(f"`{self.profile.issuer}`", forwarding)
        self.assertIn(f"`{self.profile.redirect_uri}`", forwarding)
        self.assertIn("PermitListen none", forwarding)
        self.assertIn("AllowTcpForwarding local", forwarding)

    def test_protected_shared_services_are_listed_with_their_protection(self):
        rows = table_rows(section("Protected shared services"))
        names = " ".join(rows)
        for service in ("Keycloak", "OpenBao", "Git"):
            self.assertIn(f"**{service}**", names)
        for name, cells in rows.items():
            if name.startswith("**"):
                self.assertGreater(len(cells[0]), 80, f"{name} needs a stated protection")
        keycloak = next(cells[0] for name, cells in rows.items() if name.startswith("**Keycloak**"))
        self.assertIn("identity_provider = in_memory", keycloak)
        self.assertIn(f"127.0.0.1:{self.profile.oidc_port}", keycloak)
        openbao = next(cells[0] for name, cells in rows.items() if name.startswith("**OpenBao**"))
        self.assertIn("secret_store = in_memory", openbao)
        git = next(cells[0] for name, cells in rows.items() if name.startswith("**Git**"))
        for marker in ("desired_state = local_directory", "[git_host]", "[platform_management]", "public_base_url"):
            self.assertIn(marker, git)

    def test_resource_table_matches_profile_and_tool(self):
        text = section("Resources, ports, digests and owned cleanup")
        rows = table_rows(text)
        self.assertIn(f"`{dogfood.COMPOSE_PROJECT_LABEL}={self.profile.project_name}`", text)
        published = {dogfood.SERVICE_CONSOLE: self.profile.console_port, dogfood.SERVICE_KEYCLOAK: self.profile.oidc_port}
        for service, container in dogfood.CONTAINER_NAMES.items():
            identity, ceiling, owned, cleanup = rows[f"`{container}`"]
            limits = dogfood.RESOURCE_LIMITS[service]
            self.assertIn(f"mem `{limits['mem_limit']}`, cpus `{limits['cpus']}`, pids `{limits['pids_limit']}`", ceiling)
            for path, size in dogfood.TMPFS_SPECS[service].items():
                self.assertIn(f"`{path}` {mib(size)}", ceiling)
            self.assertIn(f"logs {dogfood.LOGGING['options']['max-file']} × `{dogfood.LOGGING['options']['max-size']}`", ceiling)
            self.assertIn(f"user `{dogfood.USERS[service]}`", identity)
            if service in published:
                self.assertEqual(owned, f"`{self.profile.bind_address}:{published[service]}` → `8080/tcp`")
            else:
                self.assertIn(f"none published (internal `{dogfood.CP_PORT}`)", owned)
            self.assertIn("`docker rm --force <recorded id>`", cleanup)
        self.assertIn(f"`git archive {self.profile.source_commit}`", rows[f"`{dogfood.CONTAINER_NAMES[dogfood.SERVICE_CP]}`"][0])
        self.assertIn(f"`FROM {self.profile.keycloak_image}`", rows[f"`{dogfood.CONTAINER_NAMES[dogfood.SERVICE_KEYCLOAK]}`"][0])

        network = rows[f"`{dogfood.NETWORK_NAME}`"]
        self.assertIn(f"`{dogfood.BRIDGE_NAME}`", network[0])
        self.assertIn(f"`{self.profile.subnet}`", network[0])
        self.assertIn("`docker network rm <recorded id>`", network[3])

        gate = rows["host firewall gate"]
        self.assertIn(f"{len(dogfood.enforcement_remove_commands(self.profile))} `iptables -D` REMOVE lines", gate[3])
        self.assertIn(f"realm `{self.profile.realm}`", rows)
        local = rows["local state"][0]
        for name in dogfood.RENDERED_NAMES + (dogfood.RECEIPT_NAME,):
            self.assertIn(f"`.out/{name}`", local)
        self.assertIn(f"`.out/{dogfood.EPHEMERAL_DIR_NAME}/{dogfood.EPHEMERAL_ENV_NAME}`", local)
        self.assertIn(f"`{self.profile.keycloak_image}`", rows["images"][2])

    def test_every_pinned_value_in_the_readme_is_the_current_one(self):
        commits = set(re.findall(r"\b[0-9a-f]{40}\b", README))
        self.assertEqual(commits, {self.profile.source_commit})
        digests = set(re.findall(r"quay\.io/keycloak/keycloak@sha256:[0-9a-f]{64}", README))
        self.assertEqual(digests, {self.profile.keycloak_image})
        self.assertEqual(self.profile.source_commit, dogfood.PINNED_COMMIT)


if __name__ == "__main__":
    unittest.main()
