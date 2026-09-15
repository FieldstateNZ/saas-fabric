/** Disposable sample data. This module is never imported by the production application. */
const names = ['Northstar', 'Acme Industries', 'Harbour Health', 'Meridian']
export const clients = names.map((name, index) => ({
  id: ['northstar', 'acme', 'harbour', 'meridian'][index], displayName: name,
  hosts: [`${['northstar', 'acme', 'harbour', 'meridian'][index]}.example.test`],
  realm: ['northstar', 'acme', 'harbour', 'meridian'][index], revision: 'preview-1',
}))
export const identities = Object.fromEntries(clients.map((client, index) => [client.id, {
  realm: client.realm, roles: ['Client Realm Administrator', 'Client Realm User'], apiVersion: 'fabric.fieldstate.nz/v2', revision: 'preview-1',
  clients: [{ id: 'client-portal', type: 'oidc', pkce: 's256', redirect: { strategy: 'claimedHttps', uris: [`https://${client.hosts[0]}/callback`] } },
    ...(index < 2 ? [{ id: 'field-service', type: 'oidc', pkce: 's256', redirect: { strategy: 'claimedHttps', uris: [`https://${client.hosts[0]}/field-service/callback`] } }] : [])],
  reconciliation: { status: index === 2 ? 'pending' : 'applied', observedAtUnix: Math.floor(Date.now() / 1000) - index * 240, detail: null },
}]))
export const platform = {
  environment: 'Fieldstate', lastCheck: { atUnixSeconds: Math.floor(Date.now() / 1000), outcome: 'success', detail: null },
  components: ['SaaS Fabric', 'Client portal', 'Field service'].map((component, index) => ({
    component, desired: '0.3.0', newer: index === 0 ? '0.3.1' : null, running: 'unknown', policy: 'automatic',
    artifact: 'oci', paused: false, desiredState: index === 0 ? 'update-available' : 'current', hold: null, diagnostics: [],
  })),
}
const application = { slug: 'fabric-preview', account: 'example', installed: true, repository: 'example/preview' }
export const integrations = {
  '/api/integrations/git': { status: 'connected', connection: 'Sample client configuration', last_success_at: Math.floor(Date.now() / 1000), managed: true, application },
  '/api/integrations/platform': { managed: true, application },
}
