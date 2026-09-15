import type { Catalogue, ClientProductRequest, ClientProductResponse } from '../api/catalogue-types'

/**
 * What `ClientForm` starts editing: an existing client's product state
 * reshaped into a request, or a blank one seeded with the catalogue's
 * defaults.
 *
 * Pulled out of the component so the shape of "a client with nothing set
 * yet" lives in one place — the platform's default region and timezone
 * apply the same way whether this is a brand-new client or one being
 * reconfigured without its own region or timezone recorded yet.
 */
export function initialClientProductRequest(
  catalogue: Catalogue,
  existing: ClientProductResponse | undefined,
): ClientProductRequest {
  if (!existing) {
    return {
      displayName: '',
      legalName: '',
      hosts: [],
      region: catalogue.settings.defaultRegion,
      timezone: catalogue.settings.timezone,
      configuration: {},
      applications: [],
    }
  }

  return {
    displayName: existing.client.displayName,
    hosts: [...existing.client.hosts],
    legalName: existing.product.legalName,
    region: existing.product.region || catalogue.settings.defaultRegion,
    timezone: existing.product.timezone || catalogue.settings.timezone,
    configuration: existing.product.configuration,
    applications: existing.product.applications.map((a) => ({
      applicationId: a.applicationId,
      version: a.release.version,
      planId: a.planId,
      configuration: a.configuration,
    })),
  }
}
