import crypto from 'node:crypto';
import type { Context, Next } from 'hono';

export const REQUEST_ID_HEADER = 'X-Request-Id';

/**
 * Ensures every incoming request has a correlation `X-Request-Id` UUID,
 * stores it on the Hono context for upstream propagation, and echoes it
 * back on the HTTP response.
 */
export async function requestIdMiddleware(c: Context, next: Next) {
  const incoming = c.req.header(REQUEST_ID_HEADER)?.trim();
  const requestId = incoming && incoming.length <= 128 ? incoming : crypto.randomUUID();
  c.set('requestId', requestId);
  await next();
  c.header(REQUEST_ID_HEADER, requestId);
}
