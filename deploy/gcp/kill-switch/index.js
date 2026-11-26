'use strict';

const { GoogleAuth } = require('google-auth-library');

const BILLING_API_BASE = 'https://cloudbilling.googleapis.com/v1';
const auth = new GoogleAuth({
  scopes: ['https://www.googleapis.com/auth/cloud-billing'],
});

/**
 * Decodes a Pub/Sub CloudEvent (Gen 2) or legacy Pub/Sub message payload.
 */
function parseBudgetPayload(cloudEvent) {
  const encoded =
    cloudEvent?.data?.message?.data ||
    cloudEvent?.message?.data ||
    cloudEvent?.data ||
    null;

  if (!encoded || typeof encoded !== 'string') {
    throw new Error('Missing base64 Pub/Sub message data in event payload');
  }

  const decoded = Buffer.from(encoded, 'base64').toString('utf8');
  return JSON.parse(decoded);
}

/**
 * Formats HTTP errors from google-auth-library without leaking credentials or headers.
 */
function formatBillingApiError(err) {
  const status = err?.response?.status ?? err?.code ?? 'UNKNOWN';
  const data = err?.response?.data;
  const apiMessage =
    (typeof data === 'object' && data !== null && (data.error?.message || JSON.stringify(data))) ||
    (typeof data === 'string' && data) ||
    err?.message ||
    String(err);
  return { status, apiMessage };
}

/**
 * Fetches current billing info for the configured GCP project.
 */
async function getProjectBillingInfo(client, projectId) {
  const url = `${BILLING_API_BASE}/projects/${encodeURIComponent(projectId)}/billingInfo`;
  const res = await client.request({ url, method: 'GET' });
  return res.data;
}

/**
 * Detaches the billing account from the project, immediately stopping future billable usage.
 */
async function disableProjectBilling(client, projectId) {
  const url = `${BILLING_API_BASE}/projects/${encodeURIComponent(projectId)}/billingInfo`;
  const res = await client.request({
    url,
    method: 'PUT',
    data: { billingAccountName: '' },
  });
  return res.data;
}

/**
 * Cloud Function entry point (Gen 2 CloudEvent / Pub/Sub trigger).
 */
exports.stopBilling = async (cloudEvent) => {
  const projectId = process.env.GCP_PROJECT_ID || process.env.GOOGLE_CLOUD_PROJECT;
  if (!projectId) {
    console.error('[KillSwitch] Missing GCP_PROJECT_ID environment variable.');
    return;
  }

  const maxCost = Number.parseFloat(process.env.MAX_COST_USD || '2.00');
  const dryRun = (process.env.DRY_RUN || 'true').toLowerCase() === 'true';

  if (!Number.isFinite(maxCost) || maxCost <= 0) {
    console.error(`[KillSwitch] Invalid MAX_COST_USD value: '${process.env.MAX_COST_USD}'`);
    return;
  }

  let budgetData;
  try {
    budgetData = parseBudgetPayload(cloudEvent);
  } catch (err) {
    console.error('[KillSwitch] Failed to parse Pub/Sub budget event:', err.message);
    return;
  }

  const costAmount = Number(budgetData.costAmount ?? 0);
  const budgetAmount = Number(budgetData.budgetAmount ?? 0);
  const currencyCode = String(budgetData.currencyCode || 'USD');

  console.log(
    JSON.stringify({
      component: 'billing-kill-switch',
      projectId,
      costAmount,
      budgetAmount,
      currencyCode,
      maxCostThreshold: maxCost,
      dryRun,
    }),
  );

  if (!Number.isFinite(costAmount) || costAmount < maxCost) {
    console.log(
      `[KillSwitch] Cost (${costAmount} ${currencyCode}) is below threshold (${maxCost} ${currencyCode}). No action taken.`,
    );
    return;
  }

  const client = await auth.getClient();

  let billingInfo;
  try {
    billingInfo = await getProjectBillingInfo(client, projectId);
  } catch (err) {
    const { status, apiMessage } = formatBillingApiError(err);
    console.error(
      `[KillSwitch] getProjectBillingInfo failed (HTTP ${status}) for project '${projectId}': ${apiMessage}`,
    );
    throw err;
  }

  if (!billingInfo || billingInfo.billingEnabled !== true) {
    console.log(`[KillSwitch] Billing is already disabled for project '${projectId}'.`);
    return;
  }

  if (dryRun) {
    console.warn(
      `[KillSwitch][DRY_RUN=true] Threshold exceeded (${costAmount} >= ${maxCost} ${currencyCode}). Would detach billing from '${projectId}'.`,
    );
    return;
  }

  console.warn(
    `[KillSwitch] Threshold exceeded (${costAmount} >= ${maxCost} ${currencyCode}). Detaching billing account from '${projectId}' NOW.`,
  );
  try {
    await disableProjectBilling(client, projectId);
  } catch (err) {
    const { status, apiMessage } = formatBillingApiError(err);
    console.error(
      `[KillSwitch] disableProjectBilling failed (HTTP ${status}) for project '${projectId}': ${apiMessage}`,
    );
    throw err;
  }
  console.warn(`[KillSwitch] Billing successfully disabled for project '${projectId}'.`);
};
