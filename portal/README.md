# IkedaDev SaaS Portal (`portal/bff` + `portal/web`)

Portal administrativo multi-tenant promovido como aplicación de primera clase dentro de `ikedadev-platform`. Integra de punta a punta los **7 microservicios en Rust** (`events-api`, `crm-api`, `identity-api`, `catalog-api`, `bookings-api`, `telemetry-api` y `content-api`) con **autenticación OIDC de producción mediante Keycloak**, **Design System Kropy** con personalización dinámica por tenant (`tenants.config`), editor visual **TipTap** para Blog/CMS, trazabilidad `X-Request-Id` y telemetría/auditoría en tiempo real.

---

## Arquitectura del Portal SaaS

```text
Browser (portal/web — React 19 + TypeScript + Vite :5173)
        │  Cookie HttpOnly `ikeda_bff_session` (nunca expone JWTs ni API keys al navegador)
        │  ThemeProvider (Kropy Design System + variables CSS dinámicas por tenant)
        │  Tracker batch de eventos TRACKING (`page.viewed` -> POST /api/telemetry/track)
        ▼
Portal BFF (portal/bff — Hono + Zod + Node 22 :4000)
        │  1) OAuth2 Authorization Code + PKCE contra Keycloak (:8180 / realm `ikeda`)
        │  2) Auto-refresh de access_token + propagación de `Authorization: Bearer <JWT>` y `X-Request-Id`
        │  3) Configuración y branding por tenant (`GET/PATCH /api/settings`, `GET /api/branding/:slug`)
        │  4) Emisión asíncrona (fire-and-forget) de eventos `AUDIT` hacia `telemetry-api`
        ├──► events-api    (:8080)  Valida RS256 JWKS + claim `tenant_id` (RLS)
        ├──► crm-api       (:8081)  Valida RS256 JWKS + claim `tenant_id` (RLS)
        ├──► identity-api  (:8082)  Valida RS256 JWKS + usuarios Keycloak + `tenants.config`
        ├──► catalog-api   (:8083)  Valida RS256 JWKS + claim `tenant_id` (RLS)
        ├──► bookings-api  (:8084)  Reservas multi-servicio (snapshots + estados + idempotencia)
        ├──► telemetry-api (:8085)  Eventos append-only (`TRACKING` / `AUDIT`), timeline y métricas (RLS)
        └──► content-api   (:8086)  Headless CMS (Artículos TipTap, Media Library R2/BYOS, SSR+SSG, RLS)
```

---

## Design System Kropy y Branding Dinámico por Tenant

El portal adopta el **Kropy Design System** como estética base por defecto:

- **Tipografía:** `Plus Jakarta Sans` (pesos 300 a 800, cargada en `portal/web/index.html`).
- **Paleta Light por defecto:** `--color-primary: #059669` (Emerald 600), `--color-primary-hover: #047857`, `--color-primary-soft: #ecfdf5`, `--color-accent: #0f172a`, `--color-bg: #ffffff`, `--color-surface: #ffffff`, `--color-border: #e2e8f0`.
- **Paleta Dark por defecto (`.dark` en `<html>`):** `--color-primary: #10b981` (Emerald 500), `--color-primary-hover: #059669`, `--color-primary-soft: rgba(16, 185, 129, 0.12)`, `--color-accent: #262626`, `--color-bg: #0a0a0a`, `--color-surface: #171717`, `--color-border: #262626`.
- **Geometría y superficies:** `--radius-card: 24px`, `--radius-panel: 16px`, `--radius-input: 12px`, `--radius-pill: 9999px` y cabecera glassmorphic (`--surface-glass`).

### Personalización en Vivo (`SettingsView` / `/api/settings`)

Cada tenant almacena su configuración en la columna JSONB `tenants.config` de `identity_db` (gestionada por `identity-api`):

- **Branding:** `displayName`, `logoUrl`, `primaryColor` (`#RRGGBB`), `accentColor`, `themeMode` (`light` | `dark` | `system`).
- **Regionalización:** `currency` (`CLP`, `USD`, `EUR`, `MXN`, `ARS`, `COP`, `PEN`), `timezone` y `language` (`es`, `en`).
- **Feature Toggles de Módulos (`features`):** `crm`, `catalog`, `events`, `bookings`, `telemetry`, `content`. Desactivar un módulo oculta su entrada en el `Sidebar` y protege la ruta automáticamente.

### Endpoints de Configuración y Branding en `portal/bff`

| Método y Ruta | Autenticación | Descripción |
| :--- | :---: | :--- |
| `GET /api/settings` | Sesión autenticada | Devuelve la configuración efectiva del tenant autenticado (fusionada con defaults de Kropy), metadatos del tenant y `canEditSettings`. |
| `PATCH /api/settings` | `tenant:admin` / `platform:admin` | Actualiza `tenants.config` del tenant de la sesión en `identity-api` y registra un evento `AUDIT`. |
| `GET /api/branding/:slug` | Pública (pre-login) | Devuelve el branding público seguro (`displayName`, `logoUrl`, `primaryColor`, `accentColor`, `themeMode`) para personalizar `LoginView` cuando se accede con `?tenant=<slug>` o subdominio. |

---

## Autenticación Multi-Tenant OIDC (Keycloak) y `PLATFORM_ADMIN_API_KEY`

Al levantar Docker Compose, Keycloak 26 importa automáticamente [`deploy/keycloak/ikeda-realm.json`](../deploy/keycloak/ikeda-realm.json):

- **Realm:** `ikeda` (`http://localhost:8180/realms/ikeda`)
- **Protocol Mapper:** inyecta el atributo de usuario `tenant_id` como claim JWT `tenant_id` en el access token y en el ID token (requerido por `ikeda-core`).
- **Cliente `identity-api-admin`:** cuenta de servicio confidencial con permisos `realm-management` para que `identity-api` cree y deshabilite usuarios reales en Keycloak.
- **Cliente `tenant-portal-bff`:** cliente OIDC confidencial con PKCE (`S256`) y callback en `http://localhost:4000/auth/callback`.
- **Usuarios demo pre-cargados** (asociados a `demo-tenant`, UUID `11111111-1111-4111-8111-111111111111`):
  - **Admin (`tenant:admin`, `platform:admin`):** `admin@demo.ikedadev.com` / `Admin123!`
  - **Staff (`tenant:member`):** `staff@demo.ikedadev.com` / `Staff123!`

### Nota sobre `PLATFORM_ADMIN_API_KEY` (Opcional)

El BFF **no utiliza ninguna `TENANT_API_KEY` estática** para tráfico de negocio: todas las llamadas a los 6 microservicios reenvían estrictamente el JWT RS256 del usuario autenticado (`Authorization: Bearer <access_token>`).

La variable de entorno opcional **`PLATFORM_ADMIN_API_KEY`** se utiliza **exclusivamente** en `portal/bff/src/routes/settings.ts` (lectura/actualización de `tenants.config` en `identity-api` y resolución pública de `GET /api/branding/:slug`), en caso de que un usuario `tenant:admin` no posea el rol `platform:admin` en su JWT. El BFF impone siempre el `tenantId` extraído de la sesión autenticada, por lo que ningún tenant puede leer o modificar la configuración de otro.

---

## Librería de Componentes UI (`portal/web/src/ui/`)

Todas las vistas del portal (`DashboardView`, `ClientsView`, `CatalogView`, `BookingsView`, `TelemetryView`, `IdentityView`, `SettingsView` y `LoginView`) están construidas sobre un sistema de componentes reutilizables en `portal/web/src/ui/` y `portal/web/src/components/` con estilos puros en `src/styles/tokens.css` y `src/styles/ui.css`:

- **`DataTable<T>`:** tabla genérica con búsqueda integrada, ordenamiento, paginación unificada, filas `<Skeleton />` durante carga, `<EmptyState />` cuando no hay resultados y transformación automática a tarjetas apiladas en pantallas `<768px`.
- **`DrawerProvider` (`useDrawer()`):** panel lateral deslizante (`sm` | `md` | `lg`) con backdrop difuminado, cabecera/pie fijos, cierre con `Escape`/clic exterior y advertencia de cambios sin guardar.
- **`ModalProvider` (`useModal()`):** pila de modales con `z-index` incremental y helper asíncrono `confirm({ title, message, variant })` para confirmaciones destructivas.
- **`ToastProvider` (`useToast()`):** notificaciones apiladas arriba a la derecha con autocierre configurable e integración global con `bffClient.ts` para errores de red, `403`, `409 Conflict` y `422`.
- **`Skeleton`, `StatCard`, `StatusBadge`, `Button`, `Input` / `Select` / `Textarea`, `EmptyState`:** primitivas visuales consistentes con el Design System Kropy.
- **`ThemeProvider` + `AppLayout` / `Topbar` / `Sidebar`:** inyección reactiva de variables CSS por tenant, conmutador de tema claro/oscuro y menú lateral responsivo filtrado según `features.*`.

---

## Observabilidad, Auditoría y Telemetría

- **Correlación `X-Request-Id` de extremo a extremo:** El BFF reutiliza el `X-Request-Id` entrante o genera un UUID nuevo, lo reenvía en todas las llamadas hacia los 6 microservicios Rust y lo devuelve en la respuesta HTTP para correlacionar trazas en logs y eventos.
- **Auditoría asíncrona (`AUDIT` fire-and-forget):** Al crear o actualizar clientes (`client.created`, `client.updated`), ítems del catálogo (`catalog.created`, `catalog.updated`), reservas (`booking.created`, `booking.updated`, `booking.cancelled`) o ajustes del negocio, el BFF registra eventos `AUDIT` en `telemetry-api` (`TELEMETRY_API_URL`) de forma asíncrona sin bloquear la operación principal.
- **Endpoints BFF `/api/telemetry/*`:**
  - `POST /api/telemetry/track`: recibe lotes de eventos `TRACKING` desde el frontend y los envía a `POST /v1/events/batch`.
  - `GET /api/telemetry/events`: consulta eventos paginados mediante `POST /v1/events/search` (filtros `kind`, `event_name`, `source`).
  - `GET /api/telemetry/metrics`: obtiene el resumen de métricas mediante `POST /v1/metrics/summary` (`from`, `to`, `kind`, `source`; últimos 7 días por defecto).
- **Tracker batch en el Frontend (`portal/web/src/api/tracker.ts`):** Encola eventos `TRACKING` (como `page.viewed` al navegar entre vistas) y los envía en lote a `POST /api/telemetry/track`, ignorando errores de red de forma silenciosa.
- **Vista "Actividad / Telemetría" (`TelemetryView`):** Muestra KPIs en `<StatCard />`, desglose por `event_name` y línea de tiempo en `<DataTable />` con filtro `ALL` / `TRACKING` / `AUDIT`.

---

## Desarrollo Local

### 1. Levantar la plataforma (Postgres + Keycloak + 6 APIs Rust)

Desde la raíz del monorepo (`ikedadev-platform/`):

```bash
# Stack completo: PostgreSQL 17 (6 DBs) + Keycloak 26 + 6 microservicios Rust
docker compose --profile full up -d --build

# Opcional: sumar observabilidad local (Loki :3100 + Promtail + Grafana :3000)
docker compose --profile full --profile observability up -d --build
```

Verifica los endpoints:
- Keycloak OIDC discovery: `http://localhost:8180/realms/ikeda/.well-known/openid-configuration`
- Microservicios Rust: `http://localhost:8080/health` .. `8085/health`

### 2. Configurar e instalar `portal/bff` y `portal/web`

```bash
# BFF
cd portal/bff
cp .env.example .env
npm install

# Web
cd ../web
cp .env.example .env
npm install
```

### 3. Poblar datos completos de la plataforma (`npm run seed`)

El script `portal/bff/src/seed.ts` es **100% idempotente** y siembra **Keycloak + los 6 microservicios** (`identity-api`, `crm-api`, `catalog-api`, `events-api`, `bookings-api`, `telemetry-api`) con **2 tenants reales** para validar branding dinámico y aislamiento multi-tenant (PostgreSQL RLS):

```bash
cd portal/bff
npm run seed
```

> Usa `SKIP_KEYCLOAK=true npm run seed` si deseas omitir el paso de sincronización con Keycloak Admin API (por ejemplo, si ya importaste un realm limpio). En caso contrario, el seed asegura automáticamente los roles `telemetry:read` y `telemetry:write` en el realm `ikeda` y los asigna a los usuarios demo existentes sin necesidad de borrar volúmenes.

#### Tenants y credenciales demo generadas por el seed

| Tenant (`slug`) | Negocio y Branding | Cuenta / Contraseña | Rol | Qué incluye |
| :--- | :--- | :--- | :--- | :--- |
| **`demo-tenant`** | **Clínica Kropy Estética** (`#059669` Emerald) | `admin@demo.ikedadev.com` / `Admin123!` | `platform:admin`, `tenant:admin` | 2 calendarios, 3 categorías, 7 ítems (`SERVICE`, `PRODUCT`, `BUNDLE`), 8 clientes, 8 reservas (todos los estados), 18 eventos `TRACKING`/`AUDIT`, 1 API Key y equipo completo |
| **`demo-tenant`** | **Clínica Kropy Estética** | `staff@demo.ikedadev.com` / `Staff123!` | `tenant:member` | Acceso operativo estándar sobre `demo-tenant` |
| **`demo-tenant`** | **Clínica Kropy Estética** | `dra.sofia@demo.ikedadev.com` / `Sofia123!` | `tenant:member` | Profesional provisto vía `identity-api` + Keycloak |
| **`demo-tenant`** | **Clínica Kropy Estética** | `recepcion@demo.ikedadev.com` / `Recepcion123!` | `tenant:member` | Recepción provista vía `identity-api` + Keycloak |
| **`barberia-norte`** | **Barbería Norte Studio** (`#2563EB` Corporate Blue) | `admin@barberianorte.cl` / `Norte123!` | `tenant:admin` | 1 calendario, 2 categorías, 3 ítems, 3 clientes, 2 reservas y 6 eventos (100% aislados por RLS respecto a `demo-tenant`) |

#### Tips para probar branding pre-login y aislamiento multi-tenant (`?tenant=<slug>`)

- **Clínica Kropy Estética:** abre `http://localhost:5173/?tenant=demo-tenant` — `LoginView` cargará el branding esmeralda (`#059669`) vía `GET /api/branding/demo-tenant` antes de iniciar sesión.
- **Barbería Norte Studio:** abre `http://localhost:5173/?tenant=barberia-norte` — `LoginView` cambiará dinámicamente al branding azul corporativo (`#2563EB`) vía `GET /api/branding/barberia-norte`. Al iniciar sesión con `admin@barberianorte.cl` / `Norte123!`, verás únicamente los clientes, catálogo, reservas y telemetría de Barbería Norte (cero fuga de datos desde `demo-tenant`).

### 4. Ejecutar BFF y Web en paralelo

```bash
# Terminal A — Portal BFF (:4000)
cd portal/bff
npm run dev

# Terminal B — Portal Web (:5173)
cd portal/web
npm run dev
```

Abre **http://localhost:5173** e inicia sesión con `admin@demo.ikedadev.com` / `Admin123!`.

### Validación de tipos y build

```bash
cd portal/bff && npx tsc --noEmit
cd portal/web && npx tsc --noEmit && npm run build
```

---

## Despliegue en Producción (Cloud Run + Cloudflare Pages)

- **Portal BFF en Google Cloud Run (`.github/workflows/portal-bff.yml`):**
  - Se dispara automáticamente al modificar `portal/bff/**` o `.github/workflows/portal-bff.yml` en `develop` (`qa`) o `main` (`prod`).
  - Construye la imagen Node 22 Alpine desde `context: portal/bff` (`portal/bff/Dockerfile`).
  - Mantiene el nombre del servicio Cloud Run **`tenant-portal-bff-qa`** / **`tenant-portal-bff-prod`** para preservar los mapeos DNS existentes (ej. `https://platform-bff-qa.ikedadev.com`) y los bindings IAM (`bff-invoker`).
  - Inyecta `GCP_IAM_AUTH=true`, las URLs privadas de los 6 microservicios (incluido `TELEMETRY_API_URL`) y el secreto opcional `PLATFORM_ADMIN_API_KEY`.
- **Portal Web en Cloudflare Pages (`portal/web`):**
  - **Root directory:** `portal/web`
  - **Build command:** `npm run build`
  - **Build output directory:** `dist`
  - **Environment variable:** `VITE_BFF_URL` apuntando al dominio público del BFF (ej. `https://platform-bff-qa.ikedadev.com`).

Consulta [`deploy/gcp/README.md`](../deploy/gcp/README.md) para el runbook completo de infraestructura, variables de entorno y WAF.
