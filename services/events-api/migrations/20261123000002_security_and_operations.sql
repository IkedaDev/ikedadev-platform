-- ==============================================================================
-- IkedaDev Platform: Security Hardening & Operations Migration
-- File: 20261123000002_security_and_operations.sql
-- Description:
--   1. Adds missing uniqueness constraints & GIN index on existing tables.
--   2. Adds api_keys table + RLS + SECURITY DEFINER authentication function.
--   3. Adds webhook_subscriptions table + RLS.
--   4. Adds idempotency_keys table + RLS.
-- ==============================================================================

-- ------------------------------------------------------------------------------
-- 1. Constraints & Indexes on Existing Tables
-- ------------------------------------------------------------------------------

-- Prevent duplicate exceptions for the exact same occurrence within a tenant
ALTER TABLE event_exceptions
    ADD CONSTRAINT uq_event_exceptions_occurrence UNIQUE (tenant_id, event_id, original_starts_at);

-- Prevent duplicate attendee registration for the same email within a tenant
ALTER TABLE attendees
    ADD CONSTRAINT uq_attendees_event_email UNIQUE (tenant_id, event_id, email);

-- GIN index for high-performance JSONB querying on metadata
CREATE INDEX IF NOT EXISTS idx_events_metadata
    ON events USING GIN (metadata)
    WHERE deleted_at IS NULL;

-- ------------------------------------------------------------------------------
-- 2. API Keys Table & Authentication
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

CREATE INDEX IF NOT EXISTS idx_api_keys_tenant
    ON api_keys(tenant_id)
    WHERE revoked_at IS NULL;

-- Enable and force Row Level Security
ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_api_keys ON api_keys
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- SECURITY DEFINER function to authenticate API Keys before tenant context is set
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

-- ------------------------------------------------------------------------------
-- 3. Webhook Subscriptions Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS webhook_subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    url TEXT NOT NULL,
    secret VARCHAR(255) NOT NULL,
    events TEXT[] NOT NULL DEFAULT '{}',
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_webhooks_tenant
    ON webhook_subscriptions(tenant_id)
    WHERE deleted_at IS NULL;

-- Enable and force Row Level Security
ALTER TABLE webhook_subscriptions ENABLE ROW LEVEL SECURITY;
ALTER TABLE webhook_subscriptions FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_webhook_subscriptions ON webhook_subscriptions
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 4. Idempotency Keys Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS idempotency_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    idempotency_key VARCHAR(255) NOT NULL,
    request_hash VARCHAR(64) NOT NULL,
    response_status SMALLINT NOT NULL,
    response_body JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    CONSTRAINT uq_idempotency_tenant_key UNIQUE (tenant_id, idempotency_key)
);

-- Enable and force Row Level Security
ALTER TABLE idempotency_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE idempotency_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_idempotency_keys ON idempotency_keys
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);
