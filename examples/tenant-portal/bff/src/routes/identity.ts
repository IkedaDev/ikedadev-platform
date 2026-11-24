import { Hono } from 'hono';
import { z } from 'zod';
import type { RequestContext } from '../clients/http.js';
import { identityClient } from '../clients/identityClient.js';

const createUserSchema = z.object({
  email: z.string().email('Email inválido'),
  full_name: z.string().trim().min(1, 'El nombre completo es obligatorio'),
  temporary_password: z.string().min(6).optional(),
  roles: z.array(z.string()).optional().default(['tenant:member']),
  send_invite_email: z.boolean().optional().default(false),
});

const createApiKeySchema = z.object({
  name: z.string().trim().min(1, 'El nombre de la API Key es obligatorio'),
  scopes: z.array(z.string()).optional().default([]),
  expires_at: z.string().datetime().optional().nullable(),
});

function getCtx(c: { req: { header: (name: string) => string | undefined } }): RequestContext {
  const authorization = c.req.header('Authorization');
  return authorization ? { authorization } : {};
}

export const identityRoutes = new Hono();

// GET /api/identity/users
identityRoutes.get('/users', async (c) => {
  const ctx = getCtx(c);
  const users = await identityClient.listUsers(ctx);
  return c.json({ success: true, data: users });
});

// POST /api/identity/users
identityRoutes.post('/users', async (c) => {
  const ctx = getCtx(c);
  const raw = await c.req.json();
  const parsed = createUserSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        message: 'Datos de usuario inválidos',
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }
  const created = await identityClient.createUser(parsed.data, ctx);
  return c.json({ success: true, data: created }, 201);
});

// DELETE /api/identity/users/:id
identityRoutes.delete('/users/:id', async (c) => {
  const ctx = getCtx(c);
  const id = c.req.param('id');
  await identityClient.deleteUser(id, ctx);
  return c.json({ success: true, message: 'Usuario deshabilitado y eliminado' });
});

// GET /api/identity/api-keys
identityRoutes.get('/api-keys', async (c) => {
  const ctx = getCtx(c);
  const keys = await identityClient.listApiKeys(ctx);
  return c.json({ success: true, data: keys });
});

// POST /api/identity/api-keys
identityRoutes.post('/api-keys', async (c) => {
  const ctx = getCtx(c);
  const raw = await c.req.json();
  const parsed = createApiKeySchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        message: 'Datos de API Key inválidos',
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }
  const created = await identityClient.createApiKey(parsed.data, ctx);
  return c.json({ success: true, data: created }, 201);
});

// DELETE /api/identity/api-keys/:id
identityRoutes.delete('/api-keys/:id', async (c) => {
  const ctx = getCtx(c);
  const id = c.req.param('id');
  await identityClient.revokeApiKey(id, ctx);
  return c.json({ success: true, message: 'API Key revocada' });
});
