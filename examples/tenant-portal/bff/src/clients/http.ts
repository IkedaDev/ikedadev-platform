import { config } from '../config.js';
import type { ApiResponse } from '../types/platform.js';

export interface RequestContext {
  authorization?: string;
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
  apiKey?: string;
}

export class ServiceHttpClient {
  private readonly serviceName: string;
  private readonly baseUrl: string;
  private readonly apiKey: string;

  constructor(options: HttpClientOptions) {
    this.serviceName = options.serviceName;
    this.baseUrl = options.baseUrl.replace(/\/+$/, '');
    this.apiKey = options.apiKey ?? config.TENANT_API_KEY;
  }

  public async request<T>(
    method: 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE',
    path: string,
    options?: {
      body?: unknown;
      query?: Record<string, string | number | boolean | undefined | null>;
      ctx?: RequestContext;
      extraHeaders?: Record<string, string>;
    },
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

    // Production auth priority:
    // - When a user's Keycloak Bearer JWT is present, forward ONLY `Authorization: Bearer`
    //   so Rust microservices validate RS256 JWKS and extract tenant_id, user_id, and roles.
    // - Otherwise (e.g. CLI seed script), authenticate M2M via `X-Api-Key`.
    if (options?.ctx?.authorization) {
      headers['Authorization'] = options.ctx.authorization;
    } else {
      headers['X-Api-Key'] = this.apiKey;
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
    options?: {
      body?: unknown;
      query?: Record<string, string | number | boolean | undefined | null>;
      ctx?: RequestContext;
      extraHeaders?: Record<string, string>;
    },
  ): Promise<T> {
    const envelope = await this.request<ApiResponse<T>>(method, path, options);
    return envelope.data as T;
  }

  public async health(): Promise<{ ok: boolean; status: number }> {
    try {
      const res = await fetch(`${this.baseUrl}/health`, {
        headers: { Accept: 'application/json' },
      });
      return { ok: res.ok, status: res.status };
    } catch {
      return { ok: false, status: 503 };
    }
  }
}
