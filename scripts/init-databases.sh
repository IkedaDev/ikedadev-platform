#!/bin/sh
set -eu

# ==============================================================================
# IkedaDev Platform — Local PostgreSQL Multi-Database Initialization
# Runs automatically via /docker-entrypoint-initdb.d/ on fresh Postgres volumes.
#
# Creates:
#   - events_db    (already created by POSTGRES_DB)
#   - crm_db
#   - identity_db
#   - catalog_db
#   - bookings_db
#   - telemetry_db
# Applies all SQL migrations from /migrations/<service>/ and seeds a local dev
# tenant + M2M API key in identity_db for immediate local testing.
# ==============================================================================

echo ">>> [init-databases] Creating additional service databases..."
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" <<-EOSQL
    SELECT 'CREATE DATABASE crm_db'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'crm_db')\gexec

    SELECT 'CREATE DATABASE identity_db'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'identity_db')\gexec

    SELECT 'CREATE DATABASE catalog_db'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'catalog_db')\gexec

    SELECT 'CREATE DATABASE bookings_db'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'bookings_db')\gexec

    SELECT 'CREATE DATABASE telemetry_db'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'telemetry_db')\gexec

    SELECT 'CREATE DATABASE content_db'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'content_db')\gexec

    SELECT 'CREATE DATABASE media_db'
    WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'media_db')\gexec
EOSQL

apply_migrations() {
    db_name="$1"
    dir_path="$2"

    if [ ! -d "$dir_path" ]; then
        echo ">>> [init-databases] Warning: migration directory $dir_path not found, skipping $db_name"
        return
    fi

    for sql_file in "$dir_path"/*.sql; do
        [ -e "$sql_file" ] || continue
        echo ">>> [init-databases] Applying $(basename "$sql_file") to $db_name..."
        psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$db_name" -f "$sql_file"
    done
}

echo ">>> [init-databases] Creating dedicated NOBYPASSRLS application role (ikeda_app)..."
APP_DB_USER="${IKEDA_APP_USER:-ikeda_app}"
APP_DB_PASSWORD="${IKEDA_APP_PASSWORD:-ikeda_app_password}"

psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$POSTGRES_DB" \
    -v app_user="$APP_DB_USER" \
    -v app_password="$APP_DB_PASSWORD" <<-'EOSQL'
    SELECT set_config('init.app_user', :'app_user', false);
    SELECT set_config('init.app_password', :'app_password', false);
    DO $$
    DECLARE
        v_user TEXT := current_setting('init.app_user');
        v_pass TEXT := current_setting('init.app_password');
    BEGIN
        IF NOT EXISTS (SELECT FROM pg_catalog.pg_roles WHERE rolname = v_user) THEN
            EXECUTE format(
                'CREATE ROLE %I LOGIN PASSWORD %L NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS',
                v_user,
                v_pass
            );
        ELSE
            EXECUTE format(
                'ALTER ROLE %I WITH LOGIN PASSWORD %L NOSUPERUSER NOCREATEDB NOCREATEROLE NOINHERIT NOBYPASSRLS',
                v_user,
                v_pass
            );
        END IF;
    END
    $$;
EOSQL

grant_app_privileges() {
    db_name="$1"
    echo ">>> [init-databases] Granting least-privilege NOBYPASSRLS access on $db_name to $APP_DB_USER..."
    psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "$db_name" \
        -v db_name="$db_name" \
        -v app_user="$APP_DB_USER" <<-'EOSQL'
        GRANT CONNECT ON DATABASE :"db_name" TO :"app_user";
        GRANT USAGE ON SCHEMA public TO :"app_user";
        GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA public TO :"app_user";
        GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA public TO :"app_user";
        ALTER DEFAULT PRIVILEGES IN SCHEMA public
            GRANT SELECT, INSERT, UPDATE, DELETE ON TABLES TO :"app_user";
        ALTER DEFAULT PRIVILEGES IN SCHEMA public
            GRANT USAGE, SELECT ON SEQUENCES TO :"app_user";
        SELECT set_config('init.app_user', :'app_user', false);
        DO $$
        BEGIN
            IF EXISTS (
                SELECT 1
                FROM pg_proc p
                JOIN pg_namespace n ON n.oid = p.pronamespace
                WHERE n.nspname = 'public' AND p.proname = 'authenticate_api_key'
            ) THEN
                EXECUTE format(
                    'GRANT EXECUTE ON FUNCTION public.authenticate_api_key(VARCHAR) TO %I',
                    current_setting('init.app_user')
                );
            END IF;
        END
        $$;
EOSQL
}

apply_migrations "events_db"    "/migrations/events"
apply_migrations "crm_db"       "/migrations/crm"
apply_migrations "identity_db"  "/migrations/identity"
apply_migrations "catalog_db"   "/migrations/catalog"
apply_migrations "bookings_db"  "/migrations/bookings"
apply_migrations "telemetry_db" "/migrations/telemetry"
apply_migrations "content_db"   "/migrations/content"
apply_migrations "media_db"     "/migrations/media"

grant_app_privileges "events_db"
grant_app_privileges "crm_db"
grant_app_privileges "identity_db"
grant_app_privileges "catalog_db"
grant_app_privileges "bookings_db"
grant_app_privileges "telemetry_db"
grant_app_privileges "content_db"
grant_app_privileges "media_db"

# Seed a deterministic local development tenant + API key in identity_db
DEV_TENANT_ID="11111111-1111-4111-8111-111111111111"
DEV_API_KEY="${LOCAL_DEV_API_KEY:-ik_live_dev_tenant_portal_local_2026}"
DEV_API_KEY_PREFIX="$(printf '%s' "$DEV_API_KEY" | cut -c1-16)"
DEV_API_KEY_HASH="$(printf '%s' "$DEV_API_KEY" | sha256sum | awk '{print $1}')"

echo ">>> [init-databases] Seeding local dev tenant & API key in identity_db..."
psql -v ON_ERROR_STOP=1 --username "$POSTGRES_USER" --dbname "identity_db" <<-EOSQL
    BEGIN;
    INSERT INTO tenants (id, slug, name, status, config)
    VALUES (
        '${DEV_TENANT_ID}',
        'demo-tenant',
        'Demo Tenant Local',
        'ACTIVE',
        '{"env": "local"}'::jsonb
    )
    ON CONFLICT (id) DO NOTHING;

    SELECT set_config('app.current_tenant', '${DEV_TENANT_ID}', true);

    INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
    VALUES (
        '${DEV_TENANT_ID}',
        'Local Tenant Portal BFF Key',
        '${DEV_API_KEY_PREFIX}',
        '${DEV_API_KEY_HASH}',
        ARRAY['tenant:admin', 'platform:admin', 'events:write', 'crm:write', 'catalog:write', 'identity:write', 'bookings:write', 'telemetry:read', 'telemetry:write', 'content:read', 'content:write', 'content:publish', 'media:read', 'media:write', 'media:delete'],
        true
    )
    ON CONFLICT (key_hash) DO NOTHING;

    INSERT INTO tenant_users (tenant_id, keycloak_user_id, email, full_name, roles, is_active)
    VALUES (
        '${DEV_TENANT_ID}',
        '22222222-2222-4222-8222-222222222222',
        'admin@demo.ikedadev.com',
        'Admin Demo Tenant',
        ARRAY['platform:admin', 'tenant:admin', 'events:write', 'crm:write', 'catalog:write', 'identity:write', 'bookings:write', 'telemetry:read', 'telemetry:write', 'content:read', 'content:write', 'content:publish', 'media:read', 'media:write', 'media:delete'],
        true
    )
    ON CONFLICT (keycloak_user_id) DO NOTHING;

    INSERT INTO tenant_users (tenant_id, keycloak_user_id, email, full_name, roles, is_active)
    VALUES (
        '${DEV_TENANT_ID}',
        '33333333-3333-4333-8333-333333333333',
        'staff@demo.ikedadev.com',
        'Staff Demo Tenant',
        ARRAY['tenant:member', 'events:write', 'crm:write', 'bookings:write', 'telemetry:read', 'telemetry:write', 'content:read', 'content:write', 'media:read', 'media:write'],
        true
    )
    ON CONFLICT (keycloak_user_id) DO NOTHING;
    COMMIT;
EOSQL

echo ">>> [init-databases] All 6 databases initialized and local dev key ready!"
