# ikedadev-platform

Monorepo modular en **Rust (Cargo Workspace)** diseñado bajo **Clean Architecture** y **aislamiento multi-tenant estricto (PostgreSQL RLS)** para alojar el ecosistema de microservicios de la plataforma IkedaDev, acompañado del **Portal SaaS administrativo (`portal/bff` + `portal/web`)** con **Design System Kropy**, branding dinámico por tenant y autenticación **Keycloak OIDC**.

---

## 🏛️ Estructura del Monorepo

```text
ikedadev-platform/
├── Cargo.toml                           # Workspace root con dependencias unificadas
├── docker-compose.yml                   # Entorno local por perfiles (postgres, keycloak, apis, full, observability)
├── crates/
│   └── ikeda-core/                      # Librería compartida por los 6 microservicios
│       ├── response/                    # ApiResponse<T>, PaginatedApiResponse<T>, PaginationMeta
│       ├── error/                       # AppError mapeado a HTTP con enmascaramiento seguro en 500
│       ├── tenant/                      # TenantContext, Extractor Axum (JWT JWKS + CachedApiKeyValidator)
│       ├── criteria/                    # Criteria Pattern (AND/OR recursivo) + sqlx_builder (feature `sqlx`)
│       └── observability/               # init_tracing (JSON/pretty), http_trace_layer y request_id_layer (X-Request-Id)
├── services/
│   ├── events-api/                      # Calendarios, eventos, recurrencia RFC 5545 y disponibilidad
│   ├── crm-api/                         # Clientes finales multi-tenant, JSONB metadata y búsqueda Criteria
│   ├── identity-api/                    # Tenants, API Keys M2M (SHA-256) y usuarios sincronizados con Keycloak
│   ├── catalog-api/                     # Categorías, servicios, productos, bundles, variantes y batch lookup
│   ├── bookings-api/                    # Reservas multi-servicio con snapshots congelados, estados e idempotencia
│   ├── telemetry-api/                   # Eventos append-only (TRACKING/AUDIT), timeline de cliente y métricas
│   └── content-api/                     # Headless CMS multi-tenant (TipTap JSON/HTML, Media Library R2/BYOS, SSR+SSG)
├── deploy/
│   ├── keycloak/ikeda-realm.json        # Realm `ikeda` auto-importado (roles, clientes OIDC y usuarios demo)
│   ├── observability/                   # Stack opcional local/K3s: Grafana Loki, Promtail y Grafana pre-aprovisionado
│   ├── k8s/base/                        # Manifiestos Kustomize genéricos (Deployment, Service, ConfigMap)
│   └── argo/workflow-template.yaml      # Plantilla declarativa para Argo Workflows
├── scripts/
│   ├── init-databases.sh                # Crea las 6 DBs, aplica migraciones y siembra tenant/key local
│   ├── test-api.sh / test-api.ps1       # Scripts de smoke test local
├── docs/
│   └── platform-documentation.html      # Documentación técnica interactiva de la plataforma
├── portal/                              # Portal SaaS multi-tenant (BFF Hono + Web React/Vite + Design System Kropy)
│   ├── README.md                        # Guía completa de arquitectura, branding, telemetría y despliegue
│   ├── bff/                             # Backend-for-Frontend con sesiones HttpOnly, /api/settings, X-Request-Id y AUDIT
│   └── web/                             # SPA administrativa en React 19 + TypeScript + Vite (Cloudflare Pages)
└── .github/workflows/
    ├── ci.yml                           # Quality Gate del workspace (fmt, clippy -D warnings, tests)
    ├── events-api.yml                   # Build OCI multi-stage (cargo-chef) y push a ghcr.io / Artifact Registry
    ├── crm-api.yml                      # Build OCI multi-stage (cargo-chef) y push a ghcr.io / Artifact Registry
    ├── identity-api.yml                 # Build OCI multi-stage (cargo-chef) y push a ghcr.io / Artifact Registry
    ├── catalog-api.yml                  # Build OCI multi-stage (cargo-chef) y push a ghcr.io / Artifact Registry
    ├── bookings-api.yml                 # Build OCI multi-stage (cargo-chef) y push a ghcr.io / Artifact Registry
    ├── telemetry-api.yml                # Build OCI multi-stage (cargo-chef) y push a ghcr.io / Artifact Registry
    ├── content-api.yml                  # Build OCI multi-stage (cargo-chef) y deploy a Cloud Run (content-api-*)
    └── portal-bff.yml                   # Build Node 22 Alpine desde portal/bff y deploy a Cloud Run (tenant-portal-bff-*)
```

---

## 🗺️ Servicios, Puertos, Bases de Datos y OpenAPI

| Servicio | Puerto Local | Base de Datos | Swagger UI | OpenAPI JSON | Health Check |
| :--- | :---: | :---: | :--- | :--- | :--- |
| **`events-api`** | `8080` | `events_db` | `http://localhost:8080/swagger-ui` | `http://localhost:8080/openapi.json` | `http://localhost:8080/health` |
| **`crm-api`** | `8081` | `crm_db` | `http://localhost:8081/swagger-ui` | `http://localhost:8081/openapi.json` | `http://localhost:8081/health` |
| **`identity-api`** | `8082` | `identity_db` | `http://localhost:8082/swagger-ui` | `http://localhost:8082/openapi.json` | `http://localhost:8082/health` |
| **`catalog-api`** | `8083` | `catalog_db` | `http://localhost:8083/swagger-ui` | `http://localhost:8083/openapi.json` | `http://localhost:8083/health` |
| **`bookings-api`** | `8084` | `bookings_db` | `http://localhost:8084/swagger-ui` | `http://localhost:8084/openapi.json` | `http://localhost:8084/health` |
| **`telemetry-api`** | `8085` | `telemetry_db` | `http://localhost:8085/swagger-ui` | `http://localhost:8085/openapi.json` | `http://localhost:8085/health` |
| **`content-api`** | `8086` | `content_db` | `http://localhost:8086/swagger-ui` | `http://localhost:8086/openapi.json` | `http://localhost:8086/health` |
| **Keycloak (`ikeda`)** | `8180` | In-memory (dev) | — | `http://localhost:8180/realms/ikeda/.well-known/openid-configuration` | `http://localhost:8180` |
| **PostgreSQL 17** | `5432` | Multi-DB (`events_db`, `crm_db`, `identity_db`, `catalog_db`, `bookings_db`, `telemetry_db`, `content_db`) | — | — | `pg_isready` |
| **Grafana / Loki** *(opcional)* | `3000` / `3100` | Volumen `loki_data` | — | — | `http://localhost:3100/ready` |
| **Portal SaaS BFF (`portal/bff`)** | `4000` | — (stateless + Keycloak) | — | — | `http://localhost:4000/health` |
| **Portal SaaS Web (`portal/web`)** | `5173` | — | — | — | `http://localhost:5173` |

---

## 🚀 Pila Tecnológica

- **Backend (Rust 1.85+, Edición 2021):** `axum`, `tokio`, `tower-http`, `sqlx` (PostgreSQL + `tls-rustls`), `utoipa` + `utoipa-swagger-ui`, `jsonwebtoken` (JWKS RS256), `rrule` (RFC 5545).
- **Observabilidad y Telemetría:** módulo compartido `ikeda_core::observability` (logs JSON/pretty vía `tracing`, correlación `X-Request-Id`, latencia y `tenant_id` sin registrar credenciales sensibles), microservicio `telemetry-api` (`:8085`) para eventos `TRACKING`/`AUDIT` y perfil opcional `observability` en Docker Compose (**Loki + Promtail + Grafana** en `deploy/observability/`).
- **Identidad y Autenticación:** Keycloak 26 (OIDC Authorization Code + PKCE para usuarios, `client_credentials` para provisión desde `identity-api`) y API Keys M2M (SHA-256) centralizadas en `identity_db` con caché en memoria (`CachedApiKeyValidator`).
- **Aislamiento Multi-Tenant:** PostgreSQL **Row-Level Security (RLS)** habilitado y forzado (`ENABLE` + `FORCE ROW LEVEL SECURITY`) en todas las tablas de negocio.
- **Portal SaaS (`portal/`):** BFF en TypeScript + Hono (`portal/bff`, `@hono/node-server`, `zod`, endpoints `/api/settings` y `/api/branding/:slug`, secreto opcional `PLATFORM_ADMIN_API_KEY` en GCP) y SPA en React 19 + Vite (`portal/web`, desplegable en Cloudflare Pages) con **Design System Kropy**, branding dinámico por tenant y feature toggles desde `SettingsView`.

---

## 🔒 Aislamiento Multi-Tenant y Seguridad

Todas las tablas de negocio en los 6 servicios aplican **RLS estricto** basado en la variable de sesión transaccional `app.current_tenant`:

```sql
ALTER TABLE <tabla> ENABLE ROW LEVEL SECURITY;
ALTER TABLE <tabla> FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_<tabla> ON <tabla>
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);
```

- Cada repositorio SQLx abre una transacción mediante `begin_tenant_tx(pool, tenant_id)` ejecutando `SELECT set_config('app.current_tenant', $1, true);` (local a la transacción, seguro con connection poolers como PgBouncer/Neon).
- `events-api`, `crm-api`, `catalog-api`, `bookings-api` y `telemetry-api` validan `X-Api-Key` contra `IDENTITY_DATABASE_URL` mediante la función `SECURITY DEFINER` `authenticate_api_key($1)` envuelta en `CachedApiKeyValidator` (con fallback a `DATABASE_URL` solo para pruebas locales aisladas).
- El traductor de `Criteria` a SQLx (`ikeda-core` con feature `sqlx`) valida todos los campos contra una whitelist por entidad, escapa wildcards `ILIKE` (`ESCAPE '\'`), limita la profundidad de anidamiento y usa parámetros vinculados (`push_bind`).
- El feature flag `dev-auth` está **deshabilitado por defecto** en todos los `Cargo.toml` y `Dockerfile`.

---

## 🐳 Levantar el Entorno Local

El archivo [`docker-compose.yml`](docker-compose.yml) usa perfiles para levantar solo lo que necesites:

```bash
# 1. Solo PostgreSQL (crea las 6 DBs, corre migraciones y siembra tenant/key de desarrollo)
docker compose up -d

# 2. PostgreSQL + Keycloak (auto-importa deploy/keycloak/ikeda-realm.json en http://localhost:8180)
docker compose --profile keycloak up -d

# 3. PostgreSQL + los 6 microservicios Rust compilados en contenedor
docker compose --profile apis up -d

# 4. Stack completo (PostgreSQL + Keycloak + los 6 microservicios Rust)
docker compose --profile full up -d

# 5. Stack completo + observabilidad local opcional (Loki :3100 + Promtail + Grafana :3000)
docker compose --profile full --profile observability up -d
```

También puedes ejecutar cada microservicio directamente con Cargo (copiando su `.env.example` a `.env` si deseas sobreescribir variables):

```bash
cargo run -p events-api
cargo run -p crm-api
cargo run -p identity-api
cargo run -p catalog-api
cargo run -p bookings-api
cargo run -p telemetry-api
```

### Portal SaaS (`portal/`)

Consulta [`portal/README.md`](portal/README.md) para instrucciones completas sobre el **Design System Kropy**, branding dinámico y feature toggles (`SettingsView` / `GET/PATCH /api/settings`), telemetría y despliegue en Cloud Run + Cloudflare Pages. En resumen:

```bash
# Terminal 1 — Portal BFF (puerto 4000)
cd portal/bff && npm install && npm run dev

# Terminal 2 — Portal Web React/Vite (puerto 5173)
cd portal/web && npm install && npm run dev
```

### Documentación Técnica

Abre [`docs/platform-documentation.html`](docs/platform-documentation.html) directamente en tu navegador para explorar la arquitectura, diagramas y contratos de la plataforma.

---

## 🛠️ Comandos de Calidad y Validación

Desde la raíz del workspace:

```bash
# 1. Formato estricto
cargo fmt --all -- --check

# 2. Linter Clippy sin advertencias (producción y con dev-auth)
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features ikeda-core/dev-auth -- -D warnings

# 3. Suite de tests del workspace
cargo test --workspace
cargo test -p ikeda-core --features dev-auth
```

Para validar el portal TypeScript:

```bash
cd portal/bff && npx tsc --noEmit
cd portal/web && npx tsc --noEmit && npm run build
```

---

## 🔄 CI/CD Modular (GitHub Actions)

Los workflows en `.github/workflows/` operan de forma independiente con filtros `paths:`:

1. **[`ci.yml`](.github/workflows/ci.yml):** Quality Gate en PRs y pushes a `main`/`develop` (`cargo fmt`, `cargo clippy -D warnings`, `cargo test --workspace --lib`).
2. **[`events-api.yml`](.github/workflows/events-api.yml), [`crm-api.yml`](.github/workflows/crm-api.yml), [`identity-api.yml`](.github/workflows/identity-api.yml), [`catalog-api.yml`](.github/workflows/catalog-api.yml), [`bookings-api.yml`](.github/workflows/bookings-api.yml), [`telemetry-api.yml`](.github/workflows/telemetry-api.yml):** Compilan cada servicio con `cargo-chef` y Docker Buildx únicamente cuando cambia su carpeta, `crates/ikeda-core`, `Cargo.toml` o `Cargo.lock`, publicando imágenes inmutables etiquetadas con `prod-<short-sha>` / `qa-<short-sha>` y despliegue automatizado en Google Cloud Run o Kubernetes ([`deploy/k8s/base`](deploy/k8s/base), [`deploy/argo`](deploy/argo)).
3. **[`portal-bff.yml`](.github/workflows/portal-bff.yml):** Construye la imagen Node 22 Alpine de `portal/bff` y la despliega en Google Cloud Run manteniendo el servicio `tenant-portal-bff-qa` / `tenant-portal-bff-prod` (con `PLATFORM_ADMIN_API_KEY` opcional para `/api/settings`). El frontend `portal/web` se despliega en Cloudflare Pages.

---

## 🌐 Generación de Tipos TypeScript desde OpenAPI

```bash
npx openapi-typescript http://localhost:8080/openapi.json -o src/api/events.d.ts
npx openapi-typescript http://localhost:8081/openapi.json -o src/api/crm.d.ts
npx openapi-typescript http://localhost:8082/openapi.json -o src/api/identity.d.ts
npx openapi-typescript http://localhost:8083/openapi.json -o src/api/catalog.d.ts
npx openapi-typescript http://localhost:8084/openapi.json -o src/api/bookings.d.ts
npx openapi-typescript http://localhost:8085/openapi.json -o src/api/telemetry.d.ts
```
