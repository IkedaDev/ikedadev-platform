import { config } from '../config.js';
import type {
  Article,
  ArticleRevision,
  ArticleStatus,
  ContentCategory,
  ContentTag,
  Criteria,
  PaginatedApiResponse,
  PreviewTokenResponse,
  PublishWebhook,
  WebhookTestResult,
} from '../types/platform.js';
import { ServiceHttpClient, type RequestContext } from './http.js';

const http = new ServiceHttpClient({
  serviceName: 'content-api',
  baseUrl: config.CONTENT_API_URL,
});

export const contentClient = {
  health: () => http.health(),

  // Articles
  searchArticles(criteria: Criteria, ctx?: RequestContext): Promise<PaginatedApiResponse<Article>> {
    return http.request<PaginatedApiResponse<Article>>('POST', '/v1/articles/search', {
      body: criteria,
      ctx,
    });
  },

  getArticle(id: string, ctx?: RequestContext): Promise<Article> {
    return http.requestData<Article>('GET', `/v1/articles/${encodeURIComponent(id)}`, { ctx });
  },

  getArticleBySlug(slug: string, ctx?: RequestContext): Promise<Article> {
    return http.requestData<Article>('GET', `/v1/articles/by-slug/${encodeURIComponent(slug)}`, {
      ctx,
    });
  },

  createArticle(payload: Record<string, unknown>, ctx?: RequestContext): Promise<Article> {
    return http.requestData<Article>('POST', '/v1/articles', { body: payload, ctx });
  },

  updateArticle(
    id: string,
    payload: Record<string, unknown>,
    ctx?: RequestContext,
  ): Promise<Article> {
    return http.requestData<Article>('PATCH', `/v1/articles/${encodeURIComponent(id)}`, {
      body: payload,
      ctx,
    });
  },

  updateArticleStatus(
    id: string,
    payload: { status: ArticleStatus; scheduled_for?: string | null },
    ctx?: RequestContext,
  ): Promise<Article> {
    return http.requestData<Article>('PATCH', `/v1/articles/${encodeURIComponent(id)}/status`, {
      body: payload,
      ctx,
    });
  },

  deleteArticle(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/articles/${encodeURIComponent(id)}`, { ctx });
  },

  listRevisions(articleId: string, ctx?: RequestContext): Promise<ArticleRevision[]> {
    return http.requestData<ArticleRevision[]>(
      'GET',
      `/v1/articles/${encodeURIComponent(articleId)}/revisions`,
      { ctx },
    );
  },

  restoreRevision(
    articleId: string,
    revisionId: string,
    ctx?: RequestContext,
  ): Promise<Article> {
    return http.requestData<Article>(
      'POST',
      `/v1/articles/${encodeURIComponent(articleId)}/revisions/${encodeURIComponent(revisionId)}/restore`,
      { ctx },
    );
  },

  generatePreviewToken(articleId: string, ctx?: RequestContext): Promise<PreviewTokenResponse> {
    return http.requestData<PreviewTokenResponse>(
      'POST',
      `/v1/articles/${encodeURIComponent(articleId)}/preview-token`,
      { ctx },
    );
  },

  previewArticleByToken(token: string): Promise<Article> {
    return http.requestData<Article>('GET', '/v1/articles/preview', {
      query: { token },
    });
  },

  // Categories & Tags
  listCategories(ctx?: RequestContext): Promise<ContentCategory[]> {
    return http.requestData<ContentCategory[]>('GET', '/v1/categories', { ctx });
  },

  createCategory(
    payload: Record<string, unknown>,
    ctx?: RequestContext,
  ): Promise<ContentCategory> {
    return http.requestData<ContentCategory>('POST', '/v1/categories', { body: payload, ctx });
  },

  updateCategory(
    id: string,
    payload: Record<string, unknown>,
    ctx?: RequestContext,
  ): Promise<ContentCategory> {
    return http.requestData<ContentCategory>('PATCH', `/v1/categories/${encodeURIComponent(id)}`, {
      body: payload,
      ctx,
    });
  },

  deleteCategory(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/categories/${encodeURIComponent(id)}`, { ctx });
  },

  listTags(ctx?: RequestContext): Promise<ContentTag[]> {
    return http.requestData<ContentTag[]>('GET', '/v1/tags', { ctx });
  },

  createTag(payload: Record<string, unknown>, ctx?: RequestContext): Promise<ContentTag> {
    return http.requestData<ContentTag>('POST', '/v1/tags', { body: payload, ctx });
  },

  deleteTag(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/tags/${encodeURIComponent(id)}`, { ctx });
  },

  // Deploy Hooks (SSG)
  listWebhooks(ctx?: RequestContext): Promise<PublishWebhook[]> {
    return http.requestData<PublishWebhook[]>('GET', '/v1/webhooks', { ctx });
  },

  createWebhook(
    payload: Record<string, unknown>,
    ctx?: RequestContext,
  ): Promise<PublishWebhook> {
    return http.requestData<PublishWebhook>('POST', '/v1/webhooks', { body: payload, ctx });
  },

  updateWebhook(
    id: string,
    payload: Record<string, unknown>,
    ctx?: RequestContext,
  ): Promise<PublishWebhook> {
    return http.requestData<PublishWebhook>('PATCH', `/v1/webhooks/${encodeURIComponent(id)}`, {
      body: payload,
      ctx,
    });
  },

  deleteWebhook(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/webhooks/${encodeURIComponent(id)}`, { ctx });
  },

  testWebhook(id: string, ctx?: RequestContext): Promise<WebhookTestResult> {
    return http.requestData<WebhookTestResult>(
      'POST',
      `/v1/webhooks/${encodeURIComponent(id)}/test`,
      { ctx },
    );
  },
};
