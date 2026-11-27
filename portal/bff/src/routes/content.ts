import { Hono } from 'hono';
import { z } from 'zod';
import {
  canPublishContent,
  canReadContent,
  canWriteContent,
  forbiddenResponse,
} from '../auth/permissions.js';
import { contentClient } from '../clients/contentClient.js';
import { telemetryClient } from '../clients/telemetryClient.js';
import { getAuthRequestContext as getCtx } from '../middleware/auth.js';
import type { ArticleStatus, Criteria } from '../types/platform.js';

const UUID_REGEX = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function isValidUuid(value: string): boolean {
  return UUID_REGEX.test(value);
}

const articleSeoSchema = z.object({
  meta_title: z.string().max(160).optional().nullable(),
  meta_description: z.string().max(320).optional().nullable(),
  canonical_url: z.string().max(2048).optional().nullable(),
  og_image_url: z.string().max(2048).optional().nullable(),
  noindex: z.boolean().optional(),
});

const createArticleSchema = z.object({
  category_id: z.string().uuid().optional().nullable(),
  slug: z.string().trim().max(180).optional().nullable(),
  title: z.string().trim().min(1, 'El título es obligatorio').max(255),
  excerpt: z.string().optional().nullable(),
  content_json: z.record(z.unknown()).optional(),
  content_html: z.string().optional(),
  cover_image_url: z.string().max(2048).optional().nullable(),
  author_name_snapshot: z.string().max(150).optional().nullable(),
  seo: articleSeoSchema.optional(),
  metadata: z.record(z.unknown()).optional(),
  tag_ids: z.array(z.string().uuid()).optional(),
});

const updateArticleSchema = z.object({
  category_id: z.string().uuid().optional().nullable(),
  slug: z.string().trim().max(180).optional(),
  title: z.string().trim().min(1).max(255).optional(),
  excerpt: z.string().optional().nullable(),
  content_json: z.record(z.unknown()).optional(),
  content_html: z.string().optional(),
  cover_image_url: z.string().max(2048).optional().nullable(),
  seo: articleSeoSchema.optional(),
  metadata: z.record(z.unknown()).optional(),
  tag_ids: z.array(z.string().uuid()).optional(),
});

const updateArticleStatusSchema = z.object({
  status: z.enum(['DRAFT', 'SCHEDULED', 'PUBLISHED', 'ARCHIVED']),
  scheduled_for: z.string().datetime().optional().nullable(),
});

const createCategorySchema = z.object({
  name: z.string().trim().min(1, 'El nombre es obligatorio').max(150),
  slug: z.string().trim().max(150).optional(),
  description: z.string().optional().nullable(),
  display_order: z.coerce.number().int().optional(),
});

const updateCategorySchema = z.object({
  name: z.string().trim().min(1).max(150).optional(),
  slug: z.string().trim().max(150).optional(),
  description: z.string().optional().nullable(),
  display_order: z.coerce.number().int().optional(),
});

const createTagSchema = z.object({
  name: z.string().trim().min(1, 'El nombre es obligatorio').max(100),
  slug: z.string().trim().max(100).optional(),
});

const createWebhookSchema = z.object({
  name: z.string().trim().min(1, 'El nombre es obligatorio').max(150),
  target_url: z.string().url('URL inválida'),
  events: z
    .array(z.enum(['article.published', 'article.updated', 'article.unpublished']))
    .min(1)
    .optional(),
  is_active: z.boolean().optional(),
});

const updateWebhookSchema = z.object({
  name: z.string().trim().min(1).max(150).optional(),
  target_url: z.string().url('URL inválida').optional(),
  events: z
    .array(z.enum(['article.published', 'article.updated', 'article.unpublished']))
    .min(1)
    .optional(),
  is_active: z.boolean().optional(),
});

export const contentRoutes = new Hono();

// ============================================================================
// Articles
// ============================================================================

contentRoutes.get('/articles', async (c) => {
  const ctx = getCtx(c);
  if (!canReadContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para ver los artículos.');
  }
  const page = Math.max(1, Number(c.req.query('page') ?? 1) || 1);
  const limit = Math.min(100, Math.max(1, Number(c.req.query('limit') ?? 20) || 20));
  const status = c.req.query('status')?.trim();
  const q = c.req.query('q')?.trim();
  const categoryId = c.req.query('category_id')?.trim();

  const rules: Array<{ field: string; operator: 'eq' | 'contains'; value: unknown }> = [];
  if (status && ['DRAFT', 'SCHEDULED', 'PUBLISHED', 'ARCHIVED'].includes(status)) {
    rules.push({ field: 'status', operator: 'eq', value: status });
  }
  if (q) {
    rules.push({ field: 'title', operator: 'contains', value: q });
  }
  if (categoryId) {
    if (!isValidUuid(categoryId)) {
      return c.json(
        { success: false, code: 'VALIDATION_ERROR', message: 'category_id debe ser un UUID válido' },
        422,
      );
    }
    rules.push({ field: 'category_id', operator: 'eq', value: categoryId });
  }

  const criteria: Criteria = {
    ...(rules.length > 0 ? { filter: { and: rules } } : {}),
    orderBy: [{ field: 'updated_at', direction: 'desc' }],
    pagination: { page, limit },
  };

  const result = await contentClient.searchArticles(criteria, ctx);
  return c.json(result);
});

contentRoutes.get('/articles/preview', async (c) => {
  const token = c.req.query('token')?.trim();
  if (!token) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'Falta el parámetro token' },
      422,
    );
  }
  const article = await contentClient.previewArticleByToken(token);
  return c.json({ success: true, data: article });
});

contentRoutes.get('/articles/by-slug/:slug', async (c) => {
  const ctx = getCtx(c);
  const slug = c.req.param('slug').trim();
  if (!slug) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'slug es obligatorio' },
      422,
    );
  }
  const article = await contentClient.getArticleBySlug(slug, ctx);
  return c.json({ success: true, data: article });
});

contentRoutes.get('/articles/:id', async (c) => {
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de artículo inválido' },
      422,
    );
  }
  const ctx = getCtx(c);
  const article = await contentClient.getArticle(id, ctx);
  return c.json({ success: true, data: article });
});

contentRoutes.post('/articles', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para crear artículos.');
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = createArticleSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Datos de artículo inválidos',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const article = await contentClient.createArticle(parsed.data, ctx);
  return c.json({ success: true, data: article }, 201);
});

contentRoutes.patch('/articles/:id', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para editar artículos.');
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de artículo inválido' },
      422,
    );
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = updateArticleSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Datos de actualización inválidos',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const article = await contentClient.updateArticle(id, parsed.data, ctx);
  return c.json({ success: true, data: article });
});

contentRoutes.patch('/articles/:id/status', async (c) => {
  const ctx = getCtx(c);
  if (!canPublishContent(ctx.roles)) {
    return forbiddenResponse(
      c,
      'No tienes permisos para publicar, programar o archivar artículos.',
    );
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de artículo inválido' },
      422,
    );
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = updateArticleStatusSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Estado o fecha programada inválida',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const article = await contentClient.updateArticleStatus(
    id,
    {
      status: parsed.data.status as ArticleStatus,
      scheduled_for: parsed.data.scheduled_for ?? null,
    },
    ctx,
  );

  // Fire-and-forget AUDIT telemetry event on editorial lifecycle transitions
  const eventName =
    article.status === 'PUBLISHED'
      ? 'content.article.published'
      : article.status === 'SCHEDULED'
        ? 'content.article.scheduled'
        : article.status === 'ARCHIVED'
          ? 'content.article.archived'
          : 'content.article.status_changed';

  telemetryClient.emitAuditEventAsync(
    {
      event_name: eventName,
      resource_type: 'article',
      resource_id: article.id,
      properties: {
        slug: article.slug,
        title: article.title,
        status: article.status,
        scheduled_for: article.scheduled_for,
      },
    },
    ctx,
  );

  return c.json({ success: true, data: article });
});

contentRoutes.delete('/articles/:id', async (c) => {
  const ctx = getCtx(c);
  if (!canPublishContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para eliminar artículos.');
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de artículo inválido' },
      422,
    );
  }
  await contentClient.deleteArticle(id, ctx);

  telemetryClient.emitAuditEventAsync(
    {
      event_name: 'content.article.deleted',
      resource_type: 'article',
      resource_id: id,
    },
    ctx,
  );

  return c.json({ success: true });
});

contentRoutes.get('/articles/:id/revisions', async (c) => {
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de artículo inválido' },
      422,
    );
  }
  const ctx = getCtx(c);
  const revisions = await contentClient.listRevisions(id, ctx);
  return c.json({ success: true, data: revisions });
});

contentRoutes.post('/articles/:id/revisions/:revisionId/restore', async (c) => {
  const ctx = getCtx(c);
  if (!canPublishContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para restaurar revisiones.');
  }
  const id = c.req.param('id');
  const revisionId = c.req.param('revisionId');
  if (!isValidUuid(id) || !isValidUuid(revisionId)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de artículo o revisión inválido' },
      422,
    );
  }
  const article = await contentClient.restoreRevision(id, revisionId, ctx);
  return c.json({ success: true, data: article });
});

contentRoutes.post('/articles/:id/preview-token', async (c) => {
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de artículo inválido' },
      422,
    );
  }
  const ctx = getCtx(c);
  const token = await contentClient.generatePreviewToken(id, ctx);
  return c.json({ success: true, data: token });
});

// ============================================================================
// Taxonomy (Categories & Tags)
// ============================================================================

contentRoutes.get('/categories', async (c) => {
  const ctx = getCtx(c);
  const data = await contentClient.listCategories(ctx);
  return c.json({ success: true, data });
});

contentRoutes.post('/categories', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para crear categorías editoriales.');
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = createCategorySchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Categoría inválida',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const data = await contentClient.createCategory(parsed.data, ctx);
  return c.json({ success: true, data }, 201);
});

contentRoutes.patch('/categories/:id', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para editar categorías editoriales.');
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de categoría inválido' },
      422,
    );
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = updateCategorySchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Datos de categoría inválidos',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const data = await contentClient.updateCategory(id, parsed.data, ctx);
  return c.json({ success: true, data });
});

contentRoutes.delete('/categories/:id', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para eliminar categorías editoriales.');
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de categoría inválido' },
      422,
    );
  }
  await contentClient.deleteCategory(id, ctx);
  return c.json({ success: true });
});

contentRoutes.get('/tags', async (c) => {
  const ctx = getCtx(c);
  const data = await contentClient.listTags(ctx);
  return c.json({ success: true, data });
});

contentRoutes.post('/tags', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para crear etiquetas.');
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = createTagSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Etiqueta inválida',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const data = await contentClient.createTag(parsed.data, ctx);
  return c.json({ success: true, data }, 201);
});

contentRoutes.delete('/tags/:id', async (c) => {
  const ctx = getCtx(c);
  if (!canWriteContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para eliminar etiquetas.');
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de etiqueta inválido' },
      422,
    );
  }
  await contentClient.deleteTag(id, ctx);
  return c.json({ success: true });
});

// ============================================================================
// Deploy Hooks (SSG)
// ============================================================================

contentRoutes.get('/webhooks', async (c) => {
  const ctx = getCtx(c);
  if (!canPublishContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para ver los Deploy Hooks.');
  }
  const data = await contentClient.listWebhooks(ctx);
  return c.json({ success: true, data });
});

contentRoutes.post('/webhooks', async (c) => {
  const ctx = getCtx(c);
  if (!canPublishContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para crear Deploy Hooks.');
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = createWebhookSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Deploy Hook inválido',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const data = await contentClient.createWebhook(parsed.data, ctx);
  return c.json({ success: true, data }, 201);
});

contentRoutes.patch('/webhooks/:id', async (c) => {
  const ctx = getCtx(c);
  if (!canPublishContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para editar Deploy Hooks.');
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de webhook inválido' },
      422,
    );
  }
  const raw = await c.req.json().catch(() => null);
  const parsed = updateWebhookSchema.safeParse(raw);
  if (!parsed.success) {
    return c.json(
      {
        success: false,
        code: 'VALIDATION_ERROR',
        message: 'Datos de Deploy Hook inválidos',
        details: parsed.error.flatten().fieldErrors,
      },
      422,
    );
  }
  const data = await contentClient.updateWebhook(id, parsed.data, ctx);
  return c.json({ success: true, data });
});

contentRoutes.delete('/webhooks/:id', async (c) => {
  const ctx = getCtx(c);
  if (!canPublishContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para eliminar Deploy Hooks.');
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de webhook inválido' },
      422,
    );
  }
  await contentClient.deleteWebhook(id, ctx);
  return c.json({ success: true });
});

contentRoutes.post('/webhooks/:id/test', async (c) => {
  const ctx = getCtx(c);
  if (!canPublishContent(ctx.roles)) {
    return forbiddenResponse(c, 'No tienes permisos para probar Deploy Hooks.');
  }
  const id = c.req.param('id');
  if (!isValidUuid(id)) {
    return c.json(
      { success: false, code: 'VALIDATION_ERROR', message: 'ID de webhook inválido' },
      422,
    );
  }
  const data = await contentClient.testWebhook(id, ctx);
  return c.json({ success: true, data });
});
