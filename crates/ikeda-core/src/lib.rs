//! # Ikeda Core
//!
//! Shared core library for the `ikedadev-platform` microservices ecosystem.
//! Provides uniform API responses, centralized error handling, multi-tenant
//! request context extraction, and dynamic criteria filtering.

pub mod criteria;
pub mod error;
pub mod observability;
pub mod prelude;
pub mod response;
pub mod tenant;

pub use criteria::{
    Criteria, FilterCondition, FilterGroup, FilterOperator, FilterRule, OrderBy, OrderDirection,
    Pagination,
};
pub use error::{AppError, ValidationErrorDetail};
pub use observability::{http_trace_layer, init_tracing, request_id_layer, REQUEST_ID_HEADER};
pub use response::{ApiResponse, PaginatedApiResponse, PaginationMeta};
pub use tenant::{AuthSource, TenantContext};
