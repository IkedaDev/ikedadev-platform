# Reglas de Seguridad, RLS y RBAC (Invariantes P0)

## 1. Aislamiento Multi-Tenant en PostgreSQL (P0)
- Toda tabla tenant-scoped **debe** declarar en migraciones SQL:
  - `ALTER TABLE <tabla> ENABLE ROW LEVEL SECURITY;`
  - `ALTER TABLE <tabla> FORCE ROW LEVEL SECURITY;`
  - Política con `USING` y `WITH CHECK` sobre `NULLIF(current_setting('app.current_tenant', true), '')::uuid`.
- El usuario de aplicación en runtime es `ikeda_app` con `NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS`. Jamás conectes los microservicios como `postgres`.
- Cada transacción de repositorio debe ejecutar `SELECT set_config('app.current_tenant', $1, true)` (`is_local = true`).
- En Neon/Cloud Run, `IDENTITY_READER_DATABASE_URL` usa el rol mínimo `api_key_reader` (`NOBYPASSRLS`, solo `EXECUTE ON FUNCTION public.authenticate_api_key(VARCHAR)`).

## 2. Credenciales y Sesiones en `portal/bff`
- **Cero `TENANT_API_KEY` estática en runtime del BFF:** las rutas de usuario reenvían exclusivamente `Authorization: Bearer <jwt>` extraído de la sesión OIDC (`ikeda_bff_session`). `ServiceHttpClient` elimina cualquier `x-api-key` inyectado y jamás envía `X-Api-Key` y `Authorization` a la vez.
- **`PLATFORM_ADMIN_API_KEY` restringida:** solo se usa en `portal/bff/src/routes/settings.ts` y únicamente cuando `ctx.authMethod === 'session'` (nunca en requests Bearer directos).
- `GET /api/settings` debe permanecer accesible para `staff` (requerido por `ThemeProvider` para branding y `tenant.features`); solo `PATCH /api/settings` exige `tenant:admin` o `platform:admin`.
- Pre-validación ligera en BFF: usa `portal/bff/src/auth/permissions.ts` leyendo `ctx.roles` desde memoria sin agregar roundtrips extra a `identity-api`.

## 3. Controles Específicos de `content-api` y `media-api`
- **`content-api`:**
  - Sanitización obligatoria de HTML editorial con `ammonia::Builder::empty()` (whitelist estricta en `services/content-api/src/domain/services.rs`).
  - Anti-SSRF obligatorio (`validate_webhook_target_url`) en creación, actualización, test y despacho de Deploy Hooks: solo `https://`, bloqueando `localhost`, `.local`, `.internal`, metadatos cloud e IPs privadas/link-local/CGNAT.
  - Preview tokens HS256 con TTL 30 min (`PREVIEW_TOKEN_SECRET`).
- **`media-api`:**
  - Whitelist MIME estricta: solo `image/webp`, `image/jpeg`, `image/png`, `image/avif`. **Prohibido SVG**, HTML o ejecutables.
  - Keys de almacenamiento generadas en backend (`tenants/{tenant_id}/{context}/{asset_id}.{ext}`), cuotas verificadas (5 MiB/archivo, 500 MiB/tenant por defecto).
  - Subida/edición en contexto `branding` reservada exclusivamente a `tenant:admin` / `platform:admin`.

## 4. RBAC Estricto en Frontend (`portal/web`)
- Fuente única: `portal/web/src/auth/permissions.ts` (`usePermissions()`, `can()`, `canAccessSection()`).
- Ocultar en Sidebar/Topbar y bloquear navegación directa a módulos desactivados en `tenant.features` o sin permiso (`IdentityView` y `SettingsView` ocultos y bloqueados para `staff`).
- Ocultar (no solo deshabilitar) botones de mutación y `emptyAction` cuando el rol no tiene permiso de escritura.
- En `ContentView`, `staff` sin `content:publish` solo edita borradores `DRAFT`; artículos `PUBLISHED`/`SCHEDULED`/`ARCHIVED` se abren en modo solo lectura y nunca se muestran botones de publicar, archivar, eliminar ni pestaña Deploy Hooks.
