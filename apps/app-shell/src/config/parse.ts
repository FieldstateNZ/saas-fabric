import { parseDocument } from 'yaml'
import { configSchema, type ShellConfig } from './schema'

export type ConfigResult = { ok: true; config: ShellConfig } | { ok: false; errors: string[] }

/** YAML is data: reject aliases, unknown keys and unregistered components before rendering. */
export function parseConfig(source: string): ConfigResult {
  try {
    if (source.length > 32_000) return { ok: false, errors: ['Configuration must be smaller than 32 KB.'] }
    const document = parseDocument(source, { uniqueKeys: true })
    const problems = [...document.errors, ...document.warnings]
    if (problems.length) return { ok: false, errors: problems.map((issue) => issue.message) }
    const input: unknown = document.toJS({ maxAliasCount: 0 })
    const result = configSchema.safeParse(input)
    if (!result.success) return { ok: false, errors: result.error.issues.map((issue) =>
      `${issue.path.join('.') || 'config'}: ${issue.message}`) }
    return { ok: true, config: result.data }
  } catch (error) {
    return { ok: false, errors: [error instanceof Error ? error.message : 'Unable to parse YAML.'] }
  }
}
