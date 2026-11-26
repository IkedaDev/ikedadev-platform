import assert from 'node:assert/strict';
import { afterEach, describe, it } from 'node:test';
import { Hono } from 'hono';
import { ServiceHttpClient } from './clients/http.js';
import { identityClient } from './clients/identityClient.js';
import { telemetryClient } from './clients/telemetryClient.js';
import { config } from './config.js';
import { identityRoutes } from './routes/identity.js';
import { settingsRoutes } from './routes/settings.js';

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
