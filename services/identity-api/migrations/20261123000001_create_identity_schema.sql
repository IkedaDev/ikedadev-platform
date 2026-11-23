-- ==============================================================================
-- IkedaDev Platform: Identity Service Initial Schema
-- File: 20261123000001_create_identity_schema.sql
-- Description:
--   Centralizes platform tenant provisioning, tenant_users (linked to Keycloak),
--   and M2M api_keys with PostgreSQL Row-Level Security (RLS).
-- ==============================================================================

-- ------------------------------------------------------------------------------
-- 1. Tenants Table (Platform-level Registry)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    slug VARCHAR(100) UNIQUE NOT NULL,
    name VARCHAR(255) NOT NULL,
    status VARCHAR(50) NOT NULL DEFAULT 'ACTIVE',
    config JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_tenants_slug
    ON tenants (slug)
    WHERE deleted_at IS NULL;

-- ------------------------------------------------------------------------------
-- 2. Tenant Users Table (Synchronized with Keycloak + RLS)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS tenant_users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    keycloak_user_id UUID NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL,
    full_name VARCHAR(255) NOT NULL,
    roles TEXT[] NOT NULL DEFAULT '{}',
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_tenant_users_tenant_email
    ON tenant_users (tenant_id, email)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_tenant_users_tenant
    ON tenant_users (tenant_id)
    WHERE deleted_at IS NULL;

ALTER TABLE tenant_users ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_users FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_tenant_users ON tenant_users
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 3. API Keys Table (Centralized per-tenant M2M Keys + RLS)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id),
    name VARCHAR(150) NOT NULL,
    key_prefix VARCHAR(16) NOT NULL,
    key_hash VARCHAR(64) NOT NULL UNIQUE,
    scopes TEXT[] NOT NULL DEFAULT '{}',
    is_active BOOLEAN NOT NULL DEFAULT true,
    expires_at TIMESTAMPTZ DEFAULT NULL,
    last_used_at TIMESTAMPTZ DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_identity_api_keys_tenant
    ON api_keys (tenant_id)
    WHERE revoked_at IS NULL;

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_identity_api_keys ON api_keys
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- Pre-RLS authentication function
CREATE OR REPLACE FUNCTION authenticate_api_key(p_key_hash VARCHAR(64))
RETURNS TABLE (tenant_id UUID, scopes TEXT[])
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
    SELECT tenant_id, scopes
    FROM api_keys
    WHERE key_hash = p_key_hash
      AND is_active = true
      AND revoked_at IS NULL
      AND (expires_at IS NULL OR expires_at > NOW())
    LIMIT 1;
$$;
