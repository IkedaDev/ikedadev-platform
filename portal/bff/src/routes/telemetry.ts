import { Hono } from 'hono';
import { z } from 'zod';
import {
  canReadTelemetry,
  canWriteTelemetry,
  forbiddenResponse,
} from '../auth/permissions.js';
import { telemetryClient } from '../clients/telemetryClient.js';
import { getAuthRequestContext as getCtx } from '../middleware/auth.js';
import type {
  FilterRule,
  IngestTelemetryEventPayload,
  TelemetryEventKind,
} from '../types/platform.js';

const trackEventItemSchema = z.object({
  kind: z.enum(['TRACKING', 'AUDIT']).optional().default('TRACKING'),
  event_name: z
    .string()
    .trim()
    .min(3, 'event_name es obligatorio')
    .max(120, 'event_name excede 120 caracteres'),
  source: z.string().trim().min(1).max(50).optional().default('web'),
  client_id: z.string().uuid().optional().nullable(),
  actor_id: z.string().uuid().optional().nullable(),
  anonymous_id: z.string().max(128).optional().nullable(),
  session_id: z.string().max(128).optional().nullable(),
  resource_type: z.string().max(64).optional().nullable(),
  resource_id: z.string().uuid().optional().nullable(),
  request_id: z.string().max(128).optional().nullable(),
  properties: z.record(z.unknown()).optional().default({}),
  context: z.record(z.unknown()).optional().default({}),
  occurred_at: z.string().datetime().optional(),
});

const trackBatchSchema = z.object({
  events: z
    .array(trackEventItemSchema)
    .min(1, 'Debes enviar al menos 1 evento')
    .max(100, 'El lote máximo es de 100 eventos'),
});

const UUID_REGEX =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function parsePositiveInt(raw: string | undefined, fallback: number, max: number): number {
  if (!raw) return fallback;
  const n = Number.parseInt(raw, 10);
  if (!Number.isFinite(n) || n < 1) return fallback;
  return Math.min(n, max);
}

function parseKind(raw: string | undefined): TelemetryEventKind | undefined {
  if (raw === 'TRACKING' || raw === 'AUDIT') return raw;
  return undefined;
}

export const telemetryRoutes = new Hono();

// POST /api/telemetry/track
telemetryRoutes.post('/track', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteTelemetry(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para registrar eventos de telemetría.');
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = trackBatchSchema.safeParse(raw);

  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Lote de eventos de telemetría inválido',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }

  const validActorId = ctx.actorId && UUID_REGEX.test(ctx.actorId) ? ctx.actorId : null;

  const events: IngestTelemetryEventPayload[] = parsed.data.events.map((ev) => ({
    ...ev,
    // Strictly enforce TRACKING kind, server-derived actor_id & request_id, and prevent 'bff' source spoofing
    kind: 'TRACKING',
    source: ev.source.toLowerCase() === 'bff' ? 'web' : ev.source,
    actor_id: validActorId,
    request_id: ctx.requestId ?? null,
  }));

  const result = await telemetryClient.ingestBatch(events, ctx);
  return c.json({ success: true, data: result }, 201);
});

// GET /api/telemetry/events?page=1&limit=25&kind=TRACKING|AUDIT&event_name=...&source=...
telemetryRoutes.get('/events', async (c) => {
  const ctx = getCtx(c);
  if (!canReadTelemetry(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para consultar eventos de telemetría.');
  }
  const page = parsePositiveInt(c.req.query('page'), 1, 1000);
  const limit = parsePositiveInt(c.req.query('limit'), 25, 100);
  const kind = parseKind(c.req.query('kind'));
  const eventName = c.req.query('event_name')?.trim() || undefined;
  const source = c.req.query('source')?.trim() || undefined;

  const rules: FilterRule[] = [];
  if (kind) {
    rules.push({ field: 'kind', operator: 'eq', value: kind });
  }
  if (eventName) {
    rules.push({ field: 'event_name', operator: 'eq', value: eventName });
  }
  if (source) {
    rules.push({ field: 'source', operator: 'eq', value: source });
  }

  const result = await telemetryClient.searchEvents(
    {
      filter: rules.length > 0 ? { and: rules } : undefined,
      orderBy: [{ field: 'occurred_at', direction: 'desc' }],
      pagination: { page, limit },
    },
    ctx,
  );

  return c.json(result);
});

// GET /api/telemetry/metrics?from=...&to=...&kind=...&source=...
telemetryRoutes.get('/metrics', async (c) => {
  const ctx = getCtx(c);
  if (!canReadTelemetry(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para consultar métricas de telemetría.');
  }
  const now = new Date();
  const defaultFrom = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000).toISOString();
  const defaultTo = now.toISOString();

  const from = c.req.query('from')?.trim() || defaultFrom;
  const to = c.req.query('to')?.trim() || defaultTo;
  const kind = parseKind(c.req.query('kind'));
  const source = c.req.query('source')?.trim() || undefined;

  const summary = await telemetryClient.getMetrics({ from, to, kind, source }, ctx);

  return c.json({
    success: true,
    data: summary,
    meta: { from, to, kind: kind ?? null, source: source ?? null },
  });
});

// GET /api/telemetry/clients/:clientId/timeline?page=1&limit=25&kind=TRACKING|AUDIT
telemetryRoutes.get('/clients/:clientId/timeline', async (c) => {
  const ctx = getCtx(c);
  if (!canReadTelemetry(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para consultar el historial de actividad.');
  }
  const clientId = c.req.param('clientId');
  if (!UUID_REGEX.test(clientId)) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'clientId debe ser un UUID válido',
      },
      422,
    );
  }
  const page = parsePositiveInt(c.req.query('page'), 1, 1000);
  const limit = parsePositiveInt(c.req.query('limit'), 25, 100);
  const kind = parseKind(c.req.query('kind'));

  const result = await telemetryClient.getClientTimeline(
    clientId,
    { page, limit, kind },
    ctx,
  );

  return c.json(result);
});
