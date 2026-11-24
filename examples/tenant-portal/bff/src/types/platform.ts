// Shared types aligned 1:1 with ikeda-core and the 4 Rust microservices.

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
// BFF Enriched Models
// ============================================================================

export interface EnrichedBookingItem {
  id: string;
  name: string;
  item_type: 'SERVICE' | 'PRODUCT' | 'BUNDLE';
  base_price: number;
  currency: string;
  duration_minutes?: number;
}

export interface EnrichedBooking {
  id: string;
  calendar_id: string;
  calendar_name: string | null;
  title: string;
  starts_at: string;
  ends_at: string;
  status: EventStatus;
  client_id: string | null;
  client_name: string | null;
  client_email: string | null;
  items: EnrichedBookingItem[];
  total_price: number;
  currency: string;
  notes: string | null;
}
