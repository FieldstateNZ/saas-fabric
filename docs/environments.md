# Environments

Three of them, and they are not the same kind of thing.

| | tracks | changes reach it | breakage |
|---|---|---|---|
| **LucentRoot** | `main` | automatically, on merge | acceptable |
| **Production** | a version tag | only by promotion | not acceptable |
| **Release** | — | a decision, not an event | — |

## LucentRoot is where work becomes visible

It is the integration environment, and it tracks `main`. Every merge publishes
SHA-tagged images and LucentRoot moves to them.

**Breakage is acceptable there; invisibility is not.** Work that is merged but
not running on LucentRoot is work nobody can look at, and this repository
produced a console with four new tabs and a working Secrets feature that sat
unseen for a week because LucentRoot was pinned to a release.

That is why the delivery rule (`docs/delivery.md`) ends where it does. A slice
is not complete when its tests pass; it is complete when it is on LucentRoot
and can be used.

## Production is pinned, and nothing arrives by merging

It runs a version tag, and reaches it only by promotion. Nothing is deployed
there because it merged, passed, or looked ready.

## A release is a decision

Tagging `v0.x.y` says: *what we have been exercising on LucentRoot is coherent
enough to freeze.* It is not a build step and not a consequence of merging.

## What publishes what

```text
pull request   builds all three images, publishes nothing
merge to main  gates, then publishes  ghcr.io/…:sha-<commit>
tag v0.x.y     gates, then publishes  ghcr.io/…:0.x.y  and  :sha-<commit>
```

A merge publishes a **commit** and never a version. Naming an unreleased image
after a released version is how a cluster ends up running something nobody
chose.

The gates run again on a merge, and that is not duplication: squashing produces
a new commit, so the gates that passed on the pull request passed on a commit
that no longer exists — and the new one is what LucentRoot will run.
