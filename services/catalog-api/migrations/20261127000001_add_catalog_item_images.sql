-- ==============================================================================
-- IkedaDev Platform: Add Image Support to Catalog Items
-- File: 20261127000001_add_catalog_item_images.sql
-- Description:
--   Additive migration adding cover image_url and optional gallery_urls (max 8)
--   to catalog_items. Existing RLS policies on catalog_items apply automatically.
-- ==============================================================================

ALTER TABLE catalog_items
    ADD COLUMN IF NOT EXISTS image_url TEXT DEFAULT NULL,
    ADD COLUMN IF NOT EXISTS gallery_urls TEXT[] NOT NULL DEFAULT '{}';

ALTER TABLE catalog_items
    DROP CONSTRAINT IF EXISTS chk_catalog_items_image_url_len,
    ADD CONSTRAINT chk_catalog_items_image_url_len
        CHECK (image_url IS NULL OR char_length(image_url) <= 2048);

ALTER TABLE catalog_items
    DROP CONSTRAINT IF EXISTS chk_catalog_items_gallery_urls_max,
    ADD CONSTRAINT chk_catalog_items_gallery_urls_max
        CHECK (cardinality(gallery_urls) <= 8);
