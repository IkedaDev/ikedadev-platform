use crate::domain::models::{
    Article, ArticleRevision, ArticleSeo, ArticleStatus, ContentCategory, ContentTag,
    PublishWebhook,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ikeda_core::criteria::Criteria;
use ikeda_core::error::AppError;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CreateArticleData {
    pub tenant_id: Uuid,
    pub category_id: Option<Uuid>,
    pub slug: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub content_json: serde_json::Value,
    pub content_html: String,
    pub cover_image_url: Option<String>,
    pub author_id: Option<Uuid>,
    pub author_name_snapshot: Option<String>,
    pub word_count: i32,
    pub reading_time_minutes: i32,
    pub seo: ArticleSeo,
    pub metadata: serde_json::Value,
    pub tag_ids: Vec<Uuid>,
}

#[derive(Debug, Clone)]
pub struct UpdateArticleData {
    pub category_id: Option<Option<Uuid>>,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub excerpt: Option<Option<String>>,
    pub content_json: Option<serde_json::Value>,
    pub content_html: Option<String>,
    pub cover_image_url: Option<Option<String>>,
    pub word_count: Option<i32>,
    pub reading_time_minutes: Option<i32>,
    pub seo: Option<ArticleSeo>,
    pub metadata: Option<serde_json::Value>,
    pub tag_ids: Option<Vec<Uuid>>,
}

#[derive(Debug, Clone)]
pub struct CreateRevisionData {
    pub tenant_id: Uuid,
    pub article_id: Uuid,
    pub title: String,
    pub content_json: serde_json::Value,
    pub content_html: String,
    pub edited_by: Option<Uuid>,
}

#[derive(Debug, Clone)]
pub struct CreateCategoryData {
    pub tenant_id: Uuid,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub display_order: i32,
}

#[derive(Debug, Clone)]
pub struct UpdateCategoryData {
    pub name: Option<String>,
    pub slug: Option<String>,
    pub description: Option<Option<String>>,
    pub display_order: Option<i32>,
}

#[derive(Debug, Clone)]
pub struct CreateTagData {
    pub tenant_id: Uuid,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone)]
pub struct CreateWebhookData {
    pub tenant_id: Uuid,
    pub name: String,
    pub target_url: String,
    pub events: Vec<String>,
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct UpdateWebhookData {
    pub name: Option<String>,
    pub target_url: Option<String>,
    pub events: Option<Vec<String>>,
    pub is_active: Option<bool>,
}

#[async_trait]
pub trait ArticleRepository: Send + Sync {
    async fn create(&self, data: CreateArticleData) -> Result<Article, AppError>;
    async fn find_by_id(&self, tenant_id: Uuid, id: Uuid) -> Result<Option<Article>, AppError>;
    async fn find_by_slug(&self, tenant_id: Uuid, slug: &str) -> Result<Option<Article>, AppError>;
    async fn search(
        &self,
        tenant_id: Uuid,
        criteria: &Criteria,
    ) -> Result<(Vec<Article>, u64), AppError>;
    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        data: UpdateArticleData,
    ) -> Result<Article, AppError>;
    async fn update_status(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status: ArticleStatus,
        published_at: Option<DateTime<Utc>>,
        scheduled_for: Option<DateTime<Utc>>,
    ) -> Result<Article, AppError>;
    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<Article, AppError>;

    /// Creates a new revision snapshot and prunes old versions beyond `keep_last` (20).
    async fn create_revision(
        &self,
        data: CreateRevisionData,
        keep_last: i64,
    ) -> Result<ArticleRevision, AppError>;
    async fn list_revisions(
        &self,
        tenant_id: Uuid,
        article_id: Uuid,
    ) -> Result<Vec<ArticleRevision>, AppError>;
    async fn find_revision(
        &self,
        tenant_id: Uuid,
        article_id: Uuid,
        revision_id: Uuid,
    ) -> Result<Option<ArticleRevision>, AppError>;
}

#[async_trait]
pub trait TaxonomyRepository: Send + Sync {
    async fn create_category(&self, data: CreateCategoryData) -> Result<ContentCategory, AppError>;
    async fn list_categories(&self, tenant_id: Uuid) -> Result<Vec<ContentCategory>, AppError>;
    async fn update_category(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        data: UpdateCategoryData,
    ) -> Result<ContentCategory, AppError>;
    async fn soft_delete_category(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AppError>;

    async fn create_tag(&self, data: CreateTagData) -> Result<ContentTag, AppError>;
    async fn list_tags(&self, tenant_id: Uuid) -> Result<Vec<ContentTag>, AppError>;
    async fn soft_delete_tag(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AppError>;
}

#[async_trait]
pub trait WebhookRepository: Send + Sync {
    async fn create(&self, data: CreateWebhookData) -> Result<PublishWebhook, AppError>;
    async fn list(&self, tenant_id: Uuid) -> Result<Vec<PublishWebhook>, AppError>;
    async fn find_by_id(
        &self,
        tenant_id: Uuid,
        id: Uuid,
    ) -> Result<Option<PublishWebhook>, AppError>;
    async fn find_active_for_event(
        &self,
        tenant_id: Uuid,
        event_name: &str,
    ) -> Result<Vec<PublishWebhook>, AppError>;
    async fn update(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        data: UpdateWebhookData,
    ) -> Result<PublishWebhook, AppError>;
    async fn record_delivery(
        &self,
        tenant_id: Uuid,
        id: Uuid,
        status_code: i16,
    ) -> Result<(), AppError>;
    async fn soft_delete(&self, tenant_id: Uuid, id: Uuid) -> Result<(), AppError>;
}
