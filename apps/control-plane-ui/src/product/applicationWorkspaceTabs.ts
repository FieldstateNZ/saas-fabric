/**
 * Every section of `ApplicationWorkspace`, in the order its tab bar lists
 * them.
 *
 * Shared between `ApplicationWorkspace` and `ApplicationWorkspaceTab` rather
 * than declared in either one, so both sides of the tab switch are checked
 * against the same closed set — see {@link ApplicationTab}.
 */
export const APPLICATION_TABS = [
  'Definition',
  'Components',
  'Resources',
  'Features',
  'Client configuration',
  'Plans',
  'Navigation',
  'Entry points',
  'Releases',
] as const

/**
 * One of `ApplicationWorkspace`'s tabs.
 *
 * `ApplicationWorkspaceTab` types its `tab` prop with this rather than
 * `string`, so a misspelt tab name fails to compile instead of silently
 * falling through to render nothing.
 */
export type ApplicationTab = (typeof APPLICATION_TABS)[number]
