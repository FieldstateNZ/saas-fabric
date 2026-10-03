import type { ApplicationDefinition } from '../../api/catalogue-types'
import { when } from '../../components/registries/registry-words'
import { declaring } from '../effective'
import { DeclaredFields } from './DeclaredFields'
import { DeclaredResources } from './DeclaredResources'
import { shortDigest } from './words'

/**
 * Beside the fields or resources an operator authored, the ones each
 * described component declares: read-only, labelled with the component
 * that declares them, and in component order -- the order
 * `effectiveFields` and `effectiveResources` put them in effect.
 *
 * Each is labelled with the component descriptor's digest -- shortened, the
 * full value on hover -- and when it was resolved (ADR 0026 section 8):
 * that is the document the declared entries were frozen from.
 *
 * Nothing at all when no component is resolved: an application with no
 * described component declares nothing, and saying so would be noise.
 */
export function DeclaredByComponents({
  definition,
  what,
}: {
  readonly definition: ApplicationDefinition
  readonly what: 'fields' | 'resources'
}) {
  const declared = declaring(definition)
  if (declared.length === 0) {
    return null
  }

  return (
    <section className="collection">
      <h2>{what === 'fields' ? 'Declared fields' : 'Declared resources'}</h2>
      <p className="support-note">
        Declared by a component’s descriptor and in effect beside the ones above. Read-only here:
        they change only when another version of the component is selected.
      </p>
      {declared.map(({ component, resolution }) => (
        <section key={component.id} aria-label={`Declared by ${component.name}`}>
          <h3>
            Declared by {component.name}{' '}
            <small className="mono">
              {resolution.repository} {resolution.version}
            </small>
          </h3>
          <p className="support-note">
            Component descriptor{' '}
            <span className="mono" title={resolution.descriptorDigest}>
              {shortDigest(resolution.descriptorDigest)}
            </span>
            , resolved {when(resolution.resolvedAt)}.
          </p>
          {what === 'fields' ? (
            <DeclaredFields fields={resolution.descriptor.spec.fields} />
          ) : (
            <DeclaredResources resources={resolution.descriptor.spec.resources} />
          )}
        </section>
      ))}
    </section>
  )
}
