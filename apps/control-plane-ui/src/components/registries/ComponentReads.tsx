import type { ComponentReads, HostReads, ImageRead } from '../../api/registry-types'

/** How one image is read, from what the control plane reported. */
function how(image: ImageRead): string {
  switch (image.read) {
    case 'credential':
      return 'read with this registry’s credential'
    case 'anonymous':
      return image.registered
        ? 'read anonymously: no credential is presented for it'
        : 'read anonymously: the repository is not registered here, so no credential is presented'
    case 'credentialRefused':
      return 'not read: the realm refused this registry’s credential, which is not presented again until it is replaced'
    case 'notRead':
      return 'not read now: nothing is read through this registry'
  }
}

/**
 * The images one host's reads cover, by component and role, and -- where
 * anything reads them -- how.
 */
function Images({ images, read }: { readonly images: readonly ImageRead[]; readonly read: boolean }) {
  return (
    <ul className="registry__repositories">
      {images.map((image) => (
        <li key={`${image.component}/${image.role}`}>
          <strong>{image.component}</strong> ({image.role}):{' '}
          <span className="mono">{image.repository}</span>
          {read && ` — ${how(image)}`}
        </li>
      ))}
    </ul>
  )
}

/**
 * The managed components read through one registry, and how each image is
 * read now: with the registry's credential, anonymously, or not at all
 * (ADR 0026 section 5).
 *
 * Built from the environment's pins and the registry records: nothing here
 * says a read succeeded -- the platform panel says that -- only where each
 * goes and what it presents. Nothing at all when no managed component is
 * read through it.
 */
export function ComponentReadsPart({ host }: { readonly host: HostReads | undefined }) {
  if (host === undefined || host.images.length === 0) {
    return null
  }
  return (
    <div className="registry__part">
      <h4 className="registry__part-heading">Managed components read through it</h4>
      <Images images={host.images} read />
    </div>
  )
}

/** The hosts a managed component is read from, when they were observed. */
export function observedHosts(reads: ComponentReads | null): readonly HostReads[] {
  return reads?.state === 'observed' ? reads.hosts : []
}

/**
 * What the section says about reads that belong to no registered registry:
 * a host read only as the deployment's own, anonymously; a host nothing
 * reads, because no registry is registered for it; or that the reads could
 * not be worked out now -- never shown as "no components".
 */
export function UnregisteredReads({ reads }: { readonly reads: ComponentReads | null }) {
  if (reads === null) {
    return null
  }
  if (reads.state === 'unavailable') {
    return (
      <p className="integration__diagnostic">
        Which managed components are read through each registry could not be worked out now (
        {reads.code}).
      </p>
    )
  }
  const hosts = observedHosts(reads).filter((host) => !host.registered)
  return (
    <>
      {hosts.map((host) => (
        <section className="integration registry" key={host.host} aria-label={`Reads from ${host.host}`}>
          <h3 className="integration__name">{host.host}</h3>
          <p className={host.deployment ? 'integration__detail' : 'integration__diagnostic'}>
            {host.deployment
              ? 'No registry is registered for the deployment’s host, so these are read anonymously through the deployment’s registry.'
              : 'No registry is registered for this host, so nothing reads these images: the host is refused by name.'}
          </p>
          <Images images={host.images} read={host.deployment} />
        </section>
      ))}
    </>
  )
}
