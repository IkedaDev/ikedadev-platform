use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Supported SSG Deploy Hook event names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum WebhookEvent {
    #[serde(rename = "article.published")]
    ArticlePublished,
    #[serde(rename = "article.updated")]
    ArticleUpdated,
    #[serde(rename = "article.unpublished")]
    ArticleUnpublished,
}

impl WebhookEvent {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ArticlePublished => "article.published",
            Self::ArticleUpdated => "article.updated",
            Self::ArticleUnpublished => "article.unpublished",
        }
    }
}

/// Tenant-configured SSG Deploy Hook (Cloudflare Pages, Vercel, Netlify, etc.).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PublishWebhook {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub target_url: String,
    pub events: Vec<String>,
    pub is_active: bool,
    pub last_triggered_at: Option<DateTime<Utc>>,
    pub last_status_code: Option<i16>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}
