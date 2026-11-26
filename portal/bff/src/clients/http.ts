import { config } from '../config.js';
import type { ApiResponse } from '../types/platform.js';
import { getGcpIdentityToken } from './gcpAuth.js';

export interface RequestContext {
  authorization?: string;
  requestId?: string;
  actorId?: string;
  tenantId?: string;
  roles?: string[];
}

export class UpstreamApiError extends Error {
  public readonly status: number;
  public readonly service: string;
  public readonly details?: unknown;

  constructor(service: string, status: number, message: string, details?: unknown) {
    super(message);
    this.name = 'UpstreamApiError';
    this.service = service;
    this.status = status;
    this.details = details;
  }
}

export interface HttpClientOptions {
  serviceName: string;
  baseUrl: string;
}

export interface ServiceRequestOptions {
  body?: unknown;
  query?: Record<string, string | number | boolean | undefined | null>;
  ctx?: RequestContext;
  extraHeaders?: Record<string, string>;
  /**
   * Explicit M2M API Key override. Used ONLY by trusted server routes (e.g. `settings.ts`
   * with `PLATFORM_ADMIN_API_KEY`) or CLI scripts (`seed.ts`).
   * When omitted, requests strictly forward the user's `Authorization: Bearer <jwt>`.
   */
  apiKeyOverride?: string;
}

export class ServiceHttpClient {
  private readonly serviceName: string;
  private readonly baseUrl: string;

  constructor(options: HttpClientOptions) {
    this.serviceName = options.serviceName;
    this.baseUrl = options.baseUrl.replace(/\/+$/, '');
  }

  public async request<T>(
    method: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE',
    path: string,
    options?: ServiceRequestOptions,
  ): Promise<T> {
    const url = new URL(`${this.baseUrl}${path.startsWith('/') ? path : `/${path}`}`);
    if (options?.query) {
      for (const [key, value] of Object.entries(options.query)) {
        if (value !== undefined && value !== null && value !== '') {
          url.searchParams.set(key, String(value));
        }
      }
    }

    const headers: Record<string, string> = {
      Accept: 'application/json',
      ...options?.extraHeaders,
    };

    if (options?.body !== undefined) {
      headers['Content-Type'] = 'application/json';
    }

    if (options?.ctx?.requestId) {
      headers['X-Request-Id'] = options.ctx.requestId;
    }

    // Strict multi-tenant auth propagation:
    // - Use `X-Api-Key` ONLY when an explicit `apiKeyOverride` is passed (e.g. settings.ts
    //   with PLATFORM_ADMIN_API_KEY, or CLI seed script).
    // - Otherwise, forward ONLY the authenticated user's `Authorization: Bearer <access_token>`
    //   so Rust microservices validate RS256 JWKS and extract tenant_id, user_id, and roles.
    if (options?.apiKeyOverride) {
      headers['X-Api-Key'] = options.apiKeyOverride;
    } else if (options?.ctx?.authorization) {
      headers['Authorization'] = options.ctx.authorization;
    }

    // Optional GCP Cloud Run private service-to-service IAM authentication.
    // Sent in X-Serverless-Authorization so Authorization / X-Api-Key stay intact for ikeda-core.
    if (config.GCP_IAM_AUTH) {
      const idToken = await getGcpIdentityToken(this.baseUrl);
      headers['X-Serverless-Authorization'] = `Bearer ${idToken}`;
    }

    let response: Response;
    try {
      response = await fetch(url.toString(), {
        method,
        headers,
        body: options?.body !== undefined ? JSON.stringify(options.body) : undefined,
      });
    } catch (err) {
      const reason = err instanceof Error ? err.message : String(err);
      throw new UpstreamApiError(
        this.serviceName,
        503,
        `Unable to reach ${this.serviceName} at ${this.baseUrl}: ${reason}`,
      );
    }

    const rawText = await response.text();
    let parsed: unknown = null;
    if (rawText) {
      try {
        parsed = JSON.parse(rawText);
      } catch {
        parsed = rawText;
      }
    }

    if (!response.ok) {
      const message =
        typeof parsed === 'object' &&
        parsed !== null &&
        'message' in parsed &&
        typeof (parsed as { message: unknown }).message === 'string'
          ? (parsed as { message: string }).message
          : `${this.serviceName} responded with HTTP ${response.status}`;

      throw new UpstreamApiError(this.serviceName, response.status, message, parsed);
    }

    return parsed as T;
  }

  /**
   * Convenience helper that unwraps `ApiResponse<T>.data`.
   */
  public async requestData<T>(
    method: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE',
    path: string,
    options?: ServiceRequestOptions,
  ): Promise<T> {
    const envelope = await this.request<ApiResponse<T>>(method, path, options);
    return envelope.data as T;
  }

  public async health(): Promise<{ ok: boolean; status: number }> {
    try {
      const headers: Record<string, string> = { Accept: 'application/json' };
      if (config.GCP_IAM_AUTH) {
        const idToken = await getGcpIdentityToken(this.baseUrl);
        headers['X-Serverless-Authorization'] = `Bearer ${idToken}`;
      }
      const res = await fetch(`${this.baseUrl}/health`, { headers });
      return { ok: res.ok, status: res.status };
    } catch {
      return { ok: false, status: 503 };
    }
  }
}
