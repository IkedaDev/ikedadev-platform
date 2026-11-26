-- ==============================================================================
-- IkedaDev Platform: Telemetry Service Initial Schema
-- File: 20261126000001_create_telemetry_schema.sql
-- Description:
--   Append-only multi-tenant telemetry (TRACKING & AUDIT) events with strict
--   PostgreSQL Row-Level Security (RLS) and local api_keys support for isolated
--   integration testing.
-- ==============================================================================

-- ------------------------------------------------------------------------------
-- 1. Telemetry Events Table (Append-Only)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS telemetry_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    kind VARCHAR(20) NOT NULL CHECK (kind IN ('TRACKING', 'AUDIT')),
    event_name VARCHAR(120) NOT NULL,
    source VARCHAR(50) NOT NULL DEFAULT 'web',
    client_id UUID DEFAULT NULL,
    actor_id UUID DEFAULT NULL,
    anonymous_id VARCHAR(128) DEFAULT NULL,
    session_id VARCHAR(128) DEFAULT NULL,
    resource_type VARCHAR(64) DEFAULT NULL,
    resource_id UUID DEFAULT NULL,
    request_id VARCHAR(128) DEFAULT NULL,
    properties JSONB NOT NULL DEFAULT '{}'::jsonb,
    context JSONB NOT NULL DEFAULT '{}'::jsonb,
    occurred_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_telemetry_tenant_occurred
    ON telemetry_events (tenant_id, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_telemetry_tenant_kind_name
    ON telemetry_events (tenant_id, kind, event_name, occurred_at DESC);

CREATE INDEX IF NOT EXISTS idx_telemetry_tenant_client
    ON telemetry_events (tenant_id, client_id, occurred_at DESC)
    WHERE client_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_telemetry_tenant_resource
    ON telemetry_events (tenant_id, resource_type, resource_id)
    WHERE resource_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_telemetry_tenant_session
    ON telemetry_events (tenant_id, session_id)
    WHERE session_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_telemetry_properties_gin
    ON telemetry_events USING GIN (properties);

ALTER TABLE telemetry_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE telemetry_events FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_telemetry_events ON telemetry_events
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 2. Local API Keys Support (Decoupled telemetry_db / Integration Tests)
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

CREATE INDEX IF NOT EXISTS idx_telemetry_api_keys_tenant
    ON api_keys (tenant_id)
    WHERE revoked_at IS NULL;

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_telemetry_api_keys ON api_keys
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
