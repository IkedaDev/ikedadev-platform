import { config } from '../config.js';
import type {
  Criteria,
  MediaAsset,
  MediaContext,
  MediaQuotaUsage,
  PaginatedApiResponse,
  PresignedUpload,
} from '../types/platform.js';
import { ServiceHttpClient, type RequestContext } from './http.js';

const http = new ServiceHttpClient({
  serviceName: 'media-api',
  baseUrl: config.MEDIA_API_URL,
});

export const mediaClient = {
  health: () => http.health(),

  presign(
    payload: {
      file_name: string;
      mime_type: string;
      size_bytes: number;
      context?: MediaContext;
    },
    ctx?: RequestContext,
  ): Promise<PresignedUpload> {
    return http.requestData<PresignedUpload>('POST', '/v1/media/presign', { body: payload, ctx });
  },

  confirm(
    payload: {
      asset_id: string;
      alt_text?: string | null;
      width?: number | null;
      height?: number | null;
      metadata?: Record<string, unknown>;
    },
    ctx?: RequestContext,
  ): Promise<MediaAsset> {
    return http.requestData<MediaAsset>('POST', '/v1/media/confirm', { body: payload, ctx });
  },

  list(
    params: { context?: MediaContext; page?: number; page_size?: number },
    ctx?: RequestContext,
  ): Promise<PaginatedApiResponse<MediaAsset>> {
    return http.request<PaginatedApiResponse<MediaAsset>>('GET', '/v1/media', {
      query: {
        context: params.context,
        page: params.page ?? 1,
        page_size: params.page_size ?? 24,
      },
      ctx,
    });
  },

  search(criteria: Criteria, ctx?: RequestContext): Promise<PaginatedApiResponse<MediaAsset>> {
    return http.request<PaginatedApiResponse<MediaAsset>>('POST', '/v1/media/search', {
      body: criteria,
      ctx,
    });
  },

  getQuota(ctx?: RequestContext): Promise<MediaQuotaUsage> {
    return http.requestData<MediaQuotaUsage>('GET', '/v1/media/quota', { ctx });
  },

  getById(id: string, ctx?: RequestContext): Promise<MediaAsset> {
    return http.requestData<MediaAsset>('GET', `/v1/media/${encodeURIComponent(id)}`, { ctx });
  },

  update(
    id: string,
    payload: {
      alt_text?: string | null;
      context?: MediaContext;
      metadata?: Record<string, unknown>;
    },
    ctx?: RequestContext,
  ): Promise<MediaAsset> {
    return http.requestData<MediaAsset>('PATCH', `/v1/media/${encodeURIComponent(id)}`, {
      body: payload,
      ctx,
    });
  },

  delete(id: string, permanent: boolean, ctx?: RequestContext): Promise<void> {
    return http.request<void>('DELETE', `/v1/media/${encodeURIComponent(id)}`, {
      query: permanent ? { permanent: 'true' } : undefined,
      ctx,
    });
  },

  restore(id: string, ctx?: RequestContext): Promise<MediaAsset> {
    return http.requestData<MediaAsset>('POST', `/v1/media/${encodeURIComponent(id)}/restore`, {
      ctx,
    });
  },
};
