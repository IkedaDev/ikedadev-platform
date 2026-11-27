import 'dotenv/config';
import { z } from 'zod';

export const DEFAULT_DEV_KEYCLOAK_CLIENT_SECRET = 'ikeda-tenant-portal-bff-secret-2026';

export function parseAllowedOrigins(corsOrigin: string): string[] {
  return corsOrigin
    .split(',')
    .map((o) => o.trim())
    .filter((o) => Boolean(o) && o !== '*');
}

export function resolveCorsOrigin(origin: string | undefined, allowedOrigins: string[]): string {
  if (!origin) return '';
  if (allowedOrigins.includes(origin)) {
    return origin;
  }
  return '';
}

export const envSchema = z
  .object({
    PORT: z.coerce.number().int().positive().default(4000),
    BFF_PUBLIC_URL: z.string().url().default('http://localhost:4000'),
    WEB_APP_URL: z.string().url().default('http://localhost:5173'),
    CORS_ORIGIN: z.string().min(1).default('http://localhost:5173'),

    // Keycloak / OIDC settings
    KEYCLOAK_PUBLIC_URL: z.string().url().default('http://localhost:8180'),
    KEYCLOAK_INTERNAL_URL: z.string().url().default('http://localhost:8180'),
    KEYCLOAK_REALM: z.string().min(1).default('ikeda'),
    KEYCLOAK_CLIENT_ID: z.string().min(1).default('tenant-portal-bff'),
    KEYCLOAK_CLIENT_SECRET: z.string().min(1).default(DEFAULT_DEV_KEYCLOAK_CLIENT_SECRET),
    SESSION_COOKIE_NAME: z.string().min(1).default('ikeda_bff_session'),

    // Optional Platform Admin API Key (used ONLY in src/routes/settings.ts)
    PLATFORM_ADMIN_API_KEY: z.preprocess(
      (val) => (typeof val === 'string' && val.trim() === '' ? undefined : val),
      z.string().min(1).optional(),
    ),

    // Optional GCP Cloud Run IAM auth (X-Serverless-Authorization via metadata server)
    GCP_IAM_AUTH: z
      .enum(['true', 'false'])
      .default('false')
      .transform((v) => v === 'true'),

    // Upstream Rust services
    IDENTITY_API_URL: z.string().url().default('http://localhost:8082'),
    CRM_API_URL: z.string().url().default('http://localhost:8081'),
    EVENTS_API_URL: z.string().url().default('http://localhost:8080'),
    CATALOG_API_URL: z.string().url().default('http://localhost:8083'),
    BOOKINGS_API_URL: z.string().url().default('http://localhost:8084'),
    TELEMETRY_API_URL: z.string().url().default('http://localhost:8085'),
    CONTENT_API_URL: z.string().url().default('http://localhost:8086'),
    MEDIA_API_URL: z.string().url().default('http://localhost:8087'),
  })
  .superRefine((data, ctx) => {
    const rawOrigins = data.CORS_ORIGIN.split(',')
      .map((o) => o.trim())
      .filter(Boolean);
    if (rawOrigins.includes('*')) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['CORS_ORIGIN'],
        message: "Wildcard '*' is not allowed in CORS_ORIGIN when credentials are enabled",
      });
    }

    const isProd =
      data.BFF_PUBLIC_URL.startsWith('https://') || process.env.NODE_ENV === 'production';
    if (isProd && data.KEYCLOAK_CLIENT_SECRET === DEFAULT_DEV_KEYCLOAK_CLIENT_SECRET) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ['KEYCLOAK_CLIENT_SECRET'],
        message: 'Default dev KEYCLOAK_CLIENT_SECRET is not allowed in HTTPS/production environments',
      });
    }
  });

const parsed = envSchema.safeParse(process.env);

if (!parsed.success) {
  console.error('❌ Invalid BFF environment variables:');
  for (const issue of parsed.error.issues) {
    console.error(`  - ${issue.path.join('.')}: ${issue.message}`);
  }
  process.exit(1);
}

export const config = parsed.data;
export type AppConfig = typeof config;
