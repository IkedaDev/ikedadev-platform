use super::taxonomy::{ContentCategory, ContentTag};
use chrono::{DateTime, Utc};
use ikeda_core::error::AppError;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// Editorial lifecycle status of an article.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ArticleStatus {
    Draft,
    Scheduled,
    Published,
    Archived,
}

impl ArticleStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Scheduled => "SCHEDULED",
            Self::Published => "PUBLISHED",
            Self::Archived => "ARCHIVED",
        }
    }

    pub fn parse(value: &str) -> Result<Self, AppError> {
        match value.trim().to_ascii_uppercase().as_str() {
            "DRAFT" => Ok(Self::Draft),
            "SCHEDULED" => Ok(Self::Scheduled),
            "PUBLISHED" => Ok(Self::Published),
            "ARCHIVED" => Ok(Self::Archived),
            other => Err(AppError::BadRequest(format!(
                "Invalid article status: '{other}'"
            ))),
        }
    }

    /// Validates whether a transition from `self` to `next` is allowed by the editorial state machine.
    pub fn can_transition_to(self, next: Self) -> bool {
        match (self, next) {
            (Self::Draft, Self::Draft | Self::Scheduled | Self::Published | Self::Archived) => true,
            (Self::Scheduled, Self::Draft | Self::Scheduled | Self::Published | Self::Archived) => {
                true
            }
            (Self::Published, Self::Draft | Self::Published | Self::Archived) => true,
            (Self::Archived, Self::Draft | Self::Published | Self::Archived) => true,
            (Self::Published | Self::Archived, Self::Scheduled) => false,
        }
    }
}

/// SEO metadata stored as JSONB on an article.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ArticleSeo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub canonical_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub og_image_url: Option<String>,
    #[serde(default)]
    pub noindex: bool,
}

/// Primary editorial article entity.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Article {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub category_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<ContentCategory>,
    pub tags: Vec<ContentTag>,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub content_json: serde_json::Value,
    pub content_html: String,
    pub cover_image_url: Option<String>,
    pub status: ArticleStatus,
    pub published_at: Option<DateTime<Utc>>,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub author_id: Option<Uuid>,
    pub author_name_snapshot: Option<String>,
    pub word_count: i32,
    pub reading_time_minutes: i32,
    pub seo: ArticleSeo,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Historical snapshot of an article created when editing a published article.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArticleRevision {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub article_id: Uuid,
    pub version_number: i32,
    pub title: String,
    pub content_json: serde_json::Value,
    pub content_html: String,
    pub edited_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
