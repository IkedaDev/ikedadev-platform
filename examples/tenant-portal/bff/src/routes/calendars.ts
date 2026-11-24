import { Hono } from 'hono';
import { z } from 'zod';
import { eventsClient } from '../clients/eventsClient.js';
import type { RequestContext } from '../clients/http.js';

const createCalendarSchema = z.object({
  name: z.string().trim().min(1, 'El nombre del calendario es obligatorio'),
  timezone: z.string().min(1).optional().default('America/Santiago'),
  allow_overlaps: z.boolean().optional().default(false),
  working_hours: z.record(z.unknown()).optional().nullable(),
});

import { getAuthRequestContext as getCtx } from '../middleware/auth.js';

export const calendarsRoutes = new Hono();

// GET /api/calendars
calendarsRoutes.get('/', async (c) => {
  const ctx = getCtx(c);
  const calendars = await eventsClient.listCalendars(ctx);
  return c.json({ success: true, data: calendars });
});

// POST /api/calendars
calendarsRoutes.post('/', async (c) => {
  const ctx = getCtx(c);
  const raw = await c.req.json();
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
