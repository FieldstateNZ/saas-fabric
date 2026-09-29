import type { ComponentResolution } from '../../api/component-types'
import { when } from '../../components/registries/registry-words'
import { DeclaredFields } from './DeclaredFields'
import { DeclaredResources } from './DeclaredResources'
import { shortDigest } from './words'

/** A digest, shortened, with the whole of it on hover. */
function Digest({ value }: { readonly value: string }) {
  return (
    <span className="mono" title={value}>
      {shortDigest(value)}
    </span>
  )
}

/**
 * What the server recorded when a version of a described component was
 * selected, read-only (ADR 0026 sections 7 and 8).
 *
 * # What Fabric observed then, and nothing about now
 *
 * Every fact here is from the resolution: the repository and version, both
 * digests Fabric computed, the one commit, when it was resolved, and the
 * frozen component descriptor's images, needs, fields and resources. None
 * is a claim that the artifact is still in its registry -- nothing has
 * asked since -- so the section says when it was read and what declared
 * it, and never "available".
 *
 * The primary image is the one the component descriptor is attached to:
 * the role naming the resolution's repository at its primary digest.
 * Capabilities are needs; nothing provisions one yet (ADR 0021).
 */
export function ResolutionView({ resolution }: { readonly resolution: ComponentResolution }) {
  const { spec } = resolution.descriptor
  const images = Object.entries(spec.images)

  return (
    <section className="declared" aria-label="Declared by the component descriptor">
      <p className="support-note">
        Declared by the component descriptor Fabric read on {when(resolution.resolvedAt)}. This is
        what Fabric observed then, not a claim that the artifact is still there. It is read-only
        here: choose another version to change it.
      </p>
      <dl className="facts">
        <dt>Repository</dt>
        <dd className="mono">{resolution.repository}</dd>
        <dt>Version</dt>
        <dd>{resolution.version}</dd>
        <dt>Primary image digest</dt>
        <dd>
          <Digest value={resolution.primaryDigest} />
        </dd>
        <dt>Component descriptor digest</dt>
        <dd>
          <Digest value={resolution.descriptorDigest} />
        </dd>
        <dt>Source commit</dt>
        <dd className="mono">{resolution.revision}</dd>
        <dt>Resolved</dt>
        <dd>{when(resolution.resolvedAt)}</dd>
        <dt>Needs</dt>
        <dd>
          {spec.capabilities.length === 0
            ? 'Nothing from the platform'
            : spec.capabilities.join(', ')}
        </dd>
      </dl>
      {spec.description !== '' && <p>{spec.description}</p>}

      <h3>Images</h3>
      <ul className="declared__images">
        {images.map(([role, image]) => {
          const primary =
            image.repository === resolution.repository && image.digest === resolution.primaryDigest
          return (
            <li key={role}>
              <strong>{role}</strong>
              {primary && ' (primary)'}: <span className="mono">{image.repository}</span>{' '}
              <Digest value={image.digest} />
            </li>
          )
        })}
      </ul>

      <h3>Declared fields</h3>
      <DeclaredFields fields={spec.fields} />
      <h3>Declared resources</h3>
      <DeclaredResources resources={spec.resources} />
    </section>
  )
}
