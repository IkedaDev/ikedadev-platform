#!/bin/sh
set -eu

# ==============================================================================
# IkedaDev Platform — Local PostgreSQL Multi-Database Initialization
# Runs automatically via /docker-entrypoint-initdb.d/ on fresh Postgres volumes.
#
# Creates:
#   - events_db   (already created by POSTGRES_DB)
#   - crm_db
#   - identity_db
#   - catalog_db
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

apply_migrations "events_db"   "/migrations/events"
apply_migrations "crm_db"      "/migrations/crm"
apply_migrations "identity_db" "/migrations/identity"
apply_migrations "catalog_db"  "/migrations/catalog"

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
    ON CONFLICT (slug) DO NOTHING;

    SELECT set_config('app.current_tenant', '${DEV_TENANT_ID}', true);

    INSERT INTO api_keys (tenant_id, name, key_prefix, key_hash, scopes, is_active)
    VALUES (
        '${DEV_TENANT_ID}',
        'Local Tenant Portal BFF Key',
        '${DEV_API_KEY_PREFIX}',
        '${DEV_API_KEY_HASH}',
        ARRAY['tenant:admin', 'platform:admin', 'events:write', 'crm:write', 'catalog:write', 'identity:write'],
        true
    )
    ON CONFLICT (key_hash) DO NOTHING;
    COMMIT;
EOSQL

echo ">>> [init-databases] All 4 databases initialized and local dev key ready!"
