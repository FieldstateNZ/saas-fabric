/**
 * Signing an operator back in without asking them to click anything.
 *
 * # The problem this solves
 *
 * The token lives in memory (see `session.ts`), so **every** page load starts
 * signed out: a refresh, a new tab, and the two round trips through GitHub
 * that creating and installing the application require. `session.ts` says a
 * reload "costs one redirect that the provider's own session makes invisible",
 * and that was true of the redirect and false of the console — it rendered a
 * button and waited for a click that told the provider nothing it did not
 * already know.
 *
 * # Why `prompt=none` rather than storing the token
 *
 * The alternative is keeping the token in `sessionStorage`, which puts a
 * bearer where every script on the page can read it. `prompt=none` asks the
 * provider to answer from the session it already holds and to answer with an
 * *error* rather than a login page when it cannot. So the console can always
 * try, and the cost of being wrong is one redirect instead of a prompt the
 * operator did not ask for.
 *
 * # Why it cannot loop
 *
 * An attempt is recorded before the browser leaves and cleared the moment an
 * outcome is observed — a code, an error, or a return carrying neither. An
 * attempt already in flight is never made a second time, so the worst case is
 * one wasted redirect and the sign-in button, not a console bouncing off its
 * identity provider forever.
 */

/** Set while a silent attempt is in flight, so exactly one is ever made. */
const ATTEMPTED = 'fabric.signin.silent'

/**
 * The errors that mean "not without asking the operator".
 *
 * These are the provider doing as it was told, not a fault: there is no
 * session, or the session cannot satisfy this request without a prompt. The
 * console shows its sign-in button and says nothing alarming. Anything else is
 * a real failure and is worth putting in front of somebody.
 */
const NEEDS_THE_OPERATOR = new Set([
  'login_required',
  'interaction_required',
  'consent_required',
  'account_selection_required',
])

/** Whether a silent attempt is in flight and has not yet been accounted for. */
export function attemptPending(): boolean {
  return sessionStorage.getItem(ATTEMPTED) !== null
}

/** Records that this tab is about to try, before the browser leaves. */
export function recordAttempt(): void {
  sessionStorage.setItem(ATTEMPTED, 'yes')
}

/** Accounts for an attempt, however it turned out. */
export function forgetAttempt(): void {
  sessionStorage.removeItem(ATTEMPTED)
}

/** The `error` this page load carries, if the provider returned one. */
export function callbackError(): string | null {
  return new URLSearchParams(window.location.search).get('error')
}

/** Whether an error means the operator must sign in by hand. */
export function needsTheOperator(error: string): boolean {
  return NEEDS_THE_OPERATOR.has(error)
}
