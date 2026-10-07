# Disposable dogfood activation packet

Status: NOT AUTHORIZED until a named authorizer approves this exact packet (by its sha256 below) for one activation.
This profile is NOT WorkSpec, runtime or tenant acceptance. Off-host backup is not a gate for it.

## Source and images

- source commit: bb864f4cb5204d176c511a83b540fb7e90c66c16
- control plane image: sha256:a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1 (git archive of the commit, target control-plane-api)
- console image: sha256:b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2 (git archive of the commit, target console)
- keycloak image: sha256:c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3 (derived FROM quay.io/keycloak/keycloak@sha256:9d1f1b2b7261ff53c66cb1092dfcdc34a5fb77e81f9e6a6e75b8b6a795de8067)
- canary probe image: node:22-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5
- daemon: ID 47e4864c-619b-4190-8ef5-65d1feceb155, Name lucentroot, ServerVersion 29.5.3

Rendered artefacts (sha256; `check` refuses any other bytes):

- control-plane.toml: 3dd6e2566dae041c4d7dd64e208ce1ad7c713e85f6d05fc23796f215a49e405b
- console.nginx.conf: c4fab1a2c23b1a7901045d5d4f7b23d323038d10d8934110e0fefd00a90a6531
- compose.json: 368fbff92ce49348049b7781676b4655ab47ed8a5a9f4148f66ba52053e77610
- pins.lock.json: 4f73c5b69e45410c7764dc2d2e263afed074f8e97373f78932f49b7b2139ccc3

## Ports

- console: 127.0.0.1:18780 -> console:8080/tcp
- disposable keycloak (issuer http://127.0.0.1:18781/realms/fabric-dogfood): 127.0.0.1:18781 -> keycloak:8080/tcp
- control plane: unpublished, cp:8081 on fabric-dogfood-internal only
- network: fabric-dogfood-internal, bridge fabric-dogfood0, subnet 10.213.7.0/24, internal, IPv6 off, no masquerade
- SSH key options: restrict,port-forwarding,permitopen="127.0.0.1:18780",permitopen="127.0.0.1:18781",command="/bin/false"
- SSH forward: ssh -N -o ExitOnForwardFailure=yes -L 127.0.0.1:18780:127.0.0.1:18780 -L 127.0.0.1:18781:127.0.0.1:18781 <forwarding-user>@<execution-host>
- sshd for the forwarding user only (required; OpenSSH 7.8 or newer): AllowTcpForwarding local, PermitListen none, X11Forwarding no, PermitTTY no

## Host firewall gate (applied by the operator before activate; activate and canaries refuse without it)

iptables -I INPUT 1 -i fabric-dogfood0 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
iptables -I INPUT 2 -i fabric-dogfood0 -j DROP
iptables -I DOCKER-USER 1 -i fabric-dogfood0 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
iptables -I DOCKER-USER 2 -s 10.213.7.0/24 ! -d 10.213.7.0/24 -j DROP
iptables -I DOCKER-USER 3 -i fabric-dogfood0 ! -o fabric-dogfood0 -j DROP

Exact inverse (after reset, one rule at a time; no flush, no policy change):

iptables -D INPUT -i fabric-dogfood0 -j DROP
iptables -D INPUT -i fabric-dogfood0 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
iptables -D DOCKER-USER -i fabric-dogfood0 ! -o fabric-dogfood0 -j DROP
iptables -D DOCKER-USER -s 10.213.7.0/24 ! -d 10.213.7.0/24 -j DROP
iptables -D DOCKER-USER -i fabric-dogfood0 -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT

Expected order: INPUT starts 2 rules, DOCKER-USER starts 3 rules, ESTABLISHED accepted first in each.

## Resource ceilings

| container | image | user | memory | cpus | pids | tmpfs | logs |
|---|---|---|---|---|---|---|---|
| fabric-dogfood-cp | sha256:a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1 | 65532:65532 | 512m | 1.0 | 256 | /var/lib/fabric/state 16 MiB, /tmp 16 MiB | 2 x 5m |
| fabric-dogfood-console | sha256:b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2 | 101:101 | 128m | 0.5 | 64 | /tmp 32 MiB | 2 x 5m |
| fabric-dogfood-keycloak | sha256:c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3 | 1000:1000 | 1536m | 2.0 | 512 | /opt/keycloak/data 64 MiB, /opt/keycloak/lib/quarkus 256 MiB, /tmp 64 MiB | 2 x 5m |
| fabric-dogfood-canary (transient, --rm) | node:22-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5 | 1000:1000 | 64m | 0.25 | 32 | none (read-only root) | none |

## Canary plan

Pre (`canaries --phase pre --yes`, after activate, before anyone signs in). Fails closed; writes a redacted receipt
with `iptables -S` and the network inspect under .canaries/:

- preconditions: daemon identity, ownership receipt, the three recorded containers running, the recorded network,
  the gate as the first rules of INPUT and DOCKER-USER, both loopback ports answering from the host
- positive controls from the probe on fabric-dogfood-internal: console:8080/healthz and keycloak:8080 realm discovery must answer 200
- must be blocked (timeout, unreachable or dns; a refusal fails): the bridge gateway on every host listening port
  observed in /proc/net/tcp and tcp6 plus 22, 53, 80, 443, 2379, 3000, 5432, 6443, 8080, 8200, 8443, 9000, 10250
  and 18780, 18781; every host listener bound to a specific address;
  1.1.1.1:53; https://example.com/ (which must fail name resolution: a resolved name leaked a lookup)

Post (`canaries --phase post`, read-only, once after reset with the gate in place and again after the REMOVE
lines): no container, network or volume carries com.docker.compose.project=fabric-dogfood-disposable; no container
or network has this profile's names; interface fabric-dogfood0 is gone; .out/ is gone; the gate is complete or
absent (no iptables rule names fabric-dogfood0 or 10.213.7.0/24), never partial.

## Activation sequence

Done before this packet could be printed: the reviewed files staged onto a detached checkout of the source
commit (README, staging), `build --yes` recording the image IDs above, `check` passing. After approval only:

1. Install the SSH key line and sshd Match block above for the forwarding user.
2. Apply the five gate rules above.
3. `activate --yes --packet-sha256 <the sha256 below>`; it refuses if these artefacts no longer produce this
   packet. Type a new synthetic operator password at the controlling terminal.
4. `canaries --phase pre --yes`. Any failure: stop and roll back; nobody signs in.
5. Open the SSH forward; sign in at the console origin as the printed operator username.

## Cleanup and rollback

The gate is never removed before nothing owned remains. Cleanup, in order, from any point after step 1:

1. `reset --yes`: docker rm --force each recorded container ID and docker network rm the recorded network ID,
   each re-verified first, then listed again; delete .out/ by exact file name. No prune, no compose down, no
   volume. Images are kept. If it refuses (a listing, inspection or removal it cannot verify), it keeps the
   receipt and local state: stop here, keep the gate, resolve, and run it again.
2. `canaries --phase post` with the gate still in place; it must pass.
3. Only then the five REMOVE lines above.
4. `canaries --phase post` again; it must pass and record the gate as absent.
5. Remove the SSH key line and the sshd Match block.

If activate fails after anything was created it rolls back itself and prints one of two outcomes:
- "Rolled back and verified": nothing owned remains and the credential and receipt are deleted; continue at
  cleanup step 1, which then only deletes the local state.
- "ROLLBACK NOT VERIFIED": keep the gate. The receipt and credential are kept; recover with `reset --yes` (by hand
  when there is no receipt) and continue at cleanup step 1. Nothing in this trial is persisted, so there is
  nothing to restore.
packet-sha256: 304f412f7d1eb45e7288ca5a0c3e52e133ae8cbdae9f3c1844ab9e0862950254
