import { Hono } from 'hono';
import { catalogClient } from '../clients/catalogClient.js';
import { crmClient } from '../clients/crmClient.js';
import { getAuthRequestContext } from '../middleware/auth.js';
import { listEnrichedBookings } from './bookings.js';

export const dashboardRoutes = new Hono();

// GET /api/dashboard
dashboardRoutes.get('/', async (c) => {
  const ctx = getAuthRequestContext(c);

  const now = new Date();
  const in30Days = new Date(now.getTime() + 30 * 24 * 60 * 60 * 1000);

  const [clientsRes, catalogRes, bookingsRes] = await Promise.allSettled([
    crmClient.searchClients(
      {
        filter: { and: [{ field: 'is_active', operator: 'eq', value: true }] },
        orderBy: [{ field: 'created_at', direction: 'desc' }],
        pagination: { page: 1, limit: 5 },
      },
      ctx,
    ),
    catalogClient.searchItems(
      {
        filter: { and: [{ field: 'is_active', operator: 'eq', value: true }] },
        orderBy: [{ field: 'created_at', direction: 'desc' }],
        pagination: { page: 1, limit: 5 },
      },
      ctx,
    ),
    listEnrichedBookings(
      {
        start: now.toISOString(),
        end: in30Days.toISOString(),
        limit: 50,
      },
      ctx,
    ),
  ]);

  const clientsPage = clientsRes.status === 'fulfilled' ? clientsRes.value : null;
  const catalogPage = catalogRes.status === 'fulfilled' ? catalogRes.value : null;
  const upcomingBookings = bookingsRes.status === 'fulfilled' ? bookingsRes.value : [];

  const activeBookings = upcomingBookings.filter(
    (b) => b.status !== 'CANCELLED' && b.status !== 'NO_SHOW',
  );
  const upcomingRevenue = activeBookings.reduce((sum, b) => sum + b.total_price, 0);
  const currency = activeBookings[0]?.currency ?? catalogPage?.data[0]?.currency ?? 'CLP';

  return c.json({
    success: true,
    data: {
      kpis: {
        totalClients: clientsPage?.pagination.total_items ?? 0,
        activeCatalogItems: catalogPage?.pagination.total_items ?? 0,
        upcomingBookingsCount: activeBookings.length,
        upcomingRevenue,
        currency,
      },
      modules: {
        crm: clientsRes.status === 'fulfilled',
        catalog: catalogRes.status === 'fulfilled',
        bookings: bookingsRes.status === 'fulfilled',
      },
      recentClients: clientsPage?.data ?? [],
      recentCatalogItems: catalogPage?.data ?? [],
      upcomingBookings: upcomingBookings.slice(0, 10),
    },
  });
});
