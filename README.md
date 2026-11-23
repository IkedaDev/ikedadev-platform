# ikedadev-platform

Monorepo modular en Rust (Cargo Workspace) diseñado bajo Clean Architecture para alojar APIs multi-inquilino de alto rendimiento.

## 🏛️ Estructura del Workspace

```text
ikedadev-platform/
├── Cargo.toml                       # Workspace root con dependencias unificadas
├── crates/
│   └── ikeda-core/                  # Librería compartida por TODOS los microservicios
│       ├── response/                # ApiResponse<T>, PaginatedApiResponse<T>, PaginationMeta
│       ├── error/                   # AppError mapeado a códigos HTTP con enmascaramiento seguro
│       ├── tenant/                  # TenantContext + Extractor Axum (X-Tenant-Id, JWT/JWKS)
│       └── criteria/                # Criteria Pattern (árbol dinámico AND/OR, operadores y orden)
├── services/
│   └── events-api/                  # API de Calendarios y Eventos multi-tenant tipo Google Calendar
│       ├── migrations/              # Migraciones PostgreSQL con Row-Level Security (RLS)
│       ├── src/
│       │   ├── domain/              # Modelos puros, algoritmo RFC 5545 y puertos de repositorio
│       │   ├── application/         # Casos de uso (CreateCalendar, CreateEvent, CheckAvailability)
│       │   ├── infrastructure/      # Adaptadores SQLx (RLS transaccional) y Upstash QStash
│       │   └── presentation/        # Controladores Axum, Swagger UI (/swagger-ui) y ApiDoc
│       └── tests/                   # Tests de integración multi-tenant (aislamiento estricto)
├── deploy/
│   ├── k8s/                         # Manifiestos Kustomize genéricos (Deployment, Service, ConfigMap)
│   └── argo/                        # WorkflowTemplate declarativo para Argo Workflows
└── .github/workflows/
    └── ci-cd.yml                    # Pipeline universal (Quality Gate + OCI Buildx + Multi-deploy)
```

---

## 🚀 Pila Tecnológica

- **Lenguaje:** Rust 1.85+ (Edición 2021).
- **Web Framework:** `axum` + `tokio` + `tower` + `tower-http` (CORS, TraceLayer).
- **Documentación OpenAPI:** `utoipa` + `utoipa-swagger-ui` (contrato OpenAPI v3).
- **Base de Datos:** `sqlx` (PostgreSQL / Neon) con **Row-Level Security (RLS)** obligatorio.
- **Motor de Recurrencia:** `rrule` (RFC 5545) con soporte para excepciones y cancelaciones.
- **Mensajería Diferida:** Upstash QStash para webhooks y recordatorios programados.

---

## 🔒 Aislamiento Multi-Tenant (PostgreSQL RLS)

Todas las tablas (`calendars`, `events`, `event_exceptions`, `attendees`) tienen habilitado y forzado **Row-Level Security**:

```sql
CREATE POLICY tenant_isolation_events ON events
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);
```

Cada consulta se ejecuta dentro de una transacción gestionada por `begin_tenant_tx(pool, tenant_id)` que corre:
```sql
SELECT set_config('app.current_tenant', $1, true);
```
El parámetro `true` asegura que el contexto de tenant sea local a la transacción y se destruya automáticamente al hacer `COMMIT` o `ROLLBACK`, evitando fugas entre conexiones recicladas en pools (PgBouncer o Neon).

---

## 🛠️ Comandos de Desarrollo y Calidad

Ejecuta desde la raíz del workspace:

```bash
# 1. Verificación de formato estricto
cargo fmt --all -- --check

# 2. Análisis estático y linter sin advertencias
cargo clippy --workspace --all-targets --all-features -- -D warnings

# 3. Suite completa de tests unitarios y de integración
cargo test --workspace

# 4. Iniciar events-api localmente
cargo run -p events-api
```

Una vez levantada la API:
- **Swagger UI interactivo:** `http://localhost:8080/swagger-ui`
- **Contrato OpenAPI JSON:** `http://localhost:8080/openapi.json`
- **Health check:** `http://localhost:8080/health`

---

## 🔄 CI/CD Universal y Despliegue Portable

El pipeline en [`.github/workflows/ci-cd.yml`](file:///a:/IkedaDev/ikedadev-platform/.github/workflows/ci-cd.yml) está diseñado para ser **agnóstico al proveedor**:

1. **Quality Gate:** Ejecuta formateo, linter estricto y los 45 tests unitarios/de integración.
2. **Build de Contenedor OCI Multi-Arch:** Usa `cargo-chef` con Docker Buildx (AMD64 y ARM64) y sube la imagen inmutable a **GitHub Packages (`ghcr.io`)** sin costo alguno.
3. **Despliegue Flexible:**
   - **Hoy (Google Cloud Run / Capa Gratuita):** Consume la imagen directamente desde `ghcr.io`.
   - **Mañana (Kubernetes / K3s / ArgoCD / Argo Workflows):** Manifiestos nativos en [`deploy/k8s/base`](file:///a:/IkedaDev/ikedadev-platform/deploy/k8s/base) con Kustomize y plantilla para Argo Workflows en [`deploy/argo`](file:///a:/IkedaDev/ikedadev-platform/deploy/argo).

---

## 🌐 Sincronización con Frontends (TypeScript)

Para consumir la API con tipado 100% sincronizado desde React, Astro o Angular, corre en tu repositorio frontend:

```bash
npx openapi-typescript http://localhost:8080/openapi.json -o src/api/schema.d.ts
```

O apuntando a tu entorno en la nube:

```bash
npx openapi-typescript https://events.api.ikedadev.com/openapi.json -o src/api/schema.d.ts
```
