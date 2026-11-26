/**
 * Lightweight, SDK-free Google Cloud Run Identity Token fetcher with in-memory TTL cache.
 *
 * Used exclusively when `GCP_IAM_AUTH=true` to authenticate BFF -> Private Cloud Run calls
 * via the `X-Serverless-Authorization` header, leaving `Authorization` (Keycloak JWT) and
 * `X-Api-Key` completely untouched for `ikeda-core`.
 */

const METADATA_BASE =
  'http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity';
const REFRESH_BUFFER_MS = 60_000; // Refresh 60s before JWT expiration
const FALLBACK_TTL_MS = 50 * 60 * 1000; // 50 minutes fallback if exp claim is missing

interface CachedIdToken {
  token: string;
  expiresAtMs: number;
}

const tokenCache = new Map<string, CachedIdToken>();

function extractJwtExpMs(jwt: string): number | null {
  const parts = jwt.split('.');
  if (parts.length < 2) return null;
  try {
    const payload = JSON.parse(Buffer.from(parts[1], 'base64url').toString('utf8')) as {
      exp?: unknown;
    };
    return typeof payload.exp === 'number' && Number.isFinite(payload.exp)
      ? payload.exp * 1000
      : null;
  } catch {
    return null;
  }
}

export async function getGcpIdentityToken(serviceBaseUrl: string): Promise<string> {
  const audience = new URL(serviceBaseUrl).origin;
  const now = Date.now();
  const cached = tokenCache.get(audience);

  if (cached && now < cached.expiresAtMs - REFRESH_BUFFER_MS) {
    return cached.token;
  }

  const metadataUrl = `${METADATA_BASE}?audience=${encodeURIComponent(audience)}`;
  const res = await fetch(metadataUrl, {
    headers: { 'Metadata-Flavor': 'Google' },
  });

  if (!res.ok) {
    const text = await res.text().catch(() => '');
    throw new Error(
      `Failed to fetch GCP Identity Token for ${audience} (HTTP ${res.status}): ${text}`,
    );
  }

  const token = (await res.text()).trim();
  const expMs = extractJwtExpMs(token) ?? now + FALLBACK_TTL_MS;

  tokenCache.set(audience, { token, expiresAtMs: expMs });
  return token;
}

export function clearGcpIdentityTokenCache(): void {
  tokenCache.clear();
}
