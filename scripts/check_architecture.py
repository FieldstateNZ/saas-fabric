#!/usr/bin/env python3
"""Enforce the SaaS Fabric architectural invariants that are structural.

Most of the platform's invariants are behavioural and are pinned by tests --
that a tenant predicate survives a hostile filter, that an unsupported
operation fails closed, that a secret never reaches a log line. Those belong
in Rust, next to the code that could break them, and that is where they are.

A handful are not behavioural at all. They are statements about what this
workspace is *allowed to contain*: which crate may name a protocol type,
which layers may know what HTTP is, what may appear in the dependency graph
at all. No unit test can fail when one of those is violated, because the
violation is the code compiling in the first place. This script is where
those live.

Each check below states the invariant, then the specification section or ADR
it comes from, then what a violation would actually cost. A check with no
consequence written down is a check nobody will understand well enough to fix
when it fires.

Usage:
    python3 scripts/check_architecture.py

Exit status is 0 when every invariant holds, 1 otherwise.

Dependency facts come from `cargo metadata` rather than from parsing
`Cargo.toml` by hand. It is the authoritative resolution, it sees the whole
transitive graph -- which is what "no database driver is linked anywhere"
actually requires -- and it does not need a TOML parser newer than the Python
on the average machine.
"""

from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CRATES = REPO_ROOT / "crates"

# The composition roots. Each is the one crate in its plane allowed to know
# about every other crate in that plane at once, because assembling them is its
# entire job.
HOST = "fabric-api"
CONTROL_PLANE_HOST = "fabric-control-plane-api"

# The two planes, as sets of crates. `fabric-core` belongs to neither: it is
# the shared kernel, and both planes depend on it deliberately.
#
# The separation these two lists express is the point of the whole control
# plane increment. The runtime plane serves tenant requests and must keep
# working when Git and Keycloak are down; the control plane administers
# desired state and is useless without them. An edge between them would put
# control-plane availability behind every tenant request (specification §6),
# which is exactly the coupling the architecture forbids.
RUNTIME_PLANE = frozenset(
    {
        "fabric-identity",
        "fabric-tenant-runtime",
        "fabric-connector",
        "fabric-connector-ndc",
        "fabric-data-api",
        "fabric-api",
        "fabric-fga-auth",
    }
)

CONTROL_PLANE = frozenset(
    {
        "fabric-client-model",
        "fabric-reconciliation",
        "fabric-control-plane",
        "fabric-client-git",
        "fabric-keycloak",
        "fabric-openbao",
        "fabric-control-plane-api",
    }
)

# The crate that owns Keycloak's protocol. ADR 0008 makes this boundary the
# point of adopting Keycloak behind a port: SaaS Fabric manages *client
# identity*, and Keycloak is the thing that happens to implement it.
KEYCLOAK_CRATE = "fabric-keycloak"

# The crate that owns the Git hosting provider's protocol. Same boundary, same
# reasoning: the control plane writes *desired state*, and Git is where it
# happens to land (specification §8).
GIT_CRATE = "fabric-client-git"

# The crate that owns the NDC protocol. ADR 0001 makes this boundary the whole
# point of adopting NDC: the specification is an internal connector protocol,
# never the platform's public contract.
NDC_CRATE = "fabric-connector-ndc"

# What the host is permitted to name from the NDC crate. These two are
# deployment wiring -- choosing a connector process and building it at
# startup -- not request-path vocabulary. Anything else appearing here would
# mean NDC concepts had begun leaking toward the Data API.
NDC_NAMES_THE_HOST_MAY_USE = frozenset(
    {
        "NdcConnectorConfig",
        "build_ndc_connector",
    }
)

# Crates that model a domain. None of them may know what HTTP is.
#
# In the runtime plane the reason is that the Data API's shape must be
# replaceable without touching tenant resolution. In the control plane it is
# the same shape of claim about a different pair: `fabric-client-model` and
# `fabric-reconciliation` decide what a client is and what has to change about
# an identity provider, and neither should acquire an opinion about how an
# operator asked or how an adapter talks. A transport type reaching into any of
# them is how that stops being true.
#
# `fabric-identity` is deliberately absent, and it is worth saying why rather
# than leaving the omission to look like an oversight. Turning an inbound HTTP
# request into a tenant identity is that crate's entire purpose: it owns the
# axum extractor that makes `TenantIdentity` a handler parameter, and the
# `IntoResponse` for the ways that can fail. The transport-independent half --
# `IdentityResolver`, the token readers, the configuration -- takes a
# `http::HeaderMap` and knows nothing about a server. Moving the extractor
# into `fabric-data-api` would buy a cleaner-looking dependency list at the
# cost of scattering identity extraction across two crates, which is a worse
# trade than the one it fixes.
DOMAIN_CRATES = frozenset(
    {
        "fabric-core",
        "fabric-connector",
        "fabric-tenant-runtime",
        "fabric-client-model",
        "fabric-reconciliation",
    }
)

HTTP_CRATES = frozenset({"axum", "tower", "tower-http", "hyper", "reqwest"})

# Database drivers. The runtime plane never opens a database connection --
# every physical connection lives inside a connector process (ADR 0001), and
# applications receive query results over the Data API, never a connection
# (specification section 2). The strongest available form of that claim is
# that no driver is linked into this workspace at all, which is checkable.
DATABASE_DRIVERS = frozenset(
    {
        "sqlx",
        "tokio-postgres",
        "postgres",
        "mysql",
        "mysql_async",
        "rusqlite",
        "tiberius",
        "diesel",
        "sea-orm",
        "mongodb",
    }
)

# Control-plane clients. Section 6 is explicit that Git and Kubernetes are
# never in the request path, and the strongest form of that is structural: if
# no client is linked, no handler can reach one no matter how a future change
# is written.
#
# This stayed a **workspace-wide** ban after the control plane arrived, and
# that is worth saying out loud, because the obvious reading is that a control
# plane needs a Git library. It does not: `fabric-client-git` speaks the
# hosting provider's contents API over HTTPS, so the platform needs no clone,
# no working copy, and no disk — and the claim "nothing in this binary can
# invoke Git" stays true of every binary the workspace builds.
CONTROL_PLANE_CLIENTS = frozenset(
    {
        "kube",
        "k8s-openapi",
        "kube-client",
        "kube-runtime",
        "git2",
        "gix",
        "gitoxide",
    }
)


# The only files permitted to name the tenant header, because rejecting it is
# what they do. Deliberately a file list rather than a crate: see the check.
TENANT_HEADER_IS_REJECTED_IN = frozenset(
    {
        # Declares BANNED_TENANT_HEADER.
        "crates/fabric-identity/src/config.rs",
        # Reads it for one purpose: to refuse the request.
        "crates/fabric-identity/src/resolver.rs",
    }
)


class Failure:
    """One violated invariant, with enough context to act on."""

    def __init__(self, invariant: str, detail: str, consequence: str) -> None:
        self.invariant = invariant
        self.detail = detail
        self.consequence = consequence

    def render(self) -> str:
        return f"  {self.invariant}\n    {self.detail}\n    why it matters: {self.consequence}"


class Graph:
    """The workspace's dependency graph, as cargo resolved it."""

    def __init__(self, metadata: dict) -> None:
        self._members = {
            package["name"]
            for package in metadata["packages"]
            if package["id"] in metadata["workspace_members"]
        }
        self._direct = {
            package["name"]: {dependency["name"] for dependency in package["dependencies"]}
            for package in metadata["packages"]
            if package["name"] in self._members
        }
        # Everything cargo resolved, workspace crates included. This is what
        # makes "no driver is linked anywhere" a claim about the built binary
        # rather than about what someone remembered to write down.
        self._resolved = {package["name"] for package in metadata["packages"]}

    @property
    def crates(self) -> list[str]:
        """Workspace crate names, sorted."""
        return sorted(self._members)

    def direct_dependencies(self, crate: str) -> set[str]:
        """Everything a crate declares, across normal, dev and build tables."""
        return self._direct.get(crate, set())

    def internal_dependencies(self, crate: str) -> set[str]:
        """The workspace crates a crate depends on."""
        return self.direct_dependencies(crate) & self._members

    def resolved_contains(self, names) -> set[str]:
        """Which of `names` cargo resolved into the graph at all."""
        return self._resolved & set(names)


def load_graph() -> Graph:
    """Ask cargo for the resolved workspace metadata."""
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--all-features"],
        cwd=REPO_ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return Graph(json.loads(result.stdout))


def source_files(crate: str):
    """Every Rust source file in a crate, tests included.

    `tests/` is scanned as well as `src/`. An integration test is compiled
    Rust that can name whatever it likes, so a check that only looked at
    `src/` would let an NDC type or a tenant header in through the back door
    -- and a test is exactly where someone would first reach for one.
    """
    root = CRATES / crate
    return sorted(
        path
        for directory in ("src", "tests", "benches", "examples")
        if (root / directory).is_dir()
        for path in (root / directory).rglob("*.rs")
    )


def strip_comments_and_docs(text: str) -> str:
    """Remove line comments and doc comments.

    Prose is allowed to discuss anything -- the NDC boundary is *explained* in
    several crates that must never *use* it, and a check that could not tell
    the difference would punish the documentation that makes the boundary
    understandable.
    """
    return re.sub(r"^\s*(//!|///|//).*$", "", text, flags=re.MULTILINE)


def check_ndc_containment(graph: Graph) -> list[Failure]:
    """NDC vocabulary appears only where ADR 0001 permits it."""
    failures = []
    pattern = re.compile(r"\bNdc[A-Z]\w*|\bndc_models\b|\bNDC_VERSION\w*")

    for crate in graph.crates:
        if crate == NDC_CRATE:
            continue

        for path in source_files(crate):
            code = strip_comments_and_docs(path.read_text(encoding="utf-8"))
            found = {match.group(0) for match in pattern.finditer(code)}

            if crate == HOST:
                found -= NDC_NAMES_THE_HOST_MAY_USE

            if found:
                failures.append(
                    Failure(
                        "NDC types stay inside fabric-connector-ndc (ADR 0001)",
                        f"{path.relative_to(REPO_ROOT)} names {sorted(found)}",
                        "NDC is an internal connector protocol. A protocol type "
                        "reaching a crate above this boundary is the first step "
                        "toward the public Data API becoming the NDC API, which "
                        "would make the connector impossible to replace.",
                    )
                )

    # The dependency edge itself, not just the vocabulary.
    for crate in graph.crates:
        if crate in (NDC_CRATE, HOST):
            continue
        if NDC_CRATE in graph.direct_dependencies(crate):
            failures.append(
                Failure(
                    "Only the host depends on fabric-connector-ndc (ADR 0001)",
                    f"{crate} declares a dependency on {NDC_CRATE}",
                    "Every crate that can see the NDC crate is a crate a "
                    "protocol detail can leak into. Only the composition root "
                    "needs it, and only to build a connector at startup.",
                )
            )

    return failures


def check_domain_crates_have_no_transport(graph: Graph) -> list[Failure]:
    """Domain crates do not know what HTTP is."""
    failures = []

    for crate in sorted(DOMAIN_CRATES):
        if crate not in graph.crates:
            continue

        offenders = graph.direct_dependencies(crate) & HTTP_CRATES
        # fabric-identity reads bearer tokens out of headers, so it needs the
        # `http` types -- but `http` is a type crate, not a server or client,
        # and is deliberately absent from HTTP_CRATES for that reason.
        if offenders:
            failures.append(
                Failure(
                    "Domain crates carry no HTTP transport",
                    f"{crate} declares {sorted(offenders)}",
                    "The Data API's shape must be replaceable without touching "
                    "tenant resolution or the connector boundary. A transport "
                    "dependency in a domain crate is how that stops being true.",
                )
            )

    return failures


def check_no_forbidden_dependencies(graph: Graph) -> list[Failure]:
    """No database driver and no control-plane client, anywhere."""
    failures = []

    # Checked against the whole resolved graph, not per crate. A driver
    # arriving transitively is exactly as linked as one declared directly --
    # it compiles into the binary either way -- and it is far easier to miss
    # in review, because no manifest in this repository mentions it.
    drivers = graph.resolved_contains(DATABASE_DRIVERS)
    if drivers:
        failures.append(
            Failure(
                "The runtime plane opens no database connections (ADR 0001, section 2)",
                f"the resolved graph contains {sorted(drivers)}; "
                f"{describe_paths(graph, drivers)}",
                "Physical connections belong to connector processes. A driver "
                "linked here means the platform could hand an application a "
                "connection, or open one on the request path.",
            )
        )

    control_plane = graph.resolved_contains(CONTROL_PLANE_CLIENTS)
    if control_plane:
        failures.append(
            Failure(
                "Git and Kubernetes are never in the request path (section 6)",
                f"the resolved graph contains {sorted(control_plane)}; "
                f"{describe_paths(graph, control_plane)}",
                "Reconciled state is read from local files that the control "
                "plane writes. A client linked here means a handler could call "
                "the API server while serving a request, coupling every "
                "tenant's latency to the control plane's availability.",
            )
        )

    return failures


def describe_paths(graph: "Graph", names: set) -> str:
    """Says whether each name was declared directly or arrived transitively.

    A transitive arrival is the harder case to act on, so the message has to
    distinguish them rather than just naming the crate.
    """
    parts = []
    for name in sorted(names):
        declarers = [crate for crate in graph.crates if name in graph.direct_dependencies(crate)]
        parts.append(f"{name} via {', '.join(declarers)}" if declarers else f"{name} transitively")
    return "; ".join(parts)


def check_tenant_header_is_never_a_source(graph: Graph) -> list[Failure]:
    """`X-Tenant-Id` is rejected, never read as an identity (section 11)."""
    failures = []
    # Case-insensitive on purpose. HTTP header names are case-insensitive and
    # `http` lowercases the lookup key, so `headers.get("X-TENANT-ID")` reads
    # the very same header a case-sensitive pattern would miss.
    header = re.compile(r"x-tenant-id", re.IGNORECASE)

    for crate in graph.crates:
        for path in source_files(crate):
            # Prose may discuss the header freely; the module docs explaining
            # why it is refused are the reason anyone understands the rule.
            code = strip_comments_and_docs(path.read_text(encoding="utf-8"))
            if not header.search(code):
                continue

            # Scoped to the two files that reject it, not to the whole of
            # `fabric-identity`. Exempting the entire crate would exempt the
            # one crate where reading the header is even plausible, which is
            # the opposite of what this check is for. Tests may name it
            # anywhere, since asserting the rejection means writing it down.
            relative = path.relative_to(REPO_ROOT)
            permitted = (
                str(relative) in TENANT_HEADER_IS_REJECTED_IN
                or "tests" in relative.parts
                or path.name.endswith("_tests.rs")
            )

            if not permitted:
                failures.append(
                    Failure(
                        "X-Tenant-Id is rejected, never read (section 11)",
                        f"{relative} mentions the header",
                        "Tenant identity comes from the canonical bearer claim "
                        "and nothing else. A caller-supplied header that any "
                        "code path reads is a cross-tenant access primitive.",
                    )
                )

    return failures


def check_dependency_direction(graph: Graph) -> list[Failure]:
    """The graph in docs/architecture/crate-dependencies.md is the real one."""
    expected = {
        "fabric-core": set(),
        "fabric-identity": {"fabric-core"},
        "fabric-connector": {"fabric-core"},
        "fabric-tenant-runtime": {"fabric-core", "fabric-connector"},
        "fabric-connector-ndc": {"fabric-core", "fabric-connector", "fabric-tenant-runtime"},
        "fabric-data-api": {
            "fabric-core",
            "fabric-identity",
            "fabric-tenant-runtime",
            "fabric-connector",
        },
        "fabric-api": {
            "fabric-core",
            "fabric-identity",
            "fabric-tenant-runtime",
            "fabric-connector",
            "fabric-connector-ndc",
            "fabric-data-api",
        },
        # The control plane. A separate graph, sharing only `fabric-core`.
        "fabric-client-model": {"fabric-core"},
        "fabric-reconciliation": {"fabric-core", "fabric-client-model"},
        "fabric-control-plane": {
            "fabric-core",
            "fabric-client-model",
            "fabric-reconciliation",
        },
        # The two adapters depend *inward* on the ports they implement, which
        # is why the arrows point this way and not the other. Nothing in the
        # control-plane domain depends on either of them; only the composition
        # root does.
        #
        # `fabric-keycloak` implements two ports from two crates, because
        # Keycloak is two things to this platform: the identity provider
        # reconciliation drives (`IdentityProvider`, owned by
        # `fabric-reconciliation`) and the realm operators themselves sign in
        # against (`OperatorSignIn`, owned by `fabric-control-plane`). The
        # second edge is the same shape as `fabric-client-git`'s and is there
        # for the same reason.
        "fabric-keycloak": {
            "fabric-core",
            "fabric-client-model",
            "fabric-reconciliation",
            "fabric-control-plane",
        },
        "fabric-client-git": {
            "fabric-core",
            "fabric-client-model",
            "fabric-control-plane",
        },
        # The third adapter, and the first that is not about a client at all:
        # it implements the ports through which the platform keeps its *own*
        # durable state. Same direction as the other two.
        "fabric-openbao": {
            "fabric-core",
            "fabric-control-plane",
        },
        # The trust boundary itself: it verifies a tenant user's token against a
        # registry of issuers and produces the identity a decision is made
        # about. No edge to `fabric-identity` on purpose -- that derives a
        # tenant from a token the ingress already established, this establishes
        # trust in the first place (ADR 0016).
        "fabric-fga-auth": {
            "fabric-core",
        },
        "fabric-control-plane-api": {
            "fabric-core",
            "fabric-client-model",
            "fabric-reconciliation",
            "fabric-control-plane",
            "fabric-keycloak",
            "fabric-client-git",
            "fabric-openbao",
        },
    }

    failures = []
    for crate in graph.crates:
        allowed = expected.get(crate)
        if allowed is None:
            failures.append(
                Failure(
                    "Every crate's place in the graph is declared",
                    f"{crate} is not described in docs/architecture/crate-dependencies.md",
                    "A crate nobody has placed in the graph is a crate whose "
                    "dependency direction nothing is checking.",
                )
            )
            continue

        unexpected = graph.internal_dependencies(crate) - allowed
        if unexpected:
            failures.append(
                Failure(
                    "Dependencies point one way (docs/architecture/crate-dependencies.md)",
                    f"{crate} depends on {sorted(unexpected)}, which the documented graph does not allow",
                    "The layering is what keeps the Data API replaceable and the "
                    "connector boundary swappable. Either the change is wrong, or "
                    "the document is out of date -- and the document is the thing "
                    "reviewers read.",
                )
            )

    return failures


def check_the_planes_do_not_meet(graph: Graph) -> list[Failure]:
    """No crate in one plane depends on a crate in the other."""
    failures = []

    for crate in graph.crates:
        if crate in RUNTIME_PLANE:
            offenders = graph.direct_dependencies(crate) & CONTROL_PLANE
            other = "the control plane"
        elif crate in CONTROL_PLANE:
            offenders = graph.direct_dependencies(crate) & RUNTIME_PLANE
            other = "the runtime plane"
        else:
            continue

        if offenders:
            failures.append(
                Failure(
                    "The runtime and control planes share only fabric-core (ADR 0008)",
                    f"{crate} depends on {sorted(offenders)}, which is in {other}",
                    "The runtime plane must keep serving tenants while Git and "
                    "Keycloak are unreachable, and the control plane must be "
                    "deployable on a different network with a different identity "
                    "model. One edge between them puts control-plane availability "
                    "behind every tenant request, which section 6 forbids.",
                )
            )

    return failures


def check_adapter_containment(graph: Graph) -> list[Failure]:
    """Platform-service vocabulary stays inside its adapter crate.

    Two adapters, one rule. `fabric-keycloak` owns every Keycloak
    representation, and `fabric-client-git` owns every Git-hosting concept.
    Neither vocabulary may appear anywhere else -- which is the same
    containment ADR 0001 applies to NDC, for the same reason: a representation
    that escapes its adapter turns the platform's own model into a thin wrapper
    over somebody else's, and the API stops being about clients and starts
    being about realms and blobs.
    """
    adapters = (
        (
            KEYCLOAK_CRATE,
            re.compile(
                r"\b\w*Representation\b|\bRealmUpdate\b|\bTokenResponse\b"
                r"|\bpublicClient\b|\bstandardFlowEnabled\b|\bopenid-connect\b"
            ),
            "Keycloak representations stay inside fabric-keycloak (ADR 0008)",
            "Keycloak is an implementation of client identity, not the "
            "platform's model of it. A representation reaching a crate above "
            "the adapter is the first step toward the control-plane API "
            "becoming the Keycloak admin API, which the UI is explicitly not "
            "allowed to be (section 16).",
        ),
        (
            GIT_CRATE,
            re.compile(
                r"\bContentsEntry\b|\bPutContents\w*\b|\bWrittenContent\b"
                r"|\bcontents/\b|\bblob_sha\b|\bgit_url\b"
            ),
            "Git-hosting details stay inside fabric-client-git (section 8)",
            "Git is an implementation detail of the control plane. A path, a "
            "blob, or a commit reaching a crate above the adapter is how an "
            "operator ends up being told which file and which line, instead of "
            "which client and which rule.",
        ),
    )

    failures = []

    for owner, pattern, invariant, consequence in adapters:
        for crate in graph.crates:
            if crate == owner:
                continue

            for path in source_files(crate):
                code = strip_comments_and_docs(path.read_text(encoding="utf-8"))
                found = {match.group(0) for match in pattern.finditer(code)}

                if found:
                    failures.append(
                        Failure(
                            invariant,
                            f"{path.relative_to(REPO_ROOT)} names {sorted(found)}",
                            consequence,
                        )
                    )

        # The dependency edge itself, not just the vocabulary. Only the
        # control-plane composition root may see an adapter crate.
        for crate in graph.crates:
            if crate in (owner, CONTROL_PLANE_HOST):
                continue
            if owner in graph.direct_dependencies(crate):
                failures.append(
                    Failure(
                        invariant,
                        f"{crate} declares a dependency on {owner}",
                        consequence,
                    )
                )

    return failures


def check_the_browser_gets_no_platform_credentials() -> list[Failure]:
    """The operator UI holds no credential and calls no platform service.

    Section 15 and acceptance criteria 13 and 14: the browser must never
    receive a Keycloak administrative credential and must never call the
    Keycloak admin API -- or the Git host -- directly.

    Checked as a property of the UI's own source rather than of what a
    particular response happened to contain, because that is the form the rule
    actually takes: the UI talks to the SaaS Fabric control-plane API and to
    nothing else. A fetch to another origin is the violation, whether or not a
    credential is in the same commit.
    """
    ui = REPO_ROOT / "apps" / "control-plane-ui" / "src"
    if not ui.is_dir():
        return []

    forbidden = re.compile(
        r"client_secret|clientSecret"
        r"|/admin/realms"
        r"|api\.github\.com"
        r"|keycloak.*admin|admin.*keycloak",
        re.IGNORECASE,
    )

    failures = []

    for path in sorted(ui.rglob("*.ts")) + sorted(ui.rglob("*.tsx")):
        text = path.read_text(encoding="utf-8")
        # Line comments are prose and may explain the boundary; the rest is code.
        code = re.sub(r"^\s*(//|\*|/\*).*$", "", text, flags=re.MULTILINE)
        found = {match.group(0) for match in forbidden.finditer(code)}

        if found:
            failures.append(
                Failure(
                    "The operator UI reaches only the control-plane API (section 15)",
                    f"{path.relative_to(REPO_ROOT)} names {sorted(found)}",
                    "A browser that can call Keycloak's admin API or the Git "
                    "host is a browser holding a credential for one of them. "
                    "The control plane exists so that no such credential ever "
                    "leaves the cluster.",
                )
            )

    return failures


def check_the_console_widens_its_policy_only_for_the_manifest_post() -> list[Failure]:
    """The console's CSP permits github.com for form submission, and only that.

    Two failures this guards, in opposite directions.

    Tightening `form-action` back to `'self'` silently breaks in-product
    installation at its first step: creating the GitHub App is a cross-origin
    form POST, because the App Manifest flow takes the manifest as POST data
    and navigates the operator to an approval screen. Nothing else fails, no
    test goes red, and the console reports only a console message the operator
    will not see.

    Widening any *other* directive is the opposite mistake. The exception is
    for submitting a form, not for loading or calling anything: github.com in
    `default-src` or `connect-src` would let the console fetch from an origin
    the control plane exists to keep it away from -- the rule
    `check_the_browser_gets_no_platform_credentials` states about source, held
    here about the policy that enforces it.
    """
    conf = REPO_ROOT / "apps" / "control-plane-ui" / "nginx.conf"
    if not conf.is_file():
        return []

    text = re.sub(r"^\s*#.*$", "", conf.read_text(encoding="utf-8"), flags=re.MULTILINE)
    header = re.search(r'add_header\s+Content-Security-Policy\s+"([^"]*)"', text)

    if not header:
        return [
            Failure(
                "The console serves a Content-Security-Policy",
                "apps/control-plane-ui/nginx.conf declares no CSP header",
                "The console is the operator's only interface and it ships "
                "with a policy. Serving none is not a smaller policy.",
            )
        ]

    directives = {}
    for directive in header.group(1).split(";"):
        parts = directive.split()
        if parts:
            directives[parts[0]] = set(parts[1:])

    failures = []
    host = "https://github.com"

    if host not in directives.get("form-action", set()):
        failures.append(
            Failure(
                "The console can post the App manifest to GitHub",
                f"form-action is {sorted(directives.get('form-action', set()))},"
                f" which does not permit {host}",
                "Creating the GitHub App is a cross-origin form POST. Without "
                "this the browser blocks the submission and in-product "
                "installation stops at its first step, silently.",
            )
        )

    for name, values in sorted(directives.items()):
        if name != "form-action" and any("github" in value for value in values):
            failures.append(
                Failure(
                    "GitHub is permitted for form submission and nothing else",
                    f"{name} names {sorted(value for value in values if 'github' in value)}",
                    "The console posts a manifest to GitHub; it never loads "
                    "from GitHub or calls it. A fetch to the Git host is a "
                    "browser holding a credential for it.",
                )
            )

    return failures


CHECKS = (
    ("NDC containment", check_ndc_containment),
    ("Platform-service adapter containment", check_adapter_containment),
    ("The planes do not meet", check_the_planes_do_not_meet),
    ("Transport stays out of the domain", check_domain_crates_have_no_transport),
    ("No drivers, no control-plane clients", check_no_forbidden_dependencies),
    ("X-Tenant-Id is never an identity source", check_tenant_header_is_never_a_source),
    ("Dependency direction", check_dependency_direction),
    # Takes no graph: it is a statement about the UI's source, not about
    # cargo's resolution. `main` calls it with no argument.
    ("The browser gets no platform credentials", check_the_browser_gets_no_platform_credentials),
    ("The console's policy widens for one post", check_the_console_widens_its_policy_only_for_the_manifest_post),
)


def main() -> int:
    graph = load_graph()
    if not graph.crates:
        print("error: no workspace crates found -- is this the repository root?", file=sys.stderr)
        return 1

    total = 0
    for title, check in CHECKS:
        # One check inspects the UI's source rather than the crate graph, so it
        # takes no argument. Branching on the signature keeps the table one
        # list rather than two.
        failures = check() if check.__code__.co_argcount == 0 else check(graph)
        status = "ok" if not failures else f"FAILED ({len(failures)})"
        print(f"{status:>14}  {title}")
        for failure in failures:
            print(failure.render())
        total += len(failures)

    print()
    if total:
        print(f"{total} architectural invariant(s) violated.")
        return 1

    print(f"OK: {len(CHECKS)} architectural invariants hold across {len(graph.crates)} crates.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
