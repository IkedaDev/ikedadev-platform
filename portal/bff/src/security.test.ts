import assert from 'node:assert/strict';
import { afterEach, describe, it } from 'node:test';
import { Hono } from 'hono';
import { cors } from 'hono/cors';
import {
  clearAllSessionsForTest,
  createSession,
  deleteSession,
  getSessionCount,
  getValidSession,
} from './auth/sessionStore.js';
import { bookingsClient } from './clients/bookingsClient.js';
import { catalogClient } from './clients/catalogClient.js';
import { crmClient } from './clients/crmClient.js';
import { eventsClient } from './clients/eventsClient.js';
import { ServiceHttpClient, UpstreamApiError } from './clients/http.js';
import { identityClient } from './clients/identityClient.js';
import { telemetryClient } from './clients/telemetryClient.js';
import {
  config,
  DEFAULT_DEV_KEYCLOAK_CLIENT_SECRET,
  envSchema,
  parseAllowedOrigins,
  resolveCorsOrigin,
} from './config.js';
import { authRoutes, signOidcTx, verifyOidcTx } from './routes/auth.js';
import { bookingsRoutes } from './routes/bookings.js';
import { itemsRoutes } from './routes/catalog.js';
import { clientsRoutes } from './routes/clients.js';
import { contentRoutes } from './routes/content.js';
import { identityRoutes } from './routes/identity.js';
import { mediaRoutes } from './routes/media.js';
import { requireAuth } from './middleware/auth.js';
import {
  cacheTenantBranding,
  clearSlugBrandingCacheForTest,
  getSlugBrandingCacheSize,
  isSafeLogoUrl,
  publicBrandingRoutes,
  settingsRoutes,
} from './routes/settings.js';
import { telemetryRoutes } from './routes/telemetry.js';

const TENANT_A_ID = '11111111-1111-4111-8111-111111111111';
const TENANT_B_ID = '22222222-2222-4222-8222-222222222222';
const originalFetch = globalThis.fetch;

afterEach(() => {
  globalThis.fetch = originalFetch;
});

describe('ServiceHttpClient — Strict Multi-Tenant Credential Isolation', () => {
  it('never sends X-Api-Key on logged-in user requests (even if smuggled in extraHeaders)', async () => {
    let capturedHeaders: Record<string, string> = {};

    globalThis.fetch = (async (_input: unknown, init?: RequestInit) => {
      capturedHeaders = (init?.headers ?? {}) as Record<string, string>;
      return new Response(JSON.stringify({ success: true, data: [] }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    }) as typeof fetch;

    const client = new ServiceHttpClient({
      serviceName: 'identity-api',
      baseUrl: 'http://localhost:8082',
    });

    await client.requestData('GET', '/v1/users', {
      ctx: {
        authorization: 'Bearer user-jwt-barberia-norte',
        tenantId: TENANT_B_ID,
        roles: ['tenant:admin'],
        authMethod: 'session',
      },
      extraHeaders: {
        'X-Api-Key': 'ik_live_should_be_stripped',
        'x-api-key': 'ik_live_should_also_be_stripped',
        'X-Custom-Header': 'kept',
      },
    });

    const headerKeysLower = Object.keys(capturedHeaders).map((k) => k.toLowerCase());
    assert.equal(capturedHeaders['Authorization'], 'Bearer user-jwt-barberia-norte');
    assert.equal(headerKeysLower.includes('x-api-key'), false);
    assert.equal(capturedHeaders['X-Custom-Header'], 'kept');
  });

  it('never sends both X-Api-Key and Authorization simultaneously when apiKeyOverride is used', async () => {
    let capturedHeaders: Record<string, string> = {};

    globalThis.fetch = (async (_input: unknown, init?: RequestInit) => {
      capturedHeaders = (init?.headers ?? {}) as Record<string, string>;
      return new Response(JSON.stringify({ success: true, data: {} }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    }) as typeof fetch;

    const client = new ServiceHttpClient({
      serviceName: 'identity-api',
      baseUrl: 'http://localhost:8082',
    });

    await client.requestData('GET', `/v1/tenants/${TENANT_A_ID}`, {
      ctx: {
        authorization: 'Bearer user-jwt',
        tenantId: TENANT_A_ID,
        roles: ['tenant:admin'],
        authMethod: 'session',
      },
      apiKeyOverride: 'ik_live_platform_admin_key',
    });

    const headerKeysLower = Object.keys(capturedHeaders).map((k) => k.toLowerCase());
    assert.equal(capturedHeaders['X-Api-Key'], 'ik_live_platform_admin_key');
    assert.equal(headerKeysLower.includes('authorization'), false);
  });
});

describe('settingsRoutes — Session-Only Tenant Scoping & RBAC', () => {
  function mountSettingsWithSession(session: {
    tenantId: string;
    roles: string[];
    authMethod: 'session' | 'bearer';
    authorization: string;
  }) {
    const app = new Hono();
    app.use('/api/settings/*', async (c, next) => {
      c.set('tenantId' as never, session.tenantId as never);
      c.set('roles' as never, session.roles as never);
      c.set('authMethod' as never, session.authMethod as never);
      c.set('authorization' as never, session.authorization as never);
      await next();
    });
    app.route('/api/settings', settingsRoutes);
    return app;
  }

  it('ignores any client-injected tenantId in query/body and always targets session tenantId', async () => {
    const requestedTenantIds: string[] = [];
    const originalGetTenant = identityClient.getTenant;
    const originalPatchTenant = identityClient.patchTenant;
    const originalAudit = telemetryClient.emitAuditEventAsync;

    identityClient.getTenant = async (tenantId: string) => {
      requestedTenantIds.push(tenantId);
      return {
        id: tenantId,
        slug: 'barberia-norte',
        name: 'Barbería Norte',
        status: 'ACTIVE',
        config: {},
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        deleted_at: null,
      };
    };
    identityClient.patchTenant = async (tenantId: string, cmd) => {
      requestedTenantIds.push(tenantId);
      return {
        id: tenantId,
        slug: 'barberia-norte',
        name: 'Barbería Norte',
        status: 'ACTIVE',
        config: cmd.config ?? {},
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        deleted_at: null,
      };
    };
    telemetryClient.emitAuditEventAsync = () => {};

    try {
      const app = mountSettingsWithSession({
        tenantId: TENANT_B_ID,
        roles: ['tenant:admin'],
        authMethod: 'session',
        authorization: 'Bearer session-jwt',
      });

      // Attempt to inject Tenant A via query param & body
      const res = await app.request(`/api/settings?tenantId=${TENANT_A_ID}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          tenantId: TENANT_A_ID,
          branding: { displayName: 'Barbería Norte Updated' },
        }),
      });

      assert.equal(res.status, 200);
      assert.deepEqual(requestedTenantIds, [TENANT_B_ID, TENANT_B_ID]);
    } finally {
      identityClient.getTenant = originalGetTenant;
      identityClient.patchTenant = originalPatchTenant;
      telemetryClient.emitAuditEventAsync = originalAudit;
    }
  });

  it('blocks non-admin (tenant:member) from PATCH /api/settings with 403', async () => {
    const app = mountSettingsWithSession({
      tenantId: TENANT_B_ID,
      roles: ['tenant:member'],
      authMethod: 'session',
      authorization: 'Bearer staff-jwt',
    });

    const res = await app.request('/api/settings', {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ branding: { displayName: 'Unauthorized' } }),
    });

    assert.equal(res.status, 403);
  });

  it('never elevates raw Bearer requests with PLATFORM_ADMIN_API_KEY', async () => {
    let capturedApiKeyOverride: string | undefined = 'SENTINEL';
    const originalGetTenant = identityClient.getTenant;
    const originalPlatformKey = config.PLATFORM_ADMIN_API_KEY;
    (config as { PLATFORM_ADMIN_API_KEY?: string }).PLATFORM_ADMIN_API_KEY = 'ik_live_secret_admin';

    identityClient.getTenant = async (tenantId: string, opts) => {
      capturedApiKeyOverride = opts?.apiKeyOverride;
      return {
        id: tenantId,
        slug: 'barberia-norte',
        name: 'Barbería Norte',
        status: 'ACTIVE',
        config: {},
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        deleted_at: null,
      };
    };

    try {
      const app = mountSettingsWithSession({
        tenantId: TENANT_A_ID,
        roles: ['tenant:admin'],
        authMethod: 'bearer',
        authorization: 'Bearer untrusted-raw-bearer',
      });

      const res = await app.request('/api/settings', { method: 'GET' });
      assert.equal(res.status, 200);
      assert.equal(capturedApiKeyOverride, undefined);
    } finally {
      identityClient.getTenant = originalGetTenant;
      (config as { PLATFORM_ADMIN_API_KEY?: string }).PLATFORM_ADMIN_API_KEY = originalPlatformKey;
    }
  });
});

describe('identityRoutes — BFF RBAC Defense in Depth', () => {
  it('blocks tenant:member from creating/deleting users and managing API keys', async () => {
    const app = new Hono();
    app.use('/api/identity/*', async (c, next) => {
      c.set('tenantId' as never, TENANT_A_ID as never);
      c.set('roles' as never, ['tenant:member'] as never);
      c.set('authMethod' as never, 'session' as never);
      c.set('authorization' as never, 'Bearer staff-jwt' as never);
      await next();
    });
    app.route('/api/identity', identityRoutes);

    const r1 = await app.request('/api/identity/users', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ email: 'a@b.cl', full_name: 'Test', roles: ['tenant:member'] }),
    });
    assert.equal(r1.status, 403);

    const r2 = await app.request(`/api/identity/users/${TENANT_A_ID}`, { method: 'DELETE' });
    assert.equal(r2.status, 403);

    const r3 = await app.request('/api/identity/api-keys', { method: 'GET' });
    assert.equal(r3.status, 403);

    const r4 = await app.request('/api/identity/api-keys', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ name: 'Key', scopes: [] }),
    });
    assert.equal(r4.status, 403);

    const r5 = await app.request(`/api/identity/api-keys/${TENANT_A_ID}`, { method: 'DELETE' });
    assert.equal(r5.status, 403);
  });
});

describe('[BFF-01] CORS Origin Strict Allowlist', () => {
  it('rejects wildcard * in CORS_ORIGIN config schema and never reflects arbitrary Origin', async () => {
    const parsedWildcard = envSchema.safeParse({ CORS_ORIGIN: '*' });
    assert.equal(parsedWildcard.success, false);

    const allowed = parseAllowedOrigins('*, https://portal.kropy.cl');
    assert.deepEqual(allowed, ['https://portal.kropy.cl']);
    assert.equal(resolveCorsOrigin('https://evil.example', allowed), '');
    assert.equal(resolveCorsOrigin(undefined, allowed), '');
    assert.equal(resolveCorsOrigin('https://portal.kropy.cl', allowed), 'https://portal.kropy.cl');

    const app = new Hono();
    app.use(
      '*',
      cors({
        origin: (origin) => resolveCorsOrigin(origin, allowed),
        credentials: true,
      }),
    );
    app.get('/ping', (c) => c.json({ ok: true }));

    const evilRes = await app.request('/ping', {
      headers: { Origin: 'https://evil.example' },
    });
    assert.equal(evilRes.headers.get('Access-Control-Allow-Origin'), null);

    const goodRes = await app.request('/ping', {
      headers: { Origin: 'https://portal.kropy.cl' },
    });
    assert.equal(goodRes.headers.get('Access-Control-Allow-Origin'), 'https://portal.kropy.cl');
  });
});

describe('[BFF-02] & [BFF-04] Session Store — Logout Refresh Race & Bounded Capacity', () => {
  afterEach(() => {
    clearAllSessionsForTest();
  });

  it('does not resurrect a logged-out session when an in-flight token refresh resolves', async () => {
    let resolveFetch!: (res: Response) => void;
    globalThis.fetch = (() =>
      new Promise<Response>((resolve) => {
        resolveFetch = resolve;
      })) as typeof fetch;

    const session = createSession({
      access_token: 'initial-access',
      refresh_token: 'initial-refresh',
      expires_in: 1, // < 30s triggers proactive refresh immediately
      token_type: 'Bearer',
    });

    const refreshPromise = getValidSession(session.id);

    // User logs out while refresh fetch is in flight
    const deleted = deleteSession(session.id);
    assert.ok(deleted);

    resolveFetch(
      new Response(
        JSON.stringify({
          access_token: 'refreshed-access',
          refresh_token: 'refreshed-refresh',
          expires_in: 300,
          token_type: 'Bearer',
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      ),
    );

    const result = await refreshPromise;
    assert.equal(result, null);
    assert.equal(getSessionCount(), 0);
  });

  it('enforces maxSessions cap and evicts oldest sessions FIFO', () => {
    const createdIds: string[] = [];
    for (let i = 0; i < 8; i += 1) {
      const s = createSession(
        {
          access_token: `tok-${i}`,
          expires_in: 3600,
          token_type: 'Bearer',
        },
        { maxSessions: 5 },
      );
      createdIds.push(s.id);
    }

    assert.equal(getSessionCount(), 5);
    assert.equal(deleteSession(createdIds[0]), null);
    assert.equal(deleteSession(createdIds[2]), null);
    assert.ok(deleteSession(createdIds[7]));
  });
});

describe('[BFF-03] Path Parameter Validation & Encoding', () => {
  it('rejects non-UUID path params with 422 on clients, telemetry, and bookings routes', async () => {
    const app = new Hono();
    app.use('/api/*', async (c, next) => {
      c.set('tenantId' as never, TENANT_A_ID as never);
      c.set('roles' as never, ['tenant:admin'] as never);
      c.set('authMethod' as never, 'session' as never);
      c.set('authorization' as never, 'Bearer jwt' as never);
      await next();
    });
    app.route('/api/clients', clientsRoutes);
    app.route('/api/telemetry', telemetryRoutes);
    app.route('/api/bookings', bookingsRoutes);

    const r1 = await app.request('/api/clients/..%2Fadmin', { method: 'DELETE' });
    assert.equal(r1.status, 422);

    const r2 = await app.request('/api/telemetry/clients/..%2F..%2Fv1%2Fevents/timeline');
    assert.equal(r2.status, 422);

    const r3 = await app.request('/api/bookings/not-a-uuid/status', {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ status: 'CANCELLED' }),
    });
    assert.equal(r3.status, 422);
  });

  it('percent-encodes path parameters in upstream client requests', async () => {
    let capturedUrl = '';
    globalThis.fetch = (async (input: unknown) => {
      capturedUrl = String(input);
      return new Response(JSON.stringify({ success: true, data: { id: 'x' } }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    }) as typeof fetch;

    await crmClient.getClient('../other?inject=1');
    assert.ok(capturedUrl.includes('/v1/clients/..%2Fother%3Finject%3D1'));
    assert.equal(new URL(capturedUrl).search, '');
  });
});

describe('[BFF-05] Telemetry Track — Anti-Audit-Forgery & Actor Integrity', () => {
  it('forces kind=TRACKING, server actor_id, request_id, and blocks source=bff spoofing', async () => {
    const ACTOR_UUID = '33333333-3333-4333-8333-333333333333';
    const SPOOFED_UUID = '44444444-4444-4444-8444-444444444444';
    let capturedEvents: unknown[] = [];

    const originalIngestBatch = telemetryClient.ingestBatch;
    telemetryClient.ingestBatch = async (events) => {
      capturedEvents = events;
      return { accepted: events.length, event_ids: [] };
    };

    try {
      const app = new Hono();
      app.use('/api/telemetry/*', async (c, next) => {
        c.set('actorId' as never, ACTOR_UUID as never);
        c.set('requestId' as never, 'req-server-123' as never);
        c.set('roles' as never, ['tenant:member', 'telemetry:write'] as never);
        c.set('authorization' as never, 'Bearer member-jwt' as never);
        await next();
      });
      app.route('/api/telemetry', telemetryRoutes);

      const res = await app.request('/api/telemetry/track', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          events: [
            {
              kind: 'AUDIT',
              event_name: 'settings.updated',
              source: 'bff',
              actor_id: SPOOFED_UUID,
              request_id: 'forged-req-id',
            },
          ],
        }),
      });

      assert.equal(res.status, 201);
      assert.equal(capturedEvents.length, 1);
      const ev = capturedEvents[0] as Record<string, unknown>;
      assert.equal(ev.kind, 'TRACKING');
      assert.equal(ev.source, 'web');
      assert.equal(ev.actor_id, ACTOR_UUID);
      assert.equal(ev.request_id, 'req-server-123');
    } finally {
      telemetryClient.ingestBatch = originalIngestBatch;
    }
  });
});

describe('[BFF-06] OIDC Transaction Signing & Production Secret Guard', () => {
  it('signs and verifies OIDC tx cookie and rejects tampered payloads', () => {
    const signed = signOidcTx({ state: 'state-123', codeVerifier: 'verifier-456' });
    assert.deepEqual(verifyOidcTx(signed), { state: 'state-123', codeVerifier: 'verifier-456' });

    const [payloadB64, sig] = signed.split('.');
    const tamperedPayload = Buffer.from(
      JSON.stringify({ state: 'attacker-state', codeVerifier: 'verifier-456' }),
    ).toString('base64url');
    assert.equal(verifyOidcTx(`${tamperedPayload}.${sig}`), null);
    assert.equal(verifyOidcTx(payloadB64), null);
  });

  it('clears OIDC tx cookie and maps error to allowlisted message on callback ?error=', async () => {
    const app = new Hono();
    app.route('/auth', authRoutes);

    const res = await app.request(
      '/auth/callback?error=access_denied&error_description=Phishing+Link+Evil',
      {
        headers: { Cookie: 'ikeda_oidc_tx=anything' },
      },
    );
    assert.equal(res.status, 302);
    const location = res.headers.get('location') ?? '';
    assert.equal(location.includes('Phishing'), false);
    assert.ok(location.includes('auth_error='));
    const setCookieHeader = res.headers.get('set-cookie') ?? '';
    assert.ok(setCookieHeader.includes('ikeda_oidc_tx='));
  });

  it('rejects default dev KEYCLOAK_CLIENT_SECRET when BFF_PUBLIC_URL is https://, but allows localhost', () => {
    const httpsDefault = envSchema.safeParse({
      BFF_PUBLIC_URL: 'https://bff.kropy.cl',
      KEYCLOAK_CLIENT_SECRET: DEFAULT_DEV_KEYCLOAK_CLIENT_SECRET,
    });
    assert.equal(httpsDefault.success, false);

    const localDefault = envSchema.safeParse({
      BFF_PUBLIC_URL: 'http://localhost:4000',
      KEYCLOAK_CLIENT_SECRET: DEFAULT_DEV_KEYCLOAK_CLIENT_SECRET,
    });
    assert.equal(localDefault.success, true);
  });
});

describe('[BFF-07] Outbound Fetch Timeouts', () => {
  it('maps aborted/timed-out upstream requests to UpstreamApiError 504', async () => {
    globalThis.fetch = ((_input: unknown, init?: RequestInit) =>
      new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener('abort', () => {
          reject(init.signal?.reason ?? Object.assign(new Error('Timed out'), { name: 'TimeoutError' }));
        });
      })) as typeof fetch;

    const client = new ServiceHttpClient({
      serviceName: 'bookings-api',
      baseUrl: 'http://localhost:8084',
    });

    await assert.rejects(
      () => client.request('GET', '/v1/bookings', { timeoutMs: 30 }),
      (err: unknown) => err instanceof UpstreamApiError && err.status === 504,
    );
  });
});

describe('[BFF-08] Booking Orchestration Resilience', () => {
  function mountBookings() {
    const app = new Hono();
    app.use('/api/bookings/*', async (c, next) => {
      c.set('tenantId' as never, TENANT_A_ID as never);
      c.set('actorId' as never, TENANT_A_ID as never);
      c.set('requestId' as never, 'req-booking-test' as never);
      c.set('roles' as never, ['tenant:member', 'bookings:write', 'events:write'] as never);
      c.set('authorization' as never, 'Bearer jwt' as never);
      await next();
    });
    app.route('/api/bookings', bookingsRoutes);
    return app;
  }

  it('returns 422 on malformed JSON instead of throwing 500', async () => {
    const app = mountBookings();
    const res = await app.request('/api/bookings', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: '{invalid-json',
    });
    assert.equal(res.status, 422);
  });

  it('rejects POST /api/bookings with 404 when any requested catalogItemId is missing', async () => {
    const originalGetClient = crmClient.getClient;
    const originalBatch = catalogClient.batchGetItems;
    const originalGetCal = eventsClient.getCalendar;

    const item1 = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
    const item2Missing = 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb';

    crmClient.getClient = async () => ({
      id: TENANT_A_ID,
      tenant_id: TENANT_A_ID,
      keycloak_user_id: null,
      full_name: 'Ana',
      email: 'ana@test.cl',
      phone: null,
      tax_id: null,
      address: null,
      notes: null,
      is_active: true,
      metadata: {},
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      deleted_at: null,
    });
    catalogClient.batchGetItems = async () => [
      {
        id: item1,
        tenant_id: TENANT_A_ID,
        category_id: null,
        sku: 'S1',
        name: 'Corte',
        description: null,
        image_url: null,
        gallery_urls: [],
        base_price: 10000,
        currency: 'CLP',
        tax_rate_bps: 1900,
        tax_included: true,
        item_type: 'SERVICE',
        duration_minutes: 30,
        buffer_minutes: 0,
        is_active: true,
        metadata: {},
        created_at: new Date().toISOString(),
        updated_at: new Date().toISOString(),
        deleted_at: null,
      },
    ];
    eventsClient.getCalendar = async () => ({
      id: TENANT_B_ID,
      tenant_id: TENANT_A_ID,
      name: 'Box 1',
      timezone: 'America/Santiago',
      allow_overlaps: false,
      working_hours: null,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
      deleted_at: null,
    });

    try {
      const app = mountBookings();
      const res = await app.request('/api/bookings', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          clientId: TENANT_A_ID,
          calendarId: TENANT_B_ID,
          catalogItemIds: [item1, item2Missing],
          startsAt: '2026-12-01T15:00:00.000Z',
        }),
      });
      assert.equal(res.status, 404);
    } finally {
      crmClient.getClient = originalGetClient;
      catalogClient.batchGetItems = originalBatch;
      eventsClient.getCalendar = originalGetCal;
    }
  });

  it('cleans up duplicate event on idempotent replay and surfaces slotReleased=false when cancel deleteEvent fails', async () => {
    const originalTransition = bookingsClient.transitionStatus;
    const originalDeleteEvent = eventsClient.deleteEvent;
    const originalGetClient = crmClient.getClient;
    const originalListCals = eventsClient.listCalendars;
    const originalAudit = telemetryClient.emitAuditEventAsync;

    let deleteAttempts = 0;
    bookingsClient.transitionStatus = async () => ({
      id: TENANT_A_ID,
      tenant_id: TENANT_A_ID,
      booking_code: 'BK-001',
      client_id: TENANT_A_ID,
      calendar_id: TENANT_B_ID,
      event_id: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
      starts_at: '2026-12-01T15:00:00.000Z',
      ends_at: '2026-12-01T15:30:00.000Z',
      timezone: 'America/Santiago',
      status: 'CANCELLED',
      items: [],
      subtotal: 10000,
      tax_total: 0,
      total_price: 10000,
      total_duration_minutes: 30,
      currency: 'CLP',
      notes: null,
      cancellation_reason: 'Test',
      cancelled_at: new Date().toISOString(),
      completed_at: null,
      metadata: {},
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString(),
    });
    eventsClient.deleteEvent = async () => {
      deleteAttempts += 1;
      throw new UpstreamApiError('events-api', 503, 'Service Unavailable');
    };
    crmClient.getClient = async () => {
      throw new Error('skip');
    };
    eventsClient.listCalendars = async () => [];
    telemetryClient.emitAuditEventAsync = () => {};

    try {
      const app = mountBookings();
      const res = await app.request(`/api/bookings/${TENANT_A_ID}/status`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ status: 'CANCELLED', cancellationReason: 'Test' }),
      });
      assert.equal(res.status, 200);
      const body = (await res.json()) as { meta?: { slotReleased?: boolean } };
      assert.equal(deleteAttempts, 2); // initial + 1 retry on 5xx
      assert.equal(body.meta?.slotReleased, false);
    } finally {
      bookingsClient.transitionStatus = originalTransition;
      eventsClient.deleteEvent = originalDeleteEvent;
      crmClient.getClient = originalGetClient;
      eventsClient.listCalendars = originalListCals;
      telemetryClient.emitAuditEventAsync = originalAudit;
    }
  });
});

describe('[BFF-09] Public Branding Cache Bounds, TTL & logoUrl Sanitization', () => {
  afterEach(() => {
    clearSlugBrandingCacheForTest();
  });

  it('rejects protocol-relative // and javascript: logoUrls', () => {
    assert.equal(isSafeLogoUrl(''), true);
    assert.equal(isSafeLogoUrl('/assets/logo.png'), true);
    assert.equal(isSafeLogoUrl('https://cdn.kropy.cl/logo.png'), true);
    assert.equal(isSafeLogoUrl('//evil.example/logo.svg'), false);
    assert.equal(isSafeLogoUrl('javascript:alert(1)'), false);
  });

  it('enforces max size and TTL expiration on slugBrandingCache', async () => {
    const branding = {
      displayName: 'Test Clinic',
      logoUrl: '',
      primaryColor: '#059669',
      accentColor: '#0f172a',
      themeMode: 'light' as const,
    };

    for (let i = 0; i < 6; i += 1) {
      cacheTenantBranding(`tenant-${i}`, { ...branding, displayName: `Tenant ${i}` }, { maxSize: 3 });
    }
    assert.equal(getSlugBrandingCacheSize(), 3);

    // Expired entry should fall back to default branding on public route
    cacheTenantBranding('expired-slug', { ...branding, displayName: 'Expired' }, { ttlMs: -1 });
    const app = new Hono();
    app.route('/api/branding', publicBrandingRoutes);
    const res = await app.request('/api/branding/expired-slug');
    const json = (await res.json()) as { data: { branding: { displayName: string } } };
    assert.equal(json.data.branding.displayName, 'Kropy');
  });
});

describe('[BFF-10] Content CMS Routes — Bearer-Only Auth & Strict UUID Validation', () => {
  function mountContentApp() {
    const app = new Hono();
    app.use('/api/content/*', async (c, next) => {
      c.set('tenantId' as never, TENANT_A_ID as never);
      c.set('roles' as never, ['tenant:admin'] as never);
      c.set('authMethod' as never, 'session' as never);
      c.set('authorization' as never, 'Bearer user-content-jwt' as never);
      await next();
    });
    app.route('/api/content', contentRoutes);
    return app;
  }

  it('rejects non-UUID path parameters with 422 before calling content-api', async () => {
    let fetchCalled = false;
    globalThis.fetch = (async () => {
      fetchCalled = true;
      return new Response(JSON.stringify({ success: true, data: {} }), { status: 200 });
    }) as typeof fetch;

    const app = mountContentApp();
    for (const [method, path] of [
      ['GET', '/api/content/articles/not-a-uuid'],
      ['PATCH', '/api/content/articles/not-a-uuid'],
      ['DELETE', '/api/content/articles/not-a-uuid'],
      ['PATCH', '/api/content/categories/bad-id'],
      ['DELETE', '/api/content/tags/bad-id'],
      ['POST', '/api/content/webhooks/bad-id/test'],
    ] as const) {
      const res = await app.request(path, {
        method,
        headers: { 'Content-Type': 'application/json' },
        body: method === 'PATCH' ? JSON.stringify({ title: 'x' }) : undefined,
      });
      assert.equal(res.status, 422, `Expected 422 on ${method} ${path}`);
    }
    assert.equal(fetchCalled, false);
  });

  it('forwards only Bearer token (never X-Api-Key) to content-api', async () => {
    let capturedHeaders: Record<string, string> = {};
    globalThis.fetch = (async (_url: unknown, init?: RequestInit) => {
      capturedHeaders = (init?.headers ?? {}) as Record<string, string>;
      return new Response(
        JSON.stringify({
          success: true,
          data: [],
          pagination: {
            page: 1,
            limit: 20,
            total_items: 0,
            total_pages: 0,
            has_next_page: false,
            has_prev_page: false,
          },
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      );
    }) as typeof fetch;

    const app = mountContentApp();
    const res = await app.request('/api/content/articles');
    assert.equal(res.status, 200);
    const headerKeysLower = Object.keys(capturedHeaders).map((k) => k.toLowerCase());
    assert.equal(capturedHeaders['Authorization'], 'Bearer user-content-jwt');
    assert.equal(headerKeysLower.includes('x-api-key'), false);
  });
});

describe('[BFF-11] Media API Routes — Auth Guard, Bearer-Only & UUID Validation', () => {
  it('blocks unauthenticated requests to /api/media with 401 when protected by requireAuth', async () => {
    const app = new Hono();
    app.use('/api/media/*', requireAuth);
    app.route('/api/media', mediaRoutes);

    const res = await app.request('/api/media');
    assert.equal(res.status, 401);
  });

  it('rejects non-UUID path parameters with 422 before calling media-api', async () => {
    let fetchCalled = false;
    globalThis.fetch = (async () => {
      fetchCalled = true;
      return new Response(JSON.stringify({ success: true, data: {} }), { status: 200 });
    }) as typeof fetch;

    const app = new Hono();
    app.use('/api/media/*', async (c, next) => {
      c.set('tenantId' as never, TENANT_A_ID as never);
      c.set('roles' as never, ['tenant:admin'] as never);
      c.set('authMethod' as never, 'session' as never);
      c.set('authorization' as never, 'Bearer user-media-jwt' as never);
      await next();
    });
    app.route('/api/media', mediaRoutes);

    for (const [method, path] of [
      ['GET', '/api/media/bad-id'],
      ['PATCH', '/api/media/bad-id'],
      ['DELETE', '/api/media/bad-id'],
      ['POST', '/api/media/bad-id/restore'],
    ] as const) {
      const res = await app.request(path, {
        method,
        headers: { 'Content-Type': 'application/json' },
        body: method === 'PATCH' ? JSON.stringify({ alt_text: 'x' }) : undefined,
      });
      assert.equal(res.status, 422, `Expected 422 on ${method} ${path}`);
    }
    assert.equal(fetchCalled, false);
  });

  it('forwards only Bearer token (never X-Api-Key) to media-api', async () => {
    let capturedHeaders: Record<string, string> = {};
    globalThis.fetch = (async (_url: unknown, init?: RequestInit) => {
      capturedHeaders = (init?.headers ?? {}) as Record<string, string>;
      return new Response(
        JSON.stringify({
          success: true,
          data: {
            used_bytes: 0,
            max_bytes: 524288000,
            available_bytes: 524288000,
            max_file_size_bytes: 5242880,
            asset_count: 0,
            usage_percent: 0,
          },
        }),
        { status: 200, headers: { 'Content-Type': 'application/json' } },
      );
    }) as typeof fetch;

    const app = new Hono();
    app.use('/api/media/*', async (c, next) => {
      c.set('tenantId' as never, TENANT_A_ID as never);
      c.set('roles' as never, ['tenant:admin'] as never);
      c.set('authMethod' as never, 'session' as never);
      c.set('authorization' as never, 'Bearer user-media-jwt' as never);
      await next();
    });
    app.route('/api/media', mediaRoutes);

    const res = await app.request('/api/media/quota');
    assert.equal(res.status, 200);
    const headerKeysLower = Object.keys(capturedHeaders).map((k) => k.toLowerCase());
    assert.equal(capturedHeaders['Authorization'], 'Bearer user-media-jwt');
    assert.equal(headerKeysLower.includes('x-api-key'), false);
  });
});

describe('[BFF-12] Lightweight Session RBAC Pre-Validation', () => {
  function mountWithRoles(roles: string[]) {
    const app = new Hono();
    app.use('/api/*', async (c, next) => {
      c.set('tenantId' as never, TENANT_A_ID as never);
      c.set('roles' as never, roles as never);
      c.set('authMethod' as never, 'session' as never);
      c.set('authorization' as never, 'Bearer user-rbac-jwt' as never);
      await next();
    });
    app.route('/api/identity', identityRoutes);
    app.route('/api/content', contentRoutes);
    app.route('/api/media', mediaRoutes);
    return app;
  }

  it('blocks staff from listing identity users, publishing/deleting articles, managing webhooks, uploading branding media, or deleting media', async () => {
    let fetchCalled = false;
    globalThis.fetch = (async () => {
      fetchCalled = true;
      return new Response(JSON.stringify({ success: true, data: {} }), { status: 200 });
    }) as typeof fetch;

    const app = mountWithRoles([
      'tenant:member',
      'content:read',
      'content:write',
      'media:read',
      'media:write',
    ]);

    const cases: Array<[string, string, unknown?]> = [
      ['GET', '/api/identity/users'],
      ['GET', '/api/content/webhooks'],
      ['PATCH', `/api/content/articles/${TENANT_A_ID}/status`, { status: 'PUBLISHED' }],
      ['DELETE', `/api/content/articles/${TENANT_A_ID}`],
      [
        'POST',
        '/api/media/presign',
        {
          file_name: 'logo.png',
          mime_type: 'image/png',
          size_bytes: 1024,
          context: 'branding',
        },
      ],
      ['DELETE', `/api/media/${TENANT_A_ID}`],
    ];

    for (const [method, path, body] of cases) {
      const res = await app.request(path, {
        method,
        headers: body ? { 'Content-Type': 'application/json' } : undefined,
        body: body ? JSON.stringify(body) : undefined,
      });
      assert.equal(res.status, 403, `Expected 403 on ${method} ${path}`);
    }
    assert.equal(fetchCalled, false);
  });
});

describe('[BFF-13] Catalog Items — Image URL & Gallery Validation', () => {
  function mountItemsApp() {
    const app = new Hono();
    app.use('/api/items/*', async (c, next) => {
      c.set('tenantId' as never, TENANT_A_ID as never);
      c.set('roles' as never, ['tenant:admin'] as never);
      c.set('authMethod' as never, 'session' as never);
      c.set('authorization' as never, 'Bearer user-catalog-jwt' as never);
      await next();
    });
    app.route('/api/items', itemsRoutes);
    return app;
  }

  it('rejects unsafe image_url and gallery_urls (javascript:, data:image/svg+xml, //...) on POST and PATCH before calling catalog-api', async () => {
    let fetchCalled = false;
    globalThis.fetch = (async () => {
      fetchCalled = true;
      return new Response(JSON.stringify({ success: true, data: {} }), { status: 200 });
    }) as typeof fetch;

    const app = mountItemsApp();
    const unsafeUrls = [
      'javascript:alert(1)',
      'data:image/svg+xml,<svg onload=alert(1)>',
      '//evil.example/img.png',
    ];

    for (const badUrl of unsafeUrls) {
      const postCover = await app.request('/api/items', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: 'Producto Malicioso',
          item_type: 'PRODUCT',
          base_price: 1000,
          image_url: badUrl,
        }),
      });
      assert.equal(postCover.status, 400, `Expected 400 on POST image_url=${badUrl}`);

      const postGallery = await app.request('/api/items', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          name: 'Producto Galería Maliciosa',
          item_type: 'PRODUCT',
          base_price: 1000,
          gallery_urls: ['https://cdn.kropy.cl/ok.webp', badUrl],
        }),
      });
      assert.equal(postGallery.status, 400, `Expected 400 on POST gallery_urls=${badUrl}`);

      const patchCover = await app.request(`/api/items/${TENANT_A_ID}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ image_url: badUrl }),
      });
      assert.equal(patchCover.status, 400, `Expected 400 on PATCH image_url=${badUrl}`);

      const patchGallery = await app.request(`/api/items/${TENANT_A_ID}`, {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ gallery_urls: [badUrl] }),
      });
      assert.equal(patchGallery.status, 400, `Expected 400 on PATCH gallery_urls=${badUrl}`);
    }

    assert.equal(fetchCalled, false);
  });
});
