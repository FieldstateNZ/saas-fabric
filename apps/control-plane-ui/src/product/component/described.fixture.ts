/**
 * A described component as the server serves one, for tests: the same
 * fixture as `fabric-client-model`'s `catalogue/described_tests.rs`, written
 * as the JSON its types render -- every descriptor section present, a
 * declared resource's defaults filled in, and a definition's `resources`
 * key absent, because the server omits it when there are none.
 *
 * Not a test itself, and imported only by tests.
 */
import type {
  ApplicationDefinition,
  ApplicationResource,
  ConfigurationField,
} from '../../api/catalogue-types'
import type {
  ApplicationComponent,
  ComponentKind,
  ComponentResolution,
} from '../../api/component-types'

export const REPOSITORY = 'registry.example.com/acme/reports'
export const PRIMARY = `sha256:${'1'.repeat(64)}`
export const FIRST = `sha256:${'3'.repeat(64)}`
export const SECOND = `sha256:${'4'.repeat(64)}`

/** A required text field, `team`. */
export const TEAM: readonly ConfigurationField[] = [
  {
    key: 'team',
    label: 'Team',
    kind: 'text',
    required: true,
    default: null,
    options: [],
    description: '',
  },
]

/** A resource, `reports`, with the defaults the server renders. */
export const REPORTS: readonly ApplicationResource[] = [
  {
    name: 'reports',
    dataSource: 'primary',
    collection: 'reports',
    keyField: 'id',
    operations: ['read', 'list'],
    queryableFields: [],
  },
]

/** A resolution at `version`, its descriptor declaring `fields` and `resources`. */
export function resolution(
  version: string,
  descriptorDigest: string,
  fields: readonly ConfigurationField[] = TEAM,
  resources: readonly ApplicationResource[] = REPORTS,
): ComponentResolution {
  return {
    repository: REPOSITORY,
    version,
    primaryDigest: PRIMARY,
    descriptorDigest,
    revision: 'c'.repeat(40),
    resolvedAt: 1_700_000_000,
    descriptor: {
      apiVersion: 'fabric.fieldstate.nz/v1',
      kind: 'Component',
      spec: {
        name: 'reports',
        title: 'Reports',
        description: '',
        version,
        images: { api: { repository: REPOSITORY, digest: PRIMARY } },
        capabilities: [],
        fields,
        resources,
      },
    },
  }
}

/** A described component `id`, as selecting creates one. */
export function described(id: string, resolved: ComponentResolution): ApplicationComponent {
  return {
    id,
    name: 'Reports',
    kind: 'described',
    reference: resolved.repository,
    version: resolved.version,
    required: false,
    policy: 'manual',
    resolution: resolved,
  }
}

/** An authored component `id` of `kind`. */
export function authored(id: string, kind: Exclude<ComponentKind, 'described'>): ApplicationComponent {
  const capability = kind === 'capability'
  return {
    id,
    name: 'Authored',
    kind,
    reference: capability ? 'Identity' : 'registry.example.com/web',
    version: capability ? '' : '1.0.0',
    required: true,
    policy: 'automatic',
  }
}

/** A definition with one plan and `components`, and no `resources` key. */
export function definition(components: readonly ApplicationComponent[]): ApplicationDefinition {
  return {
    name: 'Analytics',
    description: '',
    domain: '',
    components,
    features: [],
    plans: [
      { id: 'standard', name: 'Standard', description: '', features: [], configuration: {} },
    ],
    fields: [],
    navigation: [],
  }
}
