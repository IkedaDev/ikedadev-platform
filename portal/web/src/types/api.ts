export interface PaginationMeta {
  page: number;
  limit: number;
  total_items: number;
  total_pages: number;
  has_next_page: boolean;
  has_prev_page: boolean;
}

export interface PaginatedResponse<T> {
  success: boolean;
  data: T[];
  pagination: PaginationMeta;
  timestamp?: string;
}

export interface Criteria {
  filter?:
    | { and: Array<{ field: string; operator: string; value?: unknown } | Record<string, unknown>> }
    | { or: Array<{ field: string; operator: string; value?: unknown } | Record<string, unknown>> };
  orderBy?: Array<{ field: string; direction: 'asc' | 'desc' }>;
  pagination?: { page?: number; limit?: number };
}

export interface AuthUser {
  id: string;
  email: string;
  name: string;
  tenant_id: string | null;
  roles: string[];
}

export interface AuthMeResponse {
  authenticated: boolean;
  user: AuthUser | null;
  expiresAt?: string;
}

export interface BffHealth {
  status: 'healthy' | 'degraded';
  service: string;
  timestamp: string;
  upstreams: {
    events: { ok: boolean; status: number };
    crm: { ok: boolean; status: number };
    identity: { ok: boolean; status: number };
    catalog: { ok: boolean; status: number };
    bookings?: { ok: boolean; status: number };
    telemetry?: { ok: boolean; status: number };
    content?: { ok: boolean; status: number };
    media?: { ok: boolean; status: number };
    keycloak?: { ok: boolean; status: number };
  };
}

export type ThemeMode = 'light' | 'dark' | 'system';

export interface TenantBranding {
  displayName: string;
  logoUrl: string;
  primaryColor: string;
  accentColor: string;
  themeMode: ThemeMode;
}

export interface TenantLocale {
  currency: string;
  timezone: string;
  language: 'es' | 'en' | string;
}

export interface TenantFeatures {
  crm: boolean;
  catalog: boolean;
  events: boolean;
  bookings: boolean;
  telemetry: boolean;
  content: boolean;
}

export interface TenantSettings {
  branding: TenantBranding;
  locale: TenantLocale;
  features: TenantFeatures;
}

export interface TenantSettingsUpdate {
  branding?: Partial<TenantBranding>;
  locale?: Partial<TenantLocale>;
  features?: Partial<TenantFeatures>;
}

export interface TenantSettingsResponse {
  tenantId: string;
  slug: string;
  name: string;
  canEdit: boolean;
  settings: TenantSettings;
}

export interface PublicBrandingResponse {
  slug: string;
  branding: TenantBranding;
}

export interface Client {
  id: string;
  tenant_id: string;
  full_name: string;
  email: string | null;
  phone: string | null;
  tax_id: string | null;
  address: string | null;
  notes: string | null;
  is_active: boolean;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface Category {
  id: string;
  parent_id: string | null;
  name: string;
  slug: string;
  description: string | null;
  display_order: number;
  is_active: boolean;
}

export type CatalogItemType = 'SERVICE' | 'PRODUCT' | 'BUNDLE';

export interface CatalogItem {
  id: string;
  category_id: string | null;
  sku: string | null;
  name: string;
  description: string | null;
  base_price: number;
  currency: string;
  tax_rate_bps: number;
  tax_included: boolean;
  item_type: CatalogItemType;
  duration_minutes?: number;
  buffer_minutes?: number;
  unit_of_measure?: string | null;
  track_inventory?: boolean;
  items?: Array<{ item_id: string; quantity: number }>;
  is_active: boolean;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface Calendar {
  id: string;
  name: string;
  timezone: string;
  allow_overlaps: boolean;
  working_hours: Record<string, unknown> | null;
  created_at: string;
}

export interface TimeRange {
  starts_at: string;
  ends_at: string;
}

export interface AvailabilityResult {
  calendar_id: string;
  query_range: TimeRange;
  available_slots: TimeRange[];
  busy_slots: TimeRange[];
}

export type BookingStatus =
  | 'PENDING'
  | 'SCHEDULED'
  | 'IN_PROGRESS'
  | 'COMPLETED'
  | 'CANCELLED'
  | 'NO_SHOW';

export interface BookingItemSnapshot {
  id: string;
  tenant_id: string;
  booking_id: string;
  catalog_item_id: string;
  variant_id?: string | null;
  name_snapshot: string;
  sku_snapshot?: string | null;
  quantity: number;
  unit_price_snapshot: number;
  tax_rate_bps: number;
  tax_included: boolean;
  line_subtotal: number;
  line_tax: number;
  line_total: number;
  duration_minutes_snapshot: number;
  buffer_minutes_snapshot: number;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface EnrichedBooking {
  id: string;
  booking_code: string;
  calendar_id: string | null;
  calendar_name: string | null;
  event_id: string | null;
  starts_at: string;
  ends_at: string;
  timezone: string;
  status: BookingStatus;
  client: {
    id: string;
    full_name: string;
    email: string | null;
    phone: string | null;
  } | null;
  items: BookingItemSnapshot[];
  subtotal: number;
  tax_total: number;
  total_price: number;
  total_duration_minutes: number;
  currency: string;
  notes: string | null;
  cancellation_reason: string | null;
  cancelled_at: string | null;
  completed_at: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
}

export interface DashboardSummary {
  kpis: {
    totalClients: number;
    activeCatalogItems: number;
    upcomingBookingsCount: number;
    upcomingRevenue: number;
    currency: string;
  };
  modules?: {
    crm: boolean;
    catalog: boolean;
    bookings: boolean;
  };
  recentClients: Client[];
  recentCatalogItems: CatalogItem[];
  upcomingBookings: EnrichedBooking[];
}

export interface TenantUser {
  id: string;
  tenant_id: string;
  keycloak_user_id: string;
  email: string;
  full_name: string;
  roles: string[];
  is_active: boolean;
  created_at: string;
}

export interface ApiKeySummary {
  id: string;
  name: string;
  key_prefix: string;
  scopes: string[];
  is_active: boolean;
  expires_at: string | null;
  last_used_at: string | null;
  created_at: string;
  revoked_at: string | null;
}

export interface CreatedApiKey {
  key: ApiKeySummary;
  raw_key: string;
}

export type TelemetryEventKind = 'TRACKING' | 'AUDIT';

export interface TelemetryEvent {
  id: string;
  tenant_id: string;
  kind: TelemetryEventKind;
  event_name: string;
  source: string;
  client_id: string | null;
  actor_id: string | null;
  anonymous_id: string | null;
  session_id: string | null;
  resource_type: string | null;
  resource_id: string | null;
  request_id: string | null;
  properties: Record<string, unknown>;
  context: Record<string, unknown>;
  occurred_at: string;
  received_at: string;
}

export interface TrackEventInput {
  kind?: TelemetryEventKind;
  event_name: string;
  source?: string;
  client_id?: string | null;
  anonymous_id?: string | null;
  session_id?: string | null;
  resource_type?: string | null;
  resource_id?: string | null;
  properties?: Record<string, unknown>;
  context?: Record<string, unknown>;
  occurred_at?: string;
}

export interface TelemetryEventCount {
  event_name: string;
  count: number;
}

export interface TelemetryMetricsSummary {
  total_events: number;
  unique_clients: number;
  unique_sessions: number;
  by_event_name: TelemetryEventCount[];
}

export type ArticleStatus = 'DRAFT' | 'SCHEDULED' | 'PUBLISHED' | 'ARCHIVED';

export interface ArticleSeo {
  meta_title?: string | null;
  meta_description?: string | null;
  canonical_url?: string | null;
  og_image_url?: string | null;
  noindex?: boolean;
}

export interface ContentCategory {
  id: string;
  tenant_id: string;
  name: string;
  slug: string;
  description: string | null;
  display_order: number;
  created_at: string;
  updated_at: string;
}

export interface ContentTag {
  id: string;
  tenant_id: string;
  name: string;
  slug: string;
  created_at: string;
}

export interface Article {
  id: string;
  tenant_id: string;
  category_id: string | null;
  category?: ContentCategory | null;
  tags: ContentTag[];
  slug: string;
  title: string;
  excerpt: string | null;
  content_json: Record<string, unknown>;
  content_html: string;
  cover_image_url: string | null;
  status: ArticleStatus;
  published_at: string | null;
  scheduled_for: string | null;
  author_id: string | null;
  author_name_snapshot: string | null;
  word_count: number;
  reading_time_minutes: number;
  seo: ArticleSeo;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

export interface ArticleRevision {
  id: string;
  article_id: string;
  version_number: number;
  title: string;
  content_json: Record<string, unknown>;
  content_html: string;
  edited_by: string | null;
  created_at: string;
}

export interface PreviewTokenResponse {
  token: string;
  expires_at: string;
  preview_path: string;
}

export type MediaContext = 'general' | 'branding' | 'catalog' | 'content' | 'avatars';
export type MediaStatus = 'PENDING' | 'READY';

export interface MediaAsset {
  id: string;
  tenant_id: string;
  storage_provider: string;
  context: MediaContext;
  status: MediaStatus;
  file_name: string;
  storage_key: string;
  public_url: string;
  mime_type: string;
  size_bytes: number;
  width: number | null;
  height: number | null;
  alt_text: string | null;
  metadata: Record<string, unknown>;
  uploaded_by: string | null;
  created_at: string;
  updated_at: string;
}

export interface PresignedUpload {
  asset_id: string;
  storage_provider: string;
  context: MediaContext;
  storage_key: string;
  upload_url: string;
  public_url: string;
  expires_in_secs: number;
}

export interface MediaQuotaUsage {
  used_bytes: number;
  max_bytes: number;
  available_bytes: number;
  max_file_size_bytes: number;
  asset_count: number;
  usage_percent: number;
}

export interface PublishWebhook {
  id: string;
  tenant_id: string;
  name: string;
  target_url: string;
  events: string[];
  is_active: boolean;
  last_triggered_at: string | null;
  last_status_code: number | null;
  created_at: string;
  updated_at: string;
}

export interface WebhookTestResult {
  webhook_id: string;
  status_code: number;
  ok: boolean;
}
