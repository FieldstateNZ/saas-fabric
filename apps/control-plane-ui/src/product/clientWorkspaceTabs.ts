/**
 * Every section of `ClientWorkspace`, in the order its tab bar lists them.
 *
 * Shared between `ClientWorkspace` and `ClientWorkspaceTab` rather than
 * declared in either one, so both sides of the tab switch are checked
 * against the same closed set — see {@link ClientTab}.
 */
export const CLIENT_TABS = [
  'Overview',
  'Applications',
  'Configuration',
  'Identity',
  'Domains',
  'Activity',
  'Secrets',
  'Health',
] as const

/**
 * One of `ClientWorkspace`'s tabs.
 *
 * `ClientWorkspaceTab` types its `tab` prop with this rather than `string`,
 * so a misspelt tab name fails to compile instead of silently falling
 * through to render nothing.
 */
export type ClientTab = (typeof CLIENT_TABS)[number]
