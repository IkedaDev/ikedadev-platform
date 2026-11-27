import { Hono } from 'hono';
import { z } from 'zod';
import { mediaClient } from '../clients/mediaClient.js';
import { telemetryClient } from '../clients/telemetryClient.js';
import { getAuthRequestContext as getCtx } from '../middleware/auth.js';
import type { MediaContext } from '../types/platform.js';

const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const MEDIA_CONTEXTS = ['general', 'branding', 'catalog', 'content', 'avatars'] as const;

function isValidUuid(value: string): boolean {
  return UUID_REGEX.test(value);
}

const presignSchema = z.object({
  file_name: z.string().trim().min(1).max(255),
  mime_type: z.enum(['image/webp', 'image/jpeg', 'image/png', 'image/avif']),
  size_bytes: z.coerce.number().int().positive(),
  context: z.enum(MEDIA_CONTEXTS).optional().default('general'),
});

const confirmSchema = z.object({
  asset_id: z.string().uuid(),
  alt_text: z.string().max(255).optional().nullable(),
  width: z.coerce.number().int().positive().optional().nullable(),
  height: z.coerce.number().int().positive().optional().nullable(),
  metadata: z.record(z.unknown()).optional(),
});

const updateSchema = z.object({
  alt_text: z.string().max(255).optional().nullable(),
  context: z.enum(MEDIA_CONTEXTS).optional(),
  metadata: z.record(z.unknown()).optional(),
});

export const mediaRoutes = new Hono();

mediaRoutes.get('/', async (c) => {
  const ctx = getCtx(c);
  const rawContext = c.req.query('context')?.trim();
  let context: MediaContext | undefined;
  if (rawContext) {
    if (!MEDIA_CONTEXTS.includes(rawContext as MediaContext)) {
      return c.json(
        { success: false, code: 'VALIDATION_ERROR', message: 'Contexto de medio inválido' },
        422,
      );
    }
    context = rawContext as MediaContext;
  }
  const page = Math.max(1, Number(c.req.query('page') ?? 1) || 1);
  const pageSize = Math.min(100, Math.max(1, Number(c.req.query('page_size') ?? 24) || 24));
  const res = await mediaClient.list({ context, page, page_size: pageSize }, ctx);
  return c.json(res);
});

mediaRoutes.get('/quota', async (c) => {
  const ctx = getCtx(c);
  const data = await mediaClient.getQuota(ctx);
  return c.json({ success: true, data });
});

mediaRoutes.post('/presign', async (c) => {
  const raw = await c.req.json().catch(() => null);
  const parsed = presignSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Solicitud de subida inválida (solo WebP/JPEG/PNG/AVIF)',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const ctx = getCtx(c);
  const data = await mediaClient.presign(parsed.data, ctx);
  return c.json({ success: true, data });
});

mediaRoutes.post('/confirm', async (c) => {
  const raw = await c.req.json().catch(() => null);
  const parsed = confirmSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Confirmación de archivo inválida',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const ctx = getCtx(c);
  const asset = await mediaClient.confirm(parsed.data, ctx);

  telemetryClient.emitAuditEventAsync(
    {
      event_name: 'media.asset.uploaded',
      resource_type: 'media_asset',
      resource_id: asset.id,
      properties: {
        context: asset.context,
        mime_type: asset.mime_type,
        size_bytes: asset.size_bytes,
        storage_provider: asset.storage_provider,
      },
    },
    ctx,
  );

  return c.json({ success: true, data: asset });
});

mediaRoutes.get('/:id', async (c) => {
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de archivo inválido' },
      422,
    );
  }
  const ctx = getCtx(c);
  const data = await mediaClient.getById(id, ctx);
  return c.json({ success: true, data });
});

mediaRoutes.patch('/:id', async (c) => {
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de archivo inválido' },
      422,
    );
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = updateSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Datos de archivo inválidos',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const ctx = getCtx(c);
  const data = await mediaClient.update(id, parsed.data, ctx);
  return c.json({ success: true, data });
});

mediaRoutes.delete('/:id', async (c) => {
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de archivo inválido' },
      422,
    );
  }
  const permanent = c.req.query('permanent') === 'true';
  const ctx = getCtx(c);
  await mediaClient.delete(id, permanent, ctx);

  if (permanent) {
    telemetryClient.emitAuditEventAsync(
      {
        event_name: 'media.asset.purged',
        resource_type: 'media_asset',
        resource_id: id,
        properties: { permanent: true },
      },
      ctx,
    );
  }

  return c.json({ success: true });
});

mediaRoutes.post('/:id/restore', async (c) => {
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de archivo inválido' },
      422,
    );
  }
  const ctx = getCtx(c);
  const data = await mediaClient.restore(id, ctx);
  return c.json({ success: true, data });
});
