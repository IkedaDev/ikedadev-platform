-- ==============================================================================
-- IkedaDev Platform: Content Service (Headless CMS) Initial Schema
-- File: 20261127000001_create_content_schema.sql
-- Description:
--   Multi-tenant Headless CMS schema (categories, tags, articles, revisions,
--   media_assets, publish_webhooks) with Soft Delete and strict PostgreSQL RLS.
-- ==============================================================================

-- ------------------------------------------------------------------------------
-- 1. Content Categories Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS content_categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    name VARCHAR(150) NOT NULL,
    slug VARCHAR(150) NOT NULL,
    description TEXT DEFAULT NULL,
    display_order INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_content_categories_tenant_slug
    ON content_categories (tenant_id, slug)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_content_categories_tenant
    ON content_categories (tenant_id, display_order)
    WHERE deleted_at IS NULL;

ALTER TABLE content_categories ENABLE ROW LEVEL SECURITY;
ALTER TABLE content_categories FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_content_categories ON content_categories
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 2. Content Tags Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS content_tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_content_tags_tenant_slug
    ON content_tags (tenant_id, slug)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_content_tags_tenant
    ON content_tags (tenant_id)
    WHERE deleted_at IS NULL;

ALTER TABLE content_tags ENABLE ROW LEVEL SECURITY;
ALTER TABLE content_tags FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_content_tags ON content_tags
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 3. Articles Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS articles (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    category_id UUID DEFAULT NULL REFERENCES content_categories(id) ON DELETE SET NULL,
    slug VARCHAR(180) NOT NULL,
    title VARCHAR(255) NOT NULL,
    excerpt TEXT DEFAULT NULL,
    content_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    content_html TEXT NOT NULL DEFAULT '',
    cover_image_url TEXT DEFAULT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'DRAFT'
        CHECK (status IN ('DRAFT', 'SCHEDULED', 'PUBLISHED', 'ARCHIVED')),
    published_at TIMESTAMPTZ DEFAULT NULL,
    scheduled_for TIMESTAMPTZ DEFAULT NULL,
    author_id UUID DEFAULT NULL,
    author_name_snapshot VARCHAR(150) DEFAULT NULL,
    word_count INTEGER NOT NULL DEFAULT 0,
    reading_time_minutes INTEGER NOT NULL DEFAULT 1,
    seo JSONB NOT NULL DEFAULT '{}'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_articles_tenant_slug
    ON articles (tenant_id, slug)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_articles_tenant_status_published
    ON articles (tenant_id, status, published_at DESC)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_articles_metadata
    ON articles USING GIN (metadata)
    WHERE deleted_at IS NULL;

ALTER TABLE articles ENABLE ROW LEVEL SECURITY;
ALTER TABLE articles FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_articles ON articles
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 4. Article Tags Join Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS article_tags (
    tenant_id UUID NOT NULL,
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES content_tags(id) ON DELETE CASCADE,
    PRIMARY KEY (tenant_id, article_id, tag_id)
);

CREATE INDEX IF NOT EXISTS idx_article_tags_tenant_tag
    ON article_tags (tenant_id, tag_id);

ALTER TABLE article_tags ENABLE ROW LEVEL SECURITY;
ALTER TABLE article_tags FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_article_tags ON article_tags
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 5. Article Revisions Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS article_revisions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    article_id UUID NOT NULL REFERENCES articles(id) ON DELETE CASCADE,
    version_number INTEGER NOT NULL CHECK (version_number > 0),
    title VARCHAR(255) NOT NULL,
    content_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    content_html TEXT NOT NULL DEFAULT '',
    edited_by UUID DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT uq_article_revisions_version UNIQUE (tenant_id, article_id, version_number)
);

CREATE INDEX IF NOT EXISTS idx_article_revisions_tenant_article
    ON article_revisions (tenant_id, article_id, version_number DESC);

ALTER TABLE article_revisions ENABLE ROW LEVEL SECURITY;
ALTER TABLE article_revisions FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_article_revisions ON article_revisions
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 6. Media Assets Table
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS media_assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    storage_provider VARCHAR(30) NOT NULL DEFAULT 'R2_PLATFORM',
    file_name VARCHAR(255) NOT NULL,
    storage_key TEXT NOT NULL,
    public_url TEXT NOT NULL,
    mime_type VARCHAR(100) NOT NULL,
    size_bytes BIGINT NOT NULL CHECK (size_bytes > 0),
    alt_text VARCHAR(255) DEFAULT NULL,
    uploaded_by UUID DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS uq_media_assets_tenant_storage_key
    ON media_assets (tenant_id, storage_key)
    WHERE deleted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_media_assets_tenant_created
    ON media_assets (tenant_id, created_at DESC)
    WHERE deleted_at IS NULL;

ALTER TABLE media_assets ENABLE ROW LEVEL SECURITY;
ALTER TABLE media_assets FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_media_assets ON media_assets
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 7. Publish Webhooks Table (SSG Deploy Hooks)
-- ------------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS publish_webhooks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL,
    name VARCHAR(150) NOT NULL,
    target_url TEXT NOT NULL,
    events TEXT[] NOT NULL DEFAULT '{article.published,article.updated,article.unpublished}',
    is_active BOOLEAN NOT NULL DEFAULT true,
    last_triggered_at TIMESTAMPTZ DEFAULT NULL,
    last_status_code SMALLINT DEFAULT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ DEFAULT NULL
);

CREATE INDEX IF NOT EXISTS idx_publish_webhooks_tenant
    ON publish_webhooks (tenant_id)
    WHERE deleted_at IS NULL;

ALTER TABLE publish_webhooks ENABLE ROW LEVEL SECURITY;
ALTER TABLE publish_webhooks FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_publish_webhooks ON publish_webhooks
    FOR ALL
    USING (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid)
    WITH CHECK (tenant_id = NULLIF(current_setting('app.current_tenant', true), '')::uuid);

-- ------------------------------------------------------------------------------
-- 8. Local API Keys Support (Decoupled content_db / Integration Tests)
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

CREATE INDEX IF NOT EXISTS idx_content_api_keys_tenant
    ON api_keys (tenant_id)
    WHERE revoked_at IS NULL;

ALTER TABLE api_keys ENABLE ROW LEVEL SECURITY;
ALTER TABLE api_keys FORCE ROW LEVEL SECURITY;

CREATE POLICY tenant_isolation_content_api_keys ON api_keys
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
