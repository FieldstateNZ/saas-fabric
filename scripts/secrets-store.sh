#!/usr/bin/env bash
# Starts an OpenBao the client-secrets suite can log into, the way the pod does.
#
# The adapter authenticates by presenting a signed JWT to an auth mount, so a
# fixture that handed it a root token would exercise a code path production
# never takes. This configures the real thing: an RSA key, a JWT auth mount
# that trusts it, a role, and a signed token on disk.
set -euo pipefail

PORT=18500
ROOT=probe-root
DIR="${TMPDIR:-/tmp}/fabric-secrets-fixture"

case "${1:-up}" in
  up)
    docker rm -f fabric-secrets-store >/dev/null 2>&1 || true
    docker run -d --name fabric-secrets-store -p "${PORT}:8200" \
      -e BAO_DEV_ROOT_TOKEN_ID="${ROOT}" -e BAO_DEV_LISTEN_ADDRESS=0.0.0.0:8200 \
      openbao/openbao:latest server -dev >/dev/null

    for _ in $(seq 1 30); do
      curl -sf -m 2 "http://127.0.0.1:${PORT}/v1/sys/health" >/dev/null 2>&1 && break
      sleep 1
    done

    mkdir -p "${DIR}"
    B="http://127.0.0.1:${PORT}/v1"
    H="X-Vault-Token: ${ROOT}"

    # The client's boundary, and the mount its secrets live on.
    curl -s -o /dev/null -H "$H" -X POST "$B/sys/namespaces/acme" -d '{}'
    curl -s -o /dev/null -H "$H" -H "X-Vault-Namespace: acme" -X POST "$B/sys/mounts/secret" \
      -d '{"type":"kv","options":{"version":"2"}}'

    # A second boundary, so isolation can be asserted rather than assumed.
    curl -s -o /dev/null -H "$H" -X POST "$B/sys/namespaces/contoso" -d '{}'
    curl -s -o /dev/null -H "$H" -H "X-Vault-Namespace: contoso" -X POST "$B/sys/mounts/secret" \
      -d '{"type":"kv","options":{"version":"2"}}'

    python3 - "$DIR" <<'PY'
import base64, json, sys, time
from pathlib import Path
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import padding, rsa

directory = Path(sys.argv[1])
key = rsa.generate_private_key(public_exponent=65537, key_size=2048)

directory.joinpath("public.pem").write_bytes(
    key.public_key().public_bytes(
        serialization.Encoding.PEM,
        serialization.PublicFormat.SubjectPublicKeyInfo,
    )
)

def segment(value):
    return base64.urlsafe_b64encode(json.dumps(value).encode()).rstrip(b"=")

now = int(time.time())
signing_input = b".".join([
    segment({"alg": "RS256", "typ": "JWT"}),
    segment({
        "iss": "fabric-fixture",
        "sub": "system:serviceaccount:operator-system:saas-fabric-control-plane",
        "aud": "openbao",
        "iat": now,
        "exp": now + 86_400,
    }),
])
signature = key.sign(signing_input, padding.PKCS1v15(), hashes.SHA256())
token = signing_input + b"." + base64.urlsafe_b64encode(signature).rstrip(b"=")

directory.joinpath("token").write_bytes(token)
PY

    # The policy the platform's own credential carries.
    #
    # `+` matches exactly one namespace segment, so this grants every client's
    # boundary and nothing deeper — one policy that never needs reconciling
    # when a client is added, and that cannot reach a nested namespace.
    #
    # Policies are looked up in the *token's* namespace, so a bare `secret/*`
    # here grants the root namespace and no client's. Measured: it answers
    # `permission denied` for every client operation.
    curl -s -o /dev/null -H "$H" -X PUT "$B/sys/policies/acl/fabric-client-secrets" \
      -d '{"policy":"path \"+/secret/*\" { capabilities = [\"create\",\"read\",\"update\",\"delete\",\"list\"] }"}'

    # Enable the auth method before configuring it. Omitting this answers
    # "no handler for route", and the login then fails as "permission denied"
    # — a message about the credential for a problem with the mount.
    curl -s -o /dev/null -H "$H" -X POST "$B/sys/auth/jwt" -d '{"type":"jwt"}'

    PUBKEY=$(python3 -c "import json,sys;print(json.dumps(open('${DIR}/public.pem').read()))")
    curl -s -o /dev/null -H "$H" -X POST "$B/auth/jwt/config" \
      -d "{\"jwt_validation_pubkeys\":[${PUBKEY}],\"bound_issuer\":\"fabric-fixture\"}"
    curl -s -o /dev/null -H "$H" -X POST "$B/auth/jwt/role/saas-fabric-control-plane" \
      -d '{"role_type":"jwt","bound_audiences":["openbao"],"user_claim":"sub","token_policies":["fabric-client-secrets"],"token_ttl":"1h"}'

    cat <<EOF

Run the suite with:

  FABRIC_SECRETS_STORE=http://127.0.0.1:${PORT} \\
  FABRIC_SECRETS_TOKEN_FILE=${DIR}/token \\
  cargo test -p fabric-openbao --test client_secrets
EOF
    ;;

  down)
    docker rm -f fabric-secrets-store >/dev/null 2>&1 || true
    rm -rf "${DIR}"
    echo stopped
    ;;

  *) echo "usage: $0 [up|down]" >&2; exit 2 ;;
esac
