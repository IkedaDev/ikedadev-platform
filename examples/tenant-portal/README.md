# Tenant Portal — BFF + Dashboard Web de Ejemplo

Aplicación de referencia aislada dentro de `ikedadev-platform` que demuestra y valida de punta a punta los **4 microservicios en Rust** (`events-api`, `crm-api`, `identity-api` y `catalog-api`) mediante una arquitectura **BFF (Backend for Frontend)** en TypeScript/Hono + un **Dashboard Web** en React/Vite.

> **Aislamiento en el Monorepo:** Todo este ejemplo vive exclusivamente bajo `examples/tenant-portal/`. No modifica `Cargo.toml` raíz ni el código fuente de los servicios Rust.

## Arquitectura

```text
Browser (React + Vite :5173)
        │  Solo habla con VITE_BFF_URL (nunca conoce X-Api-Key)
        ▼
Tenant Portal BFF (Hono + Zod :4000)
        │  Inyecta X-Api-Key: <TENANT_API_KEY> (y reenvía Authorization Bearer si llega)
        ├──► events-api   (:8080)  Calendarios, disponibilidad, reservas y 409 Conflict
        ├──► crm-api      (:8081)  Clientes con Criteria search y metadata JSONB
        ├──► identity-api (:8082)  Usuarios (Keycloak) y API Keys M2M
        └──► catalog-api  (:8083)  Categorías, ítems (SERVICE/PRODUCT/BUNDLE) y batch lookup
```

---

## Prerrequisitos

- **Docker & Docker Compose** (para PostgreSQL 17 y Keycloak 26)
- **Rust / Cargo** (si ejecutas los microservicios en local fuera de Docker)
- **Node.js 20+** y **npm**

---

## Perfiles disponibles en `docker-compose.yml`

Desde la raíz del repositorio (`ikedadev-platform/`), puedes levantar exactamente lo que necesites:

| Comando | Qué levanta |
| :--- | :--- |
| `docker compose up -d` | Solo **PostgreSQL 17** (crea automáticamente `events_db`, `crm_db`, `identity_db` y `catalog_db`, aplica todas las migraciones y siembra la API Key local) |
| `docker compose --profile keycloak up -d` | **PostgreSQL 17** + **Keycloak 26** (`http://localhost:8180`) |
| `docker compose --profile apis up -d --build` | **PostgreSQL 17** + las **4 APIs en Rust** (`8080`–`8083`) |
| `docker compose --profile full up -d --build` | **Todo**: PostgreSQL + Keycloak + las 4 APIs en Rust |
| `docker compose --profile events up -d` | PostgreSQL + un servicio individual (`events`, `crm`, `identity` o `catalog`) |

> **Nota si ya tenías un volumen previo de Postgres:** `scripts/init-databases.sh` se ejecuta automáticamente cuando el volumen `postgres_data` está vacío. Si ya habías levantado Postgres anteriormente antes de este cambio, reinicia el volumen con:
> ```bash
> docker compose down -v
> ```

---

## Paso 1: Levantar PostgreSQL (y opcionalmente Keycloak) desde la raíz

```bash
# Infraestructura completa (Postgres + Keycloak)
docker compose --profile keycloak up -d
```

Esto inicia:
- **PostgreSQL 17** en `localhost:5432` (`postgres` / `postgres_password`) con las 4 bases de datos migradas (`events_db`, `crm_db`, `identity_db`, `catalog_db`) y un tenant de desarrollo (`demo-tenant`) con API Key lista para pruebas:
  - `TENANT_API_KEY`: `ik_live_dev_tenant_portal_local_2026`
- **Keycloak 26** en `http://localhost:8180` (`admin` / `admin_dev_password`)

---

## Paso 2: Levantar los 4 microservicios Rust

### Opción A: Con Docker Compose (recomendado para prueba rápida)

```bash
# Si ya levantaste Postgres + Keycloak o quieres levantar todo de una vez:
docker compose --profile full up -d --build
# (o solo Postgres + las 4 APIs sin Keycloak:)
# docker compose --profile apis up -d --build
```

### Opción B: Correr los 4 servicios con Cargo en terminales separadas

Con PostgreSQL corriendo desde el Paso 1, ejecuta cada servicio desde la raíz del repo:

```bash
# Terminal 1 — events-api (:8080)
PORT=8080 DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/events_db IDENTITY_DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/identity_db cargo run -p events-api

# Terminal 2 — crm-api (:8081)
PORT=8081 DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/crm_db IDENTITY_DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/identity_db cargo run -p crm-api

# Terminal 3 — identity-api (:8082)
PORT=8082 DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/identity_db KEYCLOAK_BASE_URL=http://localhost:8180 cargo run -p identity-api

# Terminal 4 — catalog-api (:8083)
PORT=8083 DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/catalog_db IDENTITY_DATABASE_URL=postgres://postgres:postgres_password@localhost:5432/identity_db cargo run -p catalog-api
```

Verifica que los 4 respondan `200 OK` en `/health`:
- `http://localhost:8080/health` (`events-api`)
- `http://localhost:8081/health` (`crm-api`)
- `http://localhost:8082/health` (`identity-api`)
- `http://localhost:8083/health` (`catalog-api`)

---

## Paso 3: Configurar variables de entorno e instalar dependencias

### 3.1 Configurar el BFF

```bash
cd examples/tenant-portal/bff
cp .env.example .env
npm install
```

`bff/.env.example` ya trae configurada por defecto la `TENANT_API_KEY=ik_live_dev_tenant_portal_local_2026` sembrada por `scripts/init-databases.sh` y las URLs locales de las 4 APIs (`8080`–`8083`).

### 3.2 Configurar la Web Administrativa

```bash
cd examples/tenant-portal/web
cp .env.example .env
npm install
```

---

## Paso 4: Poblar datos de prueba (`npm run seed`)

Con los 4 servicios Rust activos y `bff/.env` listo, ejecuta el seed desde `examples/tenant-portal/bff/`:

```bash
npm run seed
```

El script creará de forma idempotente:
1. **1 calendario laboral** (`allow_overlaps: false`, `America/Santiago`) en `events-api`
2. **2 categorías** (`servicios-profesionales` y `productos-cuidado`) en `catalog-api`
3. **3 ítems de catálogo** (2 `SERVICE` con duración y 1 `PRODUCT` con inventario) en `catalog-api`
4. **3 clientes** con RUT/`tax_id` y `metadata` JSONB en `crm-api`
5. **2 reservas enriquecidas** para mañana en `events-api`

---

## Paso 5: Levantar BFF y Web en paralelo

En dos terminales:

```bash
# Terminal A — BFF (http://localhost:4000)
cd examples/tenant-portal/bff
npm run dev

# Terminal B — Dashboard Web (http://localhost:5173)
cd examples/tenant-portal/web
npm run dev
```

Abre **http://localhost:5173** en tu navegador.

---

## Flujo recomendado de validación E2E

1. **Dashboard:** Verifica que las tarjetas de KPIs (Clientes, Ítems, Próximas Reservas, Ingreso Agendado) y la tabla de citas enriquecidas carguen en una sola llamada a `GET /api/dashboard`.
2. **Clientes (CRM):** Crea un nuevo cliente con RUT y `metadata`, luego prueba el buscador en tiempo real (`Criteria` con operador `contains`).
3. **Catálogo:** Crea un nuevo `SERVICE` (con `duration_minutes`) y un `PRODUCT` (con `unit_of_measure` y `track_inventory`), y filtra con las pestañas.
4. **Agenda y Reservas (y prueba de `409 Conflict`):**
   - Selecciona el calendario estricto (`allow_overlaps: false`) y consulta bloques disponibles.
   - Haz clic en un bloque libre, selecciona un cliente + 1 o más servicios y crea la reserva (`POST /api/bookings` calcula precio total y `endsAt` automáticamente).
   - Intenta crear **otra reserva en el mismo horario** para verificar la alerta visual de **HTTP 409 Conflict**.
5. **Identidad y Acceso:** Lista usuarios y emite una nueva API Key (verifica el modal de visualización única con botón de copiar y revócala después).
