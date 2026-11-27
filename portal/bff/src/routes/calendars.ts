import { Hono } from 'hono';
import { z } from 'zod';
import { canWriteEvents, forbiddenResponse } from '../auth/permissions.js';
import { eventsClient } from '../clients/eventsClient.js';
import { getAuthRequestContext as getCtx } from '../middleware/auth.js';

const createCalendarSchema = z.object({
  name: z.string().trim().min(1, 'El nombre del calendario es obligatorio'),
  timezone: z.string().min(1).optional().default('America/Santiago'),
  allow_overlaps: z.boolean().optional().default(false),
  working_hours: z.record(z.unknown()).optional().nullable(),
});

export const calendarsRoutes = new Hono();

// GET /api/calendars
calendarsRoutes.get('/', async (c) => {
  const ctx = getCtx(c);
  const calendars = await eventsClient.listCalendars(ctx);
  return c.json({ success: true, data: calendars });
});

const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

// POST /api/calendars
calendarsRoutes.post('/', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteEvents(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para crear calendarios.');
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = createCalendarSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        message: 'Datos de calendario inválidos',
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }
  const created = await eventsClient.createCalendar(parsed.data, ctx);
  return c.json({ success: true, data: created }, 201);
});

// GET /api/calendars/:id/availability?starts_at=...&ends_at=...&slot_duration_minutes=...
calendarsRoutes.get('/:id/availability', async (c) => {
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

  const starts_at = c.req.query('starts_at') || c.req.query('start');
  const ends_at = c.req.query('ends_at') || c.req.query('end');
  const slotDurationRaw =
    c.req.query('slot_duration_minutes') || c.req.query('slotDurationMinutes');

  if (!starts_at || !ends_at) {
    return c.json(
      {
        success: false,
        message: 'Debes indicar los query params starts_at (o start) y ends_at (o end) en formato ISO-8601 UTC',
      },
      400,
    );
  }

  const slot_duration_minutes = slotDurationRaw ? Number(slotDurationRaw) : undefined;

  const availability = await eventsClient.getCalendarAvailability(
    id,
    {
      starts_at,
      ends_at,
      slot_duration_minutes:
        slot_duration_minutes && Number.isFinite(slot_duration_minutes)
          ? slot_duration_minutes
          : undefined,
    },
    ctx,
  );

  return c.json({ success: true, data: availability });
});
