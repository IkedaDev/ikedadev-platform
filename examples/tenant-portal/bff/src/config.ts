import 'dotenv/config';
import { z } from 'zod';

const envSchema = z.object({
  PORT: z.coerce.number().int().positive().default(4000),
  CORS_ORIGIN: z.string().min(1).default('http://localhost:5173'),
  TENANT_API_KEY: z.string().min(1, 'TENANT_API_KEY is required in .env'),
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
