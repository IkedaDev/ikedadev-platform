import { Hono } from 'hono';
import { z } from 'zod';
import { crmClient } from '../clients/crmClient.js';
import { telemetryClient } from '../clients/telemetryClient.js';
import type { Criteria } from '../types/platform.js';

const createClientSchema = z.object({
  full_name: z.string().trim().min(1, 'El nombre completo es obligatorio'),
  email: z.string().email('Email inválido').optional().nullable().or(z.literal('')),
  phone: z.string().optional().nullable(),
  tax_id: z.string().optional().nullable(),
  address: z.string().optional().nullable(),
  notes: z.string().optional().nullable(),
  is_active: z.boolean().optional(),
  metadata: z.record(z.unknown()).optional(),
});

const updateClientSchema = createClientSchema.partial();

import { getAuthRequestContext as getCtx } from '../middleware/auth.js';

const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export const clientsRoutes = new Hono();

// POST /api/clients/search
clientsRoutes.post('/search', async (c) => {
  const ctx = getCtx(c);
  const body = (await c.req.json().catch(() => ({}))) as Criteria;
  const result = await crmClient.searchClients(body, ctx);
  return c.json(result);
});

// POST /api/clients
clientsRoutes.post('/', async (c) => {
  const ctx = getCtx(c);
  const raw = await c.req.json().catch(() => null);
  const parsed = createClientSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        message: 'Datos de cliente inválidos',
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }

  const payload = {
    ...parsed.data,
    email: parsed.data.email || null,
  };
  const created = await crmClient.createClient(payload, ctx);
  telemetryClient.emitAuditEventAsync(
    {
      event_name: 'client.created',
      resource_type: 'client',
      resource_id: created.id,
      client_id: created.id,
      actor_id: ctx.actorId,
      request_id: ctx.requestId,
      properties: {
        full_name: created.full_name,
        email: created.email,
      },
    },
    ctx,
  );
  return c.json({ success: true, data: created }, 201);
});

// PATCH /api/clients/:id
clientsRoutes.patch('/:id', async (c) => {
  const ctx = getCtx(c);
  const id = c.req.param('id');
  if (!UUID_REGEX.test(id)) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'id debe ser un UUID válido',
      },
      422,
    );
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = updateClientSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        message: 'Datos de actualización inválidos',
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }

  const updated = await crmClient.updateClient(id, parsed.data, ctx);
  telemetryClient.emitAuditEventAsync(
    {
      event_name: 'client.updated',
      resource_type: 'client',
      resource_id: updated.id,
      client_id: updated.id,
      actor_id: ctx.actorId,
      request_id: ctx.requestId,
      properties: {
        updated_fields: Object.keys(parsed.data),
      },
    },
    ctx,
  );
  return c.json({ success: true, data: updated });
});

// DELETE /api/clients/:id
clientsRoutes.delete('/:id', async (c) => {
  const ctx = getCtx(c);
  const id = c.req.param('id');
  if (!UUID_REGEX.test(id)) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'id debe ser un UUID válido',
      },
      422,
    );
  }
  await crmClient.deleteClient(id, ctx);
  telemetryClient.emitAuditEventAsync(
    {
      event_name: 'client.deleted',
      resource_type: 'client',
      resource_id: id,
      client_id: id,
      actor_id: ctx.actorId,
      request_id: ctx.requestId,
    },
    ctx,
  );
  return c.json({ success: true, message: 'Cliente eliminado' });
});
