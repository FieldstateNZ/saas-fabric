# Tenant Runtime & Data API Platform Specification

Status: Draft
Scope: Platform architecture (control plane, reconciliation, runtime plane, resource plane)

## 1. Purpose

This specification defines a SaaS platform architecture in which tenant infrastructure is declared
through GitOps and tenant-specific runtime resources are resolved transparently by platform services.

The primary objective is to allow application business logic to remain unaware of:

- tenant infrastructure topology
- database locations
- database credentials
- connection strings
- tenant provisioning
- identity-provider implementation
- database placement strategy
- secret storage implementation
- infrastructure lifecycle

Applications interact with stable platform APIs such as a Data API rather than directly connecting to
tenant infrastructure.

The platform is responsible for translating an authenticated tenant identity into the runtime resources
associated with that tenant.

## 2. Core Principle

The fundamental platform contract is:

> Applications address logical resources. The platform resolves those logical resources to
> tenant-specific physical resources at runtime.

For example, application code requests:

```
customers
orders
audit
configuration
```

rather than:

```
SQL Server sql-au-east-04
Database acme-prod
Schema customer
Key Vault secret db-password
```

The physical implementation of those resources is a platform concern.

## 3. Architectural Model

The platform consists of four primary planes.

```
┌──────────────────────────────────────────────────────────┐
│                     CONTROL PLANE                        │
│                                                          │
│ Tenant definitions                                       │
│ Environment definitions                                  │
│ Resource policies                                        │
└──────────────────────────┬───────────────────────────────┘
                           │
                           │ desired state
                           ▼
┌──────────────────────────────────────────────────────────┐
│                 GITOPS RECONCILIATION                    │
│                                                          │
│ Git repository                                           │
│ ArgoCD / equivalent                                      │
│ Operators / controllers                                  │
│ Infrastructure provisioning                              │
└──────────────────────────┬───────────────────────────────┘
                           │
                           │ reconciled resources
                           ▼
┌──────────────────────────────────────────────────────────┐
│                    RUNTIME PLANE                         │
│                                                          │
│ Tenant resolution                                        │
│ Resource bindings                                        │
│ Data API                                                 │
│ Configuration API                                        │
│ Other platform APIs                                      │
└──────────────────────────┬───────────────────────────────┘
                           │
                           ▼
┌──────────────────────────────────────────────────────────┐
│                   RESOURCE PLANE                         │
│                                                          │
│ Databases                                                │
│ Key Vaults                                               │
│ Object storage                                           │
│ Messaging                                                │
│ Configuration stores                                     │
└──────────────────────────────────────────────────────────┘
```

## 4. Tenant Lifecycle

A tenant is represented as declarative desired state.

Example:

```yaml
apiVersion: platform.example.io/v1
kind: Tenant

metadata:
  name: acme

spec:
  identity:
    realm: acme

  routing:
    hosts:
      - acme.example.com

  features:
    invoicing: true
    analytics: true

  data:
    primary:
      class: dedicated
      provider: sql
      region: au-east

    audit:
      class: shared
      provider: postgres
      region: au-east

  configuration:
    profile: enterprise
```

The tenant definition expresses intent, not physical infrastructure details.

It SHOULD NOT contain:

- database passwords
- raw connection strings
- storage access keys
- private certificates
- other runtime secrets

The Git repository is the source of truth for desired tenant state.

## 5. GitOps Reconciliation

A GitOps reconciler such as ArgoCD observes tenant definitions and causes the desired tenant
infrastructure to exist.

A tenant definition may result in resources including:

```
Tenant
 │
 ├── Keycloak realm / identity configuration
 ├── Envoy host / route
 ├── application configuration
 ├── secret references
 ├── database
 ├── storage
 └── runtime resource bindings
```

Provisioning MAY be performed through:

- Kubernetes operators
- Crossplane
- Terraform controllers
- cloud-specific operators
- custom controllers

The reconciliation implementation is not part of the application contract.

## 6. Desired State vs Runtime State

Git defines desired state.

Git MUST NOT participate in normal application request processing.

The following request flow is explicitly prohibited:

```
Request
  ↓
Read tenant
  ↓
Query Git
  ↓
Query Kubernetes
  ↓
Discover database
  ↓
Resolve secret
  ↓
Execute request
```

Instead, reconciliation MUST produce or update runtime state before application requests occur.

Conceptually:

```
Git desired state
      │
      ▼
Reconciliation
      │
      ▼
Runtime Tenant Registry
      │
      ▼
Request processing
```

## 7. Runtime Tenant Registry

The runtime plane maintains an efficient representation of currently resolved tenant resources.

Conceptually:

```
TenantRuntimeBinding
{
    TenantId

    DataBindings

    ConfigurationBindings

    SecretBindings

    FeatureBindings

    StorageBindings
}
```

Example:

```
acme
 ├── data.primary → sql-au-east-03/acme-prod
 ├── data.audit   → shared-postgres-02/tenant-482
 ├── config       → appconfig/acme
 └── secrets      → vault/tenants/acme
```

These physical bindings are internal platform details.

Applications MUST NOT depend upon them.

The registry SHOULD support:

- fast lookup
- caching
- update propagation
- invalidation
- binding versioning
- resource lifecycle changes

## 8. Trust Boundary

Authentication occurs at the platform edge.

```
             UNTRUSTED NETWORK

                    │
                    ▼

          ┌──────────────────┐
          │ Gateway / Envoy  │
          │                  │
          │ authenticate     │
          │ validate token   │
          │ enforce ingress  │
          └─────────┬────────┘

────────────────────┼────────────────────
             PLATFORM TRUST BOUNDARY

                    ▼

          Internal platform APIs
```

The runtime plane does not implement user authentication.

It does not need to understand:

- Keycloak login flows
- OAuth authorization flows
- realm implementation
- refresh tokens
- MFA
- external identity providers
- password authentication

These concerns belong to the identity and ingress layers.

## 9. Runtime Security Contract

Internal platform services operate under the following invariant:

> A request accepted by the runtime has already passed through a trusted platform ingress that has
> authenticated the caller and validated the bearer token.

Therefore, runtime services are authentication-agnostic, but operate within a defined security contract.

Platform networking MUST ensure that protected runtime APIs cannot be directly accessed through an
untrusted path.

This MAY be implemented using:

- Kubernetes NetworkPolicy
- private cluster networking
- service mesh policy
- workload identity
- mTLS
- ingress-only service exposure
- equivalent platform controls

The runtime MUST NOT expose tenant selection through an arbitrary client-controlled request header.

## 10. Tenant Identity

Tenant identity is carried as part of the established bearer-token identity context.

Example:

```json
{
  "sub": "user-123",
  "tenant_id": "acme",
  "roles": [
    "user"
  ]
}
```

The canonical tenant identity claim is:

```
tenant_id
```

The exact claim name MAY be configurable by the platform but SHOULD be standardized across platform
services.

Runtime resolution is conceptually:

```
tenantId = token.claims["tenant_id"]

binding = tenantRuntime.resolve(tenantId)
```

## 11. No Tenant Header

Tenant identity MUST NOT be selected through a caller-provided header such as:

```
X-Tenant-Id: acme
```

This prevents multiple competing sources of tenant identity.

For example, the following ambiguous state MUST NOT be possible:

```
Bearer token:
    tenant_id = globex

X-Tenant-Id:
    acme
```

There MUST be a single authoritative tenant context.

That context is derived from the bearer token.

## 12. Authentication vs Identity Context

The platform separates authentication from runtime identity resolution.

```
AUTHENTICATION

"Who is this?"

        │
        ▼

Handled by identity platform
and trusted ingress


IDENTITY CONTEXT

"Which tenant does this request represent?"

        │
        ▼

Carried in bearer token


RUNTIME RESOLUTION

"What resources belong to this tenant?"

        │
        ▼

Handled by tenant runtime


RESOURCE ACCESS

"Perform the requested operation."

        │
        ▼

Handled by platform resource APIs
```

A runtime component MAY parse claims from the bearer token.

Doing so does not make the component responsible for authentication.

## 13. Application Architecture

Business applications MUST NOT directly access tenant databases.

The following architecture is discouraged:

```
Business Application
        │
        ▼
TenantDbContextFactory
        │
        ▼
Database
```

The target architecture is:

```
Business Application
        │
        │ logical data operation
        ▼
     Data API
        │
        ▼
Tenant Runtime Resolver
        │
        ▼
Physical Data Source
```

The Data API forms the abstraction boundary between application business logic and tenant data
infrastructure.

## 14. Data API

The Data API exposes tenant-scoped logical data resources.

Its design may resemble systems such as Data API Builder.

Example:

```http
POST /data/customers
Authorization: Bearer <token>
Content-Type: application/json

{
  "name": "Alice",
  "email": "alice@example.com"
}
```

The application does not specify a tenant.

The application does not specify a database.

The processing flow is:

```
POST /data/customers
        │
        ▼
Read tenant_id claim
        │
        ▼
Resolve tenant runtime binding
        │
        ▼
Resolve logical "customers" resource
        │
        ▼
Resolve associated data binding
        │
        ▼
Execute operation
```

## 15. Logical Data Resources

Applications address logical resources rather than physical tables or database instances.

For example:

```yaml
resources:

  customers:
    dataSource: primary

  orders:
    dataSource: primary

  auditEvents:
    dataSource: audit
```

A logical resource MAY expose operations including:

```
GET
LIST
QUERY
CREATE
UPDATE
DELETE
```

The exact operation model is defined by the Data API.

## 16. Logical Data Sources

Applications MAY refer to logical data-source names.

For example:

```
primary
audit
analytics
archive
```

These names represent intent rather than infrastructure.

For Acme:

```
primary
   ↓
Azure SQL
   ↓
acme-prod
```

For another tenant:

```
primary
   ↓
shared PostgreSQL
   ↓
tenant schema 472
```

The application contract remains unchanged.

## 17. Tenant Placement

Tenant configuration may describe resource placement through policy.

Example:

```yaml
data:

  primary:
    class: dedicated
    provider: sql
    region: au-east
```

Possible classes may include:

```
shared
dedicated
high-availability
regulated
development
ephemeral
```

The placement policy is interpreted by the platform.

Applications MUST NOT depend upon placement classes.

## 18. Database Isolation Models

The runtime abstraction MUST permit different tenant isolation models.

Examples:

**Dedicated database**

```
Tenant A → Database A
Tenant B → Database B
```

**Shared database with schemas**

```
Tenant A → Shared DB → Schema A
Tenant B → Shared DB → Schema B
```

**Shared database with tenant discriminator**

```
Tenant A ─┐
Tenant B ─┼→ Shared database
Tenant C ─┘
```

**Dedicated infrastructure**

```
Tenant A
   ↓
Dedicated database cluster
```

Applications MUST NOT require awareness of which isolation model is being used.

## 19. Tenant Migration

Tenant resources MUST be capable of changing without requiring application deployment.

Example:

```
Before

acme
 ↓
shared-db-02
```

A configuration change:

```yaml
data:
  primary:
    class: dedicated
```

may result in:

```
After

acme
 ↓
acme-db-01
```

The application continues to interact with:

```
/data/customers
```

without modification.

A platform migration process is responsible for:

1. provisioning the target resource
2. migrating data
3. validating the target
4. changing the runtime binding
5. retiring the previous resource

The precise migration implementation is outside the application contract.

## 20. Runtime Binding Versioning

Runtime bindings SHOULD have a version or revision.

Example:

```json
{
  "tenantId": "acme",
  "revision": 42
}
```

This enables:

- safe cache invalidation
- migration coordination
- diagnostics
- rollback
- reconciliation status
- change auditing

Runtime services SHOULD be capable of discovering binding changes without restarting application
workloads.

## 21. Secrets

Secrets MUST NOT be stored directly in Git tenant definitions.

Git SHOULD contain secret references or secret requirements.

Example:

```yaml
data:
  primary:
    credentials:
      secretRef: tenant/acme/data-primary
```

The runtime may resolve this internally to:

```
Azure Key Vault
AWS Secrets Manager
Hashicorp Vault
Kubernetes Secret
```

Applications never receive or understand the physical secret location unless explicitly required by a
platform API.

## 22. Connection Management

Database connection management belongs inside the Data API or associated data-runtime infrastructure.

Applications MUST NOT manage per-tenant connection pools.

The data runtime SHOULD manage:

- connection creation
- connection pooling
- maximum pool size
- idle eviction
- credential rotation
- connection failure recovery
- pool disposal
- resource migration
- tenant activity

This prevents the number of application replicas multiplied by the number of tenants from creating
uncontrolled database connection growth.

## 23. Data API Authorization

Tenant resolution and authorization are separate concepts.

Tenant resolution answers:

> Which tenant resources does this request target?

Authorization answers:

> Is this established identity permitted to perform this data operation?

The Data API MAY use claims such as:

```
sub
roles
permissions
scope
```

to authorize operations.

However, authorization policy MUST NOT alter tenant selection.

The tenant is always derived from the canonical tenant identity context.

## 24. Runtime Independence From Identity Provider

Runtime APIs MUST NOT depend directly on Keycloak-specific concepts.

For example, runtime code SHOULD NOT contain logic such as:

```
if realm == ...
if keycloakClient == ...
call Keycloak to validate ...
```

The runtime contract is:

```
trusted bearer token
        +
canonical tenant claim
```

Therefore, the identity implementation may change:

```
Keycloak
Entra ID
Auth0
Customer IdP
OIDC broker
```

without requiring changes to the tenant runtime.

## 25. Example End-to-End Request

A user makes a request to:

```
https://acme.example.com/orders
```

The flow is:

```
1. User
      │
      ▼

2. Envoy / Gateway
      │
      ├── validates bearer
      ├── verifies route
      └── forwards authenticated request
      │
      ▼

3. Orders Application
      │
      │ needs customer data
      ▼

4. Data API
      │
      ├── reads tenant_id = acme
      │
      ▼

5. Tenant Runtime
      │
      ├── resolves acme
      │
      └── data.primary
      │
      ▼

6. Data Runtime
      │
      └── resolves physical connection
      │
      ▼

7. Tenant Database
```

The Orders application is unaware of steps 5–7.

## 26. Application Contract

From the perspective of an application developer, the platform contract SHOULD be approximately:

```
I receive an authenticated request.

If I need data:
    call the Data API.

If I need configuration:
    call the Configuration API.

If I need tenant feature state:
    call the Feature API.

If I need storage:
    call the Storage API.

The platform resolves the physical resources.
```

Applications SHOULD NOT:

```
open tenant database connections
read tenant connection strings
query Kubernetes
query Git repositories
select tenant databases
understand tenant placement policies
call identity providers to determine tenant context
```

## 27. Platform APIs

The Data API is the first runtime service, but the same architecture may support additional logical
services.

For example:

```
runtime.platform.internal

/data
/config
/features
/files
/events
/secrets
```

Each service follows the same model:

```
Bearer identity
      │
      ▼
Tenant resolution
      │
      ▼
Logical resource
      │
      ▼
Physical tenant resource
```

## 28. Failure Behaviour

The platform MUST fail closed when tenant context cannot be safely resolved.

**Missing tenant claim**

```
Result:
Request rejected
```

**Unknown tenant**

```
tenant_id = unknown

Result:
Request rejected
```

**Tenant runtime unavailable**

```
Result:
Service unavailable
```

The runtime MUST NOT silently use:

- a default tenant
- the first available database
- a shared fallback connection
- tenant information supplied through arbitrary request headers

## 29. Observability

Every platform request SHOULD carry sufficient context for tracing.

Telemetry SHOULD include:

```
request_id
trace_id
tenant_id
runtime_binding_revision
logical_resource
operation
physical_resource_identifier
```

Physical-resource information SHOULD normally remain internal to platform telemetry.

Application telemetry SHOULD primarily use logical resource names.

Secrets and connection strings MUST NOT appear in telemetry.

## 30. Auditability

The platform SHOULD provide an audit trail for:

```
tenant creation
tenant updates
resource provisioning
resource replacement
runtime binding changes
database migrations
feature changes
configuration changes
administrative operations
```

Changes SHOULD be traceable from:

```
Git commit
    ↓
reconciliation event
    ↓
runtime binding revision
    ↓
platform operation
```

## 31. Key Architectural Invariants

The following are fundamental platform invariants.

| # | Invariant |
|---|-----------|
| 1 | Git is the source of desired tenant state but is not in the request path. |
| 2 | Applications consume logical platform services rather than tenant infrastructure directly. |
| 3 | Business logic does not receive database connections or tenant connection strings. |
| 4 | Authentication is handled by the platform edge. |
| 5 | Runtime services consume established identity but do not implement the authentication system. |
| 6 | Tenant identity is derived from the trusted bearer token. |
| 7 | A request cannot independently select a tenant using an arbitrary header. |
| 8 | Tenant resource placement can change without redeploying applications. |
| 9 | Physical infrastructure is a runtime binding, not an application dependency. |
| 10 | Failure to resolve tenant context fails closed. |

## 32. Non-Goals

The initial platform does not attempt to:

- define a universal database abstraction across every possible datastore
- remove the need for data schemas
- make relational and non-relational databases automatically interchangeable
- replace application-level authorization
- expose Kubernetes infrastructure directly to applications
- make applications responsible for tenant provisioning
- make Git part of runtime request handling
- make the runtime responsible for user authentication

The abstraction boundary is logical resource resolution, not pretending all infrastructure behaves
identically.

## 33. Conceptual Definition

The platform can be summarized as:

> A GitOps-driven SaaS control plane and tenant runtime that maps established tenant identity onto
> logical platform services and resolves those services to tenant-specific physical infrastructure.

Or more simply:

> Tenant-aware infrastructure. Tenant-agnostic applications.

The resulting architecture is:

```
GitOps
  │
  │ defines what the tenant should have
  ▼
Reconciliation
  │
  │ makes those resources exist
  ▼
Tenant Runtime
  │
  │ determines what this tenant currently has
  ▼
Platform APIs
  │
  │ expose logical resources
  ▼
Application
```

The application owns business logic.
The platform owns tenancy.
