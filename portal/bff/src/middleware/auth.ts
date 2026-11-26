import type { Context, Next } from 'hono';
import { getCookie } from 'hono/cookie';
import {
  decodeJwtPayload,
  extractRolesFromClaims,
  getValidSession,
} from '../auth/sessionStore.js';
import type { RequestContext } from '../clients/http.js';
import { config } from '../config.js';

export interface AuthVariables {
  authorization?: string;
  actorId?: string;
  tenantId?: string;
  roles?: string[];
  requestId?: string;
}

/**
 * Production BFF authentication middleware for `/api/*`.
 * - Accepts a valid HttpOnly session cookie (auto-refreshing expired Keycloak access tokens)
 *   OR an explicit `Authorization: Bearer <jwt>` header.
 * - Extracts `actorId`, `tenantId`, and `roles` onto the Hono request context.
 * - Rejects unauthenticated browser requests with HTTP 401.
 */
export async function requireAuth(c: Context, next: Next) {
  const headerAuth = c.req.header('Authorization');
  if (headerAuth?.startsWith('Bearer ')) {
    c.set('authorization', headerAuth);
    const claims = decodeJwtPayload(headerAuth.slice('Bearer '.length).trim());
    if (typeof claims.sub === 'string' && claims.sub) {
      c.set('actorId', claims.sub);
    }
    if (typeof claims.tenant_id === 'string' && claims.tenant_id) {
      c.set('tenantId', claims.tenant_id);
    }
    c.set('roles', extractRolesFromClaims(claims));
    return next();
  }

  const sessionId = getCookie(c, config.SESSION_COOKIE_NAME);
  const session = await getValidSession(sessionId);

  if (!session) {
    return c.json(
      {
        success: false,
        code: 'UNAUTHENTICATED',
        message: 'Sesión expirada o no autenticada. Inicia sesión con Keycloak.',
      },
      401,
    );
  }

  c.set('authorization', `Bearer ${session.accessToken}`);
  if (session.user.id) {
    c.set('actorId', session.user.id);
  }
  if (session.user.tenant_id) {
    c.set('tenantId', session.user.tenant_id);
  }
  c.set('roles', session.user.roles);
  return next();
}

export function getAuthRequestContext(c: Context): RequestContext {
  const fromMiddleware = c.get('authorization') as string | undefined;
  const authorization = fromMiddleware ?? c.req.header('Authorization');
  const requestId = (c.get('requestId') as string | undefined) ?? c.req.header('X-Request-Id');
  const actorId = c.get('actorId') as string | undefined;
  const tenantId = c.get('tenantId') as string | undefined;
  const roles = c.get('roles') as string[] | undefined;

  const ctx: RequestContext = {};
  if (authorization) ctx.authorization = authorization;
  if (requestId) ctx.requestId = requestId;
  if (actorId) ctx.actorId = actorId;
  if (tenantId) ctx.tenantId = tenantId;
  if (roles) ctx.roles = roles;
  return ctx;
}
