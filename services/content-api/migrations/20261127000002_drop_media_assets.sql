-- ==============================================================================
-- IkedaDev Platform: Content Service — Extract Media Library to media-api
-- File: 20261127000002_drop_media_assets.sql
-- Description:
--   Media management has been decoupled into dedicated `services/media-api` (media_db).
--   `content-api` remains 100% agnostic and only stores validated HTTPS/HTTP image URLs.
-- ==============================================================================

DROP TABLE IF EXISTS media_assets;
