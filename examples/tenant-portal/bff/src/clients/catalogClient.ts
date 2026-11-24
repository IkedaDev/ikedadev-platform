import { config } from '../config.js';
import type {
  CatalogItem,
  Category,
  CreateCatalogItemCommand,
  CreateCategoryCommand,
  Criteria,
  PaginatedApiResponse,
  UpdateCatalogItemCommand,
} from '../types/platform.js';
import { ServiceHttpClient, type RequestContext } from './http.js';

const http = new ServiceHttpClient({
  serviceName: 'catalog-api',
  baseUrl: config.CATALOG_API_URL,
});

export const catalogClient = {
  health: () => http.health(),

  listCategories(ctx?: RequestContext): Promise<Category[]> {
    return http.requestData<Category[]>('GET', '/v1/categories', { ctx });
  },

  createCategory(cmd: CreateCategoryCommand, ctx?: RequestContext): Promise<Category> {
    return http.requestData<Category>('POST', '/v1/categories', { body: cmd, ctx });
  },

  deleteCategory(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/categories/${id}`, { ctx });
  },

  searchItems(criteria: Criteria, ctx?: RequestContext): Promise<PaginatedApiResponse<CatalogItem>> {
    return http.request<PaginatedApiResponse<CatalogItem>>('POST', '/v1/items/search', {
      body: criteria,
      ctx,
    });
  },

  getItem(id: string, ctx?: RequestContext): Promise<CatalogItem> {
    return http.requestData<CatalogItem>('GET', `/v1/items/${id}`, { ctx });
  },

  async batchGetItems(ids: string[], ctx?: RequestContext): Promise<CatalogItem[]> {
    const unique = Array.from(new Set(ids.filter(Boolean)));
    if (unique.length === 0) return [];
    return http.requestData<CatalogItem[]>('POST', '/v1/items/batch', {
      body: { ids: unique },
      ctx,
    });
  },

  createItem(cmd: CreateCatalogItemCommand, ctx?: RequestContext): Promise<CatalogItem> {
    return http.requestData<CatalogItem>('POST', '/v1/items', { body: cmd, ctx });
  },

  updateItem(
    id: string,
    cmd: UpdateCatalogItemCommand,
    ctx?: RequestContext,
  ): Promise<CatalogItem> {
    return http.requestData<CatalogItem>('PATCH', `/v1/items/${id}`, { body: cmd, ctx });
  },

  deleteItem(id: string, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/items/${id}`, { ctx });
  },
};
