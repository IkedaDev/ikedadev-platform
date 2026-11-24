import { Hono } from 'hono';
import { z } from 'zod';
import { catalogClient } from '../clients/catalogClient.js';
import { crmClient } from '../clients/crmClient.js';
import { eventsClient } from '../clients/eventsClient.js';
import { UpstreamApiError, type RequestContext } from '../clients/http.js';
import type {
  Calendar,
  CatalogItem,
  Client,
  EnrichedBooking,
  EnrichedBookingItem,
  EventRecord,
} from '../types/platform.js';

const createBookingSchema = z.object({
  clientId: z.string().uuid('clientId debe ser un UUID válido'),
  calendarId: z.string().uuid('calendarId debe ser un UUID válido'),
  catalogItemIds: z
    .array(z.string().uuid('Cada catalogItemId debe ser un UUID válido'))
    .min(1, 'Debes seleccionar al menos un servicio o ítem del catálogo'),
  startsAt: z.string().datetime({ message: 'startsAt debe ser una fecha ISO-8601 UTC válida' }),
  notes: z.string().max(1000).optional(),
});

import { getAuthRequestContext } from '../middleware/auth.js';

function getRequestContext(c: Parameters<typeof getAuthRequestContext>[0]): RequestContext {
  return getAuthRequestContext(c);
}

function getItemDurationMinutes(item: CatalogItem): number {
  if (item.item_type === 'SERVICE') {
    return item.duration_minutes;
  }
  return 0;
}

function extractBookingMetadata(metadata: Record<string, unknown> | undefined): {
  catalogItemIds: string[];
  totalPrice: number | null;
  currency: string | null;
  notes: string | null;
} {
  if (!metadata || typeof metadata !== 'object') {
    return { catalogItemIds: [], totalPrice: null, currency: null, notes: null };
  }

  const rawIds = metadata.catalogItemIds;
  const catalogItemIds = Array.isArray(rawIds)
    ? rawIds.filter((id): id is string => typeof id === 'string')
    : [];

  const totalPrice = typeof metadata.totalPrice === 'number' ? metadata.totalPrice : null;
  const currency = typeof metadata.currency === 'string' ? metadata.currency : null;
  const notes = typeof metadata.notes === 'string' ? metadata.notes : null;

  return { catalogItemIds, totalPrice, currency, notes };
}

/**
 * Shared orchestration helper used by GET /api/bookings and GET /api/dashboard.
 * Fetches events in range from events-api, resolves full metadata, and batch-loads
 * clients (crm-api) and catalog items (catalog-api) in parallel.
 */
export async function listEnrichedBookings(
  params: { start: string; end: string; calendarId?: string },
  ctx?: RequestContext,
): Promise<EnrichedBooking[]> {
  const calendars: Calendar[] = await eventsClient.listCalendars(ctx);
  const calendarMap = new Map(calendars.map((cal) => [cal.id, cal]));

  const targetCalendars = params.calendarId
    ? calendars.filter((cal) => cal.id === params.calendarId)
    : calendars;

  if (targetCalendars.length === 0) {
    return [];
  }

  // 1. Fetch event occurrences across target calendars in parallel
  const instanceLists = await Promise.all(
    targetCalendars.map((cal) =>
      eventsClient.listEventsByRange(
        {
          calendar_id: cal.id,
          starts_at: params.start,
          ends_at: params.end,
        },
        ctx,
      ),
    ),
  );

  const allInstances = instanceLists.flat();
  if (allInstances.length === 0) {
    return [];
  }

  // 2. Fetch full Event records for unique event_ids to read external_resource_id & metadata
  const uniqueEventIds = Array.from(new Set(allInstances.map((inst) => inst.event_id)));
  const fullEvents = await Promise.all(
    uniqueEventIds.map((id) =>
      eventsClient.getEvent(id, ctx).catch(() => null),
    ),
  );
  const eventMap = new Map<string, EventRecord>();
  for (const ev of fullEvents) {
    if (ev) eventMap.set(ev.id, ev);
  }

  // 3. Collect all client IDs and catalog item IDs for batch resolution
  const clientIds = new Set<string>();
  const catalogItemIds = new Set<string>();

  for (const inst of allInstances) {
    const full = eventMap.get(inst.event_id);
    if (!full) continue;
    if (full.external_resource_id) {
      clientIds.add(full.external_resource_id);
    }
    const meta = extractBookingMetadata(full.metadata);
    for (const itemId of meta.catalogItemIds) {
      catalogItemIds.add(itemId);
    }
  }

  // 4. Batch-resolve clients and catalog items in parallel
  const [clients, catalogItems] = await Promise.all([
    clientIds.size > 0 ? crmClient.findByIds(Array.from(clientIds), ctx).catch(() => []) : Promise.resolve([]),
    catalogItemIds.size > 0
      ? catalogClient.batchGetItems(Array.from(catalogItemIds), ctx).catch(() => [])
      : Promise.resolve([]),
  ]);

  const clientMap = new Map<string, Client>(clients.map((c) => [c.id, c]));
  const itemMap = new Map<string, CatalogItem>(catalogItems.map((i) => [i.id, i]));

  // 5. Assemble enriched bookings sorted by start time
  const enriched: EnrichedBooking[] = allInstances.map((inst) => {
    const full = eventMap.get(inst.event_id);
    const meta = extractBookingMetadata(full?.metadata);
    const clientId = full?.external_resource_id ?? null;
    const client = clientId ? clientMap.get(clientId) : undefined;
    const calendar = calendarMap.get(inst.calendar_id);

    const resolvedItems: EnrichedBookingItem[] = meta.catalogItemIds
      .map((id) => itemMap.get(id))
      .filter((item): item is CatalogItem => Boolean(item))
      .map((item) => ({
        id: item.id,
        name: item.name,
        item_type: item.item_type,
        base_price: item.base_price,
        currency: item.currency,
        duration_minutes: item.item_type === 'SERVICE' ? item.duration_minutes : undefined,
      }));

    const computedPrice = resolvedItems.reduce((sum, item) => sum + item.base_price, 0);
    const currency = meta.currency ?? resolvedItems[0]?.currency ?? 'CLP';

    return {
      id: inst.event_id,
      calendar_id: inst.calendar_id,
      calendar_name: calendar?.name ?? null,
      title: inst.title,
      starts_at: inst.time_range.starts_at,
      ends_at: inst.time_range.ends_at,
      status: inst.status,
      client_id: clientId,
      client_name: client?.full_name ?? null,
      client_email: client?.email ?? null,
      items: resolvedItems,
      total_price: meta.totalPrice ?? computedPrice,
      currency,
      notes: meta.notes ?? full?.description ?? null,
    };
  });

  enriched.sort(
    (a, b) => new Date(a.starts_at).getTime() - new Date(b.starts_at).getTime(),
  );

  return enriched;
}

export const bookingsRoutes = new Hono();

// GET /api/bookings?start=...&end=...&calendarId=...
bookingsRoutes.get('/', async (c) => {
  const ctx = getRequestContext(c);
  const now = new Date();
  const defaultStart = new Date(now.getTime() - 24 * 60 * 60 * 1000).toISOString();
  const defaultEnd = new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000).toISOString();

  const start = c.req.query('start') || c.req.query('starts_at') || defaultStart;
  const end = c.req.query('end') || c.req.query('ends_at') || defaultEnd;
  const calendarId = c.req.query('calendarId') || c.req.query('calendar_id') || undefined;

  const bookings = await listEnrichedBookings({ start, end, calendarId }, ctx);

  return c.json({
    success: true,
    data: bookings,
    meta: { start, end, calendarId: calendarId ?? null, total: bookings.length },
  });
});

// POST /api/bookings
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
        errors: parsed.error.flatten().fieldErrors,
      },
      400,
    );
  }

  const { clientId, calendarId, catalogItemIds, startsAt, notes } = parsed.data;

  // 1. Fetch real catalog items (batch) & client info in parallel
  const [items, client] = await Promise.all([
    catalogClient.batchGetItems(catalogItemIds, ctx),
    crmClient.getClient(clientId, ctx),
  ]);

  if (items.length === 0) {
    return c.json(
      {
        success: false,
        code: 'CATALOG_ITEMS_NOT_FOUND',
        message: 'Ninguno de los ítems de catálogo seleccionados existe o está activo.',
      },
      404,
    );
  }

  const itemById = new Map(items.map((item) => [item.id, item]));
  const missingIds = catalogItemIds.filter((id) => !itemById.has(id));
  if (missingIds.length > 0) {
    return c.json(
      {
        success: false,
        code: 'CATALOG_ITEMS_NOT_FOUND',
        message: `Ítems no encontrados en el catálogo: ${missingIds.join(', ')}`,
      },
      404,
    );
  }

  // 2. Calculate total price and total duration (sum(duration_minutes))
  const orderedItems = catalogItemIds.map((id) => itemById.get(id)!);
  const totalPrice = orderedItems.reduce((acc, item) => acc + item.base_price, 0);
  const sumDuration = orderedItems.reduce((acc, item) => acc + getItemDurationMinutes(item), 0);
  const totalDurationMinutes = sumDuration > 0 ? sumDuration : 30;
  const currency = orderedItems[0]?.currency ?? 'CLP';

  const startDate = new Date(startsAt);
  const endsAt = new Date(startDate.getTime() + totalDurationMinutes * 60 * 1000).toISOString();

  const itemNames = orderedItems.map((i) => i.name).join(' + ');
  const title = `${client.full_name} — ${itemNames}`;

  // 3. Create event in events-api, propagating 409 Conflict cleanly
  try {
    const createdEvent = await eventsClient.createEvent(
      {
        calendar_id: calendarId,
        title,
        description: notes ?? null,
        starts_at: startDate.toISOString(),
        ends_at: endsAt,
        status: 'CONFIRMED',
        external_resource_id: clientId,
        metadata: {
          catalogItemIds,
          totalPrice,
          totalDurationMinutes,
          currency,
          notes: notes ?? null,
        },
      },
      ctx,
    );

    const enriched: EnrichedBooking = {
      id: createdEvent.id,
      calendar_id: createdEvent.calendar_id,
      calendar_name: null,
      title: createdEvent.title,
      starts_at: createdEvent.time_range.starts_at,
      ends_at: createdEvent.time_range.ends_at,
      status: createdEvent.status,
      client_id: client.id,
      client_name: client.full_name,
      client_email: client.email,
      items: orderedItems.map((item) => ({
        id: item.id,
        name: item.name,
        item_type: item.item_type,
        base_price: item.base_price,
        currency: item.currency,
        duration_minutes: item.item_type === 'SERVICE' ? item.duration_minutes : undefined,
      })),
      total_price: totalPrice,
      currency,
      notes: notes ?? null,
    };

    return c.json(
      {
        success: true,
        message: 'Reserva creada exitosamente',
        data: enriched,
      },
      201,
    );
  } catch (err) {
    if (err instanceof UpstreamApiError && err.status === 409) {
      return c.json(
        {
          success: false,
          code: 'BOOKING_CONFLICT',
          message:
            'Conflicto de horario (409): Ya existe una reserva en ese bloque horario y el calendario no permite solapamientos.',
          details: {
            calendarId,
            startsAt: startDate.toISOString(),
            endsAt,
            upstreamMessage: err.message,
          },
        },
        409,
      );
    }
    throw err;
  }
});

// DELETE /api/bookings/:id
bookingsRoutes.delete('/:id', async (c) => {
  const ctx = getRequestContext(c);
  const id = c.req.param('id');
  await eventsClient.deleteEvent(id, ctx);
  return c.json({ success: true, message: 'Reserva eliminada' });
});
