import { serve } from '@hono/node-server';
import { Hono } from 'hono';
import { cors } from 'hono/cors';
import { logger } from 'hono/logger';
import type { ContentfulStatusCode } from 'hono/utils/http-status';
import { checkKeycloakHealth } from './auth/sessionStore.js';
import { catalogClient } from './clients/catalogClient.js';
import { crmClient } from './clients/crmClient.js';
import { eventsClient } from './clients/eventsClient.js';
import { UpstreamApiError } from './clients/http.js';
import { identityClient } from './clients/identityClient.js';
import { config } from './config.js';
import { requireAuth } from './middleware/auth.js';
import { authRoutes } from './routes/auth.js';
import { bookingsRoutes } from './routes/bookings.js';
import { calendarsRoutes } from './routes/calendars.js';
import { categoriesRoutes, itemsRoutes } from './routes/catalog.js';
import { clientsRoutes } from './routes/clients.js';
import { dashboardRoutes } from './routes/dashboard.js';
import { identityRoutes } from './routes/identity.js';

const app = new Hono();

const allowedOrigins = config.CORS_ORIGIN.split(',')
  .map((o) => o.trim())
  .filter(Boolean);

app.use('*', logger());
app.use(
  '*',
  cors({
    origin: (origin) => {
      if (!origin) return allowedOrigins[0] ?? '*';
      if (allowedOrigins.includes('*') || allowedOrigins.includes(origin)) {
        return origin;
      }
      return '';
    },
    allowMethods: ['GET', 'POST', 'PATCH', 'PUT', 'DELETE', 'OPTIONS'],
    allowHeaders: ['Content-Type', 'Authorization'],
    credentials: true,
  }),
);

// Public Health endpoint with upstream microservice & Keycloak checks
app.get('/health', async (c) => {
  const [events, crm, identity, catalog, keycloak] = await Promise.all([
    eventsClient.health(),
    crmClient.health(),
    identityClient.health(),
    catalogClient.health(),
    checkKeycloakHealth(),
  ]);

  const services = { events, crm, identity, catalog, keycloak };
  const allHealthy = Object.values(services).every((s) => s.ok);

  return c.json({
    status: allHealthy ? 'healthy' : 'degraded',
    service: 'tenant-portal-bff',
    timestamp: new Date().toISOString(),
    upstreams: services,
  });
});

// OIDC Auth Routes (Keycloak Authorization Code + PKCE + HttpOnly Session)
app.route('/auth', authRoutes);

// Protect all `/api/*` endpoints with Keycloak session/JWT authentication
app.use('/api/*', requireAuth);

// BFF Orchestration & CRUD Routes
app.route('/api/dashboard', dashboardRoutes);
app.route('/api/bookings', bookingsRoutes);
app.route('/api/clients', clientsRoutes);
app.route('/api/categories', categoriesRoutes);
app.route('/api/items', itemsRoutes);
app.route('/api/calendars', calendarsRoutes);
app.route('/api/identity', identityRoutes);

app.notFound((c) =>
  c.json(
    {
      success: false,
      code: 'NOT_FOUND',
      message: `Ruta no encontrada: ${c.req.method} ${c.req.path}`,
    },
    404,
  ),
);

app.onError((err, c) => {
  if (err instanceof UpstreamApiError) {
    const status = (
      err.status >= 400 && err.status <= 599 ? err.status : 502
    ) as ContentfulStatusCode;

    return c.json(
      {
        success: false,
        code: status === 409 ? 'CONFLICT' : 'UPSTREAM_ERROR',
        service: err.service,
        message: err.message,
        details: err.details,
      },
      status,
    );
  }

  console.error('[BFF Unhandled Error]', err);
  return c.json(
    {
      success: false,
      code: 'INTERNAL_SERVER_ERROR',
      message: 'Error interno del BFF',
    },
    500,
  );
});

serve(
  {
    fetch: app.fetch,
    port: config.PORT,
  },
  (info) => {
    console.log(`🚀 Tenant Portal BFF escuchando en http://localhost:${info.port}`);
    console.log(`   CORS_ORIGIN:      ${config.CORS_ORIGIN}`);
    console.log(`   KEYCLOAK_REALM:   ${config.KEYCLOAK_PUBLIC_URL}/realms/${config.KEYCLOAK_REALM}`);
    console.log(`   EVENTS_API_URL:   ${config.EVENTS_API_URL}`);
    console.log(`   CRM_API_URL:      ${config.CRM_API_URL}`);
    console.log(`   IDENTITY_API_URL: ${config.IDENTITY_API_URL}`);
    console.log(`   CATALOG_API_URL:  ${config.CATALOG_API_URL}`);
  },
);
