import type {
  ApiKeySummary,
  Article,
  ArticleRevision,
  ArticleStatus,
  AuthMeResponse,
  AvailabilityResult,
  BffHealth,
  BookingStatus,
  Calendar,
  CatalogItem,
  Category,
  Client,
  ContentCategory,
  ContentTag,
  CreatedApiKey,
  Criteria,
  DashboardSummary,
  EnrichedBooking,
  MediaAsset,
  PaginatedResponse,
  PresignedUpload,
  PreviewTokenResponse,
  PublicBrandingResponse,
  PublishWebhook,
  TelemetryEvent,
  TelemetryEventKind,
  TelemetryMetricsSummary,
  TenantSettingsResponse,
  TenantSettingsUpdate,
  TenantUser,
  TrackEventInput,
  WebhookTestResult,
} from '../types/api';

const BASE_URL = (import.meta.env.VITE_BFF_URL || 'http://localhost:4000').replace(/\/+$/, '');

export class BffApiError extends Error {
  public readonly status: number;
  public readonly code?: string;
  public readonly details?: unknown;
  public notified = false;

  constructor(status: number, message: string, code?: string, details?: unknown) {
    super(message);
    this.name = 'BffApiError';
    this.status = status;
    this.code = code;
    this.details = details;
  }
}

export type GlobalErrorNotifier = (error: BffApiError) => void;
export type UnauthorizedHandler = () => void;

let globalErrorNotifier: GlobalErrorNotifier | null = null;
let unauthorizedHandler: UnauthorizedHandler | null = null;

export function registerGlobalErrorNotifier(notifier: GlobalErrorNotifier | null): () => void {
  globalErrorNotifier = notifier;
  return () => {
    if (globalErrorNotifier === notifier) {
      globalErrorNotifier = null;
    }
  };
}

export function registerUnauthorizedHandler(handler: UnauthorizedHandler | null): () => void {
  unauthorizedHandler = handler;
  return () => {
    if (unauthorizedHandler === handler) {
      unauthorizedHandler = null;
    }
  };
}

function notifyIfHandledStatus(error: BffApiError, silent?: boolean): void {
  if (silent || !globalErrorNotifier) return;
  if (
    error.status === 0 ||
    error.status === 403 ||
    error.status === 409 ||
    error.status === 422
  ) {
    error.notified = true;
    globalErrorNotifier(error);
  }
}

async function request<T>(
  method: 'GET' | 'POST' | 'PATCH' | 'DELETE',
  path: string,
  options?: {
    body?: unknown;
    query?: Record<string, string | number | undefined>;
    silent?: boolean;
    skipUnauthorizedHandler?: boolean;
  },
): Promise<T> {
  const url = new URL(`${BASE_URL}${path}`);
  if (options?.query) {
    for (const [k, v] of Object.entries(options.query)) {
      if (v !== undefined && v !== '') {
        url.searchParams.set(k, String(v));
      }
    }
  }

  const headers: Record<string, string> = {
    Accept: 'application/json',
  };
  if (options?.body !== undefined) {
    headers['Content-Type'] = 'application/json';
  }

  let res: Response;
  try {
    res = await fetch(url.toString(), {
      method,
      headers,
      credentials: 'include',
      body: options?.body !== undefined ? JSON.stringify(options.body) : undefined,
    });
  } catch (err) {
    const apiErr = new BffApiError(
      0,
      'No se pudo conectar con el servidor. Verifica tu conexión a internet e intenta nuevamente.',
      'NETWORK_ERROR',
      err,
    );
    notifyIfHandledStatus(apiErr, options?.silent);
    throw apiErr;
  }

  const text = await res.text();
  let data: Record<string, unknown> | null = null;
  if (text) {
    try {
      data = JSON.parse(text) as Record<string, unknown>;
    } catch {
      data = { message: text };
    }
  }

  if (!res.ok) {
    const apiErr = new BffApiError(
      res.status,
      typeof data?.message === 'string' ? data.message : `Error HTTP ${res.status}`,
      typeof data?.code === 'string' ? data.code : undefined,
      data?.details ?? data?.errors,
    );
    if (
      res.status === 401 &&
      path.startsWith('/api/') &&
      !options?.skipUnauthorizedHandler &&
      unauthorizedHandler
    ) {
      unauthorizedHandler();
    }
    notifyIfHandledStatus(apiErr, options?.silent);
    throw apiErr;
  }

  return data as T;
}

function unwrapData<T>(payload: T | { success?: boolean; data?: T }): T {
  if (
    payload !== null &&
    typeof payload === 'object' &&
    'data' in payload &&
    (payload as { data?: T }).data !== undefined
  ) {
    return (payload as { data: T }).data;
  }
  return payload as T;
}

export const bffClient = {
  baseUrl: BASE_URL,

  loginUrl: () => `${BASE_URL}/auth/login`,

  getMe: () => request<AuthMeResponse>('GET', '/auth/me', { silent: true }),

  logout: () => request<{ success: boolean; logoutUrl: string }>('POST', '/auth/logout'),

  health: () => request<BffHealth>('GET', '/health', { silent: true }),

  // Tenant Settings & Public Branding
  getSettings: async (opts?: {
    silent?: boolean;
    skipUnauthorizedHandler?: boolean;
  }): Promise<TenantSettingsResponse> => {
    const res = await request<TenantSettingsResponse | { success: boolean; data: TenantSettingsResponse }>(
      'GET',
      '/api/settings',
      {
        silent: opts?.silent ?? true,
        skipUnauthorizedHandler: opts?.skipUnauthorizedHandler,
      },
    );
    return unwrapData(res);
  },

  updateSettings: async (payload: TenantSettingsUpdate): Promise<TenantSettingsResponse> => {
    const res = await request<TenantSettingsResponse | { success: boolean; data: TenantSettingsResponse }>(
      'PATCH',
      '/api/settings',
      { body: payload },
    );
    return unwrapData(res);
  },

  getPublicBranding: async (slug: string): Promise<PublicBrandingResponse> => {
    const safeSlug = encodeURIComponent(slug.trim().toLowerCase());
    const res = await request<PublicBrandingResponse | { success: boolean; data: PublicBrandingResponse }>(
      'GET',
      `/api/branding/${safeSlug}`,
      { silent: true },
    );
    return unwrapData(res);
  },

  // Dashboard
  getDashboard: async () => {
    const res = await request<{ success: boolean; data: DashboardSummary }>('GET', '/api/dashboard');
    return res.data;
  },

  // CRM Clients
  searchClients: (criteria: Criteria) =>
    request<PaginatedResponse<Client>>('POST', '/api/clients/search', { body: criteria }),

  createClient: async (payload: {
    full_name: string;
    email?: string | null;
    phone?: string | null;
    tax_id?: string | null;
    address?: string | null;
    notes?: string | null;
    metadata?: Record<string, unknown>;
  }) => {
    const res = await request<{ success: boolean; data: Client }>('POST', '/api/clients', {
      body: payload,
    });
    return res.data;
  },

  updateClient: async (
    id: string,
    payload: Partial<{
      full_name: string;
      email: string | null;
      phone: string | null;
      tax_id: string | null;
      address: string | null;
      notes: string | null;
      is_active: boolean;
      metadata: Record<string, unknown>;
    }>,
  ) => {
    const res = await request<{ success: boolean; data: Client }>('PATCH', `/api/clients/${id}`, {
      body: payload,
    });
    return res.data;
  },

  deleteClient: (id: string) => request<{ success: boolean }>('DELETE', `/api/clients/${id}`),

  // Catalog Categories & Items
  listCategories: async () => {
    const res = await request<{ success: boolean; data: Category[] }>('GET', '/api/categories');
    return res.data;
  },

  createCategory: async (payload: {
    name: string;
    slug: string;
    description?: string | null;
    display_order?: number;
  }) => {
    const res = await request<{ success: boolean; data: Category }>('POST', '/api/categories', {
      body: payload,
    });
    return res.data;
  },

  updateCategory: async (
    id: string,
    payload: Partial<{
      name: string;
      slug: string;
      description: string | null;
      display_order: number;
      is_active: boolean;
    }>,
  ) => {
    const res = await request<{ success: boolean; data: Category }>(
      'PATCH',
      `/api/categories/${id}`,
      { body: payload },
    );
    return res.data;
  },

  deleteCategory: (id: string) => request<{ success: boolean }>('DELETE', `/api/categories/${id}`),

  searchItems: (criteria: Criteria) =>
    request<PaginatedResponse<CatalogItem>>('POST', '/api/items/search', { body: criteria }),

  createItem: async (payload: Record<string, unknown>) => {
    const res = await request<{ success: boolean; data: CatalogItem }>('POST', '/api/items', {
      body: payload,
    });
    return res.data;
  },

  updateItem: async (id: string, payload: Record<string, unknown>) => {
    const res = await request<{ success: boolean; data: CatalogItem }>(
      'PATCH',
      `/api/items/${id}`,
      { body: payload },
    );
    return res.data;
  },

  deleteItem: (id: string) => request<{ success: boolean }>('DELETE', `/api/items/${id}`),

  // Calendars & Availability
  listCalendars: async () => {
    const res = await request<{ success: boolean; data: Calendar[] }>('GET', '/api/calendars');
    return res.data;
  },

  createCalendar: async (payload: {
    name: string;
    timezone: string;
    allow_overlaps: boolean;
  }) => {
    const res = await request<{ success: boolean; data: Calendar }>('POST', '/api/calendars', {
      body: payload,
    });
    return res.data;
  },

  getAvailability: async (
    calendarId: string,
    params: { starts_at: string; ends_at: string; slot_duration_minutes?: number },
  ) => {
    const res = await request<{ success: boolean; data: AvailabilityResult }>(
      'GET',
      `/api/calendars/${calendarId}/availability`,
      { query: params },
    );
    return res.data;
  },

  // Bookings (BFF Orchestrated: catalog-api + events-api + bookings-api)
  listBookings: async (params: {
    start?: string;
    end?: string;
    calendarId?: string;
    status?: BookingStatus;
  }) => {
    const res = await request<{ success: boolean; data: EnrichedBooking[] }>(
      'GET',
      '/api/bookings',
      { query: params },
    );
    return res.data;
  },

  createBooking: async (payload: {
    clientId: string;
    calendarId: string;
    catalogItemIds: string[];
    startsAt: string;
    notes?: string;
  }) => {
    const res = await request<{ success: boolean; data: EnrichedBooking }>(
      'POST',
      '/api/bookings',
      { body: payload },
    );
    return res.data;
  },

  transitionBookingStatus: async (
    id: string,
    payload: {
      status: BookingStatus;
      cancellationReason?: string;
    },
  ) => {
    const res = await request<{ success: boolean; data: EnrichedBooking }>(
      'PATCH',
      `/api/bookings/${id}/status`,
      { body: payload },
    );
    return res.data;
  },

  deleteBooking: (id: string) => request<{ success: boolean }>('DELETE', `/api/bookings/${id}`),

  // Identity
  listUsers: async () => {
    const res = await request<{ success: boolean; data: TenantUser[] }>(
      'GET',
      '/api/identity/users',
    );
    return res.data;
  },

  createUser: async (payload: {
    email: string;
    full_name: string;
    temporary_password?: string;
    roles: string[];
  }) => {
    const res = await request<{ success: boolean; data: TenantUser }>(
      'POST',
      '/api/identity/users',
      { body: payload },
    );
    return res.data;
  },

  deleteUser: (id: string) =>
    request<{ success: boolean }>('DELETE', `/api/identity/users/${id}`),

  listApiKeys: async () => {
    const res = await request<{ success: boolean; data: ApiKeySummary[] }>(
      'GET',
      '/api/identity/api-keys',
    );
    return res.data;
  },

  createApiKey: async (payload: { name: string; scopes: string[] }) => {
    const res = await request<{ success: boolean; data: CreatedApiKey }>(
      'POST',
      '/api/identity/api-keys',
      { body: payload },
    );
    return res.data;
  },

  revokeApiKey: (id: string) =>
    request<{ success: boolean }>('DELETE', `/api/identity/api-keys/${id}`),

  // Telemetry & Audit
  trackTelemetryEvents: async (events: TrackEventInput[]) => {
    const res = await request<{ success: boolean; data: { accepted: number } }>(
      'POST',
      '/api/telemetry/track',
      { body: { events }, silent: true },
    );
    return res.data;
  },

  listTelemetryEvents: (params?: {
    page?: number;
    limit?: number;
    kind?: TelemetryEventKind;
    event_name?: string;
    source?: string;
  }) =>
    request<PaginatedResponse<TelemetryEvent>>('GET', '/api/telemetry/events', {
      query: params,
    }),

  getClientTimeline: (
    clientId: string,
    params?: {
      page?: number;
      limit?: number;
      kind?: TelemetryEventKind;
    },
  ) =>
    request<PaginatedResponse<TelemetryEvent>>(
      'GET',
      `/api/telemetry/clients/${encodeURIComponent(clientId)}/timeline`,
      { query: params },
    ),

  getTelemetryMetrics: async (params?: {
    from?: string;
    to?: string;
    kind?: TelemetryEventKind;
    source?: string;
  }) => {
    const res = await request<{
      success: boolean;
      data: TelemetryMetricsSummary;
      meta?: { from: string; to: string };
    }>('GET', '/api/telemetry/metrics', {
      query: params,
    });
    return res.data;
  },

  // Content CMS (Headless Blog)
  listArticles: (params?: {
    page?: number;
    limit?: number;
    status?: ArticleStatus | '';
    q?: string;
    category_id?: string;
  }) =>
    request<PaginatedResponse<Article>>('GET', '/api/content/articles', {
      query: params,
    }),

  getArticle: async (id: string) => {
    const res = await request<{ success: boolean; data: Article }>(
      'GET',
      `/api/content/articles/${encodeURIComponent(id)}`,
    );
    return res.data;
  },

  createArticle: async (payload: Record<string, unknown>) => {
    const res = await request<{ success: boolean; data: Article }>('POST', '/api/content/articles', {
      body: payload,
    });
    return res.data;
  },

  updateArticle: async (id: string, payload: Record<string, unknown>) => {
    const res = await request<{ success: boolean; data: Article }>(
      'PATCH',
      `/api/content/articles/${encodeURIComponent(id)}`,
      { body: payload },
    );
    return res.data;
  },

  updateArticleStatus: async (
    id: string,
    payload: { status: ArticleStatus; scheduled_for?: string | null },
  ) => {
    const res = await request<{ success: boolean; data: Article }>(
      'PATCH',
      `/api/content/articles/${encodeURIComponent(id)}/status`,
      { body: payload },
    );
    return res.data;
  },

  deleteArticle: (id: string) =>
    request<{ success: boolean }>('DELETE', `/api/content/articles/${encodeURIComponent(id)}`),

  listArticleRevisions: async (id: string) => {
    const res = await request<{ success: boolean; data: ArticleRevision[] }>(
      'GET',
      `/api/content/articles/${encodeURIComponent(id)}/revisions`,
    );
    return res.data;
  },

  restoreArticleRevision: async (articleId: string, revisionId: string) => {
    const res = await request<{ success: boolean; data: Article }>(
      'POST',
      `/api/content/articles/${encodeURIComponent(articleId)}/revisions/${encodeURIComponent(revisionId)}/restore`,
    );
    return res.data;
  },

  generateArticlePreviewToken: async (id: string) => {
    const res = await request<{ success: boolean; data: PreviewTokenResponse }>(
      'POST',
      `/api/content/articles/${encodeURIComponent(id)}/preview-token`,
    );
    return res.data;
  },

  listContentCategories: async () => {
    const res = await request<{ success: boolean; data: ContentCategory[] }>(
      'GET',
      '/api/content/categories',
    );
    return res.data;
  },

  createContentCategory: async (payload: {
    name: string;
    slug?: string;
    description?: string | null;
    display_order?: number;
  }) => {
    const res = await request<{ success: boolean; data: ContentCategory }>(
      'POST',
      '/api/content/categories',
      { body: payload },
    );
    return res.data;
  },

  updateContentCategory: async (
    id: string,
    payload: {
      name?: string;
      slug?: string;
      description?: string | null;
      display_order?: number;
    },
  ) => {
    const res = await request<{ success: boolean; data: ContentCategory }>(
      'PATCH',
      `/api/content/categories/${encodeURIComponent(id)}`,
      { body: payload },
    );
    return res.data;
  },

  deleteContentCategory: (id: string) =>
    request<{ success: boolean }>('DELETE', `/api/content/categories/${encodeURIComponent(id)}`),

  listContentTags: async () => {
    const res = await request<{ success: boolean; data: ContentTag[] }>('GET', '/api/content/tags');
    return res.data;
  },

  createContentTag: async (payload: { name: string; slug?: string }) => {
    const res = await request<{ success: boolean; data: ContentTag }>('POST', '/api/content/tags', {
      body: payload,
    });
    return res.data;
  },

  deleteContentTag: (id: string) =>
    request<{ success: boolean }>('DELETE', `/api/content/tags/${encodeURIComponent(id)}`),

  listMediaAssets: (params?: { page?: number; page_size?: number }) =>
    request<PaginatedResponse<MediaAsset>>('GET', '/api/content/media', { query: params }),

  presignMediaUpload: async (payload: {
    file_name: string;
    mime_type: string;
    size_bytes: number;
  }) => {
    const res = await request<{ success: boolean; data: PresignedUpload }>(
      'POST',
      '/api/content/media/presign',
      { body: payload },
    );
    return res.data;
  },

  confirmMediaUpload: async (payload: {
    asset_id: string;
    file_name: string;
    storage_key: string;
    public_url?: string | null;
    mime_type: string;
    size_bytes: number;
    alt_text?: string | null;
  }) => {
    const res = await request<{ success: boolean; data: MediaAsset }>(
      'POST',
      '/api/content/media/confirm',
      { body: payload },
    );
    return res.data;
  },

  updateMediaAsset: async (id: string, payload: { alt_text?: string | null }) => {
    const res = await request<{ success: boolean; data: MediaAsset }>(
      'PATCH',
      `/api/content/media/${encodeURIComponent(id)}`,
      { body: payload },
    );
    return res.data;
  },

  deleteMediaAsset: (id: string) =>
    request<{ success: boolean }>('DELETE', `/api/content/media/${encodeURIComponent(id)}`),

  listContentWebhooks: async () => {
    const res = await request<{ success: boolean; data: PublishWebhook[] }>(
      'GET',
      '/api/content/webhooks',
    );
    return res.data;
  },

  createContentWebhook: async (payload: {
    name: string;
    target_url: string;
    events?: string[];
    is_active?: boolean;
  }) => {
    const res = await request<{ success: boolean; data: PublishWebhook }>(
      'POST',
      '/api/content/webhooks',
      { body: payload },
    );
    return res.data;
  },

  updateContentWebhook: async (
    id: string,
    payload: {
      name?: string;
      target_url?: string;
      events?: string[];
      is_active?: boolean;
    },
  ) => {
    const res = await request<{ success: boolean; data: PublishWebhook }>(
      'PATCH',
      `/api/content/webhooks/${encodeURIComponent(id)}`,
      { body: payload },
    );
    return res.data;
  },

  deleteContentWebhook: (id: string) =>
    request<{ success: boolean }>('DELETE', `/api/content/webhooks/${encodeURIComponent(id)}`),

  testContentWebhook: async (id: string) => {
    const res = await request<{ success: boolean; data: WebhookTestResult }>(
      'POST',
      `/api/content/webhooks/${encodeURIComponent(id)}/test`,
    );
    return res.data;
  },
};
