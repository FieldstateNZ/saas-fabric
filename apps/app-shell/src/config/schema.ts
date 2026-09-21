import { z } from 'zod'

const text = z.string().trim().min(1).max(160)
const color = z.string().regex(/^#[0-9a-fA-F]{6}$/, 'Use a quoted six-digit hex color, e.g. "#4a5c42"')
const unique = (items: string[]) => new Set(items).size === items.length
const header = z.array(z.enum(['brand', 'navigation'])).max(2).refine(unique, 'Region IDs must be unique')
const content = z.array(z.enum(['page', 'configuration'])).min(1).max(2)
  .refine(unique, 'Region IDs must be unique').refine((ids) => ids.includes('page'), 'Content must include page')

/** Only registered content IDs are accepted. Unknown keys catch configuration typos. */
export const configSchema = z.strictObject({
  branding: z.strictObject({
    name: text,
    subtitle: text.default('Application workspace'),
    mark: z.string().trim().min(1).max(3).default('f'),
    footer: text.default('Local prototype'),
  }),
  theme: z.strictObject({
    primary: color.default('#4a5c42'),
    background: color.default('#f4f0e8'),
    borderRadius: z.number().int().min(0).max(24).default(10),
  }).prefault({}),
  navigation: z.array(z.strictObject({
    id: z.string().regex(/^[a-z][a-z0-9-]*$/, 'Use a lowercase route ID, e.g. overview'),
    label: text,
    content: z.enum(['welcome', 'notes']),
  })).min(1).max(8).refine((items) => unique(items.map((item) => item.id)), 'Navigation IDs must be unique'),
  layout: z.strictObject({
    header: header.default(['brand']),
    sidebar: z.strictObject({
      width: z.number().int().min(180).max(320).default(232),
      collapsible: z.boolean().default(true),
      regions: header.default(['navigation']),
    }).prefault({}),
    content: content.default(['page']),
    footer: z.array(z.literal('footer')).max(1).default(['footer']),
  }).prefault({}),
}).superRefine((config, context) => {
  const navCount = [...config.layout.header, ...config.layout.sidebar.regions]
    .filter((id) => id === 'navigation').length
  if (navCount !== 1) context.addIssue({ code: 'custom', path: ['layout'],
    message: 'Place navigation exactly once, in header or sidebar.regions' })
})

export type ShellConfig = z.infer<typeof configSchema>
export type NavigationEntry = ShellConfig['navigation'][number]
