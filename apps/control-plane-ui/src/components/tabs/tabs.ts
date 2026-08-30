/** The tabs a client detail view offers, in the order they are shown. */
export const CLIENT_TABS = [
  'Overview',
  'Secrets',
  'Authorization',
  'Identity',
  'Modules',
  'Config',
  'Health',
] as const

/** One of the client detail tabs. */
export type ClientTab = (typeof CLIENT_TABS)[number]
