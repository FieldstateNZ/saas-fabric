# fabric-fga-check

The Data API's layer-2 adapter (ADR 0027). It answers one question for
`fabric-data-api`: may the caller who sent this token perform this operation
on this record? It answers by asking the Fabric authorization front
(`fabric-fga-auth-api`, ADR 0016).

## How a question becomes a request

```text
CheckQuestion { operation: Update, object: customers:42 }   + the caller's bearer
        │
        ▼
POST {endpoint}/v1/check
Authorization: Bearer <the caller's own token>
{"relation": "can_update", "object": "customers:42"}
        │
        ▼
200 {"allowed": true}   → Allowed
200 {"allowed": false}  → Denied
```

- **The relation is computed.** ADR 0013 maps declared relations (`viewer`,
  `editor`, …) onto a computed `can_<operation>` for each operation. This crate
  asks the computed one, so it never needs to know what an editor may do.
- **The token is the caller's.** The runtime has no credential of its own on
  this path, and ADR 0016 says it must not.
- **There is no user, store or tenant in the body.** The front takes all three
  from the verified token, and it refuses a body that names them.

## Every failure refuses

| Front's answer | Result | What the Data API answers |
|---|---|---|
| connection refused, reset or timed out (5s) | `Unavailable` | 503, retryable |
| any `5xx` | `Unavailable` | 503, retryable |
| `401`, `400`, `403`, a redirect, any other non-`200` | `Internal` | 500 |
| `200` with a body that is not `{"allowed": bool}` | `Internal` | 500 |

A `403` is not how the front denies, so it is not read as a denial. A `401`
means the front's issuer registry refused a token the runtime's ingress
accepted, which is the platform disagreeing with itself.

The client follows no redirects and uses no proxy from the environment,
because either would carry the caller's token to an address nobody configured.

## Status

Not wired. OpenFGA is not deployed and no front endpoint is configured
(D01-10a), so `fabric-api` does not depend on this crate. Wiring it means
naming a `FrontEndpoint` in a separately reviewed change. That change also
decides whether the hop must be TLS: `FrontEndpoint` accepts `http` and
`https` today.

## Tests

`tests/front_check_over_loopback.rs` runs the adapter against a fake front on
`127.0.0.1`. It uses a real socket so the test sees what actually goes out.
No OpenFGA, no credential and no external network are involved.

Log filter: the crate is `fabric-fga-check`, and the target is
`fabric_fga_check`. It emits no events of its own: the Data API records every
refusal and every 5xx.
