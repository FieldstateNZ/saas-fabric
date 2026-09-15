/** The tabs a client detail view offers, in the order they are shown. */
export const CLIENT_TABS = [
  'Overview',
  'Applications',
  'Configuration',
  'Identity',
  'Domains',
  'Activity',
  'Secrets',
  'Authorization',
  'Modules',
  'Health',
] as const

/** One of the client detail tabs. */
export type ClientTab = (typeof CLIENT_TABS)[number]
