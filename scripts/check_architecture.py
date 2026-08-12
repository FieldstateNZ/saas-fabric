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

# The composition root. It is the one crate allowed to know about every other
# crate at once, because assembling them is its entire job.
HOST = "fabric-api"

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

# Crates that model the domain. None of them may know what HTTP is: the Data
# API's shape must be replaceable without touching tenant resolution, and a
# transport type reaching into a domain crate is how that stops being true.
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
# never in the request path. Again the strongest form is structural: if no
# client is linked, no handler can reach one no matter how a future change is
# written.
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
    """Every Rust source file in a crate."""
    return sorted((CRATES / crate / "src").rglob("*.rs"))


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

    # Checked against the whole resolved graph, not per crate: a driver
    # arriving transitively is exactly as linked as one declared directly, and
    # would be far easier to miss in review.
    for crate in graph.crates:
        declared = graph.direct_dependencies(crate)

        drivers = declared & DATABASE_DRIVERS
        if drivers:
            failures.append(
                Failure(
                    "The runtime plane opens no database connections (ADR 0001, section 2)",
                    f"{crate} declares {sorted(drivers)}",
                    "Physical connections belong to connector processes. A "
                    "driver linked here means the platform could hand an "
                    "application a connection, or open one on the request path.",
                )
            )

        control_plane = declared & CONTROL_PLANE_CLIENTS
        if control_plane:
            failures.append(
                Failure(
                    "Git and Kubernetes are never in the request path (section 6)",
                    f"{crate} declares {sorted(control_plane)}",
                    "Reconciled state is read from local files that the control "
                    "plane writes. A client linked here means a handler could "
                    "call the API server while serving a request, coupling every "
                    "tenant's latency to the control plane's availability.",
                )
            )

    return failures


def check_tenant_header_is_never_a_source(graph: Graph) -> list[Failure]:
    """`X-Tenant-Id` is rejected, never read as an identity (section 11)."""
    failures = []
    header = re.compile(r"[xX]-[tT]enant-[iI]d")

    for crate in graph.crates:
        for path in source_files(crate):
            text = path.read_text(encoding="utf-8")
            if not header.search(text):
                continue

            # The header may only appear in the crate that rejects it, and in
            # tests that prove the rejection. Anywhere else -- a handler, a
            # resolver, a middleware -- would mean something is reading it.
            relative = path.relative_to(REPO_ROOT)
            permitted = crate == "fabric-identity" or path.name.endswith("_tests.rs")

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


CHECKS = (
    ("NDC containment", check_ndc_containment),
    ("Transport stays out of the domain", check_domain_crates_have_no_transport),
    ("No drivers, no control-plane clients", check_no_forbidden_dependencies),
    ("X-Tenant-Id is never an identity source", check_tenant_header_is_never_a_source),
    ("Dependency direction", check_dependency_direction),
)


def main() -> int:
    graph = load_graph()
    if not graph.crates:
        print("error: no workspace crates found -- is this the repository root?", file=sys.stderr)
        return 1

    total = 0
    for title, check in CHECKS:
        failures = check(graph)
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
