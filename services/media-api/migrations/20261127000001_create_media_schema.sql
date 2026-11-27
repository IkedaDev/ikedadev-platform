-- ==============================================================================
-- IkedaDev Platform: Transversal Media Service Initial Schema
-- File: 20261127000001_create_media_schema.sql
-- Description:
--   Multi-tenant media assets & tenant storage quotas with strict PostgreSQL RLS.
-- ==============================================================================

-- ------------------------------------------------------------------------------
-- 1. Media Assets Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS media_assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    storage_provider VARCHAR(30) NOT NULL DEFAULT 'R2_PLATFORM',
    context VARCHAR(30) NOT NULL DEFAULT 'general'
        CHECK (context IN ('general', 'branding', 'catalog', 'content', 'avatars')),
    status VARCHAR(20) NOT NULL DEFAULT 'PENDING'
        CHECK (status IN ('PENDING', 'READY')),
    file_name VARCHAR(255) NOT NULL,
    storage_key TEXT NOT NULL,
    public_url TEXT NOT NULL,
    mime_type VARCHAR(100) NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),
    width INTEGER DEFAULT NULL CHECK (width IS NULL OR width > 0),
    height INTEGER DEFAULT NULL CHECK (height IS NULL OR height > 0),
    alt_text VARCHAR(255) DEFAULT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    uploaded_by UUID DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_media_assets_tenant_storage_key
    ON media_assets (tenant_id, storage_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_media_assets_tenant_context_ready
    ON media_assets (tenant_id, context, created_at DESC)
    WHERE deleted_at IS NULL AND status = 'READY';

CREATE INDEX IF NOT EXISTS idx_media_assets_tenant_quota
    ON media_assets (tenant_id, status)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_media_assets_metadata
    ON media_assets USING GIN (metadata)
    WHERE deleted_at IS NULL;

ALTER TABLE media_assets ENABLE ROW LEVEL SECURITY;
ALTER TABLE media_assets FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_media_assets ON media_assets
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 2. Tenant Quotas Table (Optional per-tenant override over env defaults)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS tenant_quotas (
    tenant_id UUID PRIMARY KEY,
    max_storage_bytes BIGINT NOT NULL DEFAULT 524288000 CHECK (max_storage_bytes > 0),
    max_file_size_bytes BIGINT NOT NULL DEFAULT 5242880 CHECK (max_file_size_bytes > 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

ALTER TABLE tenant_quotas ENABLE ROW LEVEL SECURITY;
ALTER TABLE tenant_quotas FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_tenant_quotas ON tenant_quotas
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 3. Local API Keys Support (Decoupled media_db / Integration Tests)
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

CREATE INDEX IF NOT EXISTS idx_media_api_keys_tenant
    ON api_keys (tenant_id)
    WHERE revoked_at IS NULL;

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_media_api_keys ON api_keys
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
