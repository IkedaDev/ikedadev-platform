#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# IkedaDev Platform — Idempotent GCP Bootstrap (Tier 1 Free-Tier Optimized)
# ==============================================================================
# Provisions ONLY zero-fixed-cost GCP primitives in us-central1:
#   - Required Google Cloud APIs (Cloud Run, Artifact Registry, WIF, Billing, Pub/Sub, Functions)
#   - Artifact Registry Docker repository + cleanup policy (< 0.5 GB free tier)
#   - Workload Identity Federation Pool & OIDC Provider scoped strictly to GitHub repo
#   - Least-privilege Service Accounts:
#       * github-deployer      (artifactregistry.writer, run.developer, iam.serviceAccountUser)
#       * bff-invoker          (project-level roles/run.invoker for private Rust APIs)
#       * platform-api-runtime (zero admin roles; runtime SA for the 5 Rust APIs)
#   - Pub/Sub topic `billing-alerts` for the automated Billing Kill Switch
#
# Usage:
#   export GCP_PROJECT_ID="your-gcp-project-id"
#   bash deploy/gcp/bootstrap.sh
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

GCP_PROJECT_ID="${GCP_PROJECT_ID:?ERROR: Set GCP_PROJECT_ID before running bootstrap.sh}"
GCP_REGION="${GCP_REGION:-us-central1}"
GITHUB_REPO="${GITHUB_REPO:-IkedaDev/ikedadev-platform}"
AR_REPO="${AR_REPO:-ikeda-platform}"

WIF_POOL_ID="${WIF_POOL_ID:-github-pool}"
WIF_PROVIDER_ID="${WIF_PROVIDER_ID:-github-provider}"
DEPLOYER_SA_NAME="github-deployer"
BFF_INVOKER_SA_NAME="bff-invoker"
RUNTIME_SA_NAME="platform-api-runtime"
BILLING_TOPIC="billing-alerts"

if [[ "${GCP_REGION}" != "us-central1" ]]; then
  echo "ERROR: GCP_REGION must be 'us-central1' (Tier 1 free-tier policy). Got '${GCP_REGION}'." >&2
  exit 1
fi

echo "==> [1/7] Configuring active GCP project: ${GCP_PROJECT_ID} (region: ${GCP_REGION})"
gcloud config set project "${GCP_PROJECT_ID}" >/dev/null
GCP_PROJECT_NUMBER="$(gcloud projects describe "${GCP_PROJECT_ID}" --format='value(projectNumber)')"

echo "==> [2/7] Enabling required GCP APIs..."
gcloud services enable \
  run.googleapis.com \
  artifactregistry.googleapis.com \
  iamcredentials.googleapis.com \
  cloudbilling.googleapis.com \
  pubsub.googleapis.com \
  cloudfunctions.googleapis.com \
  cloudbuild.googleapis.com \
  eventarc.googleapis.com \
  --project="${GCP_PROJECT_ID}"

echo "==> [3/7] Creating Artifact Registry Docker repository '${AR_REPO}' in ${GCP_REGION}..."
if ! gcloud artifacts repositories describe "${AR_REPO}" \
  --location="${GCP_REGION}" \
  --project="${GCP_PROJECT_ID}" >/dev/null 2>&1; then
  gcloud artifacts repositories create "${AR_REPO}" \
    --repository-format=docker \
    --location="${GCP_REGION}" \
    --description="IkedaDev Platform container images (Rust APIs + BFF)" \
    --project="${GCP_PROJECT_ID}"
else
  echo "    Repository '${AR_REPO}' already exists."
fi

echo "==> Applying Artifact Registry cleanup policy (${SCRIPT_DIR}/cleanup-policy.json)..."
gcloud artifacts repositories set-cleanup-policies "${AR_REPO}" \
  --location="${GCP_REGION}" \
  --policy="${SCRIPT_DIR}/cleanup-policy.json" \
  --project="${GCP_PROJECT_ID}"

echo "==> [4/7] Creating least-privilege Service Accounts..."
ensure_sa() {
  local sa_name="$1"
  local display_name="$2"
  local sa_email="${sa_name}@${GCP_PROJECT_ID}.iam.gserviceaccount.com"
  if ! gcloud iam service-accounts describe "${sa_email}" --project="${GCP_PROJECT_ID}" >/dev/null 2>&1; then
    gcloud iam service-accounts create "${sa_name}" \
      --display-name="${display_name}" \
      --project="${GCP_PROJECT_ID}"
  else
    echo "    Service Account '${sa_email}' already exists."
  fi
}

ensure_sa "${DEPLOYER_SA_NAME}" "GitHub Actions Deployer (IkedaDev)"
ensure_sa "${BFF_INVOKER_SA_NAME}" "Tenant Portal BFF Cloud Run Invoker"
ensure_sa "${RUNTIME_SA_NAME}" "IkedaDev Rust Microservices Runtime (Zero Admin)"

DEPLOYER_SA_EMAIL="${DEPLOYER_SA_NAME}@${GCP_PROJECT_ID}.iam.gserviceaccount.com"
BFF_INVOKER_SA_EMAIL="${BFF_INVOKER_SA_NAME}@${GCP_PROJECT_ID}.iam.gserviceaccount.com"
RUNTIME_SA_EMAIL="${RUNTIME_SA_NAME}@${GCP_PROJECT_ID}.iam.gserviceaccount.com"

echo "==> [5/7] Binding least-privilege IAM roles..."
for role in \
  roles/artifactregistry.writer \
  roles/run.developer \
  roles/iam.serviceAccountUser; do
  gcloud projects add-iam-policy-binding "${GCP_PROJECT_ID}" \
    --member="serviceAccount:${DEPLOYER_SA_EMAIL}" \
    --role="${role}" \
    --condition=None >/dev/null
done

# Grant bff-invoker permission once at project level so GitHub Actions (run.developer)
# never needs elevated roles/run.admin to run setIamPolicy on each service.
gcloud projects add-iam-policy-binding "${GCP_PROJECT_ID}" \
  --member="serviceAccount:${BFF_INVOKER_SA_EMAIL}" \
  --role="roles/run.invoker" \
  --condition=None >/dev/null

echo "==> [6/7] Configuring Workload Identity Federation for '${GITHUB_REPO}'..."
if ! gcloud iam workload-identity-pools describe "${WIF_POOL_ID}" \
  --location="global" \
  --project="${GCP_PROJECT_ID}" >/dev/null 2>&1; then
  gcloud iam workload-identity-pools create "${WIF_POOL_ID}" \
    --location="global" \
    --display-name="GitHub Actions Pool" \
    --project="${GCP_PROJECT_ID}"
else
  echo "    Workload Identity Pool '${WIF_POOL_ID}' already exists."
fi

if ! gcloud iam workload-identity-pools providers describe "${WIF_PROVIDER_ID}" \
  --location="global" \
  --workload-identity-pool="${WIF_POOL_ID}" \
  --project="${GCP_PROJECT_ID}" >/dev/null 2>&1; then
  gcloud iam workload-identity-pools providers create-oidc "${WIF_PROVIDER_ID}" \
    --location="global" \
    --workload-identity-pool="${WIF_POOL_ID}" \
    --display-name="GitHub OIDC Provider" \
    --issuer-uri="https://token.actions.githubusercontent.com" \
    --attribute-mapping="google.subject=assertion.sub,attribute.actor=assertion.actor,attribute.repository=assertion.repository,attribute.ref=assertion.ref" \
    --attribute-condition="assertion.repository == '${GITHUB_REPO}'" \
    --project="${GCP_PROJECT_ID}"
else
  echo "    OIDC Provider '${WIF_PROVIDER_ID}' already exists."
fi

WIF_PROVIDER_FULL="projects/${GCP_PROJECT_NUMBER}/locations/global/workloadIdentityPools/${WIF_POOL_ID}/providers/${WIF_PROVIDER_ID}"

gcloud iam service-accounts add-iam-policy-binding "${DEPLOYER_SA_EMAIL}" \
  --project="${GCP_PROJECT_ID}" \
  --role="roles/iam.workloadIdentityUser" \
  --member="principalSet://iam.googleapis.com/projects/${GCP_PROJECT_NUMBER}/locations/global/workloadIdentityPools/${WIF_POOL_ID}/attribute.repository/${GITHUB_REPO}" >/dev/null

echo "==> [7/7] Creating Pub/Sub topic '${BILLING_TOPIC}' for Billing Kill Switch..."
if ! gcloud pubsub topics describe "${BILLING_TOPIC}" --project="${GCP_PROJECT_ID}" >/dev/null 2>&1; then
  gcloud pubsub topics create "${BILLING_TOPIC}" --project="${GCP_PROJECT_ID}"
else
  echo "    Pub/Sub topic '${BILLING_TOPIC}' already exists."
fi

cat <<EOF

================================================================================
✅ GCP Bootstrap completed successfully!
================================================================================
Add these values to GitHub Repository -> Settings -> Secrets and variables -> Actions:

  Variables (vars.*):
    GCP_PROJECT_ID                 = ${GCP_PROJECT_ID}
    GCP_REGION                     = ${GCP_REGION}
    GCP_AR_REPO                    = ${AR_REPO}
    GCP_WORKLOAD_IDENTITY_PROVIDER = ${WIF_PROVIDER_FULL}
    GCP_SERVICE_ACCOUNT            = ${DEPLOYER_SA_EMAIL}
    GCP_RUNTIME_SERVICE_ACCOUNT    = ${RUNTIME_SA_EMAIL}
    GCP_BFF_SERVICE_ACCOUNT        = ${BFF_INVOKER_SA_EMAIL}
    GCP_DEPLOY_ENABLED             = true   (set only when ready to deploy)

  Created Service Accounts:
    Deployer: ${DEPLOYER_SA_EMAIL}
    Runtime:  ${RUNTIME_SA_EMAIL}
    BFF:      ${BFF_INVOKER_SA_EMAIL}
================================================================================
EOF
