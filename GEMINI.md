# IkedaDev Platform (`ikedadev-platform`)

Plataforma SaaS multi-tenant white-label con **8 microservicios Rust 100% agnósticos**, un **BFF TypeScript/Hono** que concentra toda la orquestación y autenticación OIDC, y una **SPA React/Vite** (identidad Kropy).

> **Principio rector:** Cero llamadas HTTP entre microservicios Rust. Cada API conoce únicamente su dominio y valida API Keys vía `IDENTITY_DATABASE_URL` (`authenticate_api_key`). Toda orquestación multi-servicio (reservas + eventos + CRM + catálogo, dashboard, auditoría) vive en `portal/bff`.

---

## Mapa de Arquitectura

| Componente | Ruta | Puerto Local | Base de Datos | Responsabilidad |
| :--- | :--- | :---: | :--- | :--- |
| **Events API** | `services/events-api` | `8080` | `events_db` | Calendarios, recurrencias RFC 5545 (`rrule`), disponibilidad, excepciones, QStash. |
| **CRM API** | `services/crm-api` | `8081` | `crm_db` | Clientes multi-tenant CRUD y búsqueda dinámica `Criteria`. |
| **Identity API** | `services/identity-api` | `8082` | `identity_db` | Tenants (`/v1/tenants`), API Keys M2M (`/v1/api-keys`), usuarios sincronizados con Keycloak Admin API. |
| **Catalog API** | `services/catalog-api` | `8083` | `catalog_db` | Categorías, servicios, productos, variantes y bundles. |
| **Bookings API** | `services/bookings-api` | `8084` | `bookings_db` | Reservas con snapshots inmutables, máquina de estados e idempotencia. |
| **Telemetry API** | `services/telemetry-api` | `8085` | `telemetry_db` | Eventos append-only (`TRACKING`/`AUDIT`), timeline por cliente y métricas. |
| **Content API** | `services/content-api` | `8086` | `content_db` | Headless CMS: artículos TipTap + HTML sanitizado (`ammonia`), revisiones, taxonomías, preview HS256, Deploy Hooks SSG (anti-SSRF). |
| **Media API** | `services/media-api` | `8087` | `media_db` | Biblioteca transversal: presign/confirm (Cloudflare R2 SigV4 o local dev HMAC), whitelist MIME sin SVG, cuotas por tenant. |
| **Core Crate** | `crates/ikeda-core` | — | — | `ApiResponse`, `AppError`, `Criteria` + `sqlx_builder`, `observability`, `TenantContext`, `CachedApiKeyValidator`, `JwksJwtValidator`. |
| **Portal BFF** | `portal/bff` | `4000` | — (en memoria) | Hono BFF: OIDC PKCE (`/auth/*`), cookie `ikeda_bff_session`, orquestación, proxy autenticado. |
| **Portal Web** | `portal/web` | `5173` | — | SPA React 19 + Vite + TipTap + Design System Kropy (`src/styles/tokens.css`, `ui.css`, `src/ui/`). |
| **Keycloak** | `deploy/keycloak/ikeda-realm.json` | `8180` | — | Realm `ikeda`, claim `tenant_id`, cliente M2M `identity-api-admin`, cliente OIDC `tenant-portal-bff`. |

---

## Flujo de Autenticación y Multi-Tenancy (P0)

1. **Login OIDC:** `GET /auth/login` (PKCE S256 + cookie `ikeda_oidc_tx` firmada HMAC) → Keycloak → `GET /auth/callback` → sesión en memoria (`MAX_SESSIONS = 10_000`) + cookie `ikeda_bff_session` (`HttpOnly`, `SameSite=Lax`).
2. **Propagación BFF → Rust:** `requireAuth` (`portal/bff/src/middleware/auth.ts`) extrae el JWT de sesión (con auto-refresh < 30s) y `ServiceHttpClient` (`portal/bff/src/clients/http.ts`) reenvía **solo** `Authorization: Bearer <jwt>` (jamás `X-Api-Key` en flujos de usuario).
3. **Cloud Run Privado:** Si `GCP_IAM_AUTH=true`, el BFF adjunta el ID token de GCP en `X-Serverless-Authorization: Bearer <idToken>` dejando intacto `Authorization`.
4. **Aislamiento PostgreSQL RLS:** `TenantContext` (`crates/ikeda-core/src/tenant/extractor.rs`) verifica el JWT (RS256 vía JWKS) o API Key (SHA-256). Cada transacción ejecuta `SELECT set_config('app.current_tenant', $1, true)` conectado como `ikeda_app` (`NOBYPASSRLS`) sobre tablas con `ENABLE` + `FORCE ROW LEVEL SECURITY`.
5. **Excepción única de API Key en BFF:** `PLATFORM_ADMIN_API_KEY` solo existe en `portal/bff/src/routes/settings.ts` cuando `authMethod === 'session'`. No existe `TENANT_API_KEY` en runtime del BFF.

---

## Matriz Resumida de Roles y Scopes

- **Fuente única de permisos:**
  - BFF: `portal/bff/src/auth/permissions.ts`
  - Web: `portal/web/src/auth/permissions.ts` (`usePermissions()`, `can()`, `canAccessSection()`)
  - Realm dev: `deploy/keycloak/ikeda-realm.json`
- **Roles:**
  - `platform:admin` / `tenant:admin`: acceso total al tenant, gestión de usuarios/API Keys (`IdentityView`), branding/features (`SettingsView`), `content:publish`, `media:delete` y contexto `branding` en medios.
  - `tenant:member` (`staff`): opera según scopes granulares asignados (`crm:write`, `catalog:write`, `bookings:write`, `events:write`, `telemetry:read`, `telemetry:write`, `content:read`, `content:write`, `media:read`, `media:write`).
    - Sin acceso a `IdentityView` ni `SettingsView` (en UI se ocultan y bloquean; `GET /api/settings` sigue accesible para que `ThemeProvider` cargue branding y `tenant.features`).
    - En `ContentView`, `staff` sin `content:publish` solo edita borradores `DRAFT` y no ve acciones de publicar/programar/archivar/eliminar ni Deploy Hooks.
    - En `MediaView`, `staff` no puede subir/editar en contexto `branding` ni eliminar/purgar sin `media:delete`.

---

## Reglas Detalladas y Validación

- Reglas modulares en `.agent/rules/`:
  - `rust-microservices.md` — Clean Architecture, `ikeda-core`, `IDENTITY_DATABASE_URL`.
  - `security-rls-rbac.md` — Invariantes P0 de RLS, Auth, Content/Media y RBAC.
  - `portal-design-system.md` — Estética Kropy, primitivas `src/ui/`, Light/Dark y móvil `375px`.
  - `deploy-and-ci.md` — Cloud Run, `GCP_DEPLOY_ENABLED`, topes de costo, Neon y K3s.
- Validación completa: ejecuta el workflow `/validate` (`.agent/workflows/validate.md`).
