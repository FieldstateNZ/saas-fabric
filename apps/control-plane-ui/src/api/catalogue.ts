import { request } from './client'
import type { CatalogueCommand, StoredCatalogue, ClientProductRequest, ClientProductResponse, ProductActivity } from './catalogue-types'
export const getCatalogue = () => request<StoredCatalogue>('/api/catalogue')
export const changeCatalogue = (command: CatalogueCommand, revision: string | null) => request<StoredCatalogue>('/api/catalogue', {
  method: 'POST', headers: { 'Content-Type': 'application/json', ...(revision === null ? { 'If-None-Match': '*' } : { 'If-Match': `"${revision}"` }) }, body: JSON.stringify(command),
})
export const getProduct = (id: string) => request<ClientProductResponse>(`/api/clients/${encodeURIComponent(id)}/product`)
export const createClient = (id: string, configuration: ClientProductRequest) => request<ClientProductResponse>('/api/clients', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ id, configuration }) })
export const saveProduct = (id: string, revision: string, configuration: ClientProductRequest) => request<ClientProductResponse>(`/api/clients/${encodeURIComponent(id)}/product`, {
  method: 'PUT', headers: { 'Content-Type': 'application/json', 'If-Match': `"${revision}"` }, body: JSON.stringify(configuration),
})
export const getActivity = () => request<{ activity: ProductActivity[] }>('/api/activity')
export const getOperator = () => request<{ subject: string }>('/api/operator')
