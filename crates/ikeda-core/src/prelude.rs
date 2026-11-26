//! Convenient re-exports of common platform primitives.

pub use crate::criteria::{
    Criteria, FilterCondition, FilterGroup, FilterOperator, FilterRule, OrderBy, OrderDirection,
    Pagination, DEFAULT_LIMIT, DEFAULT_PAGE, MAX_LIMIT,
};
pub use crate::error::{AppError, ValidationErrorDetail};
pub use crate::observability::{
    http_trace_layer, init_tracing, request_id_layer, LogFormat, ObservabilityConfig,
    DEFAULT_RUST_LOG, REQUEST_ID_HEADER,
};
pub use crate::response::{ApiResponse, PaginatedApiResponse, PaginationMeta};
pub use crate::tenant::{
    hash_api_key, ApiKeyValidator, AuthSource, AuthenticatedKey, AuthenticatedUser,
    CachedApiKeyValidator, JwksConfig, JwksJwtValidator, JwtValidator, TenantContext,
    DEFAULT_API_KEY_CACHE_TTL_SECS, HEADER_API_KEY, HEADER_SCOPES, HEADER_TENANT_ID,
    HEADER_USER_ID,
};
