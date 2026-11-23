-- ==============================================================================
-- IkedaDev Platform: CRM Service Initial Schema
-- File: 20261123000001_create_crm_schema.sql
-- Description:
--   Multi-tenant end-customer (clients) database with partial unique indexes,
--   JSONB GIN indexing, strict Row-Level Security (RLS), and local api_keys auth.
-- ==============================================================================

-- ------------------------------------------------------------------------------
-- 1. Clients Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS clients (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    full_name VARCHAR(255) NOT NULL,
    email VARCHAR(255) DEFAULT NULL,
    phone VARCHAR(50) DEFAULT NULL,
    tax_id VARCHAR(50) DEFAULT NULL,
    address TEXT DEFAULT NULL,
    notes TEXT DEFAULT NULL,
    keycloak_user_id UUID DEFAULT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

-- Partial unique constraints (ignore soft-deleted records)
CREATE UNIQUE INDEX IF NOT EXISTS uq_clients_tenant_email
    ON clients (tenant_id, email)
    WHERE email IS NOT NULL AND deleted_at IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_clients_tenant_tax_id
    ON clients (tenant_id, tax_id)
    WHERE tax_id IS NOT NULL AND deleted_at IS NULL;

-- Performance indexes
CREATE INDEX IF NOT EXISTS idx_clients_tenant
    ON clients (tenant_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_clients_tenant_full_name
    ON clients (tenant_id, full_name)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_clients_metadata
    ON clients USING GIN (metadata)
    WHERE deleted_at IS NULL;

-- Enable and force Row-Level Security
ALTER TABLE clients ENABLE ROW LEVEL SECURITY;
ALTER TABLE clients FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_clients ON clients
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 2. API Keys Table & Pre-RLS Authentication Function (Decoupled crm_db)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
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

CREATE INDEX IF NOT EXISTS idx_crm_api_keys_tenant
    ON api_keys (tenant_id)
    WHERE revoked_at IS NULL;

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_crm_api_keys ON api_keys
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

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
