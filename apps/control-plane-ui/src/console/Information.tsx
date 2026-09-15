import { PageHeader, Panel, Status } from './primitives'

export function ClientDefinition() {
  return <><PageHeader title="Client definition" description="The shared foundation of every client in SaaS Fabric." actions={<Status value="neutral">Read only</Status>} />
    <Panel title="Client configuration"><div className="panel-body"><p>Every client carries the following core configuration.</p></div>
      <div className="table-wrap"><table className="fabric-table"><thead><tr><th>Field</th><th>Type</th><th>Purpose</th></tr></thead><tbody>
        {[
          ['Client ID', 'Identifier', 'A unique, stable identity for the client.'],
          ['Display name', 'Text', 'The name shown throughout the operator console.'],
          ['Domains', 'Hostnames', 'The domains declared for this client.'],
          ['Realm', 'Identifier', 'The client’s identity boundary.'],
        ].map(([field, type, purpose]) => <tr key={field}><td>{field}</td><td><span className="type-tag">{type}</span></td><td>{purpose}</td></tr>)}
      </tbody></table></div></Panel>
    <p className="support-note">Custom fields, definition versioning, and application-specific client configuration are not available yet.</p>
  </>
}
export function Settings() {
  return <><PageHeader title="Settings" description="Access and connections for your operator console." />
    <Panel title="Operator access"><div className="panel-body"><p>Operator access is managed through your organisation’s sign-in.</p>
      <p>Account details and session management are not available in the console yet.</p></div></Panel>
    <Panel title="Connections"><div className="panel-body"><p>Manage the connections used for client configuration and platform management.</p>
      <a href="#/integrations">Manage integrations →</a></div></Panel>
  </>
}
