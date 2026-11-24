import type { Context, Next } from 'hono';
import { getCookie } from 'hono/cookie';
import { getValidSession } from '../auth/sessionStore.js';
import type { RequestContext } from '../clients/http.js';
import { config } from '../config.js';

export interface AuthVariables {
  authorization?: string;
}

/**
 * Production BFF authentication middleware for `/api/*`.
 * - Accepts a valid HttpOnly session cookie (auto-refreshing expired Keycloak access tokens)
 *   OR an explicit `Authorization: Bearer <jwt>` header.
 * - Rejects unauthenticated browser requests with HTTP 401.
 */
export async function requireAuth(c: Context, next: Next) {
  const headerAuth = c.req.header('Authorization');
  if (headerAuth?.startsWith('Bearer ')) {
    c.set('authorization', headerAuth);
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
  return next();
}

export function getAuthRequestContext(c: Context): RequestContext {
  const fromMiddleware = c.get('authorization') as string | undefined;
  const authorization = fromMiddleware ?? c.req.header('Authorization');
  return authorization ? { authorization } : {};
}
