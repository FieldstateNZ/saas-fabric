/**
 * What an application's definition puts in effect: the fields and resources
 * the operator authored, and the ones each described component declares
 * (ADR 0026 section 8).
 *
 * # One helper, and every reader uses it
 *
 * The server's `ApplicationDefinition::effective_fields` and
 * `effective_resources` decide what a client must supply and what the
 * runtime is given; a console that read `fields` alone would build a
 * client's configuration form missing a field the server then requires.
 * So the same union is computed here once, in the same order -- authored
 * first, then each described component's declared ones in component order
 * -- and `effective.test.ts` pins it against a fixture shaped like the Rust
 * one.
 */
import type {
  ApplicationDefinition,
  ApplicationResource,
  ConfigurationField,
} from '../api/catalogue-types'
import type { ApplicationComponent, ComponentResolution } from '../api/component-types'

/** A component carrying a resolution, beside it. */
export interface Declaring {
  readonly component: ApplicationComponent
  readonly resolution: ComponentResolution
}

/**
 * Each component carrying a resolution, in component order. The server holds
 * that exactly the described ones carry one, and a described component added
 * here has none until a version is selected, so it declares nothing yet.
 */
export function declaring(definition: Pick<ApplicationDefinition, 'components'>): Declaring[] {
  return definition.components.flatMap((component) =>
    component.resolution === undefined ? [] : [{ component, resolution: component.resolution }],
  )
}

/** The configuration fields in effect: authored, then declared in component order. */
export function effectiveFields(definition: ApplicationDefinition): ConfigurationField[] {
  return [
    ...definition.fields,
    ...declaring(definition).flatMap(({ resolution }) => resolution.descriptor.spec.fields),
  ]
}

/**
 * The Data API resources in effect: authored, then declared in component
 * order. `resources` is absent from a definition that authors none.
 */
export function effectiveResources(definition: ApplicationDefinition): ApplicationResource[] {
  return [
    ...(definition.resources ?? []),
    ...declaring(definition).flatMap(({ resolution }) => resolution.descriptor.spec.resources),
  ]
}
