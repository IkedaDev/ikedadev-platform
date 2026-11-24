import 'dotenv/config';
import { z } from 'zod';

const envSchema = z.object({
  PORT: z.coerce.number().int().positive().default(4000),
  BFF_PUBLIC_URL: z.string().url().default('http://localhost:4000'),
  WEB_APP_URL: z.string().url().default('http://localhost:5173'),
  CORS_ORIGIN: z.string().min(1).default('http://localhost:5173'),

  // Keycloak / OIDC settings
  KEYCLOAK_PUBLIC_URL: z.string().url().default('http://localhost:8180'),
  KEYCLOAK_INTERNAL_URL: z.string().url().default('http://localhost:8180'),
  KEYCLOAK_REALM: z.string().min(1).default('ikeda'),
  KEYCLOAK_CLIENT_ID: z.string().min(1).default('tenant-portal-bff'),
  KEYCLOAK_CLIENT_SECRET: z.string().min(1).default('ikeda-tenant-portal-bff-secret-2026'),
  SESSION_COOKIE_NAME: z.string().min(1).default('ikeda_bff_session'),

  // M2M fallback key (used by seed script)
  TENANT_API_KEY: z.string().min(1).default('ik_live_dev_tenant_portal_local_2026'),

  // Upstream Rust services
  IDENTITY_API_URL: z.string().url().default('http://localhost:8082'),
  CRM_API_URL: z.string().url().default('http://localhost:8081'),
  EVENTS_API_URL: z.string().url().default('http://localhost:8080'),
  CATALOG_API_URL: z.string().url().default('http://localhost:8083'),
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
