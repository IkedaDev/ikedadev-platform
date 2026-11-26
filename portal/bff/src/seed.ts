import 'dotenv/config';
import {
  ServiceHttpClient,
  UpstreamApiError,
  type RequestContext,
  type ServiceRequestOptions,
} from './clients/http.js';
import { config } from './config.js';
import type {
  ApiKeySummary,
  BookingRecord,
  BookingStatus,
  Calendar,
  CatalogItem,
  Category,
  Client,
  CreateApiKeyCommand,
  CreateBookingPayload,
  CreateCalendarCommand,
  CreateCatalogItemCommand,
  CreateCategoryCommand,
  CreateClientCommand,
  CreateEventCommand,
  CreateTenantUserCommand,
  CreatedApiKey,
  Criteria,
  EventRecord,
  IngestBatchResponse,
  IngestTelemetryEventPayload,
  PaginatedApiResponse,
  Tenant,
  TenantUser,
  TransitionBookingStatusPayload,
  UpdateTenantCommand,
} from './types/platform.js';

// ============================================================================
// Constants & Environment
// ============================================================================

const DEMO_TENANT_ID = '11111111-1111-4111-8111-111111111111';
const DEMO_TENANT_SLUG = 'demo-tenant';
const BARBERIA_SLUG = 'barberia-norte';

const DEFAULT_DEV_API_KEY = 'ik_live_dev_tenant_portal_local_2026';
const KEYCLOAK_BASE = config.KEYCLOAK_INTERNAL_URL.replace(/\/+$/, '');
const KEYCLOAK_REALM = config.KEYCLOAK_REALM;
const SKIP_KEYCLOAK = process.env.SKIP_KEYCLOAK === 'true';

const ALL_TENANT_ADMIN_ROLES = [
  'tenant:admin',
  'events:write',
  'crm:write',
  'catalog:write',
  'identity:write',
  'bookings:write',
  'telemetry:read',
  'telemetry:write',
];

const TELEMETRY_ROLES = ['telemetry:read', 'telemetry:write'];

type AuthOptions = Pick<ServiceRequestOptions, 'ctx' | 'apiKeyOverride'>;

// ============================================================================
// Upstream HTTP Clients (all 6 Rust microservices)
// ============================================================================

const identityHttp = new ServiceHttpClient({
  serviceName: 'identity-api',
  baseUrl: config.IDENTITY_API_URL,
});
const eventsHttp = new ServiceHttpClient({
  serviceName: 'events-api',
  baseUrl: config.EVENTS_API_URL,
});
const catalogHttp = new ServiceHttpClient({
  serviceName: 'catalog-api',
  baseUrl: config.CATALOG_API_URL,
});
const crmHttp = new ServiceHttpClient({
  serviceName: 'crm-api',
  baseUrl: config.CRM_API_URL,
});
const bookingsHttp = new ServiceHttpClient({
  serviceName: 'bookings-api',
  baseUrl: config.BOOKINGS_API_URL,
});
const telemetryHttp = new ServiceHttpClient({
  serviceName: 'telemetry-api',
  baseUrl: config.TELEMETRY_API_URL,
});

// ============================================================================
// Keycloak Admin & OIDC Helpers
// ============================================================================

interface KeycloakRoleRep {
  id: string;
  name: string;
  description?: string;
}

interface KeycloakUserRep {
  id: string;
  username: string;
  email?: string;
  firstName?: string;
  lastName?: string;
  enabled?: boolean;
  attributes?: Record<string, string[]>;
}

async function fetchKeycloakToken(
  realm: string,
  params: Record<string, string>,
): Promise<string | null> {
  try {
    const res = await fetch(
      `${KEYCLOAK_BASE}/realms/${realm}/protocol/openid-connect/token`,
      {
        method: 'POST',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
        body: new URLSearchParams(params).toString(),
      },
    );
    if (!res.ok) return null;
    const data = (await res.json()) as { access_token?: string };
    return data.access_token ?? null;
  } catch {
    return null;
  }
}

async function getKeycloakAdminToken(): Promise<string | null> {
  const adminUser = process.env.KEYCLOAK_ADMIN_USER || 'admin';
  const adminPassword = process.env.KEYCLOAK_ADMIN_PASSWORD || 'admin_dev_password';

  // 1. Try master realm admin-cli password grant
  const masterToken = await fetchKeycloakToken('master', {
    grant_type: 'password',
    client_id: 'admin-cli',
    username: adminUser,
    password: adminPassword,
  });
  if (masterToken) return masterToken;

  // 2. Fallback to identity-api-admin service account client credentials
  const clientId = process.env.KEYCLOAK_ADMIN_CLIENT_ID || 'identity-api-admin';
  const clientSecret =
    process.env.KEYCLOAK_ADMIN_CLIENT_SECRET || 'ikeda-identity-admin-secret-2026';

  return fetchKeycloakToken(KEYCLOAK_REALM, {
    grant_type: 'client_credentials',
    client_id: clientId,
    client_secret: clientSecret,
  });
}

async function getUserPasswordGrantToken(
  username: string,
  password: string,
): Promise<string | null> {
  return fetchKeycloakToken(KEYCLOAK_REALM, {
    grant_type: 'password',
    client_id: config.KEYCLOAK_CLIENT_ID,
    client_secret: config.KEYCLOAK_CLIENT_SECRET,
    username,
    password,
    scope: 'openid profile email',
  });
}

async function kcAdminFetch(
  adminToken: string,
  path: string,
  init?: RequestInit,
): Promise<Response> {
  return fetch(`${KEYCLOAK_BASE}/admin/realms/${KEYCLOAK_REALM}${path}`, {
    ...init,
    headers: {
      Authorization: `Bearer ${adminToken}`,
      Accept: 'application/json',
      ...(init?.body ? { 'Content-Type': 'application/json' } : {}),
      ...(init?.headers ?? {}),
    },
  });
}

async function ensureRealmRole(
  adminToken: string,
  roleName: string,
  description: string,
): Promise<KeycloakRoleRep | null> {
  const getRes = await kcAdminFetch(adminToken, `/roles/${encodeURIComponent(roleName)}`);
  if (getRes.ok) {
    return (await getRes.json()) as KeycloakRoleRep;
  }

  const createRes = await kcAdminFetch(adminToken, '/roles', {
    method: 'POST',
    body: JSON.stringify({ name: roleName, description }),
  });
  if (createRes.ok || createRes.status === 409) {
    const refetch = await kcAdminFetch(adminToken, `/roles/${encodeURIComponent(roleName)}`);
    if (refetch.ok) {
      return (await refetch.json()) as KeycloakRoleRep;
    }
  }
  return null;
}

async function findKeycloakUserByEmail(
  adminToken: string,
  email: string,
): Promise<KeycloakUserRep | null> {
  const res = await kcAdminFetch(
    adminToken,
    `/users?email=${encodeURIComponent(email)}&exact=true`,
  );
  if (!res.ok) return null;
  const users = (await res.json()) as KeycloakUserRep[];
  return users[0] ?? null;
}

async function assignRealmRolesToUser(
  adminToken: string,
  userId: string,
  roleNames: string[],
): Promise<string[]> {
  const existingRes = await kcAdminFetch(
    adminToken,
    `/users/${userId}/role-mappings/realm`,
  );
  const existing = existingRes.ok
    ? ((await existingRes.json()) as KeycloakRoleRep[])
    : [];
  const existingNames = new Set(existing.map((r) => r.name));

  const missingNames = roleNames.filter((name) => !existingNames.has(name));
  if (missingNames.length === 0) return [];

  const roleReps: KeycloakRoleRep[] = [];
  for (const name of missingNames) {
    const rep = await ensureRealmRole(adminToken, name, `Role ${name}`);
    if (rep) roleReps.push(rep);
  }

  if (roleReps.length > 0) {
    await kcAdminFetch(adminToken, `/users/${userId}/role-mappings/realm`, {
      method: 'POST',
      body: JSON.stringify(roleReps),
    });
  }

  return roleReps.map((r) => r.name);
}

async function ensureKeycloakUser(
  adminToken: string,
  spec: {
    email: string;
    firstName: string;
    lastName: string;
    password: string;
    tenantId: string;
    roles: string[];
  },
): Promise<KeycloakUserRep | null> {
  let user = await findKeycloakUserByEmail(adminToken, spec.email);

  if (!user) {
    const createRes = await kcAdminFetch(adminToken, '/users', {
      method: 'POST',
      body: JSON.stringify({
        username: spec.email,
        email: spec.email,
        emailVerified: true,
        enabled: true,
        firstName: spec.firstName,
        lastName: spec.lastName,
        attributes: {
          tenant_id: [spec.tenantId],
        },
        credentials: [
          {
            type: 'password',
            value: spec.password,
            temporary: false,
          },
        ],
      }),
    });

    if (!createRes.ok && createRes.status !== 409) {
      const errText = await createRes.text().catch(() => '');
      console.warn(
        `  ⚠ No se pudo crear usuario Keycloak ${spec.email} (${createRes.status}): ${errText}`,
      );
      return null;
    }

    user = await findKeycloakUserByEmail(adminToken, spec.email);
    if (user) {
      console.log(`  ✓ Usuario Keycloak creado: ${spec.email} (${user.id})`);
    }
  } else {
    const currentTenantAttr = user.attributes?.tenant_id?.[0];
    if (currentTenantAttr !== spec.tenantId) {
      const updateRes = await kcAdminFetch(adminToken, `/users/${user.id}`, {
        method: 'PUT',
        body: JSON.stringify({
          username: user.username ?? spec.email,
          email: user.email ?? spec.email,
          emailVerified: true,
          enabled: true,
          firstName: spec.firstName,
          lastName: spec.lastName,
          attributes: {
            ...(user.attributes ?? {}),
            tenant_id: [spec.tenantId],
          },
        }),
      });
      if (updateRes.ok) {
        console.log(
          `  ✓ Atributo tenant_id sincronizado en Keycloak para ${spec.email} -> ${spec.tenantId}`,
        );
      } else {
        const errText = await updateRes.text().catch(() => '');
        console.warn(
          `  ⚠ No se pudo sincronizar tenant_id para ${spec.email} (${updateRes.status}): ${errText}`,
        );
      }
    } else {
      console.log(`  ↺ Usuario Keycloak existente: ${spec.email} (${user.id})`);
    }
  }

  if (user) {
    const added = await assignRealmRolesToUser(adminToken, user.id, spec.roles);
    if (added.length > 0) {
      console.log(`  ✓ Roles asignados a ${spec.email}: ${added.join(', ')}`);
    }
  }

  return user;
}

async function step0SyncKeycloak(): Promise<string | null> {
  console.log('🔐 [Paso 0] Sincronizando Keycloak (roles telemetry:read / telemetry:write)...');
  if (SKIP_KEYCLOAK) {
    console.log('  ⏭ SKIP_KEYCLOAK=true — omitiendo sincronización de Keycloak.');
    return null;
  }

  const adminToken = await getKeycloakAdminToken();
  if (!adminToken) {
    console.log(
      `  ⚠ Keycloak no accesible en ${KEYCLOAK_BASE} o credenciales inválidas. Continuando sin sincronizar Keycloak.`,
    );
    return null;
  }

  for (const roleName of TELEMETRY_ROLES) {
    const role = await ensureRealmRole(
      adminToken,
      roleName,
      roleName === 'telemetry:read'
        ? 'Read access to telemetry-api'
        : 'Write access to telemetry-api',
    );
    if (role) {
      console.log(`  ✓ Rol de realm asegurado: ${role.name}`);
    }
  }

  for (const email of ['admin@demo.ikedadev.com', 'staff@demo.ikedadev.com']) {
    const user = await findKeycloakUserByEmail(adminToken, email);
    if (!user) {
      console.log(`  ⚠ Usuario ${email} no encontrado en Keycloak, omitiendo roles.`);
      continue;
    }
    const added = await assignRealmRolesToUser(adminToken, user.id, TELEMETRY_ROLES);
    if (added.length > 0) {
      console.log(`  ✓ Roles agregados a ${email}: ${added.join(', ')}`);
    } else {
      console.log(`  ↺ ${email} ya cuenta con ${TELEMETRY_ROLES.join(', ')}`);
    }
  }

  return adminToken;
}

async function resolvePlatformAdminAuth(): Promise<AuthOptions> {
  const explicitKey =
    process.env.PLATFORM_ADMIN_API_KEY?.trim() || process.env.TENANT_API_KEY?.trim();

  if (explicitKey) {
    console.log('  🔑 Usando API Key desde variable de entorno para autenticación platform-admin.');
    return { apiKeyOverride: explicitKey };
  }

  // Try default local dev API key first
  try {
    await identityHttp.requestData<Tenant>('GET', `/v1/tenants/${DEMO_TENANT_ID}`, {
      apiKeyOverride: DEFAULT_DEV_API_KEY,
    });
    console.log('  🔑 Autenticado con API Key de desarrollo local (demo-tenant / platform:admin).');
    return { apiKeyOverride: DEFAULT_DEV_API_KEY };
  } catch {
    // Fallback to Keycloak password grant for admin@demo.ikedadev.com
  }

  if (!SKIP_KEYCLOAK) {
    const jwt = await getUserPasswordGrantToken('admin@demo.ikedadev.com', 'Admin123!');
    if (jwt) {
      console.log('  🔑 Autenticado vía Keycloak password grant (admin@demo.ikedadev.com).');
      const ctx: RequestContext = { authorization: `Bearer ${jwt}` };
      return { ctx };
    }
  }

  throw new Error(
    'No se pudo obtener autenticación platform-admin. Define PLATFORM_ADMIN_API_KEY / TENANT_API_KEY o asegúrate de que Keycloak / identity-api estén corriendo.',
  );
}

// ============================================================================
// Shared Seeding Helpers (Tenant-Agnostic & Idempotent)
// ============================================================================

function buildUtcDate(daysFromToday: number, hourUtc: number, minuteUtc = 0): Date {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() + daysFromToday);
  d.setUTCHours(hourUtc, minuteUtc, 0, 0);
  return d;
}

async function ensureCalendars(
  auth: AuthOptions,
  specs: CreateCalendarCommand[],
): Promise<Calendar[]> {
  const existing = await eventsHttp.requestData<Calendar[]>('GET', '/v1/calendars', auth);
  const result: Calendar[] = [];

  for (const spec of specs) {
    const found = existing.find((c) => c.name === spec.name);
    if (found) {
      console.log(`  ↺ Calendario existente: ${found.name} (${found.id})`);
      result.push(found);
      continue;
    }
    const created = await eventsHttp.requestData<Calendar>('POST', '/v1/calendars', {
      ...auth,
      body: spec,
    });
    console.log(`  ✓ Calendario creado: ${created.name} (${created.id})`);
    result.push(created);
  }

  return result;
}

async function ensureCategories(
  auth: AuthOptions,
  specs: CreateCategoryCommand[],
): Promise<Map<string, Category>> {
  const existing = await catalogHttp.requestData<Category[]>('GET', '/v1/categories', auth);
  const map = new Map<string, Category>();

  for (const spec of specs) {
    const found = existing.find((c) => c.slug === spec.slug);
    if (found) {
      console.log(`  ↺ Categoría existente: ${found.name} (${found.slug})`);
      map.set(spec.slug, found);
      continue;
    }
    const created = await catalogHttp.requestData<Category>('POST', '/v1/categories', {
      ...auth,
      body: spec,
    });
    console.log(`  ✓ Categoría creada: ${created.name} (${created.slug})`);
    map.set(spec.slug, created);
  }

  return map;
}

async function ensureCatalogItems(
  auth: AuthOptions,
  specs: CreateCatalogItemCommand[],
): Promise<CatalogItem[]> {
  const criteria: Criteria = { pagination: { page: 1, limit: 100 } };
  const existingPage = await catalogHttp.request<PaginatedApiResponse<CatalogItem>>(
    'POST',
    '/v1/items/search',
    { ...auth, body: criteria },
  );
  const bySku = new Map(
    existingPage.data.filter((i) => i.sku).map((i) => [i.sku!, i]),
  );

  const items: CatalogItem[] = [];
  for (const spec of specs) {
    const found = spec.sku ? bySku.get(spec.sku) : undefined;
    if (found) {
      console.log(`  ↺ Ítem existente: ${found.name} (${found.sku})`);
      items.push(found);
      continue;
    }
    const created = await catalogHttp.requestData<CatalogItem>('POST', '/v1/items', {
      ...auth,
      body: spec,
    });
    console.log(
      `  ✓ Ítem creado: ${created.name} (${created.item_type} · ${created.base_price} ${created.currency})`,
    );
    items.push(created);
  }

  return items;
}

async function ensureClients(
  auth: AuthOptions,
  specs: CreateClientCommand[],
): Promise<Client[]> {
  const criteria: Criteria = { pagination: { page: 1, limit: 100 } };
  const existingPage = await crmHttp.request<PaginatedApiResponse<Client>>(
    'POST',
    '/v1/clients/search',
    { ...auth, body: criteria },
  );
  const byEmail = new Map(
    existingPage.data.filter((c) => c.email).map((c) => [c.email!.toLowerCase(), c]),
  );

  const clients: Client[] = [];
  for (const spec of specs) {
    const found = spec.email ? byEmail.get(spec.email.toLowerCase()) : undefined;
    if (found) {
      console.log(`  ↺ Cliente existente: ${found.full_name} (${found.email})`);
      clients.push(found);
      continue;
    }
    const created = await crmHttp.requestData<Client>('POST', '/v1/clients', {
      ...auth,
      body: spec,
    });
    console.log(`  ✓ Cliente creado: ${created.full_name} (${created.email ?? 'sin email'})`);
    clients.push(created);
  }

  return clients;
}

interface BookingSeedSpec {
  calendar: Calendar;
  client: Client;
  selectedItems: CatalogItem[];
  daysOffset: number;
  hourUtc: number;
  targetStatus: BookingStatus;
  notes: string;
  cancellationReason?: string;
}

async function ensureBookings(
  auth: AuthOptions,
  specs: BookingSeedSpec[],
): Promise<void> {
  for (const spec of specs) {
    const start = buildUtcDate(spec.daysOffset, spec.hourUtc, 0);
    const totalDurationMinutes =
      spec.selectedItems.reduce(
        (sum, item) =>
          sum +
          (item.item_type === 'SERVICE'
            ? item.duration_minutes + (item.buffer_minutes ?? 0)
            : 0),
        0,
      ) || 45;
    const currency = spec.selectedItems[0]?.currency ?? 'CLP';
    const end = new Date(start.getTime() + totalDurationMinutes * 60 * 1000);
    const initialStatus: 'PENDING' | 'SCHEDULED' =
      spec.targetStatus === 'PENDING' ? 'PENDING' : 'SCHEDULED';

    try {
      const eventCmd: CreateEventCommand = {
        calendar_id: spec.calendar.id,
        title: `${spec.client.full_name} — ${spec.selectedItems.map((i) => i.name).join(' + ')}`,
        description: spec.notes,
        starts_at: start.toISOString(),
        ends_at: end.toISOString(),
        timezone: spec.calendar.timezone || 'America/Santiago',
        status: spec.targetStatus === 'CANCELLED' ? 'CANCELLED' : 'CONFIRMED',
        external_resource_id: spec.client.id,
        metadata: { source: 'seed.ts', client_id: spec.client.id },
      };

      const event = await eventsHttp.requestData<EventRecord>('POST', '/v1/events', {
        ...auth,
        body: eventCmd,
      });

      try {
        const bookingCmd: CreateBookingPayload = {
          client_id: spec.client.id,
          calendar_id: spec.calendar.id,
          event_id: event.id,
          starts_at: start.toISOString(),
          ends_at: end.toISOString(),
          timezone: spec.calendar.timezone || 'America/Santiago',
          status: initialStatus,
          currency,
          notes: spec.notes,
          metadata: { source: 'seed.ts' },
          items: spec.selectedItems.map((item) => ({
            catalog_item_id: item.id,
            variant_id: null,
            name_snapshot: item.name,
            sku_snapshot: item.sku ?? null,
            quantity: 1,
            unit_price_snapshot: item.base_price,
            tax_rate_bps: item.tax_rate_bps,
            tax_included: item.tax_included,
            duration_minutes_snapshot: item.item_type === 'SERVICE' ? item.duration_minutes : 0,
            buffer_minutes_snapshot:
              item.item_type === 'SERVICE' ? (item.buffer_minutes ?? 0) : 0,
          })),
        };

        let booking = await bookingsHttp.requestData<BookingRecord>('POST', '/v1/bookings', {
          ...auth,
          body: bookingCmd,
        });

        if (spec.targetStatus !== initialStatus) {
          const transition: TransitionBookingStatusPayload = {
            status: spec.targetStatus,
            cancellation_reason: spec.cancellationReason ?? null,
          };
          booking = await bookingsHttp.requestData<BookingRecord>(
            'PATCH',
            `/v1/bookings/${booking.id}/status`,
            { ...auth, body: transition },
          );
        }

        console.log(
          `  ✓ Reserva creada: ${booking.booking_code} [${booking.status}] (${spec.client.full_name}, ${start.toISOString()})`,
        );
      } catch (bookingErr) {
        await eventsHttp
          .request<void>('DELETE', `/v1/events/${event.id}`, auth)
          .catch(() => undefined);
        throw bookingErr;
      }
    } catch (err) {
      if (err instanceof UpstreamApiError && err.status === 409) {
        console.log(
          `  ↺ Bloque ocupado ${start.toISOString()} en ${spec.calendar.name} (409 idempotente, omitiendo).`,
        );
      } else {
        throw err;
      }
    }
  }
}

async function ensureTelemetryBatch(
  auth: AuthOptions,
  events: IngestTelemetryEventPayload[],
  label: string,
): Promise<void> {
  const existing = await telemetryHttp.request<PaginatedApiResponse<unknown>>(
    'POST',
    '/v1/events/search',
    { ...auth, body: { pagination: { page: 1, limit: 1 } } satisfies Criteria },
  );

  if (existing.pagination.total_items > 0) {
    console.log(
      `  ↺ Telemetría ya poblada para ${label} (${existing.pagination.total_items} eventos existentes), omitiendo.`,
    );
    return;
  }

  const res = await telemetryHttp.requestData<IngestBatchResponse>('POST', '/v1/events/batch', {
    ...auth,
    body: { events },
  });
  console.log(`  ✓ Telemetría poblada para ${label}: ${res.accepted} eventos ingeridos.`);
}

// ============================================================================
// Tenant 1: demo-tenant ("Clínica Kropy Estética", #059669)
// ============================================================================

async function seedDemoTenant(auth: AuthOptions): Promise<void> {
  console.log('\n🏥 [Tenant 1] Poblando demo-tenant ("Clínica Kropy Estética", #059669)...');

  // 1.1 Branding & Settings via PATCH /v1/tenants/:id
  const tenantUpdate: UpdateTenantCommand = {
    name: 'Clínica Kropy Estética',
    config: {
      branding: {
        displayName: 'Clínica Kropy Estética',
        logoUrl: '',
        primaryColor: '#059669',
        accentColor: '#0f172a',
        themeMode: 'light',
      },
      locale: {
        currency: 'CLP',
        timezone: 'America/Santiago',
        language: 'es',
      },
      features: {
        crm: true,
        catalog: true,
        events: true,
        bookings: true,
        telemetry: true,
      },
    },
  };
  await identityHttp.requestData<Tenant>('PATCH', `/v1/tenants/${DEMO_TENANT_ID}`, {
    ...auth,
    body: tenantUpdate,
  });
  console.log('  ✓ Branding & ajustes actualizados en identity-api (#059669, CLP, America/Santiago)');

  // 1.2 Team users via identity-api (provisions in Keycloak + identity_db)
  try {
    const existingUsers = await identityHttp.requestData<TenantUser[]>('GET', '/v1/users', auth);
    const existingEmails = new Set(existingUsers.map((u) => u.email.toLowerCase()));

    const extraUsers: CreateTenantUserCommand[] = [
      {
        email: 'dra.sofia@demo.ikedadev.com',
        full_name: 'Dra. Sofía Valenzuela',
        temporary_password: 'Sofia123!',
        roles: [
          'tenant:member',
          'events:write',
          'crm:write',
          'catalog:write',
          'bookings:write',
          'telemetry:read',
          'telemetry:write',
        ],
        send_invite_email: false,
      },
      {
        email: 'recepcion@demo.ikedadev.com',
        full_name: 'Camila Recepción',
        temporary_password: 'Recepcion123!',
        roles: [
          'tenant:member',
          'events:write',
          'crm:write',
          'bookings:write',
          'telemetry:read',
          'telemetry:write',
        ],
        send_invite_email: false,
      },
    ];

    for (const userCmd of extraUsers) {
      if (existingEmails.has(userCmd.email.toLowerCase())) {
        console.log(`  ↺ Usuario de equipo existente: ${userCmd.email}`);
        continue;
      }
      try {
        const created = await identityHttp.requestData<TenantUser>('POST', '/v1/users', {
          ...auth,
          body: userCmd,
        });
        console.log(`  ✓ Usuario de equipo creado: ${created.full_name} (${created.email})`);
      } catch (err) {
        if (err instanceof UpstreamApiError && (err.status === 409 || err.status === 502 || err.status === 503)) {
          console.log(`  ⚠ No se pudo provisionar ${userCmd.email} (${err.status}): ${err.message}`);
        } else {
          throw err;
        }
      }
    }
  } catch (err) {
    const reason = err instanceof Error ? err.message : String(err);
    console.log(`  ⚠ Omitiendo sincronización de usuarios de equipo: ${reason}`);
  }

  // 1.3 Sample integration API key
  const existingKeys = await identityHttp.requestData<ApiKeySummary[]>('GET', '/v1/api-keys', auth);
  const sampleKeyName = 'Integración Web Reservas';
  if (existingKeys.some((k) => k.name === sampleKeyName && k.is_active)) {
    console.log(`  ↺ API Key existente: "${sampleKeyName}"`);
  } else {
    const keyCmd: CreateApiKeyCommand = {
      name: sampleKeyName,
      scopes: ['events:write', 'bookings:write', 'crm:write', 'catalog:write'],
    };
    const createdKey = await identityHttp.requestData<CreatedApiKey>('POST', '/v1/api-keys', {
      ...auth,
      body: keyCmd,
    });
    console.log(`  ✓ API Key creada: "${createdKey.key.name}" (${createdKey.key.key_prefix}...)`);
  }

  // 1.4 Calendars (2)
  const defaultWorkingHours = {
    monday: [{ start: '09:00', end: '19:00' }],
    tuesday: [{ start: '09:00', end: '19:00' }],
    wednesday: [{ start: '09:00', end: '19:00' }],
    thursday: [{ start: '09:00', end: '19:00' }],
    friday: [{ start: '09:00', end: '18:00' }],
    saturday: [{ start: '10:00', end: '14:00' }],
  };

  const calendars = await ensureCalendars(auth, [
    {
      name: 'Box Atención 1 — Estética',
      timezone: 'America/Santiago',
      allow_overlaps: false,
      working_hours: defaultWorkingHours,
    },
    {
      name: 'Box Atención 2 — Evaluaciones',
      timezone: 'America/Santiago',
      allow_overlaps: false,
      working_hours: defaultWorkingHours,
    },
  ]);

  // 1.5 Categories (3)
  const categories = await ensureCategories(auth, [
    {
      name: 'Tratamientos Faciales',
      slug: 'tratamientos-faciales',
      description: 'Limpiezas profundas, hidratación avanzada y protocolos dermatológicos.',
      display_order: 1,
      is_active: true,
    },
    {
      name: 'Consultas y Evaluaciones',
      slug: 'consultas-evaluaciones',
      description: 'Diagnóstico facial, controles y planificación de tratamientos.',
      display_order: 2,
      is_active: true,
    },
    {
      name: 'Productos de Cuidado',
      slug: 'productos-cuidado',
      description: 'Dermocosmética profesional para rutina en casa.',
      display_order: 3,
      is_active: true,
    },
  ]);

  const catFaciales = categories.get('tratamientos-faciales')!;
  const catConsultas = categories.get('consultas-evaluaciones')!;
  const catProductos = categories.get('productos-cuidado')!;

  // 1.6 Catalog Items (4 SERVICEs + 2 PRODUCTs + 1 BUNDLE = 7)
  const baseItems = await ensureCatalogItems(auth, [
    {
      sku: 'KRP-SRV-EVAL',
      name: 'Evaluación Facial con Diagnóstico 3D',
      description: 'Análisis completo de piel y plan de tratamiento personalizado.',
      category_id: catConsultas.id,
      base_price: 25000,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'SERVICE',
      duration_minutes: 30,
      buffer_minutes: 10,
    },
    {
      sku: 'KRP-SRV-LIMP',
      name: 'Limpieza Facial Profunda Hydra',
      description: 'Exfoliación ultrasónica, extracción suave y mascarilla calmante.',
      category_id: catFaciales.id,
      base_price: 49900,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'SERVICE',
      duration_minutes: 60,
      buffer_minutes: 15,
    },
    {
      sku: 'KRP-SRV-PEEL',
      name: 'Peeling Químico Renovador',
      description: 'Renovación celular controlada para luminosidad y textura uniforme.',
      category_id: catFaciales.id,
      base_price: 58000,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'SERVICE',
      duration_minutes: 45,
      buffer_minutes: 15,
    },
    {
      sku: 'KRP-SRV-CTRL',
      name: 'Control Post-Tratamiento',
      description: 'Seguimiento clínico y ajuste de rutina dermocosmética.',
      category_id: catConsultas.id,
      base_price: 18000,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'SERVICE',
      duration_minutes: 25,
      buffer_minutes: 5,
    },
    {
      sku: 'KRP-PRD-SERUM',
      name: 'Sérum Vitamina C 15% (30ml)',
      description: 'Antioxidante iluminador de uso diario.',
      category_id: catProductos.id,
      base_price: 32900,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'PRODUCT',
      unit_of_measure: 'UNIT',
      track_inventory: true,
    },
    {
      sku: 'KRP-PRD-SPF',
      name: 'Protector Solar Toque Seco SPF 50+',
      description: 'Protección UVA/UVB de amplio espectro sin brillo.',
      category_id: catProductos.id,
      base_price: 21500,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'PRODUCT',
      unit_of_measure: 'UNIT',
      track_inventory: true,
    },
  ]);

  const evalService = baseItems[0];
  const limpService = baseItems[1];
  const peelService = baseItems[2];
  const ctrlService = baseItems[3];

  const bundleItems = await ensureCatalogItems(auth, [
    {
      sku: 'KRP-BND-GLOW',
      name: 'Pack Glow Completo (Evaluación + Limpieza Hydra)',
      description: 'Experiencia integral de diagnóstico 3D más limpieza profunda.',
      category_id: catFaciales.id,
      base_price: 67900,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'BUNDLE',
      items: [
        { item_id: evalService.id, quantity: 1 },
        { item_id: limpService.id, quantity: 1 },
      ],
    },
  ]);

  const allItems = [...baseItems, ...bundleItems];

  // 1.7 Clients (8 realistic Chilean clients)
  const clients = await ensureClients(auth, [
    {
      full_name: 'Camila Rojas Valdés',
      email: 'camila.rojas@ejemplo.cl',
      phone: '+56 9 8123 4567',
      tax_id: '18.456.789-2',
      address: 'Av. Providencia 1234, Providencia',
      notes: 'Cliente frecuente, prefiere horarios de mañana.',
      metadata: { Segmento: 'VIP', Canal: 'WhatsApp', Preferencia: 'Mañanas' },
    },
    {
      full_name: 'Matías Fernández Soto',
      email: 'matias.fernandez@ejemplo.cl',
      phone: '+56 9 7654 3210',
      tax_id: '16.987.654-K',
      address: 'Av. Apoquindo 4500, Las Condes',
      notes: 'Referido por Instagram, piel sensible.',
      metadata: { Segmento: 'Regular', Canal: 'Instagram', Piel: 'Sensible' },
    },
    {
      full_name: 'Valentina Muñoz Paredes',
      email: 'valentina.munoz@ejemplo.cl',
      phone: '+56 9 6543 2109',
      tax_id: '19.112.233-4',
      address: 'Irarrázaval 2890, Ñuñoa',
      notes: 'Solicita boleta electrónica por correo.',
      metadata: { Segmento: 'VIP', Canal: 'Web', Facturación: 'Boleta' },
    },
    {
      full_name: 'Sebastián Araya Fuentes',
      email: 'sebastian.araya@ejemplo.cl',
      phone: '+56 9 9123 8877',
      tax_id: '17.345.120-8',
      address: 'Vitacura 3100, Vitacura',
      notes: 'Interesado en tratamientos de renovación.',
      metadata: { Segmento: 'Nuevo', Canal: 'Google', Preferencia: 'Tardes' },
    },
    {
      full_name: 'Francisca Morales Lagos',
      email: 'francisca.morales@ejemplo.cl',
      phone: '+56 9 8844 2211',
      tax_id: '18.901.345-6',
      address: 'Av. Ossa 450, La Reina',
      notes: 'Asiste cada 4 semanas a limpieza Hydra.',
      metadata: { Segmento: 'VIP', Canal: 'Referido', Frecuencia: 'Mensual' },
    },
    {
      full_name: 'Diego Contreras Bravo',
      email: 'diego.contreras@ejemplo.cl',
      phone: '+56 9 7733 5511',
      tax_id: '15.876.543-1',
      address: 'Huérfanos 1055, Santiago Centro',
      notes: 'Primera evaluación dermatológica.',
      metadata: { Segmento: 'Regular', Canal: 'Web', Preferencia: 'Mediodía' },
    },
    {
      full_name: 'Javiera Espinoza Núñez',
      email: 'javiera.espinoza@ejemplo.cl',
      phone: '+56 9 6611 9900',
      tax_id: '20.234.567-3',
      address: 'Av. Pedro de Valdivia 1800, Providencia',
      notes: 'Alérgica a fragancias fuertes.',
      metadata: { Segmento: 'Regular', Canal: 'Instagram', Alergia: 'Fragancias' },
    },
    {
      full_name: 'Tomás Herrera Castillo',
      email: 'tomas.herrera@ejemplo.cl',
      phone: '+56 9 5522 7788',
      tax_id: '17.654.987-5',
      address: 'Alonso de Córdova 5151, Las Condes',
      notes: 'Plan corporativo de bienestar.',
      metadata: { Segmento: 'Corporativo', Canal: 'Convenio', Convenio: 'Empresa' },
    },
  ]);

  // 1.8 Bookings (8 bookings across today, tomorrow, next 5 days covering ALL statuses)
  await ensureBookings(auth, [
    {
      calendar: calendars[0],
      client: clients[0],
      selectedItems: [limpService],
      daysOffset: 0,
      hourUtc: 13,
      targetStatus: 'COMPLETED',
      notes: 'Sesión mensual completada sin observaciones.',
    },
    {
      calendar: calendars[1],
      client: clients[1],
      selectedItems: [evalService],
      daysOffset: 0,
      hourUtc: 15,
      targetStatus: 'SCHEDULED',
      notes: 'Evaluación inicial programada para hoy.',
    },
    {
      calendar: calendars[0],
      client: clients[2],
      selectedItems: [peelService],
      daysOffset: 0,
      hourUtc: 17,
      targetStatus: 'NO_SHOW',
      notes: 'Paciente no asistió al bloque agendado.',
    },
    {
      calendar: calendars[0],
      client: clients[3],
      selectedItems: [evalService, limpService],
      daysOffset: 1,
      hourUtc: 14,
      targetStatus: 'SCHEDULED',
      notes: 'Evaluación + limpieza profunda combinada.',
    },
    {
      calendar: calendars[1],
      client: clients[4],
      selectedItems: [ctrlService],
      daysOffset: 1,
      hourUtc: 16,
      targetStatus: 'PENDING',
      notes: 'Pendiente de confirmación por WhatsApp.',
    },
    {
      calendar: calendars[0],
      client: clients[5],
      selectedItems: [peelService],
      daysOffset: 2,
      hourUtc: 15,
      targetStatus: 'CANCELLED',
      notes: 'Reprogramación solicitada por viaje laboral.',
      cancellationReason: 'Cliente solicitó reagendar para la próxima semana',
    },
    {
      calendar: calendars[0],
      client: clients[6],
      selectedItems: [limpService],
      daysOffset: 3,
      hourUtc: 14,
      targetStatus: 'SCHEDULED',
      notes: 'Usar línea hipoalergénica sin fragancia.',
    },
    {
      calendar: calendars[1],
      client: clients[7],
      selectedItems: [evalService, ctrlService],
      daysOffset: 5,
      hourUtc: 16,
      targetStatus: 'PENDING',
      notes: 'Evaluación bajo convenio corporativo.',
    },
  ]);

  // 1.9 Telemetry (18 rich events mixing TRACKING & AUDIT with real client_id references)
  const nowMs = Date.now();
  const hoursAgo = (h: number) => new Date(nowMs - h * 60 * 60 * 1000).toISOString();

  const demoTelemetry: IngestTelemetryEventPayload[] = [
    {
      kind: 'TRACKING',
      event_name: 'page.viewed',
      source: 'web',
      session_id: 'sess-demo-01',
      properties: { path: '/dashboard', title: 'Inicio' },
      occurred_at: hoursAgo(48),
    },
    {
      kind: 'TRACKING',
      event_name: 'page.viewed',
      source: 'web',
      session_id: 'sess-demo-01',
      properties: { path: '/clients', title: 'Clientes' },
      occurred_at: hoursAgo(47),
    },
    {
      kind: 'AUDIT',
      event_name: 'client.created',
      source: 'bff',
      client_id: clients[0].id,
      resource_type: 'client',
      resource_id: clients[0].id,
      properties: { full_name: clients[0].full_name, segment: 'VIP' },
      occurred_at: hoursAgo(46),
    },
    {
      kind: 'AUDIT',
      event_name: 'client.created',
      source: 'bff',
      client_id: clients[1].id,
      resource_type: 'client',
      resource_id: clients[1].id,
      properties: { full_name: clients[1].full_name, segment: 'Regular' },
      occurred_at: hoursAgo(42),
    },
    {
      kind: 'AUDIT',
      event_name: 'client.created',
      source: 'bff',
      client_id: clients[2].id,
      resource_type: 'client',
      resource_id: clients[2].id,
      properties: { full_name: clients[2].full_name, segment: 'VIP' },
      occurred_at: hoursAgo(38),
    },
    {
      kind: 'AUDIT',
      event_name: 'catalog.created',
      source: 'bff',
      resource_type: 'catalog_item',
      resource_id: limpService.id,
      properties: { name: limpService.name, base_price: limpService.base_price },
      occurred_at: hoursAgo(35),
    },
    {
      kind: 'AUDIT',
      event_name: 'catalog.created',
      source: 'bff',
      resource_type: 'catalog_item',
      resource_id: allItems[allItems.length - 1].id,
      properties: { name: allItems[allItems.length - 1].name, item_type: 'BUNDLE' },
      occurred_at: hoursAgo(34),
    },
    {
      kind: 'TRACKING',
      event_name: 'page.viewed',
      source: 'web',
      session_id: 'sess-demo-02',
      properties: { path: '/catalog', title: 'Catálogo' },
      occurred_at: hoursAgo(30),
    },
    {
      kind: 'AUDIT',
      event_name: 'booking.created',
      source: 'bff',
      client_id: clients[0].id,
      resource_type: 'booking',
      properties: { client_name: clients[0].full_name, service: limpService.name },
      occurred_at: hoursAgo(26),
    },
    {
      kind: 'AUDIT',
      event_name: 'booking.created',
      source: 'bff',
      client_id: clients[1].id,
      resource_type: 'booking',
      properties: { client_name: clients[1].full_name, service: evalService.name },
      occurred_at: hoursAgo(22),
    },
    {
      kind: 'AUDIT',
      event_name: 'booking.created',
      source: 'bff',
      client_id: clients[3].id,
      resource_type: 'booking',
      properties: { client_name: clients[3].full_name, service: 'Evaluación + Limpieza' },
      occurred_at: hoursAgo(18),
    },
    {
      kind: 'AUDIT',
      event_name: 'booking.cancelled',
      source: 'bff',
      client_id: clients[5].id,
      resource_type: 'booking',
      properties: {
        client_name: clients[5].full_name,
        reason: 'Cliente solicitó reagendar para la próxima semana',
      },
      occurred_at: hoursAgo(14),
    },
    {
      kind: 'TRACKING',
      event_name: 'page.viewed',
      source: 'web',
      session_id: 'sess-demo-03',
      properties: { path: '/bookings', title: 'Agenda y Reservas' },
      occurred_at: hoursAgo(10),
    },
    {
      kind: 'AUDIT',
      event_name: 'client.created',
      source: 'bff',
      client_id: clients[4].id,
      resource_type: 'client',
      resource_id: clients[4].id,
      properties: { full_name: clients[4].full_name, segment: 'VIP' },
      occurred_at: hoursAgo(8),
    },
    {
      kind: 'AUDIT',
      event_name: 'booking.created',
      source: 'bff',
      client_id: clients[6].id,
      resource_type: 'booking',
      properties: { client_name: clients[6].full_name, service: limpService.name },
      occurred_at: hoursAgo(6),
    },
    {
      kind: 'AUDIT',
      event_name: 'settings.updated',
      source: 'bff',
      resource_type: 'tenant',
      resource_id: DEMO_TENANT_ID,
      properties: {
        slug: DEMO_TENANT_SLUG,
        updated_sections: ['branding', 'locale', 'features'],
        primaryColor: '#059669',
      },
      occurred_at: hoursAgo(4),
    },
    {
      kind: 'TRACKING',
      event_name: 'page.viewed',
      source: 'web',
      session_id: 'sess-demo-04',
      properties: { path: '/telemetry', title: 'Actividad' },
      occurred_at: hoursAgo(2),
    },
    {
      kind: 'TRACKING',
      event_name: 'page.viewed',
      source: 'web',
      session_id: 'sess-demo-04',
      properties: { path: '/settings', title: 'Ajustes' },
      occurred_at: hoursAgo(1),
    },
  ];

  await ensureTelemetryBatch(auth, demoTelemetry, DEMO_TENANT_SLUG);
}

// ============================================================================
// Tenant 2: barberia-norte ("Barbería Norte Studio", #2563EB)
// ============================================================================

async function seedBarberiaNorteTenant(
  platformAuth: AuthOptions,
  kcAdminToken: string | null,
): Promise<void> {
  console.log('\n💈 [Tenant 2] Poblando barberia-norte ("Barbería Norte Studio", #2563EB)...');

  const barberiaConfig = {
    branding: {
      displayName: 'Barbería Norte Studio',
      logoUrl: '',
      primaryColor: '#2563EB',
      accentColor: '#1e293b',
      themeMode: 'light',
    },
    locale: {
      currency: 'CLP',
      timezone: 'America/Santiago',
      language: 'es',
    },
    features: {
      crm: true,
      catalog: true,
      events: true,
      bookings: true,
      telemetry: true,
    },
  };

  let tenant2Id: string | null = null;

  try {
    const createdTenant = await identityHttp.requestData<Tenant>('POST', '/v1/tenants', {
      ...platformAuth,
      body: {
        slug: BARBERIA_SLUG,
        name: 'Barbería Norte Studio',
        config: barberiaConfig,
      },
    });
    tenant2Id = createdTenant.id;
    console.log(`  ✓ Tenant creado: ${createdTenant.name} (${createdTenant.slug} · ${createdTenant.id})`);
  } catch (err) {
    if (err instanceof UpstreamApiError && err.status === 409) {
      console.log(`  ↺ Tenant "${BARBERIA_SLUG}" ya existe (409). Recuperando tenant_id...`);
      if (kcAdminToken) {
        const existingKcUser = await findKeycloakUserByEmail(
          kcAdminToken,
          'admin@barberianorte.cl',
        );
        const attrTenantId = existingKcUser?.attributes?.tenant_id?.[0];
        if (attrTenantId) {
          tenant2Id = attrTenantId;
          await identityHttp
            .requestData<Tenant>('PATCH', `/v1/tenants/${tenant2Id}`, {
              ...platformAuth,
              body: { name: 'Barbería Norte Studio', config: barberiaConfig },
            })
            .catch(() => undefined);
          console.log(`  ↺ Tenant ID recuperado desde Keycloak: ${tenant2Id}`);
        }
      }
    } else {
      throw err;
    }
  }

  if (!kcAdminToken) {
    console.log(
      '  ⚠ Keycloak Admin API no disponible; omitiendo creación de usuario admin@barberianorte.cl y datos aislados de Tenant 2.',
    );
    return;
  }

  if (!tenant2Id) {
    console.log(
      '  ⚠ No se pudo resolver el tenant_id de barberia-norte; omitiendo seed de datos aislados de Tenant 2.',
    );
    return;
  }

  await ensureKeycloakUser(kcAdminToken, {
    email: 'admin@barberianorte.cl',
    firstName: 'Admin',
    lastName: 'Barbería Norte',
    password: 'Norte123!',
    tenantId: tenant2Id,
    roles: ALL_TENANT_ADMIN_ROLES,
  });

  const tenant2Jwt = await getUserPasswordGrantToken('admin@barberianorte.cl', 'Norte123!');
  if (!tenant2Jwt) {
    console.log(
      '  ⚠ No se pudo obtener JWT para admin@barberianorte.cl; omitiendo datos aislados de Tenant 2.',
    );
    return;
  }

  const tenant2Auth: AuthOptions = {
    ctx: { authorization: `Bearer ${tenant2Jwt}` },
  };
  console.log('  🔑 Autenticado como admin@barberianorte.cl (Bearer JWT aislado por RLS).');

  // 2.1 Calendar (1)
  const calendars = await ensureCalendars(tenant2Auth, [
    {
      name: 'Sillón Principal',
      timezone: 'America/Santiago',
      allow_overlaps: false,
      working_hours: {
        tuesday: [{ start: '10:00', end: '20:00' }],
        wednesday: [{ start: '10:00', end: '20:00' }],
        thursday: [{ start: '10:00', end: '20:00' }],
        friday: [{ start: '10:00', end: '21:00' }],
        saturday: [{ start: '10:00', end: '19:00' }],
      },
    },
  ]);

  // 2.2 Categories (2)
  const categories = await ensureCategories(tenant2Auth, [
    {
      name: 'Cortes y Barba',
      slug: 'cortes-barba',
      description: 'Cortes clásicos, degradados y perfilado de barba con toalla caliente.',
      display_order: 1,
      is_active: true,
    },
    {
      name: 'Productos',
      slug: 'productos-barberia',
      description: 'Ceras, aceites y cuidado masculino.',
      display_order: 2,
      is_active: true,
    },
  ]);

  const catCortes = categories.get('cortes-barba')!;
  const catProductos = categories.get('productos-barberia')!;

  // 2.3 Catalog Items (3)
  const items = await ensureCatalogItems(tenant2Auth, [
    {
      sku: 'BRB-CORTE-01',
      name: 'Corte Clásico + Degradado',
      description: 'Corte a tijera y máquina con lavado y styling.',
      category_id: catCortes.id,
      base_price: 16000,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'SERVICE',
      duration_minutes: 40,
      buffer_minutes: 5,
    },
    {
      sku: 'BRB-BARBA-01',
      name: 'Perfilado de Barba + Toalla Caliente',
      description: 'Ritual tradicional con navaja y aceite esencial.',
      category_id: catCortes.id,
      base_price: 12000,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'SERVICE',
      duration_minutes: 30,
      buffer_minutes: 5,
    },
    {
      sku: 'BRB-PRD-CERA',
      name: 'Cera Mate Fijación Fuerte (100g)',
      description: 'Acabado natural sin brillo para uso diario.',
      category_id: catProductos.id,
      base_price: 14500,
      currency: 'CLP',
      tax_rate_bps: 1900,
      tax_included: true,
      item_type: 'PRODUCT',
      unit_of_measure: 'UNIT',
      track_inventory: true,
    },
  ]);

  // 2.4 Clients (3)
  const clients = await ensureClients(tenant2Auth, [
    {
      full_name: 'Ignacio Paredes Vega',
      email: 'ignacio.paredes@ejemplo.cl',
      phone: '+56 9 8234 1100',
      notes: 'Prefiere degradado medio y barba corta.',
      metadata: { Segmento: 'Frecuente', Barbero: 'Principal' },
    },
    {
      full_name: 'Cristóbal Silva Méndez',
      email: 'cristobal.silva@ejemplo.cl',
      phone: '+56 9 7123 4455',
      notes: 'Agenda siempre los viernes en la tarde.',
      metadata: { Segmento: 'VIP', Canal: 'Instagram' },
    },
    {
      full_name: 'Benjamín Torres Ríos',
      email: 'benjamin.torres@ejemplo.cl',
      phone: '+56 9 6345 7788',
      notes: 'Nuevo cliente del barrio.',
      metadata: { Segmento: 'Nuevo', Canal: 'Walk-in' },
    },
  ]);

  // 2.5 Bookings (2)
  await ensureBookings(tenant2Auth, [
    {
      calendar: calendars[0],
      client: clients[0],
      selectedItems: [items[0], items[1]],
      daysOffset: 0,
      hourUtc: 15,
      targetStatus: 'COMPLETED',
      notes: 'Corte + barba completado.',
    },
    {
      calendar: calendars[0],
      client: clients[1],
      selectedItems: [items[0]],
      daysOffset: 1,
      hourUtc: 17,
      targetStatus: 'SCHEDULED',
      notes: 'Corte degradado agendado para mañana.',
    },
  ]);

  // 2.6 Telemetry (6)
  const nowMs = Date.now();
  const hoursAgo = (h: number) => new Date(nowMs - h * 60 * 60 * 1000).toISOString();

  await ensureTelemetryBatch(
    tenant2Auth,
    [
      {
        kind: 'TRACKING',
        event_name: 'page.viewed',
        source: 'web',
        session_id: 'sess-barberia-01',
        properties: { path: '/dashboard', title: 'Inicio' },
        occurred_at: hoursAgo(12),
      },
      {
        kind: 'AUDIT',
        event_name: 'client.created',
        source: 'bff',
        client_id: clients[0].id,
        resource_type: 'client',
        resource_id: clients[0].id,
        properties: { full_name: clients[0].full_name },
        occurred_at: hoursAgo(10),
      },
      {
        kind: 'AUDIT',
        event_name: 'catalog.created',
        source: 'bff',
        resource_type: 'catalog_item',
        resource_id: items[0].id,
        properties: { name: items[0].name, base_price: items[0].base_price },
        occurred_at: hoursAgo(8),
      },
      {
        kind: 'AUDIT',
        event_name: 'booking.created',
        source: 'bff',
        client_id: clients[0].id,
        resource_type: 'booking',
        properties: { client_name: clients[0].full_name, service: items[0].name },
        occurred_at: hoursAgo(6),
      },
      {
        kind: 'AUDIT',
        event_name: 'booking.created',
        source: 'bff',
        client_id: clients[1].id,
        resource_type: 'booking',
        properties: { client_name: clients[1].full_name, service: items[0].name },
        occurred_at: hoursAgo(3),
      },
      {
        kind: 'AUDIT',
        event_name: 'settings.updated',
        source: 'bff',
        resource_type: 'tenant',
        resource_id: tenant2Id,
        properties: { slug: BARBERIA_SLUG, primaryColor: '#2563EB' },
        occurred_at: hoursAgo(1),
      },
    ],
    BARBERIA_SLUG,
  );
}

// ============================================================================
// Main Orchestrator
// ============================================================================

async function main() {
  console.log('🌱 Iniciando seed integral multi-tenant de ikedadev-platform...\n');

  const kcAdminToken = await step0SyncKeycloak();
  const platformAuth = await resolvePlatformAdminAuth();

  await seedDemoTenant(platformAuth);
  await seedBarberiaNorteTenant(platformAuth, kcAdminToken);

  const webUrl = config.WEB_APP_URL.replace(/\/+$/, '');
  console.log('\n✅ Seed integral completado con éxito!\n');
  console.log('┌────────────────────────┬──────────────────────────────┬───────────────┬───────────────────────────────┐');
  console.log('│ Tenant                 │ Usuario                      │ Contraseña    │ Rol                           │');
  console.log('├────────────────────────┼──────────────────────────────┼───────────────┼───────────────────────────────┤');
  console.log('│ Clínica Kropy Estética │ admin@demo.ikedadev.com      │ Admin123!     │ Platform + Tenant Admin       │');
  console.log('│ Clínica Kropy Estética │ staff@demo.ikedadev.com      │ Staff123!     │ Colaborador (Staff)           │');
  console.log('│ Clínica Kropy Estética │ dra.sofia@demo.ikedadev.com  │ Sofia123!     │ Colaborador (Especialista)    │');
  console.log('│ Clínica Kropy Estética │ recepcion@demo.ikedadev.com  │ Recepcion123! │ Colaborador (Recepción)       │');
  console.log('│ Barbería Norte Studio  │ admin@barberianorte.cl       │ Norte123!     │ Tenant Admin (RLS aislado)    │');
  console.log('└────────────────────────┴──────────────────────────────┴───────────────┴───────────────────────────────┘');
  console.log('\n🌐 URLs de acceso white-label:');
  console.log(`   • Tenant 1 (Esmeralda #059669): ${webUrl}/?tenant=${DEMO_TENANT_SLUG}`);
  console.log(`   • Tenant 2 (Azul #2563EB):      ${webUrl}/?tenant=${BARBERIA_SLUG}\n`);
}

main().catch((err) => {
  console.error('\n❌ Error ejecutando seed:', err);
  process.exit(1);
});
