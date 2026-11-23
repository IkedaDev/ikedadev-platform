-- ==============================================================================
-- IkedaDev Platform: Catalog Service Initial Schema
-- File: 20261123000001_create_catalog_schema.sql
-- Description:
--   Unified multi-tenant commercial catalog (categories, services, products,
--   bundles, variants) with Soft Delete and strict PostgreSQL RLS.
-- ==============================================================================

-- ------------------------------------------------------------------------------
-- 1. Categories Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    parent_id UUID DEFAULT NULL REFERENCES categories(id) ON DELETE SET NULL,
    name VARCHAR(150) NOT NULL,
    slug VARCHAR(150) NOT NULL,
    description TEXT DEFAULT NULL,
    display_order INTEGER NOT NULL DEFAULT 0,
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_categories_tenant_slug
    ON categories (tenant_id, slug)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_categories_tenant
    ON categories (tenant_id)
    WHERE deleted_at IS NULL;

ALTER TABLE categories ENABLE ROW LEVEL SECURITY;
ALTER TABLE categories FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_categories ON categories
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 2. Catalog Items Table (SERVICE | PRODUCT | BUNDLE)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS catalog_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    category_id UUID DEFAULT NULL REFERENCES categories(id) ON DELETE SET NULL,
    item_type VARCHAR(20) NOT NULL CHECK (item_type IN ('SERVICE', 'PRODUCT', 'BUNDLE')),
    sku VARCHAR(100) DEFAULT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT DEFAULT NULL,
    base_price BIGINT NOT NULL CHECK (base_price >= 0),
    currency VARCHAR(3) NOT NULL DEFAULT 'CLP',
    tax_rate_bps INTEGER NOT NULL DEFAULT 0 CHECK (tax_rate_bps >= 0 AND tax_rate_bps <= 10000),
    tax_included BOOLEAN NOT NULL DEFAULT true,
    duration_minutes INTEGER DEFAULT NULL,
    buffer_minutes INTEGER NOT NULL DEFAULT 0 CHECK (buffer_minutes >= 0),
    unit_of_measure VARCHAR(20) DEFAULT NULL,
    track_inventory BOOLEAN NOT NULL DEFAULT false,
    is_active BOOLEAN NOT NULL DEFAULT true,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL,
    CONSTRAINT chk_service_duration CHECK (
        item_type <> 'SERVICE' OR (duration_minutes IS NOT NULL AND duration_minutes > 0)
    )
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_catalog_items_tenant_sku
    ON catalog_items (tenant_id, sku)
    WHERE sku IS NOT NULL AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_catalog_items_tenant_type
    ON catalog_items (tenant_id, item_type)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_catalog_items_tenant_category
    ON catalog_items (tenant_id, category_id)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_catalog_items_metadata
    ON catalog_items USING GIN (metadata)
    WHERE deleted_at IS NULL;

ALTER TABLE catalog_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE catalog_items FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_catalog_items ON catalog_items
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 3. Catalog Item Variants Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS catalog_item_variants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    item_id UUID NOT NULL REFERENCES catalog_items(id) ON DELETE CASCADE,
    name VARCHAR(150) NOT NULL,
    sku VARCHAR(100) DEFAULT NULL,
    price_override BIGINT DEFAULT NULL CHECK (price_override IS NULL OR price_override >= 0),
    duration_override INTEGER DEFAULT NULL CHECK (duration_override IS NULL OR duration_override > 0),
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_variants_tenant_sku
    ON catalog_item_variants (tenant_id, sku)
    WHERE sku IS NOT NULL AND deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_variants_tenant_item
    ON catalog_item_variants (tenant_id, item_id)
    WHERE deleted_at IS NULL;

ALTER TABLE catalog_item_variants ENABLE ROW LEVEL SECURITY;
ALTER TABLE catalog_item_variants FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_catalog_item_variants ON catalog_item_variants
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 4. Bundle Items Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS bundle_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    bundle_id UUID NOT NULL REFERENCES catalog_items(id) ON DELETE CASCADE,
    item_id UUID NOT NULL REFERENCES catalog_items(id) ON DELETE RESTRICT,
    quantity INTEGER NOT NULL DEFAULT 1 CHECK (quantity > 0),
    CONSTRAINT uq_bundle_item UNIQUE (tenant_id, bundle_id, item_id),
    CONSTRAINT chk_no_self_bundle CHECK (bundle_id <> item_id)
);

CREATE INDEX IF NOT EXISTS idx_bundle_items_tenant_bundle
    ON bundle_items (tenant_id, bundle_id);

ALTER TABLE bundle_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE bundle_items FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_bundle_items ON bundle_items
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 5. Local API Keys Support (Decoupled catalog_db / Integration Tests)
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

CREATE INDEX IF NOT EXISTS idx_catalog_api_keys_tenant
    ON api_keys (tenant_id)
    WHERE revoked_at IS NULL;

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_catalog_api_keys ON api_keys
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
