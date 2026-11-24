pub mod filter;
pub mod operator;
pub mod order;
pub mod pagination;

#[cfg(feature = "sqlx")]
pub mod sqlx_builder;

#[cfg(test)]
mod tests;

pub use filter::{Criteria, FilterCondition, FilterGroup, FilterRule};
pub use operator::FilterOperator;
pub use order::{OrderBy, OrderDirection};
pub use pagination::{Pagination, DEFAULT_LIMIT, DEFAULT_PAGE, MAX_LIMIT};

#[cfg(feature = "sqlx")]
pub use sqlx_builder::{
    append_filter_group, append_order_by, resolve_metadata_field, ColumnKind, ResolvedField,
    MAX_FILTER_DEPTH, MAX_METADATA_KEY_LEN,
};
