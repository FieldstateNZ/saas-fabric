# M0 integration review: PR102 to PR107 (issue #109)

Revalidation of six draft PRs against `FieldstateNZ/saas-fabric` main `1f46788ecd5c59af1a4937aae0e51cf59b8f8081`, performed 2026-10-05 by an adversarial reviewer/verifier session. Read-only: nothing was pushed, approved, merged or commented on GitHub. This is a local review, not a formal GitHub approval. Closure of #109 still needs Riley's recorded disposition per PR.

## Summary

| PR | Head SHA | Area | Behind main? | Checks on head | Local tests on head | Blocking findings | Disposition |
|---|---|---|---|---|---|---|---|
| #102 | `13d08b6e943ec4d81e2f7f8ccf6f64ee2f1731b6` | Rust tests only | No (base = main tip) | 24/24 success | 3/3 new, 337/337 crate | None | Ready for Riley approval |
| #103 | `8094188eceaf472f37fc3045fdd7760cffab6fe8` | UI hook + tests | No | 24/24 success | 350/350 UI | None | Ready for Riley approval |
| #104 | `f0c8f0f95ecf992a6ac65d4f0547e257a074bc09` | UI deps (vitest 4) | No | 28 success, 2 skipped (tag-only) | 341/341 UI, audit 0 | None | Ready for Riley approval |
| #105 | `cd5cb1dda95c07bd2e39af2e23680967160f6307` | UI NewSecret + tests | No | 24/24 success | 359/359 UI | None | Ready for Riley approval |
| #106 | `12edde9a2458da780ced99256fdfba7f14d411c1` | UI secret metadata | No | 24/24 success | 358/358 UI | None | Ready for Riley approval (see test-gap note) |
| #107 | `d35ef278a302daa1584fba8e696bf53cc9e530ec` | UI IdentityPanel | No | 24/24 success | 347/347 UI | None | HOLD: keep draft, no merge, no deploy |

All six are open drafts, `mergeable_state: clean`, with zero review threads, zero review comments and zero PR comments. There are no existing findings to resolve. Issue #109 has no comments.

Composed result: all six merged onto main in order 102, 103, 104, 105, 106, 107 with no conflicts. On that composition `npm ci` (npm 10.9.4, Node 22.22.0), `vitest run` (391/391, 52 files), `eslint .`, `tsc -b && vite build` and `npm audit` (0 findings) all pass. This reproduces the 391 composed figure in PR106 and PR107.

## Base, merge-ability and overlap

Every PR was built directly on main `1f46788`. `git merge-base origin/main <head>` returned `1f46788` for all six, and each is 0 commits behind main. Each therefore merges cleanly onto main today. I also merged all six sequentially in a scratch branch (deleted afterwards): no conflicts.

Files touched:

| PR | Files |
|---|---|
| #102 | `crates/fabric-control-plane/tests/catalogue_authorization.rs` (new) |
| #103 | `apps/control-plane-ui/src/product/useCatalogue.ts`, `.../useCatalogue.ordering.test.ts` (new) |
| #104 | `apps/control-plane-ui/package.json`, `apps/control-plane-ui/package-lock.json` |
| #105 | `.../components/tabs/NewSecret.tsx`, `.../components/tabs/Secrets.test.tsx` |
| #106 | `.../components/tabs/SecretMetadata.tsx` (new), `SecretMetadata.test.tsx` (new), `SecretRow.tsx`, `.../hooks/useSecrets.ts` |
| #107 | `.../components/IdentityPanel.tsx`, `IdentityPanel.test.tsx` (new) |

Pairwise overlap: the file sets are fully disjoint, so there is no textual conflict between any pair and merge order does not affect conflicts. Semantic adjacency only:

- #105 and #106 both change the Secrets tab (`NewSecret.tsx` versus `SecretRow.tsx`/`useSecrets.ts`). #106's test of a concurrent replacement writes through the NewSecret form; it passes standalone on unvalidated NewSecret (358) and composed with #105 (385 per PR body, 391 with all six, verified here).
- #104 changes the test runner for every UI test. Branch CI for #103, #105, #106 and #107 ran on Vitest 3.2.7. Composed evidence on Vitest 4.1.11 is the 391/391 run above, so merging #104 first is safe.
- #102 is Rust-only and independent of the five UI PRs.

## Commands run and results

Environment: Node v22.22.0, npm 10.9.4 (CI and Dockerfile use Node 22; Vitest 4.1.11 requires `^20 || ^22 || >=24`), cargo 1.98.0. UI commands were run in `apps/control-plane-ui`. The `apps/control-plane-ui` test script is `vitest run`; lint is `eslint .`; build is `tsc -b && vite build`.

| # | Command | Checkout | Result |
|---|---|---|---|
| 1 | `npm ci --no-audit --no-fund && npx vitest run` | main | 341 passed / 341 (49 files) |
| 2 | `npx vitest run` | #103 head | 350 passed / 350 (50 files) |
| 3 | `npx vitest run src/product/useCatalogue.ordering.test.ts` | #103 head | 9 passed / 9 |
| 4 | same file, after `git checkout origin/main -- .../useCatalogue.ts` (revert) | #103 + reverted fix | 6 failed, 3 passed (the 3 passes are controls) |
| 5 | `npx vitest run` | #105 head | 359 passed / 359 (49 files) |
| 6 | `npx vitest run src/components/tabs/Secrets.test.tsx`, after reverting `NewSecret.tsx` to main | #105 + reverted fix | 12 failed, 11 passed |
| 7 | `npx vitest run` | #106 head | 358 passed / 358 (50 files) |
| 8 | `npx vitest run SecretMetadata.test.tsx`, after reverting `SecretRow.tsx` to main | #106 + reverted wiring | 17 failed / 17 |
| 9 | same file, after mutating `if (current.current === sequence)` to `if (true)` | #106 + mutation | 17 passed / 17 (mutation survives, see finding) |
| 10 | `npx vitest run` | #107 head | 347 passed / 347 (50 files) |
| 11 | `npx vitest run IdentityPanel.test.tsx`, after reverting `IdentityPanel.tsx` to main | #107 + reverted fix | 2 failed, 4 passed |
| 12 | `rm -rf node_modules && npm ci`; `vitest --version`; `npx vitest run`; `npm run lint`; `npm run build`; `npm audit`; `npm audit --omit=dev` | #104 head | vitest 4.1.11; 341 passed / 341; lint clean; build ok; audit 0 and 0 |
| 13 | `npm audit` with `package.json` + lock reverted to main | #104 + reverted fix | 3 vulnerabilities (2 moderate, 1 high; brace-expansion). PR body cites 4 advisories; npm 10.9.4 reports 3 today. |
| 14 | `cargo test -p fabric-control-plane --test catalogue_authorization` | #102 head | 3 passed / 3 |
| 15 | same, after editing `handlers/change_catalogue.rs` to drop the `Operator` extractor (builds a fixed operator instead) | #102 + mutation of production handler | 3 failed / 3 |
| 16 | `cargo test -p fabric-control-plane` | #102 head | all result lines ok, about 337 passed, 0 failed |
| 17 | `cargo fmt --check -p fabric-control-plane`; `cargo clippy -p fabric-control-plane --tests -- -D warnings` | #102 head | both clean |
| 18 | sequential `git merge --no-ff` of #102..#107, then `npm ci && npx vitest run && npm run lint && npm run build && npm audit` | scratch composition | 391 passed / 391 (52 files); lint clean; build ok; audit 0 |

All reverts and mutations were restored with `git checkout` and the worktree was clean afterwards. The scratch branch was deleted.

Not run locally: the full-workspace `cargo test --workspace`, `cargo deny`, `cargo doc`, CodeQL, the ndc-postgres acceptance test, and the Docker image builds. #102 only adds a test file in one crate, so I ran that crate's whole test suite instead of the workspace. CI results for those jobs are as reported on GitHub (below). Lint, typecheck and build were run on #104 and on the composed tree, not separately on #103/#105/#106/#107 heads (CI is green for each, and the composition covers all their code).

## CI (get_check_runs on each exact head)

Each head shows duplicate-looking job names because the CI workflow ran twice (pull_request and push events); all instances succeeded.

- #102 `13d08b6`: 24 check runs, all success. Includes cargo fmt/clippy/test --workspace/doc/deny, ndc-postgres acceptance, control-plane-ui, architectural invariants, file-size policy, CodeQL aggregate and Analyze (actions, csharp, javascript-typescript, python, rust).
- #103 `8094188`: 24, all success (same set).
- #104 `f0c8f0f`: 30 runs: 28 success, 2 skipped (`component descriptor`, `gates`, the tag-only release jobs). Includes the three image builds (`saas-fabric`, `saas-fabric-control-plane`, `saas-fabric-control-plane-ui`) and `version`. Matches the issue's "28 successful, two skipped".
- #105 `cd5cb1d`: 24, all success.
- #106 `12edde9`: 24, all success. The PR body still says "Exact-head CI and CodeQL pending"; that is stale, the checks have all completed successfully.
- #107 `d35ef27`: 24, all success.

## Per-PR assessment

### #102 Test anonymous catalogue writes leave state unchanged (head `13d08b6`)

Scope: one new Rust integration test file, three tests (create, draft save, publish). Each sends a valid body with the correct precondition anonymously, asserts 401 with code `unauthenticated`, compares the whole `GET /api/catalogue` before and after, then repeats the same request as an operator as a positive control.

Review comments: none. Findings:

- NON-BLOCKING: `catalogue_authorization.rs` exercises the in-process `AcceptingOperator` test authenticator, not OIDC. The PR says this explicitly. Real auth/network posture is not demonstrated.
- NON-BLOCKING: only three of the seven command variants are covered anonymously (`selectComponentVersion`, `saveDefinition`, `saveSettings`, `saveEnvironment` are not). They share the one `Operator` extractor on `change_catalogue` (`crates/fabric-control-plane/src/handlers/change_catalogue.rs`), so the single gate is what is being proven. I confirmed that removing the extractor from the production handler fails all three tests (command 15), so the tests are sensitive to the handler, not only to the fake authenticator. The PR's own mutation (fake authenticator) also exists as a CI run.
- NON-BLOCKING: `tests/catalogue_authorization.rs:120` `definition()` JSON is copied from `product_workflows` (stated in the comment); a future schema change must update both. Style only.

Regression sensitivity: yes (mutation 15 fails all three). Disposition: ready for Riley approval.

### #103 Prevent overlapping catalogue writes and stale refresh results (head `8094188`)

Scope: `useCatalogue.ts` gains `inFlight` and `writesLanded` refs. Save and select check `inFlight` synchronously; a refresh started before a successful write is dropped; a successful write clears `loadError`. Nine deterministic hook tests.

Review comments: none. Findings:

- NON-BLOCKING (`useCatalogue.ts:219` `save`): `save` still reads `value.revision` from the render closure. Two sequential awaited saves inside one handler can send the second with the pre-write revision. The server answers 409, so this fails safe (and it is pre-existing behaviour). Not made worse by this PR.
- NON-BLOCKING (`useCatalogue.ts:158-161`): a read that started under an older write count is discarded even if it reached the server after the write and so was fresher than the write response. Another client's change in that window will not be shown until the next refresh. Acceptable trade-off and consistent with the documented intent.
- NON-BLOCKING (`useCatalogue.ts:195-214`): `inFlight` is reset in `finally`, so a thrown/refused write releases the guard. Verified by test "once a write is refused by the server, the next write is allowed out".

Regression sensitivity: yes. Reverting `useCatalogue.ts` to main fails 6 of 9 tests (3 same-render write races, stale refresh data, delayed load error, load-error recovery); the 3 passing are controls (command 4). This matches the PR's claim of six expected failures. Disposition: ready for Riley approval.

### #104 Patch control-plane UI development dependency advisories (head `f0c8f0f`)

Scope: `vitest` `^3.1.4` to `^4.1.11` in `package.json`; lockfile moves Vitest family 3.2.7 to 4.1.11, `brace-expansion` 1.1.18 to 1.1.21 and 5.0.9 to 5.0.12, removes several Vitest 3 transitive packages (cac, check-error, deep-eql, loupe, pathval, strip-literal, tinypool, tinyspy, vite-node) and adds `obug` and `@standard-schema/spec`. No production dependencies change. No source or test edits.

Review comments: none. Findings:

- NON-BLOCKING: a major test-runner bump. Clean `npm ci` with npm 10.9.4 on Node 22 works, all 341 tests pass, lint and build pass, and `npm audit` (full and `--omit=dev`) reports 0. Node 22 is what CI and the Dockerfile use, and the Vitest 4.1.11 engine range admits it.
- NON-BLOCKING: the lockfile was generated with npm 12.2.0 while CI and Docker use npm 10; I verified npm 10 installs it cleanly, as the PR claims.
- NON-BLOCKING: on main today npm 10.9.4 audit reports 3 findings (brace-expansion), not the 4 in the PR body; the Vitest/mocker advisory was not independently reproduced by my npm run. Fix still verified in the sense that the PR head audits to 0.
- Dev-only: none of these advisories affect the production image (nginx serves only `dist`).

Regression sensitivity: the test is `npm audit`: reverting manifest and lock to main yields 3 findings, PR head yields 0 (command 13). Disposition: ready for Riley approval. Recommend merging early (see order) so the other UI branches pick up the lockfile once.

### #105 Prevent malformed secret versions from changing write semantics (head `cd5cb1d`)

Scope: `NewSecret.tsx` validates the "Replacing version" text (`/^[0-9]+$/` then safe integer) before conversion, shows an accessible `role="alert"` error (`aria-invalid`, `aria-describedby`), sends nothing on refusal and keeps the draft. 18 new rendered cases assert on the serialised PUT body.

Review comments: none. Findings:

- NON-BLOCKING (`NewSecret.tsx:42,50`): `"0"` is accepted and sent as `0`. Per the PR's own comment, the server treats a check-and-set of 0 the same as blank (create-only). This is documented and well-formed, but an operator could type 0 believing it means "replace version 0". No wrong behaviour, but worth a product decision on whether to refuse 0 or label it.
- NON-BLOCKING: the validation is client-side only; the backend still accepts whatever JSON the API sends. The PR scopes this out (backend CAS contract untouched).
- NON-BLOCKING: the file now carries a header comment citing the file-size band; CI `file-size policy` passes.

Regression sensitivity: yes. Reverting `NewSecret.tsx` to main fails 12 of 23 Secrets tests, 11 pass (command 6). Matches PR claim (12 failures, 11 controls). Disposition: ready for Riley approval.

### #106 Expose observed secret versions without revealing values (head `12edde9`)

Scope: new `SecretMetadata.tsx` (per-row "Read version"/"Refresh"/"Retry" button reading metadata on demand), wired in `SecretRow.tsx` and `useSecrets.ts` (`metadata` callback over the existing `secretMetadata` API client). 17 rendered tests with synthetic fetch replies. The response shape (`version: u64`, `updatedAt: Option<String>`, camelCase) matches `crates/fabric-control-plane/src/client_secrets/values.rs`.

Review comments: none. Findings:

- NON-BLOCKING (test gap, `SecretMetadata.tsx:64`): the `current.current === sequence` stale-answer guard is not covered by any test. Replacing it with `if (true)` leaves all 17 tests passing (command 9), because after unmount `setState` is a no-op, and the `pending` ref already prevents overlapping reads from one row. The guard is effectively defensive and dead today. It is harmless, but the "Unmounted/client-switched responses are discarded" tests rely on React's unmounted-setState and on the Secrets listing remounting rows when the client changes (`useSecrets.ts` resets state to loading), not on this guard.
- NON-BLOCKING (`useSecrets.ts` effect): on a client switch there is one render with the new `read` bound to the new client before the list reloads and rows unmount; a previously shown observation could show for that single frame. Not exploitable (the version is only a number the operator copies manually, nothing is auto-filled).
- NON-BLOCKING: `SecretMetadata.tsx` `fromMetadata` rejects versions above 2^53-1; such a secret cannot be read or replaced from the console (consistent with #105).
- NON-BLOCKING: PR body says CI is pending; the check runs are all success.

Regression sensitivity: reverting the wiring (`SecretRow.tsx` to main) fails all 17 tests (command 8); the no-reveal assertion (GET only) is the key safety claim and it is asserted on the request log. Disposition: ready for Riley approval (suggest merging after #105).

### #107 Fix ordered Identity role change detection (head `d35ef27`)

Scope: `IdentityPanel.tsx` replaces `roles.join(' ') !== current.roles.join(' ')` with an element-wise, order-sensitive comparison (`IdentityPanel.tsx:49`); 6 rendered tests with synthetic API replies. Ordering is justified: `crates/fabric-client-model/src/identity.rs` stores roles as an ordered `Vec`.

Review comments: none. Findings:

- NON-BLOCKING: reorder-only edits become saveable, which will issue a PUT that may trigger reconciliation for a change with no semantic effect if the platform treats roles as a set. The PR states this intent. Needs a product call, not a code fix.
- The fix itself is correct and the reasoning (no safe delimiter) is sound.
- PR body only says "Draft and unmerged". The explicit no-merge/no-deploy restriction is stated in issue #109 ("PR107 remains draft/unmerged under its explicit no-merge/no-deploy restriction until scope changes"), not in the PR text; I found no other statement of it.

Regression sensitivity: reverting `IdentityPanel.tsx` to main fails 2 collision tests, 4 pass as controls (command 11). Matches PR claim. Disposition: HOLD. Technically sound, but per #109 it stays a draft, unmerged and undeployed until its scope changes. Do not mark ready or merge.

## Proposed merge order (for when Riley approves; this document authorises nothing)

1. #102 (test-only, no UI; zero risk; independent).
2. #104 (lockfile and test runner; merge before the other UI PRs so their branches inherit it. Because the other UI PRs do not touch the lockfile they remain textually conflict-free; each should have its CI re-run on the updated base, as their branch CI used Vitest 3.2.7. The composed run on Vitest 4.1.11 gave 391/391.)
3. #103 (catalogue hook).
4. #105 (secret version validation).
5. #106 (secret metadata; builds on the same Secrets tab as #105, so merge after it).
6. #107: not merged. Hold until scope changes.

Because the file sets are disjoint, the order is a preference for risk and CI cleanliness, not a conflict requirement. #103, #105, #106 can be merged in any relative order.

## Dispositions

- #102: ready for Riley approval.
- #103: ready for Riley approval.
- #104: ready for Riley approval (merge early).
- #105: ready for Riley approval. Optional product decision on whether "0" should be accepted.
- #106: ready for Riley approval; the uncovered stale-guard is a non-blocking test gap.
- #107: hold (keep draft; assess only; no merge, no deploy).

No PR has a blocking finding. Before approving any of them, re-run this validation if the head SHA changes from the values recorded above, since the issue requires revalidation when heads change.
