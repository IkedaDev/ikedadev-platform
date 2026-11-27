# Despliegue en GCP (Cloud Run + Artifact Registry + Neon + Cloudflare)

Runbook operativo paso a paso para desplegar **`ikedadev-platform`** (8 microservicios Rust privados —incluyendo `content-api` y `media-api`— + Portal SaaS BFF `portal/bff` desplegado como `tenant-portal-bff` público + Portal Web `portal/web` en Cloudflare Pages) en **Google Cloud Platform** con costo fijo **\$0**, región única Tier 1 (`us-central1`), autenticación sin llaves JSON (**Workload Identity Federation**) y protección automática de facturación (**Billing Kill Switch**).

---

## Arquitectura y Topes de Costo

- **Región única:** `us-central1` (capa gratuita Tier 1 de Cloud Run).
- **Cero servicios de costo fijo:** Sin Cloud SQL, sin VPC Connectors, sin Cloud NAT, sin Cloud Load Balancer, sin IPs estáticas. Todo el build Docker de Rust ocurre en GitHub Actions (`cargo-chef` + caché GHA).
- **Topes físicos en los 7 servicios Cloud Run:** `--min-instances=0`, `--max-instances=1`, `--cpu=1`, `--memory=256Mi`, `--cpu-throttling`, `--concurrency=80`, `--timeout=15s`, `RUST_LOG=info,sqlx=warn`, `LOG_FORMAT=json`.
- **Aislamiento de red/IAM:** Los 6 microservicios Rust son **privados** (`--no-allow-unauthenticated`, ejecutados con `platform-api-runtime` sin permisos admin). Solo `tenant-portal-bff` es público (`--allow-unauthenticated`) y usa `bff-invoker` + `GCP_IAM_AUTH=true` enviando `X-Serverless-Authorization: Bearer <ID_TOKEN>` sin tocar `Authorization` ni `X-Api-Key`.
- **Observabilidad nativa en Cloud Run:** Con `LOG_FORMAT=json`, los logs estructurados que los servicios emiten a `stdout` son capturados e indexados automáticamente por **Google Cloud Logging** sin agentes extra ni costo fijo. El stack opcional de Loki + Promtail + Grafana en `deploy/observability/` es únicamente para desarrollo local o clústeres K3s.
- **Portabilidad total a K3s:** Ningún servicio Rust ni `ikeda-core` incluye SDKs propietarios de GCP.

---

## Paso 1: Crear Proyecto en GCP y Vincular Facturación

```bash
export GCP_PROJECT_ID="ikedadev-platform-qa"   # Reemplaza por un ID único global
export GCP_BILLING_ACCOUNT_ID="01XXXX-XXXXXX-XXXXXX"

gcloud auth login
gcloud projects create "${GCP_PROJECT_ID}" --name="IkedaDev Platform"
gcloud billing projects link "${GCP_PROJECT_ID}" --billing-account="${GCP_BILLING_ACCOUNT_ID}"
```

---

## Paso 2: Ejecutar Bootstrap Idempotente (`bootstrap.sh`)

Desde la raíz del repositorio:

```bash
export GCP_PROJECT_ID="ikedadev-platform-qa"
export GCP_REGION="us-central1"
export GITHUB_REPO="IkedaDev/ikedadev-platform"
export AR_REPO="ikeda-platform"

bash deploy/gcp/bootstrap.sh
```

Guarda el bloque final que imprime el script (contiene `GCP_WORKLOAD_IDENTITY_PROVIDER`, `GCP_SERVICE_ACCOUNT`, etc.).

### Verificación inmediata del Bootstrap

```bash
# 1. Verificar política de limpieza de Artifact Registry (< 0.5 GB gratis)
gcloud artifacts repositories list-cleanup-policies "${AR_REPO}" \
  --location="${GCP_REGION}" \
  --project="${GCP_PROJECT_ID}"

# 2. Verificar Service Accounts creadas
gcloud iam service-accounts list --project="${GCP_PROJECT_ID}"

# 3. Verificar roles IAM (confirmar que github-deployer NO tiene Owner/Editor/run.admin)
gcloud projects get-iam-policy "${GCP_PROJECT_ID}" \
  --flatten="bindings[].members" \
  --filter="bindings.members:serviceAccount:" \
  --format="table(bindings.role,bindings.members)"

# 4. Verificar Workload Identity Pool & Provider
gcloud iam workload-identity-pools providers describe github-provider \
  --location="global" \
  --workload-identity-pool="github-pool" \
  --project="${GCP_PROJECT_ID}"
```

---

## Paso 3: Presupuesto de \$1 USD y Kill Switch Automático

### 3.1 Crear Billing Budget en la Consola de GCP
1. Ve a **Billing → Budgets & alerts → Create Budget**.
2. **Scope:** Selecciona únicamente el proyecto `${GCP_PROJECT_ID}`.
3. **Amount:** **\$1.00 USD** (Specified amount).
4. **Threshold rules:** Agrega alertas en **10%**, **50%**, **90%** y **100%** tanto en **Actual** como en **Forecasted**.
5. **Actions → Manage notifications:** Marca **Connect a Pub/Sub topic to this budget** y selecciona `projects/${GCP_PROJECT_ID}/topics/billing-alerts`.

### 3.2 Otorgar Permisos Mínimos a `billing-kill-switch`
`deploy/gcp/kill-switch/deploy.sh` asigna automáticamente los roles mínimos a nivel de proyecto y servicio (sin `Owner` ni `Editor`):
- `roles/billing.projectManager` (vincular/desvincular billing del proyecto).
- `roles/browser` (`resourcemanager.projects.get`, requerido por `GET /v1/projects/{projectId}/billingInfo`).
- `roles/eventarc.eventReceiver` y `roles/run.invoker` (sobre el servicio `billing-kill-switch` para recibir eventos Pub/Sub Gen 2).
- `roles/serviceusage.serviceUsageConsumer` (consumir `cloudbilling.googleapis.com`).

Además, otorga el permiso mínimo sobre la **Billing Account** para poder desvincular el proyecto:

```bash
# Primero despliega en modo DRY_RUN=true (esto crea la SA billing-kill-switch y sus roles de proyecto)
DRY_RUN=true MAX_COST_USD=2.00 GCP_PROJECT_ID="${GCP_PROJECT_ID}" \
  bash deploy/gcp/kill-switch/deploy.sh

# Otorga el permiso mínimo en la Billing Account (reemplaza GCP_BILLING_ACCOUNT_ID)
gcloud billing accounts add-iam-policy-binding "${GCP_BILLING_ACCOUNT_ID}" \
  --member="serviceAccount:billing-kill-switch@${GCP_PROJECT_ID}.iam.gserviceaccount.com" \
  --role="roles/billing.user"
```

> **Nota IAM:** En algunas organizaciones, desvincular un proyecto requiere `roles/billing.admin` sobre la Billing Account específica si `roles/billing.user` + `roles/billing.projectManager` devuelve `403`. Valida primero con `DRY_RUN=true` y, si necesitas desvincular en cuentas personales, asigna `roles/billing.admin` únicamente a `billing-kill-switch`.

### 3.3 Probar en Modo Seguro y Activar

```bash
# Prueba segura (costAmount 0.10 < MAX_COST_USD 2.00 -> NO desvincula billing)
gcloud pubsub topics publish billing-alerts \
  --project="${GCP_PROJECT_ID}" \
  --message='{"budgetDisplayName":"ikeda-budget","costAmount":0.10,"budgetAmount":1.00,"currencyCode":"USD"}'

# Revisar logs
gcloud functions logs read billing-kill-switch --gen2 \
  --region=us-central1 --project="${GCP_PROJECT_ID}" --limit=20

# Verificar cleanup policy aplicada también a gcf-artifacts
gcloud artifacts repositories list-cleanup-policies gcf-artifacts \
  --location=us-central1 --project="${GCP_PROJECT_ID}"

# Una vez validado, activar Kill Switch real:
DRY_RUN=false MAX_COST_USD=2.00 GCP_PROJECT_ID="${GCP_PROJECT_ID}" \
  bash deploy/gcp/kill-switch/deploy.sh
```

---

## Paso 4: Provisionar las 6 Bases en Neon (Roles `NOBYPASSRLS` + `api_key_reader`)

Crea un proyecto en **Neon** (región **AWS us-east-1** o **us-east-2**, cercana a `us-central1`) y crea las 6 bases de datos:
`identity_db`, `crm_db`, `events_db`, `catalog_db`, `bookings_db` y `telemetry_db`.

Aplica las migraciones SQL de `services/<servicio>/migrations/*.sql` en cada base correspondiente usando el rol dueño (`neondb_owner`).

### 4.1 Crear rol de aplicación con `NOBYPASSRLS` (obligatorio en las 6 bases)
El rol dueño de Neon tiene privilegios elevados; las APIs en Cloud Run **deben** conectarse con un rol `NOBYPASSRLS` para que PostgreSQL RLS sea inquebrantable:

```sql
-- Ejecutar como neondb_owner (una vez por proyecto Neon para crear el rol)
CREATE ROLE ikeda_app LOGIN PASSWORD 'CAMBIAR_PASSWORD_FUERTE_APP' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS;

-- Ejecutar DENTRO DE CADA UNA de las 6 bases (identity_db, crm_db, events_db, catalog_db, bookings_db, telemetry_db):
GRANT CONNECT ON DATABASE current_database() TO ikeda_app; -- o reemplaza por el nombre de la DB
GRANT USAGE ON SCHEMA public TO ikeda_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO ikeda_app;
GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO ikeda_app;
ALTER DEFAULT PRIVILEGES IN SCHEMA public
  GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO ikeda_app;
```

### 4.2 Crear rol `api_key_reader` de privilegio mínimo en `identity_db`
Las 5 APIs de dominio (`crm-api`, `events-api`, `catalog-api`, `bookings-api`, `telemetry-api`) usan `IDENTITY_DATABASE_URL` exclusivamente para validar API keys vía `authenticate_api_key($1)` (`SECURITY DEFINER`):

```sql
-- Ejecutar en identity_db como neondb_owner:
CREATE ROLE api_key_reader LOGIN PASSWORD 'CAMBIAR_PASSWORD_FUERTE_READER' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS;

REVOKE ALL ON DATABASE identity_db FROM PUBLIC;
REVOKE ALL ON SCHEMA public FROM api_key_reader;
REVOKE ALL ON ALL TABLES IN SCHEMA public FROM api_key_reader;

GRANT CONNECT ON DATABASE identity_db TO api_key_reader;
GRANT USAGE ON SCHEMA public TO api_key_reader;
GRANT EXECUTE ON FUNCTION public.authenticate_api_key(VARCHAR) TO api_key_reader;
```

> Usa el endpoint **pooler** de Neon (`-pooler.us-east-2.aws.neon.tech`) con `?sslmode=require` en todas las URLs.

---

## Paso 5: Configurar Variables y Secretos en GitHub Actions

En **GitHub → Settings → Environments**, crea dos entornos: **`qa`** (rama `develop`) y **`prod`** (rama `main`).

### Repository Variables (compartidas por `qa` y `prod`)

| Variable (`vars.*`) | Ejemplo / Origen |
| :--- | :--- |
| `GCP_DEPLOY_ENABLED` | `true` (déjalo en `false` u omítelo mientras configuras) |
| `GCP_PROJECT_ID` | Salida de `bootstrap.sh` |
| `GCP_REGION` | `us-central1` |
| `GCP_AR_REPO` | `ikeda-platform` |
| `GCP_WORKLOAD_IDENTITY_PROVIDER` | Salida de `bootstrap.sh` (`projects/.../providers/github-provider`) |
| `GCP_SERVICE_ACCOUNT` | `github-deployer@<GCP_PROJECT_ID>.iam.gserviceaccount.com` |

### Environment Variables (`vars.*` dentro de `qa` y `prod`)

| Variable (`vars.*`) | Descripción |
| :--- | :--- |
| `JWKS_URI` | `https://auth-qa.ikedadev.com/realms/ikeda/protocol/openid-connect/certs` |
| `JWT_ISSUER` | `https://auth-qa.ikedadev.com/realms/ikeda` |
| `KEYCLOAK_BASE_URL` | `https://auth-qa.ikedadev.com` |
| `KEYCLOAK_REALM` | `ikeda` |
| `KEYCLOAK_CLIENT_ID` | `identity-api-admin` |
| `KEYCLOAK_PUBLIC_URL` | `https://auth-qa.ikedadev.com` |
| `KEYCLOAK_INTERNAL_URL` | `https://auth-qa.ikedadev.com` |
| `KEYCLOAK_BFF_CLIENT_ID` | `tenant-portal-bff` |
| `BFF_PUBLIC_URL` | `https://bff-qa.ikedadev.com` |
| `WEB_APP_URL` | `https://portal-qa.ikedadev.com` |
| `CORS_ORIGIN` | `https://portal-qa.ikedadev.com` |
| `IDENTITY_API_URL` | URL privada `https://identity-api-qa-xxxx-uc.a.run.app` |
| `CRM_API_URL` | URL privada `https://crm-api-qa-xxxx-uc.a.run.app` |
| `EVENTS_API_URL` | URL privada `https://events-api-qa-xxxx-uc.a.run.app` |
| `CATALOG_API_URL` | URL privada `https://catalog-api-qa-xxxx-uc.a.run.app` |
| `BOOKINGS_API_URL` | URL privada `https://bookings-api-qa-xxxx-uc.a.run.app` |
| `TELEMETRY_API_URL` | URL privada `https://telemetry-api-qa-xxxx-uc.a.run.app` |

### Environment Secrets (`secrets.*` dentro de `qa` y `prod`)

| Secreto (`secrets.*`) | Descripción |
| :--- | :--- |
| `IDENTITY_DATABASE_URL` | `postgres://ikeda_app:<pass>@<neon-pooler>/identity_db?sslmode=require` |
| `CRM_DATABASE_URL` | `postgres://ikeda_app:<pass>@<neon-pooler>/crm_db?sslmode=require` |
| `EVENTS_DATABASE_URL` | `postgres://ikeda_app:<pass>@<neon-pooler>/events_db?sslmode=require` |
| `CATALOG_DATABASE_URL` | `postgres://ikeda_app:<pass>@<neon-pooler>/catalog_db?sslmode=require` |
| `BOOKINGS_DATABASE_URL` | `postgres://ikeda_app:<pass>@<neon-pooler>/bookings_db?sslmode=require` |
| `TELEMETRY_DATABASE_URL` | `postgres://ikeda_app:<pass>@<neon-pooler>/telemetry_db?sslmode=require` |
| `IDENTITY_READER_DATABASE_URL` | `postgres://api_key_reader:<pass>@<neon-pooler>/identity_db?sslmode=require` |
| `KEYCLOAK_ADMIN_CLIENT_SECRET` | Secreto del cliente `identity-api-admin` |
| `KEYCLOAK_BFF_CLIENT_SECRET` | Secreto del cliente `tenant-portal-bff` |
| `PLATFORM_ADMIN_API_KEY` | *(Opcional)* API key de plataforma usada exclusivamente por `/api/settings` y `/api/branding/:slug` en `portal/bff` para leer/actualizar `tenants.config` del tenant autenticado en `identity-api` (sin exponer `TENANT_API_KEY` estática) |
| `QSTASH_TOKEN` | Token de Upstash QStash (opcional) |

> **Recomendación de hardening para Producción (GCP Secret Manager):** Los workflows actuales inyectan los secretos vía `--set-env-vars` para mantener costo fijo **$0** en entornos `qa`/free-tier. En `prod`, se recomienda migrar las credenciales sensibles (`*_DATABASE_URL`, `KEYCLOAK_*_SECRET`, `PLATFORM_ADMIN_API_KEY`) a **Google Cloud Secret Manager**, otorgar `roles/secretmanager.secretAccessor` por secreto a `platform-api-runtime` / `bff-invoker`, y montar los valores con `--set-secrets` en lugar de variables de entorno en texto plano (evitando exposición a principals con `roles/viewer` o `run.services.get`).

---

## Paso 6: Configuración de Cloudflare (Pages + Subdominios + WAF)

1. **Cloudflare Pages (`portal/web`):**
   - Conecta el repositorio `IkedaDev/ikedadev-platform`.
   - **Root directory:** `portal/web`
   - **Build command:** `npm run build`
   - **Build output directory:** `dist`
   - **Environment variable:** `VITE_BFF_URL=https://platform-bff-qa.ikedadev.com` (o tu dominio público de BFF en prod).
   - Incluye el **Design System Kropy**, branding dinámico por tenant (`tenants.config`) y feature toggles desde `SettingsView`.
2. **Subdominios con guiones (SSL Universal gratuito con Proxy Naranja):**
   - Usa subdominios de un solo nivel (ej. `portal-qa.ikedadev.com`, `platform-bff-qa.ikedadev.com`, `auth-qa.ikedadev.com`) en lugar de sub-subdominios (`qa.bff.ikedadev.com`), para cubrirlos con el certificado Universal SSL gratuito de Cloudflare sin pagar Advanced Certificate Manager.
   - El workflow [`.github/workflows/portal-bff.yml`](../../.github/workflows/portal-bff.yml) construye desde `portal/bff` pero mantiene el nombre del servicio Cloud Run **`tenant-portal-bff-qa`** / **`tenant-portal-bff-prod`** para preservar los mapeos DNS existentes (ej. `platform-bff-qa.ikedadev.com`) y los bindings IAM (`bff-invoker`).
3. **Regla WAF Rate Limiting (Capa Gratuita):**
   - Ve a **Security → WAF → Rate limiting rules → Create rule**.
   - **Expression:** `(http.host eq "bff-qa.ikedadev.com")`
   - **Requests:** `60` requests per `10 seconds` por IP.
   - **Action:** `Block` durante `10 seconds`.

---

## Paso 7: Verificación Post-Despliegue

```bash
# 1. Obtener URL de un servicio privado (ej. bookings-api-qa)
BOOKINGS_URL="$(gcloud run services describe bookings-api-qa \
  --region=us-central1 --project="${GCP_PROJECT_ID}" --format='value(status.url)')"

# 2. Confirmar HTTP 403 Forbidden SIN Identity Token de GCP (servicio privado protegido por IAM)
curl -i "${BOOKINGS_URL}/health"

# 3. Confirmar HTTP 200 en /health y HTTP 401 Unauthorized en /v1/bookings CON Identity Token pero SIN API Key/JWT
ID_TOKEN="$(gcloud auth print-identity-token --audiences="${BOOKINGS_URL}")"
curl -i -H "X-Serverless-Authorization: Bearer ${ID_TOKEN}" "${BOOKINGS_URL}/health"
curl -i -H "X-Serverless-Authorization: Bearer ${ID_TOKEN}" "${BOOKINGS_URL}/v1/bookings/00000000-0000-0000-0000-000000000000"

# 4. Verificar health agregado desde el BFF público (debe mostrar ok: true en los 6 servicios Rust)
curl -s https://bff-qa.ikedadev.com/health | jq .
```

Finalmente, abre `https://portal-qa.ikedadev.com`, inicia sesión con Keycloak SSO y valida el ciclo completo: crear cliente → crear servicio → consultar disponibilidad → agendar reserva → cambiar estado (Completar / Cancelar).
