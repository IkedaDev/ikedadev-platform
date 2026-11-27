-- ==============================================================================
-- IkedaDev Platform: Identity Security Hardening & Soft-Delete Partial Unique
-- File: 20261126000002_hardening_and_partial_unique.sql
-- Description:
--   1. [RT-07] Enforce ACTIVE, non-deleted tenant status inside authenticate_api_key.
--   2. [RS-11] Replace unconditional tenants.slug UNIQUE with partial unique index
--      WHERE deleted_at IS NULL so soft-deleted tenant slugs can be reused.
-- ==============================================================================

CREATE OR REPLACE FUNCTION authenticate_api_key(p_key_hash VARCHAR(64))
RETURNS TABLE (tenant_id UUID, scopes TEXT[])
LANGUAGE sql
SECURITY DEFINER
SET search_path = public
AS $$
    SELECT k.tenant_id, k.scopes
    FROM api_keys k
    INNER JOIN tenants t ON t.id = k.tenant_id
    WHERE k.key_hash = p_key_hash
      AND k.is_active = true
      AND k.revoked_at IS NULL
      AND (k.expires_at IS NULL OR k.expires_at > NOW())
      AND t.status = 'ACTIVE'
      AND t.deleted_at IS NULL
    LIMIT 1;
$$;

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'ikeda_app') THEN
        GRANT EXECUTE ON FUNCTION public.authenticate_api_key(VARCHAR) TO ikeda_app;
    END IF;
END $$;

ALTER TABLE tenants DROP CONSTRAINT IF EXISTS tenants_slug_key;
DROP INDEX IF EXISTS idx_tenants_slug;
CREATE UNIQUE INDEX IF NOT EXISTS uq_tenants_slug
    ON tenants (slug)
    WHERE deleted_at IS NULL;
