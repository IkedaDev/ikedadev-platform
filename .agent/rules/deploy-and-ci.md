# Reglas de Despliegue, CI/CD y Portabilidad (`deploy/` y `.github/workflows/`)

## 1. Convenciones de Cloud Run y GitHub Actions
- Workflows por servicio en `.github/workflows/`: los 8 microservicios Rust (`<servicio>-api.yml`) + `portal-bff.yml`, además de `ci.yml` como quality gate en PR/push a `main` y `develop`.
- **Gate obligatorio:** Todo job de deploy requiere `vars.GCP_DEPLOY_ENABLED == 'true'`.
- **Naming de servicios Cloud Run:**
  - Rama `main` → entorno `prod` (`<servicio>-prod`).
  - Otras ramas / QA → entorno `qa` (`<servicio>-qa`).
  - **Excepción preservada:** `portal-bff.yml` usa `SERVICE_NAME: tenant-portal-bff` → `tenant-portal-bff-qa` / `tenant-portal-bff-prod` para no romper mapeos DNS ni bindings IAM existentes.
- Autenticación keyless vía **Workload Identity Federation** (`google-github-actions/auth@v2`), imágenes `linux/amd64` con `cargo-chef` sobre `debian:bookworm-slim` ejecutando como usuario no-root (`appuser`, UID `10001`).

## 2. IAM Privado y Topes de Costo (GCP + Neon)
- **Red y Auth entre servicios:**
  - Los 8 servicios Rust despliegan con `--no-allow-unauthenticated` y SA `platform-api-runtime`.
  - Solo `tenant-portal-bff` despliega con `--allow-unauthenticated`, SA `bff-invoker` (`roles/run.invoker`) y `GCP_IAM_AUTH=true` (enviando el ID token en `X-Serverless-Authorization`).
- **Topes de costo estrictos (capa gratuita Tier 1):**
  - Región bloqueada a `us-central1`.
  - `--min-instances=0`, `--max-instances=1`, `--cpu=1`, `--memory=256Mi`, `--cpu-throttling`, `--concurrency=80`, `--timeout=15s`.
  - Política de limpieza en Artifact Registry (`deploy/gcp/cleanup-policy.json`): purga imágenes sin tag > 24h y conserva máximo 2 versiones.
  - Billing Kill Switch en `deploy/gcp/kill-switch/index.js` (`MAX_COST_USD` por defecto `2.00`).
- **Neon PostgreSQL:** Usar pooler endpoints con `?sslmode=require`, `ikeda_app` (`NOBYPASSRLS`) por DB y `api_key_reader` para `IDENTITY_READER_DATABASE_URL`.

## 3. Portabilidad a K3s
- Prohibido acoplar los microservicios Rust o `ikeda-core` a SDKs propietarios de GCP.
- Manifiestos base listos en `deploy/k8s/base/` (UID `10001`, `readOnlyRootFilesystem: true`, probes `/health`, límite `256Mi`) y stack opcional de observabilidad Loki/Promtail/Grafana en `deploy/observability/` (`docker compose --profile observability up -d`).
