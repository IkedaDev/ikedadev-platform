#!/usr/bin/env bash
set -euo pipefail

# ==============================================================================
# IkedaDev Platform — Deploy Automated Billing Kill Switch (Cloud Functions Gen 2)
# ==============================================================================
# Usage:
#   export GCP_PROJECT_ID="your-gcp-project-id"
#   export MAX_COST_USD="2.00"
#   export DRY_RUN="true"   # Start in DRY_RUN=true, test safely, then redeploy with DRY_RUN=false
#   bash deploy/gcp/kill-switch/deploy.sh
# ==============================================================================

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GCP_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

GCP_PROJECT_ID="${GCP_PROJECT_ID:?ERROR: Set GCP_PROJECT_ID before running deploy.sh}"
GCP_REGION="${GCP_REGION:-us-central1}"
MAX_COST_USD="${MAX_COST_USD:-1000}"
DRY_RUN="${DRY_RUN:-true}"
BILLING_TOPIC="${BILLING_TOPIC:-billing-alerts}"
FUNCTION_NAME="billing-kill-switch"
SA_NAME="billing-kill-switch"
SA_EMAIL="${SA_NAME}@${GCP_PROJECT_ID}.iam.gserviceaccount.com"

if [[ "${GCP_REGION}" != "us-central1" ]]; then
  echo "ERROR: GCP_REGION must be 'us-central1'. Got '${GCP_REGION}'." >&2
  exit 1
fi

echo "==> [1/3] Ensuring dedicated Service Account '${SA_EMAIL}'..."
if ! gcloud iam service-accounts describe "${SA_EMAIL}" --project="${GCP_PROJECT_ID}" >/dev/null 2>&1; then
  gcloud iam service-accounts create "${SA_NAME}" \
    --display-name="Automated Billing Kill Switch" \
    --project="${GCP_PROJECT_ID}"
else
  echo "    Service Account '${SA_EMAIL}' already exists."
fi

# Minimal project-level billing manager role (also grant roles/billing.user on the Billing Account
# as documented in deploy/gcp/README.md so it can detach the project cleanly).
gcloud projects add-iam-policy-binding "${GCP_PROJECT_ID}" \
  --member="serviceAccount:${SA_EMAIL}" \
  --role="roles/billing.projectManager" \
  --condition=None >/dev/null

echo "==> [2/3] Deploying Cloud Function Gen 2 '${FUNCTION_NAME}' (DRY_RUN=${DRY_RUN}, MAX_COST_USD=${MAX_COST_USD})..."
gcloud functions deploy "${FUNCTION_NAME}" \
  --gen2 \
  --runtime=nodejs22 \
  --region="${GCP_REGION}" \
  --project="${GCP_PROJECT_ID}" \
  --source="${SCRIPT_DIR}" \
  --entry-point=stopBilling \
  --trigger-topic="${BILLING_TOPIC}" \
  --service-account="${SA_EMAIL}" \
  --memory=256Mi \
  --max-instances=1 \
  --timeout=30s \
  --set-env-vars="GCP_PROJECT_ID=${GCP_PROJECT_ID},MAX_COST_USD=${MAX_COST_USD},DRY_RUN=${DRY_RUN}"

echo "==> [3/3] Applying Artifact Registry cleanup policy to 'gcf-artifacts' (free-tier protection)..."
if gcloud artifacts repositories describe "gcf-artifacts" \
  --location="${GCP_REGION}" \
  --project="${GCP_PROJECT_ID}" >/dev/null 2>&1; then
  gcloud artifacts repositories set-cleanup-policies "gcf-artifacts" \
    --location="${GCP_REGION}" \
    --policy="${GCP_DIR}/cleanup-policy.json" \
    --project="${GCP_PROJECT_ID}"
else
  echo "    Note: 'gcf-artifacts' repo not yet present in ${GCP_REGION}; re-run after first deploy if created."
fi

cat <<EOF

================================================================================
✅ Kill Switch deployed (DRY_RUN=${DRY_RUN}, MAX_COST_USD=${MAX_COST_USD})
================================================================================
Safe test command (will NOT disable billing because costAmount < MAX_COST_USD):

  gcloud pubsub topics publish ${BILLING_TOPIC} \\
    --project=${GCP_PROJECT_ID} \\
    --message='{"budgetDisplayName":"ikeda-budget","costAmount":0.10,"budgetAmount":1.00,"currencyCode":"USD"}'

Inspect logs:
  gcloud functions logs read ${FUNCTION_NAME} --gen2 --region=${GCP_REGION} --project=${GCP_PROJECT_ID} --limit=20

Once verified in DRY_RUN=true, activate real protection with:
  DRY_RUN=false MAX_COST_USD=${MAX_COST_USD} GCP_PROJECT_ID=${GCP_PROJECT_ID} bash deploy/gcp/kill-switch/deploy.sh
================================================================================
EOF
