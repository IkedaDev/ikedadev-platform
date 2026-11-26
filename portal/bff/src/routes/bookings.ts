import { Hono } from 'hono';
import { z } from 'zod';
import { bookingsClient } from '../clients/bookingsClient.js';
import { catalogClient } from '../clients/catalogClient.js';
import { crmClient } from '../clients/crmClient.js';
import { eventsClient } from '../clients/eventsClient.js';
import { UpstreamApiError, type RequestContext } from '../clients/http.js';
import { telemetryClient } from '../clients/telemetryClient.js';
import { getAuthRequestContext } from '../middleware/auth.js';
import type {
  BookingItemSnapshotInput,
  BookingRecord,
  BookingStatus,
  Calendar,
  CatalogItem,
  Client,
  EnrichedBooking,
  FilterRule,
} from '../types/platform.js';

const createBookingSchema = z.object({
  clientId: z.string().uuid('clientId debe ser un UUID válido'),
  calendarId: z.string().uuid('calendarId debe ser un UUID válido'),
  catalogItemIds: z
    .array(z.string().uuid('Cada catalogItemId debe ser un UUID válido'))
    .min(1, 'Debes seleccionar al menos un servicio o ítem del catálogo'),
  startsAt: z.string().datetime({ message: 'startsAt debe ser una fecha ISO-8601 UTC válida' }),
  notes: z.string().max(1000).optional(),
  metadata: z.record(z.unknown()).optional(),
});

const transitionStatusSchema = z.object({
  status: z.enum(['PENDING', 'SCHEDULED', 'IN_PROGRESS', 'COMPLETED', 'CANCELLED', 'NO_SHOW']),
  cancellationReason: z.string().max(1000).optional(),
  metadata: z.record(z.unknown()).optional(),
});

function getRequestContext(c: Parameters<typeof getAuthRequestContext>[0]): RequestContext {
  return getAuthRequestContext(c);
}

function getItemDurationAndBuffer(item: CatalogItem): { duration: number; buffer: number } {
  if (item.item_type === 'SERVICE') {
    return {
      duration: item.duration_minutes,
      buffer: item.buffer_minutes ?? 0,
    };
  }
  return { duration: 0, buffer: 0 };
}

function buildSnapshotInputs(items: CatalogItem[]): BookingItemSnapshotInput[] {
  return items.map((item) => {
    const { duration, buffer } = getItemDurationAndBuffer(item);
    return {
      catalog_item_id: item.id,
      variant_id: null,
      name_snapshot: item.name,
      sku_snapshot: item.sku ?? null,
      quantity: 1,
      unit_price_snapshot: item.base_price,
      tax_rate_bps: item.tax_rate_bps,
      tax_included: item.tax_included,
      duration_minutes_snapshot: duration,
      buffer_minutes_snapshot: buffer,
    };
  });
}

async function enrichBookings(
  bookings: BookingRecord[],
  ctx?: RequestContext,
): Promise<EnrichedBooking[]> {
  if (bookings.length === 0) return [];

  const uniqueClientIds = Array.from(new Set(bookings.map((b) => b.client_id)));
  const [clientsList, calendars] = await Promise.all([
    Promise.all(
      uniqueClientIds.map((id) =>
        crmClient.getClient(id, ctx).catch((): Client | null => null),
      ),
    ),
    eventsClient.listCalendars(ctx).catch((): Calendar[] => []),
  ]);

  const clientMap = new Map<string, Client>();
  for (const c of clientsList) {
    if (c) clientMap.set(c.id, c);
  }
  const calendarMap = new Map<string, Calendar>(calendars.map((cal) => [cal.id, cal]));

  return bookings.map((b) => {
    const client = clientMap.get(b.client_id);
    const cal = b.calendar_id ? calendarMap.get(b.calendar_id) : undefined;
    return {
      id: b.id,
      booking_code: b.booking_code,
      calendar_id: b.calendar_id ?? null,
      calendar_name: cal?.name ?? null,
      event_id: b.event_id ?? null,
      starts_at: b.starts_at,
      ends_at: b.ends_at,
      timezone: b.timezone,
      status: b.status,
      client: client
        ? {
            id: client.id,
            full_name: client.full_name,
            email: client.email,
            phone: client.phone,
          }
        : null,
      items: b.items,
      subtotal: b.subtotal,
      tax_total: b.tax_total,
      total_price: b.total_price,
      total_duration_minutes: b.total_duration_minutes,
      currency: b.currency,
      notes: b.notes ?? null,
      cancellation_reason: b.cancellation_reason ?? null,
      cancelled_at: b.cancelled_at ?? null,
      completed_at: b.completed_at ?? null,
      metadata: b.metadata ?? {},
      created_at: b.created_at,
    };
  });
}

/**
 * Shared orchestration helper used by GET /api/bookings and GET /api/dashboard.
 * Queries `bookings-api` via `POST /v1/bookings/search` and enriches client & calendar names.
 */
export async function listEnrichedBookings(
  params: {
    start?: string;
    end?: string;
    calendarId?: string;
    status?: BookingStatus;
    limit?: number;
  },
  ctx?: RequestContext,
): Promise<EnrichedBooking[]> {
  const rules: FilterRule[] = [];
  if (params.start) {
    rules.push({ field: 'starts_at', operator: 'gte', value: params.start });
  }
  if (params.end) {
    rules.push({ field: 'starts_at', operator: 'lte', value: params.end });
  }
  if (params.calendarId) {
    rules.push({ field: 'calendar_id', operator: 'eq', value: params.calendarId });
  }
  if (params.status) {
    rules.push({ field: 'status', operator: 'eq', value: params.status });
  }

  const page = await bookingsClient.searchBookings(
    {
      filter: rules.length > 0 ? { and: rules } : undefined,
      orderBy: [{ field: 'starts_at', direction: 'asc' }],
      pagination: { page: 1, limit: params.limit ?? 100 },
    },
    ctx,
  );

  return enrichBookings(page.data, ctx);
}

export const bookingsRoutes = new Hono();

// GET /api/bookings?start=...&end=...&calendarId=...&status=...
bookingsRoutes.get('/', async (c) => {
  const ctx = getRequestContext(c);
  const now = new Date();
  const defaultStart = new Date(now.getTime() - 7 * 24 * 60 * 60 * 1000).toISOString();
  const defaultEnd = new Date(now.getTime() + 60 * 24 * 60 * 60 * 1000).toISOString();

  const start = c.req.query('start') || defaultStart;
  const end = c.req.query('end') || defaultEnd;
  const calendarId = c.req.query('calendarId') || undefined;
  const rawStatus = c.req.query('status') || undefined;
  const status = rawStatus as BookingStatus | undefined;

  const bookings = await listEnrichedBookings({ start, end, calendarId, status }, ctx);

  return c.json({
    success: true,
    data: bookings,
    meta: { start, end, count: bookings.length },
  });
});

// POST /api/bookings
// 4-step transactional orchestration with compensation:
//   1. Resolve client (crm-api) + catalog items in batch (catalog-api) + calendar (events-api)
//   2. Reserve time block in events-api (propagates 409 Conflict on overlap)
//   3. Create authoritative booking in bookings-api with frozen snapshots + Idempotency-Key
//   4. Compensate: if bookings-api fails after event creation, delete the orphan event in events-api
bookingsRoutes.post('/', async (c) => {
  const ctx = getRequestContext(c);
  const rawBody = await c.req.json();
  const parsed = createBookingSchema.safeParse(rawBody);

  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Datos de reserva inválidos',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }

  const { clientId, calendarId, catalogItemIds, startsAt, notes, metadata } = parsed.data;
  const idempotencyKey = c.req.header('Idempotency-Key')?.trim() || undefined;

  // Step 1: Parallel fetch of client, catalog items (batch), and calendar
  const [client, catalogItems, calendar] = await Promise.all([
    crmClient.getClient(clientId, ctx),
    catalogClient.batchGetItems(catalogItemIds, ctx),
    eventsClient.getCalendar(calendarId, ctx),
  ]);

  if (catalogItems.length === 0) {
    return c.json(
      {
        success: false,
        code: 'CATALOG_ITEMS_NOT_FOUND',
        message: 'No se encontraron los ítems seleccionados en catalog-api.',
      },
      404,
    );
  }

  // Preserve requested ordering / duplicates from catalogItemIds
  const catalogMap = new Map<string, CatalogItem>(catalogItems.map((i) => [i.id, i]));
  const orderedItems = catalogItemIds
    .map((id) => catalogMap.get(id))
    .filter((i): i is CatalogItem => Boolean(i));

  if (orderedItems.length === 0) {
    return c.json(
      {
        success: false,
        code: 'CATALOG_ITEMS_NOT_FOUND',
        message: 'Ninguno de los ítems seleccionados existe en catalog-api.',
      },
      404,
    );
  }

  const snapshotInputs = buildSnapshotInputs(orderedItems);
  const computedDuration = snapshotInputs.reduce(
    (sum, item) => sum + (item.duration_minutes_snapshot + item.buffer_minutes_snapshot) * item.quantity,
    0,
  );
  const totalDurationMinutes = computedDuration > 0 ? computedDuration : 30;
  const startDate = new Date(startsAt);
  const endDate = new Date(startDate.getTime() + totalDurationMinutes * 60 * 1000);
  const currency = orderedItems[0]?.currency ?? 'CLP';
  const itemNames = orderedItems.map((i) => i.name).join(' + ');

  // Step 2: Reserve block in events-api (409 Conflict propagates automatically on overlap)
  const createdEvent = await eventsClient.createEvent(
    {
      calendar_id: calendar.id,
      title: `${client.full_name} — ${itemNames}`,
      description: notes ?? `Reserva generada desde Tenant Portal (${itemNames})`,
      starts_at: startDate.toISOString(),
      ends_at: endDate.toISOString(),
      timezone: calendar.timezone || 'UTC',
      status: 'CONFIRMED',
      external_resource_id: client.id,
      metadata: {
        source: 'tenant-portal-bff',
        client_id: client.id,
      },
    },
    ctx,
  );

  // Step 3: Create authoritative booking in bookings-api (with compensation on failure)
  let booking: BookingRecord;
  try {
    booking = await bookingsClient.createBooking(
      {
        client_id: client.id,
        calendar_id: calendar.id,
        event_id: createdEvent.id,
        starts_at: startDate.toISOString(),
        ends_at: endDate.toISOString(),
        timezone: calendar.timezone || 'UTC',
        status: 'SCHEDULED',
        currency,
        notes: notes ?? null,
        metadata: {
          source: 'tenant-portal',
          ...(metadata ?? {}),
        },
        items: snapshotInputs,
      },
      ctx,
      idempotencyKey,
    );
  } catch (err) {
    // Step 4: Compensation — remove orphan event from events-api so slot is released
    try {
      await eventsClient.deleteEvent(createdEvent.id, ctx);
    } catch (compErr) {
      console.error(
        `[BFF Compensation Error] Failed to delete orphan event ${createdEvent.id}:`,
        compErr,
      );
    }
    throw err;
  }

  const enriched: EnrichedBooking = {
    id: booking.id,
    booking_code: booking.booking_code,
    calendar_id: booking.calendar_id ?? calendar.id,
    calendar_name: calendar.name,
    event_id: booking.event_id ?? createdEvent.id,
    starts_at: booking.starts_at,
    ends_at: booking.ends_at,
    timezone: booking.timezone,
    status: booking.status,
    client: {
      id: client.id,
      full_name: client.full_name,
      email: client.email,
      phone: client.phone,
    },
    items: booking.items,
    subtotal: booking.subtotal,
    tax_total: booking.tax_total,
    total_price: booking.total_price,
    total_duration_minutes: booking.total_duration_minutes,
    currency: booking.currency,
    notes: booking.notes ?? null,
    cancellation_reason: booking.cancellation_reason ?? null,
    cancelled_at: booking.cancelled_at ?? null,
    completed_at: booking.completed_at ?? null,
    metadata: booking.metadata,
    created_at: booking.created_at,
  };

  telemetryClient.emitAuditEventAsync(
    {
      event_name: 'booking.created',
      resource_type: 'booking',
      resource_id: booking.id,
      client_id: client.id,
      actor_id: ctx.actorId,
      request_id: ctx.requestId,
      properties: {
        booking_code: booking.booking_code,
        status: booking.status,
        total_price: booking.total_price,
        currency: booking.currency,
        items_count: booking.items.length,
      },
    },
    ctx,
  );

  return c.json(
    {
      success: true,
      message: 'Reserva creada y horario bloqueado exitosamente',
      data: enriched,
    },
    201,
  );
});

// PATCH /api/bookings/:id/status
// Transitions booking status in bookings-api; if CANCELLED and event_id exists,
// deletes the associated event in events-api to automatically free the time slot.
bookingsRoutes.patch('/:id/status', async (c) => {
  const ctx = getRequestContext(c);
  const id = c.req.param('id');
  const rawBody = await c.req.json();
  const parsed = transitionStatusSchema.safeParse(rawBody);

  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Estado de reserva inválido',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }

  const updated = await bookingsClient.transitionStatus(
    id,
    {
      status: parsed.data.status,
      cancellation_reason: parsed.data.cancellationReason ?? null,
      metadata: parsed.data.metadata,
    },
    ctx,
  );

  if (updated.status === 'CANCELLED' && updated.event_id) {
    try {
      await eventsClient.deleteEvent(updated.event_id, ctx);
    } catch (err) {
      if (!(err instanceof UpstreamApiError && err.status === 404)) {
        console.error(
          `[BFF Slot Release Warning] Booking ${updated.id} cancelled, but failed to delete event ${updated.event_id}:`,
          err,
        );
      }
    }
  }

  telemetryClient.emitAuditEventAsync(
    {
      event_name: updated.status === 'CANCELLED' ? 'booking.cancelled' : 'booking.updated',
      resource_type: 'booking',
      resource_id: updated.id,
      client_id: updated.client_id,
      actor_id: ctx.actorId,
      request_id: ctx.requestId,
      properties: {
        booking_code: updated.booking_code,
        status: updated.status,
        cancellation_reason: updated.cancellation_reason ?? null,
      },
    },
    ctx,
  );

  const [enriched] = await enrichBookings([updated], ctx);
  return c.json({
    success: true,
    message: `Estado actualizado a ${updated.status}`,
    data: enriched,
  });
});

// DELETE /api/bookings/:id
bookingsRoutes.delete('/:id', async (c) => {
  const ctx = getRequestContext(c);
  const id = c.req.param('id');

  const existing = await bookingsClient.getBooking(id, ctx).catch(() => null);
  await bookingsClient.deleteBooking(id, ctx);

  if (existing?.event_id) {
    try {
      await eventsClient.deleteEvent(existing.event_id, ctx);
    } catch (err) {
      if (!(err instanceof UpstreamApiError && err.status === 404)) {
        console.error(`[BFF Delete Warning] Failed to delete event ${existing.event_id}:`, err);
      }
    }
  }

  telemetryClient.emitAuditEventAsync(
    {
      event_name: 'booking.deleted',
      resource_type: 'booking',
      resource_id: id,
      client_id: existing?.client_id ?? null,
      actor_id: ctx.actorId,
      request_id: ctx.requestId,
    },
    ctx,
  );

  return c.json({ success: true, message: 'Reserva eliminada y cupo liberado' });
});
