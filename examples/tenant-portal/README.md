# Tenant Portal — BFF + Dashboard Web (Modo Producción con Keycloak + Identity API)

Aplicación de referencia aislada dentro de `ikedadev-platform` que demuestra y valida de punta a punta los **4 microservicios en Rust** (`events-api`, `crm-api`, `identity-api` y `catalog-api`) con **autenticación real de producción** mediante **Keycloak (OIDC Authorization Code + PKCE)**, sesiones `HttpOnly` en el **BFF (Hono)** y validación criptográfica **RS256/JWKS** en cada microservicio.

> **Aislamiento en el Monorepo:** El BFF y el Dashboard viven exclusivamente bajo `examples/tenant-portal/`. No modifican `Cargo.toml` raíz ni el código fuente de los servicios Rust.

## Arquitectura de Producción

```text
Browser (React + Vite :5173)
        │  Cookie HttpOnly `ikeda_bff_session` (nunca toca JWTs ni X-Api-Key en localStorage)
        ▼
Tenant Portal BFF (Hono + Zod :4000)
        │  1) OAuth2 Authorization Code + PKCE contra Keycloak (:8180 / realm `ikeda`)
        │  2) Auto-refresh de access_token en servidor + reenvío de `Authorization: Bearer <JWT>`
        ├──► events-api   (:8080)  Valida RS256 JWKS + claim `tenant_id` (RLS)
        ├──► crm-api      (:8081)  Valida RS256 JWKS + claim `tenant_id` (RLS)
        ├──► identity-api (:8082)  Valida RS256 JWKS + gestiona usuarios reales en Keycloak Admin API
        └──► catalog-api  (:8083)  Valida RS256 JWKS + claim `tenant_id` (RLS)
```

---

## Qué incluye la integración con Keycloak

Al levantar Docker Compose, Keycloak 26 importa automáticamente [deploy/keycloak/ikeda-realm.json](../../deploy/keycloak/ikeda-realm.json):

- **Realm:** `ikeda` (`http://localhost:8180/realms/ikeda`)
- **Protocol Mapper:** inyecta el atributo de usuario `tenant_id` como claim JWT `tenant_id` en el access token y en el ID token (requerido por `ikeda-core`).
- **Cliente `identity-api-admin`:** cuenta de servicio confidencial con permisos `realm-management` para que `identity-api` cree y deshabilite usuarios reales en Keycloak (sin `MockIdentityProvider`).
- **Cliente `tenant-portal-bff`:** cliente OIDC confidencial con PKCE (`S256`) y callback en `http://localhost:4000/auth/callback`.
- **Usuarios demo pre-cargados** (asociados a `demo-tenant`, UUID `11111111-1111-4111-8111-111111111111`):
  - **Admin (`tenant:admin`, `platform:admin`):** `admin@demo.ikedadev.com` / `Admin123!`
  - **Staff (`tenant:member`):** `staff@demo.ikedadev.com` / `Staff123!`

---

## Perfiles disponibles en `docker-compose.yml`

Desde la raíz del repositorio (`ikedadev-platform/`):

| Comando | Qué levanta |
| :--- | :--- |
| `docker compose --profile full up -d --build` | **Recomendado (Todo listo):** PostgreSQL 17 (4 DBs + migraciones) + Keycloak 26 (realm `ikeda`) + las **4 APIs Rust** con JWKS y Keycloak Admin activos |
| `docker compose --profile keycloak up -d` | Solo infraestructura: **PostgreSQL 17** + **Keycloak 26** (`:8180`) para correr las APIs con `cargo run` |
| `docker compose --profile apis up -d --build` | PostgreSQL 17 + las 4 APIs Rust |
| `docker compose up -d` | Solo PostgreSQL 17 |

> **Importante si ya habías levantado Postgres anteriormente:** `scripts/init-databases.sh` se ejecuta al crear el volumen por primera vez. Si necesitas inicializar limpio:
> ```bash
> docker compose down -v
> docker compose --profile full up -d --build
> ```

---

## Paso 1: Levantar la plataforma (Postgres + Keycloak + 4 APIs Rust)

```bash
docker compose --profile full up -d --build
```

Verifica que respondan:
- Keycloak OIDC discovery: `http://localhost:8180/realms/ikeda/.well-known/openid-configuration`
- Microservicios Rust: `http://localhost:8080/health`, `8081/health`, `8082/health`, `8083/health`

*(Si prefieres correr los 4 servicios con `cargo run -p ...` fuera de Docker, levanta primero `docker compose --profile keycloak up -d` y pasa `JWKS_URI=http://localhost:8180/realms/ikeda/protocol/openid-connect/certs`, `JWT_ISSUER=http://localhost:8180/realms/ikeda`, `KEYCLOAK_BASE_URL=http://localhost:8180` y `KEYCLOAK_CLIENT_SECRET=ikeda-identity-admin-secret-2026`.)*

---

## Paso 2: Configurar e instalar BFF y Web

```bash
# BFF
cd examples/tenant-portal/bff
cp .env.example .env
npm install

# Web
cd ../web
cp .env.example .env
npm install
```

`bff/.env.example` ya incluye la configuración lista para el realm `ikeda` (`KEYCLOAK_CLIENT_ID=tenant-portal-bff`, `KEYCLOAK_CLIENT_SECRET=ikeda-tenant-portal-bff-secret-2026`) y la `TENANT_API_KEY` local para el script de seed.

---

## Paso 3: Poblar datos de prueba (`npm run seed`)

```bash
cd examples/tenant-portal/bff
npm run seed
```

---

## Paso 4: Levantar BFF y Web en paralelo

```bash
# Terminal A — BFF (:4000)
cd examples/tenant-portal/bff
npm run dev

# Terminal B — Dashboard Web (:5173)
cd examples/tenant-portal/web
npm run dev
```

Abre **http://localhost:5173** en tu navegador.

---

## Flujo E2E de Producción para Probar

1. **Login SSO con Keycloak:** Al abrir `http://localhost:5173` verás la pantalla de inicio de sesión. Haz clic en **Iniciar sesión con Keycloak (SSO)** e ingresa con `admin@demo.ikedadev.com` / `Admin123!`.
2. **Validación JWT real en los 4 microservicios:** Una vez autenticado, el BFF reenvía únicamente `Authorization: Bearer <access_token>` (sin `X-Api-Key`), por lo que `events-api`, `crm-api`, `catalog-api` e `identity-api` validan tu token RS256 contra el JWKS de Keycloak y aplican RLS con el `tenant_id` del JWT.
3. **Provisionar un usuario real en Keycloak desde la UI:** Ve a **Identidad y Acceso -> Nuevo Usuario**, crea un usuario con password temporal y rol `tenant:admin` o `tenant:member`. `identity-api` lo creará realmente en Keycloak con el atributo `tenant_id` de tu tenant.
4. **Probar el nuevo usuario:** Haz clic en **Cerrar sesión** en el sidebar e inicia sesión en Keycloak con el usuario que acabas de crear.
5. **Probar el ciclo comercial completo:** Crea cliente -> crea servicio -> consulta disponibilidad -> agenda reserva -> intenta agendar en el mismo horario para ver la alerta `409 Conflict`.
