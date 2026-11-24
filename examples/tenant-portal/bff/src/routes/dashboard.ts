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

  const [clientsPage, catalogPage, upcomingBookings] = await Promise.all([
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
      },
      ctx,
    ),
  ]);

  const upcomingRevenue = upcomingBookings.reduce((sum, b) => sum + b.total_price, 0);
  const currency = upcomingBookings[0]?.currency ?? catalogPage.data[0]?.currency ?? 'CLP';

  return c.json({
    success: true,
    data: {
      kpis: {
        totalClients: clientsPage.pagination.total_items,
        activeCatalogItems: catalogPage.pagination.total_items,
        upcomingBookingsCount: upcomingBookings.length,
        upcomingRevenue,
        currency,
      },
      recentClients: clientsPage.data,
      recentCatalogItems: catalogPage.data,
      upcomingBookings: upcomingBookings.slice(0, 10),
    },
  });
});
