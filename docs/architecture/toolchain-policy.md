# Rust toolchain policy

- **Status:** Accepted
- **Applies to:** every Rust build of this workspace, local and CI.
- **Pinned in:** [`rust-toolchain.toml`](../../rust-toolchain.toml) at the
  repository root.
- **Enforced by:** rustup, which honours that file automatically, and
  [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml), which installs
  from it rather than choosing a version of its own.

## The policy

**One Rust version, named in one file, upgraded deliberately.**

1. `rust-toolchain.toml` is the only place a Rust version appears. No workflow,
   no `Cargo.toml`, and no developer's shell decides it.
2. **Upgrading is its own commit**, with the lints it turns up fixed in the same
   commit. Never bundled into a feature branch.
3. **A new lint is fixed, not silenced.** The upgrade is the moment to look at
   what the compiler learned; `#[allow]` needs the same justification any other
   suppression in this repository needs.
4. **Bump within a week of a stable release**, so the gap never grows large
   enough that an upgrade becomes its own project.

## Why pinned rather than `stable`

This workspace denies every lint. `-D warnings` in CI, plus `unwrap`, `expect`,
`panic`, `indexing_slicing` and `missing_docs` denied in `[workspace.lints]`,
and `unsafe_code` forbidden. That strictness is deliberate and worth keeping —
but it changes what a compiler upgrade *is*.

On a project that warns, a new clippy lint produces warnings somebody reads
later. Here it produces a **failing build**. And on an unpinned toolchain it
fails whichever pull request happens to be open when the release lands, in code
that pull request never touched.

That is not hypothetical. It is what happened to
[pull request #2](https://github.com/FieldstateNZ/saas-fabric/pull/2): clippy
1.98 introduced `unused_async_trait_impl`, which fired on `fabric-identity`'s
extractor — a runtime-plane file a control-plane branch had not opened. The
lint was correct and the fix was an improvement, but the review conversation it
interrupted was about something else entirely, and "this PR didn't touch that
file" was true and irrelevant.

The second reason matters just as much day to day: without a pin, "clippy is
clean" means *clean on whatever stable each developer last installed*. The same
pull request passed locally on 1.97 and failed in CI on 1.98, and nothing about
either result was wrong. A file at the repository root makes the two the same
question.

## What is given up, and how it is paid back

The honest cost: **CI stops being an early-warning system.** On `stable`, a new
lint arrives the day it ships, on somebody else's branch, for free. Pinned, it
arrives when someone bumps the pin — and if nobody does, it never arrives, and
the strictness quietly becomes strictness against a 2026 compiler forever.

Rule 4 is what pays that back, and it is the rule most likely to be forgotten.
A pin nobody bumps is worse than no pin: it keeps the reproducibility and loses
the lints, while looking maintained. If this file's `channel` is more than a
release or two behind, that is the finding, not a detail.

Bumping is cheap by design — one line, then `cargo clippy --workspace
--all-targets -- -D warnings` — precisely so that doing it often stays easy.

## How to upgrade

```bash
# 1. Edit the channel in rust-toolchain.toml, then let rustup fetch it.
rustup toolchain install

# 2. Confirm what is actually active. If this disagrees with the file, an
#    environment override (`rustup override`, RUSTUP_TOOLCHAIN) is in the way.
rustup show active-toolchain

# 3. Every gate, because a new release moves rustc, clippy and rustfmt at once.
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test --workspace
```

Then record the version in [`verification.md`](../verification.md), which names
the toolchain a run was measured on for this reason.

## What this does not pin

**Dependency versions.** Those are pinned by `Cargo.lock`, which is committed,
and governed by [`dependency-policy.md`](dependency-policy.md). Different
mechanism, different policy, same intent: a build should be reproducible, and a
change to what it contains should be visible in a diff.

**The Node version.** The operator console's CI job pins it in the workflow, and
its dependencies in `package-lock.json`. Whether that deserves the same
treatment as this file is a live question — but the pressure that produced this
policy does not exist there, because the console's lint rules are ours rather
than the toolchain's, and a Node upgrade does not invent new errors in code
nobody touched.
