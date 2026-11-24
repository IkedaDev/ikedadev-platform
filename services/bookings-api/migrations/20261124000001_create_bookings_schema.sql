-- ==============================================================================
-- IkedaDev Platform: Bookings Service Initial Schema
-- File: 20261124000001_create_bookings_schema.sql
-- Description:
--   100% tenant-agnostic multi-service bookings with frozen commercial snapshots,
--   transactional idempotency keys, Soft Delete, and strict PostgreSQL RLS.
-- ==============================================================================

-- ------------------------------------------------------------------------------
-- 1. Bookings Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS bookings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    booking_code VARCHAR(32) NOT NULL,
    client_id UUID NOT NULL,
    calendar_id UUID DEFAULT NULL,
    event_id UUID DEFAULT NULL,
    starts_at TIMESTAMPTZ NOT NULL,
    ends_at TIMESTAMPTZ NOT NULL,
    timezone VARCHAR(100) NOT NULL DEFAULT 'UTC',
    status VARCHAR(20) NOT NULL DEFAULT 'SCHEDULED'
        CHECK (status IN ('PENDING', 'SCHEDULED', 'IN_PROGRESS', 'COMPLETED', 'CANCELLED', 'NO_SHOW')),
    subtotal BIGINT NOT NULL CHECK (subtotal >= 0),
    tax_total BIGINT NOT NULL DEFAULT 0 CHECK (tax_total >= 0),
    total_price BIGINT NOT NULL CHECK (total_price >= 0),
    total_duration_minutes INTEGER NOT NULL CHECK (total_duration_minutes > 0),
    currency VARCHAR(3) NOT NULL DEFAULT 'CLP',
    notes TEXT DEFAULT NULL,
    cancellation_reason TEXT DEFAULT NULL,
    cancelled_at TIMESTAMPTZ DEFAULT NULL,
    completed_at TIMESTAMPTZ DEFAULT NULL,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_by UUID DEFAULT NULL,
    updated_by UUID DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL,
    CONSTRAINT chk_bookings_time_range CHECK (ends_at > starts_at)
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_bookings_tenant_code
    ON bookings (tenant_id, booking_code)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_bookings_tenant_starts_at
    ON bookings (tenant_id, starts_at DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_bookings_tenant_client
    ON bookings (tenant_id, client_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_bookings_tenant_status
    ON bookings (tenant_id, status)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_bookings_tenant_event
    ON bookings (tenant_id, event_id)
    WHERE event_id IS NOT NULL AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_bookings_metadata
    ON bookings USING GIN (metadata)
    WHERE deleted_at IS NULL;

ALTER TABLE bookings ENABLE ROW LEVEL SECURITY;
ALTER TABLE bookings FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_bookings ON bookings
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 2. Booking Items Table (Frozen Commercial Snapshots)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS booking_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    booking_id UUID NOT NULL REFERENCES bookings(id) ON DELETE CASCADE,
    catalog_item_id UUID NOT NULL,
    variant_id UUID DEFAULT NULL,
    name_snapshot VARCHAR(255) NOT NULL,
    sku_snapshot VARCHAR(100) DEFAULT NULL,
    quantity INTEGER NOT NULL DEFAULT 1 CHECK (quantity > 0),
    unit_price_snapshot BIGINT NOT NULL CHECK (unit_price_snapshot >= 0),
    tax_rate_bps INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate_bps >= 0 AND tax_rate_bps <= 10000),
    tax_included BOOLEAN NOT NULL DEFAULT true,
    line_subtotal BIGINT NOT NULL CHECK (line_subtotal >= 0),
    line_tax BIGINT NOT NULL DEFAULT 0 CHECK (line_tax >= 0),
    line_total BIGINT NOT NULL CHECK (line_total >= 0),
    duration_minutes_snapshot INTEGER NOT NULL DEFAULT 0 CHECK (duration_minutes_snapshot >= 0),
    buffer_minutes_snapshot INTEGER NOT NULL DEFAULT 0 CHECK (buffer_minutes_snapshot >= 0),
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_booking_items_booking
    ON booking_items (tenant_id, booking_id);

ALTER TABLE booking_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE booking_items FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_booking_items ON booking_items
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 3. Idempotency Keys Table
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
    CONSTRAINT uq_bookings_idempotency_tenant_key UNIQUE (tenant_id, idempotency_key)
);

ALTER TABLE idempotency_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE idempotency_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_bookings_idempotency_keys ON idempotency_keys
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 4. Local API Keys Support (Decoupled bookings_db / Integration Tests)
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

CREATE INDEX IF NOT EXISTS idx_bookings_api_keys_tenant
    ON api_keys (tenant_id)
    WHERE revoked_at IS NULL;

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_bookings_api_keys ON api_keys
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
