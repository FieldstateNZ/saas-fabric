import type { Client, Reconciliation } from './types'
export interface ConfigurationField { key: string; label: string; kind: 'text' | 'number' | 'boolean' | 'choice' | 'hostname' | 'identifier' | 'timezone'; required: boolean; default: string | null; options: string[]; description: string }
export type Values = Record<string, string>
export interface ApplicationComponent { id: string; name: string; kind: 'container' | 'helm' | 'capability'; reference: string; version: string; required: boolean; policy: 'automatic' | 'manual' }
export interface ApplicationFeature { id: string; name: string; description: string; implementedBy: string[] }
export interface ApplicationPlan { id: string; name: string; description: string; features: string[]; configuration: Values }
export interface NavigationItem { label: string; route: string; feature: string | null; permission: string }
export interface ApplicationDefinition { name: string; description: string; domain: string; components: ApplicationComponent[]; features: ApplicationFeature[]; plans: ApplicationPlan[]; fields: ConfigurationField[]; navigation: NavigationItem[] }
export interface ApplicationRelease { version: number; note: string; publishedAt: number; definition: ApplicationDefinition }
export interface ProductApplication { id: string; draft: ApplicationDefinition; releases: ApplicationRelease[] }
export interface ConsoleSettings { platformName: string; defaultRegion: string; timezone: string }
export interface EnvironmentRegistration { id: string; name: string; consoleUrl: string; description: string }
export interface ProductActivity { at: number; operator: string; action: string; resource: string }
export interface Catalogue { applications: ProductApplication[]; clientFields: ConfigurationField[]; settings: ConsoleSettings; environments: EnvironmentRegistration[]; activity: ProductActivity[]; definitionVersion: number }
export interface StoredCatalogue { catalogue: Catalogue; revision: string | null }
export type CatalogueCommand =
  | { action: 'createApplication'; id: string; name: string }
  | { action: 'saveApplication'; id: string; definition: ApplicationDefinition }
  | { action: 'publishApplication'; id: string; note: string }
  | { action: 'saveDefinition'; fields: ConfigurationField[] }
  | { action: 'saveSettings'; settings: ConsoleSettings }
  | { action: 'saveEnvironment'; environment: EnvironmentRegistration }
export interface AssignmentRequest { applicationId: string; version: number; planId: string; configuration: Values }
export interface ApplicationAssignment { applicationId: string; release: ApplicationRelease; planId: string; configuration: Values }
export interface ClientProduct { legalName: string; region: string; timezone: string; definitionVersion: number; configuration: Values; applications: ApplicationAssignment[]; activity: ProductActivity[] }
export interface ClientProductRequest { displayName: string; hosts: string[]; legalName: string; region: string; timezone: string; configuration: Values; applications: AssignmentRequest[] }
export interface ClientProductResponse { client: Client; product: ClientProduct; resolved: { applicationId: string; components: ApplicationComponent[]; navigation: NavigationItem[] }[]; reconciliation: Reconciliation }
