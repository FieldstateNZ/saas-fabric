import { useState } from 'react'

import { useRegistries } from '../../hooks/useRegistries'
import { DeploymentRegistryCard } from './DeploymentRegistryCard'
import { RegisterRegistryForm } from './RegisterRegistryForm'
import { RegistryCard } from './RegistryCard'
import { RegistryRefusal } from './RegistryRefusal'

/**
 * The Integrations page's image registries (ADR 0026 section 5).
 *
 * # Beside the Git cards, not among them
 *
 * `IntegrationsPanel` keeps its two applications apart because they are
 * separate product concepts; registries are a third, with their own route
 * family, their own load and their own section. Nothing here is a Git
 * application and nothing there is a registry.
 *
 * # What it shows is what was proven
 *
 * Each registry is recorded only once it has proven, and each repository
 * once its tag listing has answered, so every card states what was proven
 * and when -- never that anything is "connected". A listing that could not
 * be read is an error, not an empty list: "none registered" and "could not
 * look" send an operator to different places.
 */
export function ImageRegistries() {
  const registries = useRegistries()
  const [registering, setRegistering] = useState(false)
  const listing = registries.value

  return (
    <section className="registries" aria-labelledby="registries-heading">
      <header className="registries__header">
        <h2 className="registries__heading" id="registries-heading">
          Image registries
        </h2>
        {listing !== null && !registering && (
          <button
            type="button"
            className="integration__action"
            onClick={() => {
              setRegistering(true)
            }}
          >
            Register a registry
          </button>
        )}
      </header>
      <p className="registries__intro">
        Where component images are read from. Each registry is proven before it is recorded, and
        each repository once its tags can be listed through it.
      </p>

      {registries.loading && <p className="empty">Loading registries…</p>}
      <RegistryRefusal refusal={registries.loadError} />

      {listing !== null && (
        <>
          {registering && (
            <RegisterRegistryForm
              registries={registries}
              onClose={() => {
                setRegistering(false)
              }}
            />
          )}

          {listing.deployment !== null && (
            <DeploymentRegistryCard
              deployment={listing.deployment}
              registered={listing.registries.find((registry) => registry.deployment)}
            />
          )}

          {listing.registries.length === 0 ? (
            <p className="empty">No registries are registered.</p>
          ) : (
            listing.registries.map((registry) => (
              <RegistryCard key={registry.host} registry={registry} registries={registries} />
            ))
          )}
        </>
      )}
    </section>
  )
}
