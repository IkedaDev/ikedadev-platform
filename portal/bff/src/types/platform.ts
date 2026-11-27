// Shared types aligned 1:1 with ikeda-core and the 6 Rust microservices.

export interface ApiResponse<T> {
  success: boolean;
  message?: string;
  data?: T;
  timestamp: string;
}

export interface PaginationMeta {
  page: number;
  limit: number;
  total_items: number;
  total_pages: number;
  has_next_page: boolean;
  has_prev_page: boolean;
}

export interface PaginatedApiResponse<T> {
  success: boolean;
  message?: string;
  data: T[];
  pagination: PaginationMeta;
  timestamp: string;
}

// ============================================================================
// Criteria (ikeda-core)
// ============================================================================

export type FilterOperator =
  | 'eq'
  | 'neq'
  | 'gt'
  | 'gte'
  | 'lt'
  | 'lte'
  | 'contains'
  | 'startsWith'
  | 'endsWith'
  | 'in'
  | 'notIn'
  | 'isNull'
  | 'isNotNull';

export interface FilterCondition {
  field: string;
  operator: FilterOperator;
  value?: unknown;
}

export type FilterGroup =
  | { and: FilterRule[] }
  | { or: FilterRule[] };

export type FilterRule = FilterCondition | FilterGroup;

export interface OrderBy {
  field: string;
  direction: 'asc' | 'desc';
}

export interface Pagination {
  page?: number;
  limit?: number;
}

export interface Criteria {
  filter?: FilterGroup;
  orderBy?: OrderBy[];
  pagination?: Pagination;
}

// ============================================================================
// CRM API (crm-api)
// ============================================================================

export interface Client {
  id: string;
  tenant_id: string;
  full_name: string;
  email: string | null;
  phone: string | null;
  tax_id: string | null;
  address: string | null;
  notes: string | null;
  keycloak_user_id: string | null;
  is_active: boolean;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface CreateClientCommand {
  full_name: string;
  email?: string | null;
  phone?: string | null;
  tax_id?: string | null;
  address?: string | null;
  notes?: string | null;
  keycloak_user_id?: string | null;
  is_active?: boolean;
  metadata?: Record<string, unknown>;
}

export interface UpdateClientCommand {
  full_name?: string;
  email?: string | null;
  phone?: string | null;
  tax_id?: string | null;
  address?: string | null;
  notes?: string | null;
  keycloak_user_id?: string | null;
  is_active?: boolean;
  metadata?: Record<string, unknown>;
}

// ============================================================================
// Catalog API (catalog-api)
// ============================================================================

export interface Category {
  id: string;
  tenant_id: string;
  parent_id: string | null;
  name: string;
  slug: string;
  description: string | null;
  display_order: number;
  is_active: boolean;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface CreateCategoryCommand {
  parent_id?: string | null;
  name: string;
  slug: string;
  description?: string | null;
  display_order?: number;
  is_active?: boolean;
}

export interface UpdateCategoryCommand {
  parent_id?: string | null;
  name?: string;
  slug?: string;
  description?: string | null;
  display_order?: number;
  is_active?: boolean;
}

export interface BundleComponent {
  item_id: string;
  quantity: number;
}

export type ItemDetails =
  | {
      item_type: 'SERVICE';
      duration_minutes: number;
      buffer_minutes?: number;
    }
  | {
      item_type: 'PRODUCT';
      unit_of_measure?: string | null;
      track_inventory?: boolean;
    }
  | {
      item_type: 'BUNDLE';
      items?: BundleComponent[];
    };

export interface CatalogItemVariant {
  id: string;
  item_id: string;
  name: string;
  sku: string | null;
  price_override: number | null;
  duration_override_minutes: number | null;
  is_active: boolean;
}

export interface CatalogItemBase {
  id: string;
  tenant_id: string;
  category_id: string | null;
  sku: string | null;
  name: string;
  description: string | null;
  base_price: number;
  currency: string;
  tax_rate_bps: number;
  tax_included: boolean;
  is_active: boolean;
  metadata: Record<string, unknown>;
  variants?: CatalogItemVariant[];
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export type CatalogItem = CatalogItemBase & ItemDetails;

export type CreateCatalogItemCommand = {
  category_id?: string | null;
  sku?: string | null;
  name: string;
  description?: string | null;
  base_price: number;
  currency?: string;
  tax_rate_bps?: number;
  tax_included?: boolean;
  is_active?: boolean;
  metadata?: Record<string, unknown>;
} & ItemDetails;

export interface UpdateCatalogItemCommand {
  category_id?: string | null;
  sku?: string | null;
  name?: string;
  description?: string | null;
  base_price?: number;
  currency?: string;
  tax_rate_bps?: number;
  tax_included?: boolean;
  details?: ItemDetails;
  is_active?: boolean;
  metadata?: Record<string, unknown>;
}

// ============================================================================
// Events API (events-api)
// ============================================================================

export type EventStatus = 'CONFIRMED' | 'TENTATIVE' | 'CANCELLED';

export interface TimeRange {
  starts_at: string;
  ends_at: string;
}

export interface Calendar {
  id: string;
  tenant_id: string;
  name: string;
  timezone: string;
  allow_overlaps: boolean;
  working_hours: Record<string, unknown> | null;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface CreateCalendarCommand {
  name: string;
  timezone?: string;
  allow_overlaps?: boolean;
  working_hours?: Record<string, unknown> | null;
}

export interface EventRecord {
  id: string;
  tenant_id: string;
  calendar_id: string;
  title: string;
  description: string | null;
  time_range: TimeRange;
  timezone: string;
  rrule: string | null;
  status: EventStatus;
  external_resource_id: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface EventInstance {
  event_id: string;
  calendar_id: string;
  title: string;
  time_range: TimeRange;
  status: EventStatus;
  is_recurring: boolean;
  is_exception: boolean;
}

export interface CreateEventCommand {
  calendar_id: string;
  title: string;
  description?: string | null;
  starts_at: string;
  ends_at: string;
  timezone?: string;
  rrule?: string | null;
  status?: EventStatus;
  external_resource_id?: string | null;
  metadata?: Record<string, unknown>;
}

export interface AvailabilityResult {
  calendar_id: string;
  query_range: TimeRange;
  available_slots: TimeRange[];
  busy_slots: TimeRange[];
}

// ============================================================================
// Identity API (identity-api)
// ============================================================================

export interface Tenant {
  id: string;
  slug: string;
  name: string;
  status: string;
  config: Record<string, unknown>;
  created_at: string;
  updated_at: string;
  deleted_at: string | null;
}

export interface UpdateTenantCommand {
  name?: string;
  status?: string;
  config?: Record<string, unknown>;
}

export type ThemeMode = 'light' | 'dark' | 'system';

export interface TenantBrandingSettings {
  displayName: string;
  logoUrl: string;
  primaryColor: string;
  accentColor: string;
  themeMode: ThemeMode;
}

export interface TenantLocaleSettings {
  currency: string;
  timezone: string;
  language: string;
}

export interface TenantFeatureFlags {
  crm: boolean;
  catalog: boolean;
  events: boolean;
  bookings: boolean;
  telemetry: boolean;
  content: boolean;
}

export interface TenantSettings {
  branding: TenantBrandingSettings;
  locale: TenantLocaleSettings;
  features: TenantFeatureFlags;
}

export interface TenantSettingsPayload {
  tenantId: string;
  slug: string;
  name: string;
  canEdit: boolean;
  settings: TenantSettings;
}

export interface PublicBrandingPayload {
  slug: string;
  branding: TenantBrandingSettings;
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
  updated_at: string;
  deleted_at: string | null;
}

export interface CreateTenantUserCommand {
  email: string;
  full_name: string;
  temporary_password?: string;
  roles?: string[];
  send_invite_email?: boolean;
}

export interface ApiKeySummary {
  id: string;
  tenant_id: string;
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

export interface CreateApiKeyCommand {
  name: string;
  scopes?: string[];
  expires_at?: string | null;
}

// ============================================================================
// Bookings API (bookings-api)
// ============================================================================

export type BookingStatus =
  | 'PENDING'
  | 'SCHEDULED'
  | 'IN_PROGRESS'
  | 'COMPLETED'
  | 'CANCELLED'
  | 'NO_SHOW';

export interface BookingItemRecord {
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

export interface BookingRecord {
  id: string;
  tenant_id: string;
  booking_code: string;
  client_id: string;
  calendar_id?: string | null;
  event_id?: string | null;
  starts_at: string;
  ends_at: string;
  timezone: string;
  status: BookingStatus;
  subtotal: number;
  tax_total: number;
  total_price: number;
  total_duration_minutes: number;
  currency: string;
  notes?: string | null;
  cancellation_reason?: string | null;
  cancelled_at?: string | null;
  completed_at?: string | null;
  metadata: Record<string, unknown>;
  created_by?: string | null;
  updated_by?: string | null;
  items: BookingItemRecord[];
  created_at: string;
  updated_at: string;
  deleted_at?: string | null;
}

export interface BookingItemSnapshotInput {
  catalog_item_id: string;
  variant_id?: string | null;
  name_snapshot: string;
  sku_snapshot?: string | null;
  quantity: number;
  unit_price_snapshot: number;
  tax_rate_bps: number;
  tax_included: boolean;
  duration_minutes_snapshot: number;
  buffer_minutes_snapshot: number;
  metadata?: Record<string, unknown>;
}

export interface CreateBookingPayload {
  client_id: string;
  calendar_id?: string | null;
  event_id?: string | null;
  starts_at: string;
  ends_at: string;
  timezone?: string;
  status?: 'PENDING' | 'SCHEDULED';
  currency?: string;
  notes?: string | null;
  metadata?: Record<string, unknown>;
  items: BookingItemSnapshotInput[];
}

export interface TransitionBookingStatusPayload {
  status: BookingStatus;
  cancellation_reason?: string | null;
  metadata?: Record<string, unknown>;
}

// ============================================================================
// BFF Enriched Models
// ============================================================================

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
  items: BookingItemRecord[];
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

// ============================================================================
// Telemetry API (telemetry-api)
// ============================================================================

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

export interface IngestTelemetryEventPayload {
  kind: TelemetryEventKind;
  event_name: string;
  source?: string;
  client_id?: string | null;
  actor_id?: string | null;
  anonymous_id?: string | null;
  session_id?: string | null;
  resource_type?: string | null;
  resource_id?: string | null;
  request_id?: string | null;
  properties?: Record<string, unknown>;
  context?: Record<string, unknown>;
  occurred_at?: string;
}

export interface IngestBatchResponse {
  accepted: number;
}

export interface TelemetryMetricsQuery {
  from: string;
  to: string;
  kind?: TelemetryEventKind;
  source?: string;
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

// ============================================================================
// Content API (content-api — Headless CMS)
// ============================================================================

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
  deleted_at: string | null;
}

export interface ContentTag {
  id: string;
  tenant_id: string;
  name: string;
  slug: string;
  created_at: string;
  deleted_at: string | null;
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
  deleted_at: string | null;
}

export interface ArticleRevision {
  id: string;
  tenant_id: string;
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

export interface MediaAsset {
  id: string;
  tenant_id: string;
  storage_provider: string;
  file_name: string;
  storage_key: string;
  public_url: string;
  mime_type: string;
  size_bytes: number;
  alt_text: string | null;
  uploaded_by: string | null;
  created_at: string;
  deleted_at: string | null;
}

export interface PresignedUpload {
  asset_id: string;
  storage_provider: string;
  storage_key: string;
  upload_url: string;
  public_url: string;
  expires_in_seconds: number;
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
  deleted_at: string | null;
}

export interface WebhookTestResult {
  webhook_id: string;
  status_code: number;
  ok: boolean;
}
