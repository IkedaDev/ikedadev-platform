import { Hono } from 'hono';
import { z } from 'zod';
import { UpstreamApiError } from '../clients/http.js';
import { identityClient } from '../clients/identityClient.js';
import { telemetryClient } from '../clients/telemetryClient.js';
import { config } from '../config.js';
import { getAuthRequestContext as getCtx } from '../middleware/auth.js';
import type {
  PublicBrandingPayload,
  TenantBrandingSettings,
  TenantFeatureFlags,
  TenantLocaleSettings,
  TenantSettings,
  TenantSettingsPayload,
  ThemeMode,
} from '../types/platform.js';

const HEX_COLOR_REGEX = /^#[0-9a-fA-F]{6}$/;
const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const NIL_UUID = '00000000-0000-0000-0000-000000000000';

function isValidTenantUuid(value: string | undefined): value is string {
  return typeof value === 'string' && UUID_REGEX.test(value) && value !== NIL_UUID;
}

/**
 * Only allow `PLATFORM_ADMIN_API_KEY` elevation when the user identity was established
 * via a server-verified BFF session cookie (obtained directly from Keycloak token exchange).
 * Raw `Authorization: Bearer` headers are NOT signature-verified in the BFF, so they MUST
 * be forwarded directly to `identity-api` for cryptographic RS256 JWKS verification.
 */
function resolveSettingsApiKeyOverride(authMethod: 'session' | 'bearer' | undefined): string | undefined {
  return authMethod === 'session' ? config.PLATFORM_ADMIN_API_KEY : undefined;
}

export const DEFAULT_TENANT_SETTINGS: TenantSettings = {
  branding: {
    displayName: 'Kropy',
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
};

const brandingPatchSchema = z.object({
  displayName: z
    .string()
    .trim()
    .min(1, 'El nombre visible no puede estar vacío')
    .max(80, 'El nombre visible no puede superar 80 caracteres')
    .optional(),
  logoUrl: z
    .string()
    .trim()
    .max(500, 'La URL del logo es demasiado larga')
    .refine(
      (v) => v === '' || /^https?:\/\/.+/.test(v) || v.startsWith('/'),
      'logoUrl debe estar vacío o ser una URL válida (http/https o ruta relativa)',
    )
    .optional(),
  primaryColor: z
    .string()
    .regex(HEX_COLOR_REGEX, 'primaryColor debe tener formato hexadecimal #RRGGBB')
    .optional(),
  accentColor: z
    .string()
    .regex(HEX_COLOR_REGEX, 'accentColor debe tener formato hexadecimal #RRGGBB')
    .optional(),
  themeMode: z.enum(['light', 'dark', 'system']).optional(),
});

const localePatchSchema = z.object({
  currency: z
    .string()
    .trim()
    .min(3, 'Moneda inválida')
    .max(5, 'Moneda inválida')
    .transform((v) => v.toUpperCase())
    .optional(),
  timezone: z.string().trim().min(1, 'Zona horaria inválida').max(64).optional(),
  language: z.enum(['es', 'en']).optional(),
});

const featuresPatchSchema = z.object({
  crm: z.boolean().optional(),
  catalog: z.boolean().optional(),
  events: z.boolean().optional(),
  bookings: z.boolean().optional(),
  telemetry: z.boolean().optional(),
});

const patchSettingsSchema = z
  .object({
    branding: brandingPatchSchema.optional(),
    locale: localePatchSchema.optional(),
    features: featuresPatchSchema.optional(),
  })
  .refine(
    (data) =>
      data.branding !== undefined || data.locale !== undefined || data.features !== undefined,
    {
      message: 'Debes enviar al menos una sección (branding, locale o features) para actualizar.',
    },
  );

// In-memory public branding cache keyed by normalized tenant slug
const slugBrandingCache = new Map<string, TenantBrandingSettings>();

function normalizeSlug(slug: string): string {
  return slug.trim().toLowerCase();
}

function isPlainObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function cloneDefaultSettings(): TenantSettings {
  return {
    branding: { ...DEFAULT_TENANT_SETTINGS.branding },
    locale: { ...DEFAULT_TENANT_SETTINGS.locale },
    features: { ...DEFAULT_TENANT_SETTINGS.features },
  };
}

function isThemeMode(value: unknown): value is ThemeMode {
  return value === 'light' || value === 'dark' || value === 'system';
}

function mergeTenantConfigWithDefaults(rawConfig: unknown): TenantSettings {
  const cfg = isPlainObject(rawConfig) ? rawConfig : {};
  const rawBranding = isPlainObject(cfg.branding) ? cfg.branding : {};
  const rawLocale = isPlainObject(cfg.locale) ? cfg.locale : {};
  const rawFeatures = isPlainObject(cfg.features) ? cfg.features : {};

  const branding: TenantBrandingSettings = {
    displayName:
      typeof rawBranding.displayName === 'string' && rawBranding.displayName.trim()
        ? rawBranding.displayName.trim()
        : DEFAULT_TENANT_SETTINGS.branding.displayName,
    logoUrl:
      typeof rawBranding.logoUrl === 'string'
        ? rawBranding.logoUrl.trim()
        : DEFAULT_TENANT_SETTINGS.branding.logoUrl,
    primaryColor:
      typeof rawBranding.primaryColor === 'string' &&
      HEX_COLOR_REGEX.test(rawBranding.primaryColor)
        ? rawBranding.primaryColor
        : DEFAULT_TENANT_SETTINGS.branding.primaryColor,
    accentColor:
      typeof rawBranding.accentColor === 'string' && HEX_COLOR_REGEX.test(rawBranding.accentColor)
        ? rawBranding.accentColor
        : DEFAULT_TENANT_SETTINGS.branding.accentColor,
    themeMode: isThemeMode(rawBranding.themeMode)
      ? rawBranding.themeMode
      : DEFAULT_TENANT_SETTINGS.branding.themeMode,
  };

  const locale: TenantLocaleSettings = {
    currency:
      typeof rawLocale.currency === 'string' && rawLocale.currency.trim()
        ? rawLocale.currency.trim().toUpperCase()
        : DEFAULT_TENANT_SETTINGS.locale.currency,
    timezone:
      typeof rawLocale.timezone === 'string' && rawLocale.timezone.trim()
        ? rawLocale.timezone.trim()
        : DEFAULT_TENANT_SETTINGS.locale.timezone,
    language:
      rawLocale.language === 'es' || rawLocale.language === 'en'
        ? rawLocale.language
        : DEFAULT_TENANT_SETTINGS.locale.language,
  };

  const features: TenantFeatureFlags = {
    crm:
      typeof rawFeatures.crm === 'boolean'
        ? rawFeatures.crm
        : DEFAULT_TENANT_SETTINGS.features.crm,
    catalog:
      typeof rawFeatures.catalog === 'boolean'
        ? rawFeatures.catalog
        : DEFAULT_TENANT_SETTINGS.features.catalog,
    events:
      typeof rawFeatures.events === 'boolean'
        ? rawFeatures.events
        : DEFAULT_TENANT_SETTINGS.features.events,
    bookings:
      typeof rawFeatures.bookings === 'boolean'
        ? rawFeatures.bookings
        : DEFAULT_TENANT_SETTINGS.features.bookings,
    telemetry:
      typeof rawFeatures.telemetry === 'boolean'
        ? rawFeatures.telemetry
        : DEFAULT_TENANT_SETTINGS.features.telemetry,
  };

  return { branding, locale, features };
}

function hasSettingsAdminRole(roles: string[] | undefined): boolean {
  if (!roles || roles.length === 0) return false;
  return roles.includes('tenant:admin') || roles.includes('platform:admin');
}

function cacheTenantBranding(slug: string, branding: TenantBrandingSettings): void {
  const normalized = normalizeSlug(slug);
  if (normalized) {
    slugBrandingCache.set(normalized, { ...branding });
  }
}

export const settingsRoutes = new Hono();

// GET /api/settings — Authenticated tenant settings (strictly scoped to session tenantId)
settingsRoutes.get('/', async (c) => {
  const ctx = getCtx(c);
  if (!isValidTenantUuid(ctx.tenantId)) {
    return c.json(
      {
        success: false,
        code: 'MISSING_TENANT_ID',
        message: 'La sesión autenticada no contiene un tenant_id válido.',
      },
      400,
    );
  }

  const isAdmin = hasSettingsAdminRole(ctx.roles);
  const apiKeyOverride = resolveSettingsApiKeyOverride(ctx.authMethod);

  try {
    const tenant = await identityClient.getTenant(ctx.tenantId, {
      ctx,
      apiKeyOverride,
    });
    const settings = mergeTenantConfigWithDefaults(tenant.config);
    cacheTenantBranding(tenant.slug, settings.branding);

    const payload: TenantSettingsPayload = {
      tenantId: tenant.id,
      slug: tenant.slug,
      name: tenant.name,
      canEdit: isAdmin,
      settings,
    };

    return c.json({ success: true, data: payload });
  } catch (err) {
    if (err instanceof UpstreamApiError && err.status === 403) {
      const fallback: TenantSettingsPayload = {
        tenantId: ctx.tenantId,
        slug: '',
        name: DEFAULT_TENANT_SETTINGS.branding.displayName,
        canEdit: false,
        settings: cloneDefaultSettings(),
      };
      return c.json({ success: true, data: fallback });
    }
    throw err;
  }
});

// PATCH /api/settings — Authenticated tenant settings update (requires tenant:admin or platform:admin)
settingsRoutes.patch('/', async (c) => {
  const ctx = getCtx(c);
  if (!isValidTenantUuid(ctx.tenantId)) {
    return c.json(
      {
        success: false,
        code: 'MISSING_TENANT_ID',
        message: 'La sesión autenticada no contiene un tenant_id válido.',
      },
      400,
    );
  }

  if (!hasSettingsAdminRole(ctx.roles)) {
    return c.json(
      {
        success: false,
        code: 'FORBIDDEN',
        message: 'Se requiere rol tenant:admin o platform:admin para actualizar la configuración.',
      },
      403,
    );
  }

  const raw = await c.req.json().catch(() => null);
  const parsed = patchSettingsSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Configuración de negocio inválida',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }

  const patch = parsed.data;
  const apiKeyOverride = resolveSettingsApiKeyOverride(ctx.authMethod);

  // Read existing tenant config first so we deep-merge without clobbering unrelated keys
  const currentTenant = await identityClient.getTenant(ctx.tenantId, {
    ctx,
    apiKeyOverride,
  });

  const existingConfig = isPlainObject(currentTenant.config) ? currentTenant.config : {};
  const currentSettings = mergeTenantConfigWithDefaults(existingConfig);

  const mergedSettings: TenantSettings = {
    branding: {
      ...currentSettings.branding,
      ...(patch.branding ?? {}),
    },
    locale: {
      ...currentSettings.locale,
      ...(patch.locale ?? {}),
    },
    features: {
      ...currentSettings.features,
      ...(patch.features ?? {}),
    },
  };

  const mergedConfig: Record<string, unknown> = {
    ...existingConfig,
    branding: {
      ...(isPlainObject(existingConfig.branding) ? existingConfig.branding : {}),
      ...mergedSettings.branding,
    },
    locale: {
      ...(isPlainObject(existingConfig.locale) ? existingConfig.locale : {}),
      ...mergedSettings.locale,
    },
    features: {
      ...(isPlainObject(existingConfig.features) ? existingConfig.features : {}),
      ...mergedSettings.features,
    },
  };

  const updatedTenant = await identityClient.patchTenant(
    ctx.tenantId,
    { config: mergedConfig },
    {
      ctx,
      apiKeyOverride,
    },
  );

  const finalSettings = mergeTenantConfigWithDefaults(updatedTenant.config);
  cacheTenantBranding(updatedTenant.slug, finalSettings.branding);

  telemetryClient.emitAuditEventAsync(
    {
      event_name: 'settings.updated',
      resource_type: 'tenant',
      resource_id: updatedTenant.id,
      properties: {
        slug: updatedTenant.slug,
        updated_sections: Object.entries(patch)
          .filter(([, value]) => value !== undefined)
          .map(([key]) => key),
        branding: finalSettings.branding,
        locale: finalSettings.locale,
        features: finalSettings.features,
      },
    },
    ctx,
  );

  const payload: TenantSettingsPayload = {
    tenantId: updatedTenant.id,
    slug: updatedTenant.slug,
    name: updatedTenant.name,
    canEdit: true,
    settings: finalSettings,
  };

  return c.json({ success: true, data: payload });
});

export const publicBrandingRoutes = new Hono();

// GET /api/branding/:slug — PUBLIC route (no authentication required) for white-label login branding
publicBrandingRoutes.get('/:slug', (c) => {
  const slug = normalizeSlug(c.req.param('slug'));
  const cached = slugBrandingCache.get(slug);

  const payload: PublicBrandingPayload = {
    slug,
    branding: cached ? { ...cached } : { ...DEFAULT_TENANT_SETTINGS.branding },
  };

  return c.json({ success: true, data: payload });
});
