import crypto from 'node:crypto';
import { config } from '../config.js';

export interface SessionUser {
  id: string;
  email: string;
  name: string;
  tenant_id: string | null;
  roles: string[];
}

export interface BffSession {
  id: string;
  accessToken: string;
  refreshToken?: string;
  idToken?: string;
  expiresAt: number; // Access token expiration (Unix timestamp in ms)
  sessionExpiresAt: number; // Absolute BFF session expiration (Unix timestamp in ms)
  user: SessionUser;
}

interface KeycloakTokenResponse {
  access_token: string;
  refresh_token?: string;
  id_token?: string;
  expires_in: number;
  refresh_expires_in?: number;
  token_type: string;
}

const SESSION_MAX_TTL_MS = 8 * 60 * 60 * 1000; // 8 hours (aligned with cookie maxAge)
const SESSION_CLEANUP_INTERVAL_MS = 5 * 60 * 1000; // Sweep every 5 minutes

// Note: In a multi-replica production deployment, back this store with Redis.
const sessions = new Map<string, BffSession>();

export function pruneExpiredSessions(now = Date.now()): number {
  let removed = 0;
  for (const [id, session] of sessions.entries()) {
    const expiredWithoutRefresh = !session.refreshToken && now >= session.expiresAt;
    if (now >= session.sessionExpiresAt || expiredWithoutRefresh) {
      sessions.delete(id);
      removed += 1;
    }
  }
  return removed;
}

const cleanupTimer = setInterval(() => {
  pruneExpiredSessions();
}, SESSION_CLEANUP_INTERVAL_MS);
cleanupTimer.unref();

export function generateRandomUrlSafe(bytes = 32): string {
  return crypto.randomBytes(bytes).toString('base64url');
}

export function computePkceChallenge(verifier: string): string {
  return crypto.createHash('sha256').update(verifier).digest('base64url');
}

/**
 * Decodes JWT payload claims for BFF UI display only.
 * Cryptographic RS256 signature verification is strictly enforced by each Rust microservice via JWKS.
 */
export function decodeJwtPayload(token: string): Record<string, unknown> {
  const parts = token.split('.');
  if (parts.length < 2) return {};
  try {
    const json = Buffer.from(parts[1], 'base64url').toString('utf8');
    return JSON.parse(json) as Record<string, unknown>;
  } catch {
    return {};
  }
}

export function extractRolesFromClaims(claims: Record<string, unknown>): string[] {
  const realmAccess = claims.realm_access as { roles?: unknown } | undefined;
  const rawRoles = Array.isArray(realmAccess?.roles)
    ? realmAccess.roles
    : Array.isArray(claims.roles)
      ? claims.roles
      : [];

  return rawRoles.filter(
    (r): r is string =>
      typeof r === 'string' &&
      !r.startsWith('default-roles-') &&
      r !== 'offline_access' &&
      r !== 'uma_authorization',
  );
}

export function extractSessionUser(accessToken: string, idToken?: string): SessionUser {
  const accessClaims = decodeJwtPayload(accessToken);
  const idClaims = idToken ? decodeJwtPayload(idToken) : {};

  const roles = extractRolesFromClaims(accessClaims);

  const email =
    (typeof accessClaims.email === 'string' && accessClaims.email) ||
    (typeof idClaims.email === 'string' && idClaims.email) ||
    (typeof accessClaims.preferred_username === 'string' && accessClaims.preferred_username) ||
    '';

  const name =
    (typeof accessClaims.name === 'string' && accessClaims.name) ||
    (typeof idClaims.name === 'string' && idClaims.name) ||
    email ||
    'Usuario';

  const tenant_id =
    (typeof accessClaims.tenant_id === 'string' && accessClaims.tenant_id) ||
    (typeof idClaims.tenant_id === 'string' && idClaims.tenant_id) ||
    null;

  return {
    id: typeof accessClaims.sub === 'string' ? accessClaims.sub : '',
    email,
    name,
    tenant_id,
    roles,
  };
}

function keycloakTokenUrl(): string {
  const base = config.KEYCLOAK_INTERNAL_URL.replace(/\/+$/, '');
  return `${base}/realms/${config.KEYCLOAK_REALM}/protocol/openid-connect/token`;
}

export function keycloakAuthUrl(): string {
  const base = config.KEYCLOAK_PUBLIC_URL.replace(/\/+$/, '');
  return `${base}/realms/${config.KEYCLOAK_REALM}/protocol/openid-connect/auth`;
}

export function keycloakLogoutUrl(idToken?: string): string {
  const base = config.KEYCLOAK_PUBLIC_URL.replace(/\/+$/, '');
  const url = new URL(`${base}/realms/${config.KEYCLOAK_REALM}/protocol/openid-connect/logout`);
  url.searchParams.set('post_logout_redirect_uri', config.WEB_APP_URL);
  url.searchParams.set('client_id', config.KEYCLOAK_CLIENT_ID);
  if (idToken) {
    url.searchParams.set('id_token_hint', idToken);
  }
  return url.toString();
}

export async function exchangeCodeForTokens(params: {
  code: string;
  codeVerifier: string;
  redirectUri: string;
}): Promise<KeycloakTokenResponse> {
  const form = new URLSearchParams({
    grant_type: 'authorization_code',
    client_id: config.KEYCLOAK_CLIENT_ID,
    client_secret: config.KEYCLOAK_CLIENT_SECRET,
    code: params.code,
    code_verifier: params.codeVerifier,
    redirect_uri: params.redirectUri,
  });

  const res = await fetch(keycloakTokenUrl(), {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body: form.toString(),
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`Keycloak token exchange failed (${res.status}): ${text}`);
  }

  return (await res.json()) as KeycloakTokenResponse;
}

export async function refreshSessionTokens(session: BffSession): Promise<BffSession | null> {
  if (!session.refreshToken) {
    sessions.delete(session.id);
    return null;
  }

  const form = new URLSearchParams({
    grant_type: 'refresh_token',
    client_id: config.KEYCLOAK_CLIENT_ID,
    client_secret: config.KEYCLOAK_CLIENT_SECRET,
    refresh_token: session.refreshToken,
  });

  try {
    const res = await fetch(keycloakTokenUrl(), {
      method: 'POST',
      headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
      body: form.toString(),
    });

    if (!res.ok) {
      sessions.delete(session.id);
      return null;
    }

    const tokens = (await res.json()) as KeycloakTokenResponse;
    const now = Date.now();
    const refreshTtlMs =
      typeof tokens.refresh_expires_in === 'number' && tokens.refresh_expires_in > 0
        ? tokens.refresh_expires_in * 1000
        : SESSION_MAX_TTL_MS;
    const updated: BffSession = {
      id: session.id,
      accessToken: tokens.access_token,
      refreshToken: tokens.refresh_token ?? session.refreshToken,
      idToken: tokens.id_token ?? session.idToken,
      expiresAt: now + tokens.expires_in * 1000,
      sessionExpiresAt: Math.min(session.sessionExpiresAt, now + refreshTtlMs),
      user: extractSessionUser(tokens.access_token, tokens.id_token ?? session.idToken),
    };
    sessions.set(session.id, updated);
    return updated;
  } catch {
    sessions.delete(session.id);
    return null;
  }
}

export function createSession(tokens: KeycloakTokenResponse): BffSession {
  const id = generateRandomUrlSafe(32);
  const now = Date.now();
  const refreshTtlMs =
    typeof tokens.refresh_expires_in === 'number' && tokens.refresh_expires_in > 0
      ? tokens.refresh_expires_in * 1000
      : SESSION_MAX_TTL_MS;
  const session: BffSession = {
    id,
    accessToken: tokens.access_token,
    refreshToken: tokens.refresh_token,
    idToken: tokens.id_token,
    expiresAt: now + tokens.expires_in * 1000,
    sessionExpiresAt: now + Math.min(SESSION_MAX_TTL_MS, refreshTtlMs),
    user: extractSessionUser(tokens.access_token, tokens.id_token),
  };
  sessions.set(id, session);
  return session;
}

export async function getValidSession(sessionId: string | undefined): Promise<BffSession | null> {
  if (!sessionId) return null;
  const session = sessions.get(sessionId);
  if (!session) return null;

  const now = Date.now();
  if (now >= session.sessionExpiresAt) {
    sessions.delete(sessionId);
    return null;
  }

  // Refresh proactively if access token expires in less than 30 seconds
  if (now >= session.expiresAt - 30_000) {
    return refreshSessionTokens(session);
  }

  return session;
}

export function deleteSession(sessionId: string | undefined): BffSession | null {
  if (!sessionId) return null;
  const existing = sessions.get(sessionId) ?? null;
  sessions.delete(sessionId);
  return existing;
}

export async function checkKeycloakHealth(): Promise<{ ok: boolean; status: number }> {
  try {
    const base = config.KEYCLOAK_INTERNAL_URL.replace(/\/+$/, '');
    const res = await fetch(
      `${base}/realms/${config.KEYCLOAK_REALM}/.well-known/openid-configuration`,
      { headers: { Accept: 'application/json' } },
    );
    return { ok: res.ok, status: res.status };
  } catch {
    return { ok: false, status: 503 };
  }
}
